use std::process::Command;

use tempfile::tempdir;

#[test]
fn init_supports_toon_output() {
    let dir = tempdir().expect("temp dir");
    let index_root = dir.path().join(".rule");

    let out = Command::new(env!("CARGO_BIN_EXE_rule"))
        .arg("--toon")
        .arg("--index-root")
        .arg(&index_root)
        .arg("init")
        .output()
        .expect("rule binary should spawn");

    assert!(
        out.status.success(),
        "rule --toon init failed ({})\nstdout: {}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let rendered =
        String::from_utf8(out.stdout).expect("toon output should be utf-8");
    let parsed: serde_json::Value = toon_format::decode_default(&rendered)
        .expect("toon output should decode");

    assert_eq!(parsed["command"], "init");
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["message"], "workspace initialized");
}