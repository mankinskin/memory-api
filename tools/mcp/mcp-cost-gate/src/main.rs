//! Model-aware MCP middleware binary.
//!
//! Fronts a real MCP stdio server and enforces the price-awareness policy by
//! requiring a `caller_model` argument on every `tools/call`:
//!
//! ```text
//! mcp-cost-gate -- <real-server-command> [server args...]
//! ```
//!
//! On `tools/list` it injects a required `caller_model` argument into every
//! advertised tool schema. On `tools/call` it reads `arguments.caller_model`,
//! rejects the call if absent, refuses token-heavy calls from orchestrator-tier
//! models with delegation guidance, and otherwise strips `caller_model` and
//! forwards the cleaned call. All other traffic passes through untouched.
//!
//! Fail-open: if the price table cannot be loaded the proxy is a transparent
//! passthrough (no schema injection, no enforcement).
//!
//! Environment:
//! * `COST_GATE_TABLE` — path to `model_prices.json` (required for enforcement).
//! * `COST_GATE_X` — threshold on `output_mtok` (default 15).

use std::{
    io::{
        BufRead,
        BufReader,
        Write,
    },
    path::PathBuf,
    process::{
        Child,
        Command,
        Stdio,
    },
    sync::{
        Arc,
        Mutex,
    },
};

use mcp_cost_gate::{
    gate::{
        DEFAULT_THRESHOLD_X,
        Gate,
    },
    proxy::{
        ClientAction,
        PendingList,
        handle_client_message,
        handle_server_message,
    },
};
use serde_json::Value;

fn log(msg: &str) {
    eprintln!("[mcp-cost-gate] {msg}");
}

/// Split argv into the real server command (everything after `--`).
fn server_command(argv: &[String]) -> Vec<String> {
    if let Some(pos) = argv.iter().position(|a| a == "--") {
        argv[pos + 1..].to_vec()
    } else {
        argv[1..].to_vec()
    }
}

fn load_gate() -> Option<Gate> {
    let x = std::env::var("COST_GATE_X")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(DEFAULT_THRESHOLD_X);
    let table = std::env::var("COST_GATE_TABLE").ok().map(PathBuf::from)?;
    match Gate::load(&table, x) {
        Ok(g) => {
            log(&format!(
                "enforcing per-request caller_model (table={}, x={x})",
                table.display()
            ));
            Some(g)
        }
        Err(e) => {
            log(&format!("disabled (fail-open): {e}"));
            None
        }
    }
}

fn spawn_server(command: &[String]) -> std::io::Result<Child> {
    Command::new(&command[0])
        .args(&command[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let command = server_command(&argv);
    if command.is_empty() {
        log("no server command provided; usage: mcp-cost-gate -- <server> [args...]");
        std::process::exit(2);
    }

    let gate = load_gate().map(Arc::new);

    let mut child = match spawn_server(&command) {
        Ok(c) => c,
        Err(e) => {
            log(&format!("failed to launch server {command:?}: {e}"));
            std::process::exit(2);
        }
    };

    let child_stdin = child.stdin.take().expect("piped stdin");
    let child_stdout = child.stdout.take().expect("piped stdout");

    // Shared client-stdout writer (both threads may write to it) and the set of
    // in-flight tools/list request ids.
    let client_out = Arc::new(Mutex::new(std::io::stdout()));
    let pending = Arc::new(Mutex::new(PendingList::default()));

    // Server -> client: pass through, injecting tool schemas on list responses.
    let reader_out = Arc::clone(&client_out);
    let reader_pending = Arc::clone(&pending);
    let reader = std::thread::spawn(move || {
        let mut lines = BufReader::new(child_stdout).lines();
        while let Some(Ok(line)) = lines.next() {
            if line.trim().is_empty() {
                continue;
            }
            let out_line = match serde_json::from_str::<Value>(&line) {
                Ok(msg) => {
                    let rewritten =
                        handle_server_message(msg, &mut reader_pending.lock().unwrap());
                    serde_json::to_string(&rewritten).unwrap_or(line)
                }
                Err(_) => line,
            };
            let mut out = reader_out.lock().unwrap();
            let _ = writeln!(out, "{out_line}");
            let _ = out.flush();
        }
    });

    // Client -> server: gate tools/call, record tools/list, forward the rest.
    let mut server_in = child_stdin;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(msg) => {
                let action = handle_client_message(
                    msg,
                    gate.as_deref(),
                    &mut pending.lock().unwrap(),
                );
                match action {
                    ClientAction::Forward(v) => {
                        let s = serde_json::to_string(&v).unwrap_or(line);
                        if writeln!(server_in, "{s}").is_err() {
                            break;
                        }
                        let _ = server_in.flush();
                    }
                    ClientAction::Respond(v) => {
                        let s = serde_json::to_string(&v).unwrap_or_default();
                        let mut out = client_out.lock().unwrap();
                        let _ = writeln!(out, "{s}");
                        let _ = out.flush();
                    }
                }
            }
            Err(_) => {
                // Not JSON we understand; forward verbatim.
                if writeln!(server_in, "{line}").is_err() {
                    break;
                }
                let _ = server_in.flush();
            }
        }
    }

    // Client closed; drop the server's stdin so it can exit, then wait.
    drop(server_in);
    let _ = reader.join();
    let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(0);
    std::process::exit(code);
}
