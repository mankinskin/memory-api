use ticket_cli::cli::{CliOutput, error_output, parse_cli_from, run};

fn main() {
    let cli = match parse_cli_from(std::env::args_os()) {
        Ok(cli) => cli,
        Err(err) => {
            let wants_json = std::env::args().any(|a| a == "--json");
            let rendered = error_output(&err.to_string(), wants_json);
            eprintln!("{rendered}");
            std::process::exit(2);
        }
    };

    match run(cli) {
        Ok(CliOutput::Json(value)) => {
            match serde_json::to_string_pretty(&value) {
                Ok(rendered) => println!("{rendered}"),
                Err(err) => {
                    eprintln!("{}", error_output(&err.to_string(), true));
                    std::process::exit(1);
                }
            }
        }
        Ok(CliOutput::Text(text)) => println!("{text}"),
        Err(err) => {
            let wants_json = std::env::args().any(|a| a == "--json");
            eprintln!("{}", error_output(&err.to_string(), wants_json));
            std::process::exit(1);
        }
    }
}
