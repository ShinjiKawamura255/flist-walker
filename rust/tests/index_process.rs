//! Exercise the real binary's private indexing entrypoint before normal startup.
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn fixture(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "fw-process-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}
fn request(limit: usize) -> Value {
    json!({"request_id":41,"tab_id":7,"use_filelist":false,"include_files":true,"include_dirs":true,"max_depth":null,"follow_links":false,"complete_walker_snapshot":false,"settings":{"walker_max_entries":limit,"walker_metrics":false,"walker_adaptive_initial_limit":null,"walker_adaptive_max_limit":null}})
}
fn run(root: &Path, request: Value) -> Vec<Value> {
    run_binary(env!("CARGO_BIN_EXE_flistwalker"), root, request)
}
fn run_binary(binary: &str, root: &Path, request: Value) -> Vec<Value> {
    let mut child = Command::new(binary)
        .arg("--internal-index-worker-v1")
        .arg(root)
        .arg(request.to_string())
        .env("RUST_LOG", "trace")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "child status {status}");
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("private indexing child hung");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut bytes = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    let mut frames: Vec<Value> = Vec::new();
    while !bytes.is_empty() {
        assert!(bytes.len() >= 4);
        let size = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        assert!(size <= 8 * 1024 * 1024 && bytes.len() >= 4 + size);
        frames.push(serde_json::from_slice(&bytes[4..4 + size]).unwrap());
        bytes.drain(..4 + size);
    }
    for (i, frame) in frames.iter().enumerate() {
        assert_eq!(frame["version"], 1);
        assert_eq!(frame["request_id"], 41);
        assert_eq!(frame["tab_id"], 7);
        assert_eq!(frame["sequence"], i);
    }
    frames
}
#[test]
fn hidden_index_worker_runs_before_normal_config_startup() {
    let root = fixture("startup");
    std::fs::write(root.join("alpha.txt"), "alpha").unwrap();
    let frames = run(&root, request(50));
    assert!(frames.iter().any(|f| f["event"]["type"] == "started"));
    assert_eq!(frames.last().unwrap()["event"]["type"], "finished");
    assert!(!root.join(".flistwalker_config.json").exists());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn hidden_index_worker_uses_effective_settings_snapshot() {
    let root = fixture("settings");
    for i in 0..10 {
        std::fs::write(root.join(format!("file-{i}")), "").unwrap();
    }
    let frames = run(&root, request(2));
    assert!(frames
        .iter()
        .any(|f| f["event"]["type"] == "truncated" && f["event"]["limit"] == 2));
    std::fs::remove_dir_all(root).unwrap();
}
// macOS APFS rejects ill-formed UTF-8 file names. The native codec roundtrip
// remains tested on macOS without asking the filesystem to create one.
#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn hidden_index_worker_preserves_native_non_unicode_paths() {
    use std::os::unix::ffi::OsStringExt;
    let base = fixture("native");
    let root = base.join(std::ffi::OsString::from_vec(b"root-\xff".to_vec()));
    std::fs::create_dir(&root).unwrap();
    let name = std::ffi::OsString::from_vec(b"file-\xfe".to_vec());
    let path = root.join(name);
    std::fs::write(&path, "").unwrap();
    let frames = run(&root, request(50));
    use std::os::unix::ffi::OsStrExt;
    let expected = json!(path.as_os_str().as_bytes());
    assert!(frames.iter().any(|f| f["event"]["entries"]
        .as_array()
        .is_some_and(|es| es.iter().any(|e| e["path"] == expected))));
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn fw_hidden_index_worker_dispatches_before_public_cli() {
    let root = fixture("fw");
    std::fs::write(root.join("alpha.txt"), "").unwrap();
    let frames = run_binary(env!("CARGO_BIN_EXE_fw"), &root, request(10));
    assert_eq!(frames.last().unwrap()["event"]["type"], "finished");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn hidden_index_worker_reports_missing_root_as_failure() {
    let base = fixture("missing-root");
    let mut req = request(50);
    req["use_filelist"] = json!(true);
    let frames = run(&base.join("missing"), req);
    assert_eq!(frames.last().unwrap()["event"]["type"], "failed");
    assert!(!frames.iter().any(|f| f["event"]["type"] == "finished"));
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn hidden_index_worker_reports_non_directory_root_as_failure() {
    let base = fixture("non-directory-root");
    let root = base.join("ordinary-file");
    std::fs::write(&root, "data").unwrap();
    let frames = run(&root, request(50));
    assert_eq!(frames.last().unwrap()["event"]["type"], "failed");
    std::fs::remove_dir_all(base).unwrap();
}
