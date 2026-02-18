use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn test_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("fff-rs-cli-{name}-{nonce}"))
}

fn bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_flistwalker"))
}

#[test]
fn cli_outputs_at_most_limit_lines_for_empty_query() {
    let root = test_root("limit");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("a.txt"), "a").expect("write a");
    fs::write(root.join("b.txt"), "b").expect("write b");

    let output = Command::new(bin_path())
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--limit",
            "1",
        ])
        .output()
        .expect("run cli");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), 1);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cli_returns_non_zero_when_root_does_not_exist() {
    let missing = test_root("missing");
    let output = Command::new(bin_path())
        .args([
            "--cli",
            "--root",
            missing.to_string_lossy().as_ref(),
            "--limit",
            "5",
        ])
        .output()
        .expect("run cli");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed to canonicalize root"));
}
