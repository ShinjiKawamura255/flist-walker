#[path = "../windows_resource_build.rs"]
mod windows_resource_build;

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

fn cli_command(name: &str) -> Command {
    let settings_root = test_root(&format!("{name}-settings"));
    fs::create_dir_all(&settings_root).expect("create settings root");
    let mut command = Command::new(bin_path());
    command
        .env_remove("RUST_LOG")
        .env("HOME", &settings_root)
        .env("USERPROFILE", &settings_root)
        .env("LOCALAPPDATA", &settings_root)
        .env("APPDATA", &settings_root);
    command
}

#[test]
fn cli_prints_version_with_long_flag() {
    let output = cli_command("version-long")
        .arg("--version")
        .output()
        .expect("run cli");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        format!("flistwalker {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn cli_prints_version_with_short_flag() {
    let output = cli_command("version-short")
        .arg("-V")
        .output()
        .expect("run cli");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        format!("flistwalker {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn cli_outputs_at_most_limit_lines_for_empty_query() {
    let root = test_root("limit");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("a.txt"), "a").expect("write a");
    fs::write(root.join("b.txt"), "b").expect("write b");

    let output = cli_command("limit")
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

#[cfg(unix)]
#[test]
fn tc_006_print0_preserves_non_utf8_path_bytes() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let root = test_root("print0-non-utf8");
    fs::create_dir_all(&root).expect("create root");
    let filename = vec![b'n', 0x80, b'.', b't', b'x', b't'];
    fs::write(root.join(OsString::from_vec(filename.clone())), "x").expect("write non-UTF-8 file");

    let output = cli_command("print0-non-utf8")
        .arg("--cli")
        .arg("--root")
        .arg(&root)
        .args(["--source", "walker", "--type", "file", "--print0"])
        .output()
        .expect("run cli");

    assert!(output.status.success());
    let mut expected = filename;
    expected.push(0);
    assert_eq!(output.stdout, expected);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cli_does_not_cap_limit_to_1000() {
    let root = test_root("limit-over-1000");
    fs::create_dir_all(&root).expect("create root");

    let file_count = 1105usize;
    let mut expected = Vec::with_capacity(file_count);
    for idx in 0..file_count {
        let path = root.join(format!("item-{idx:04}.txt"));
        fs::write(&path, "x").expect("write file");
        expected.push(path);
    }

    let output = cli_command("limit-over-1000")
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--limit",
            "1105",
        ])
        .output()
        .expect("run cli");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), file_count);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cli_returns_non_zero_when_root_does_not_exist() {
    let missing = test_root("missing");
    let output = cli_command("missing")
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

#[test]
fn cli_formats_path_output_for_query() {
    let root = test_root("scored-output");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("main.rs"), "fn main() {}").expect("write main");
    fs::write(root.join("readme.md"), "readme").expect("write readme");

    let output = cli_command("scored-output")
        .args([
            "--cli",
            "main",
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
    assert_eq!(lines[0], "main.rs");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cli_returns_empty_stdout_when_no_matches() {
    let root = test_root("no-match");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("main.rs"), "fn main() {}").expect("write main");

    let output = cli_command("no-match")
        .args([
            "--cli",
            "zzzzzz",
            "--root",
            root.to_string_lossy().as_ref(),
            "--limit",
            "10",
        ])
        .output()
        .expect("run cli");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cli_interprets_filelist_paths_for_current_platform() {
    let root = test_root("filelist-platform-interpretation");
    fs::create_dir_all(root.join("nested")).expect("create nested root");
    let file = root.join("nested").join("item.txt");
    fs::write(&file, "x").expect("write file");
    fs::write(root.join("FileList.txt"), "nested\\item.txt\n").expect("write filelist");

    let output = cli_command("filelist-platform-interpretation")
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--limit",
            "10",
        ])
        .output()
        .expect("run cli");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let actual = fs::canonicalize(root.join(stdout.trim())).expect("canonicalize cli output");
    let expected = fs::canonicalize(&file).expect("canonicalize expected file");
    assert_eq!(actual, expected);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cli_returns_non_zero_when_root_is_file() {
    let root = test_root("root-is-file");
    fs::create_dir_all(&root).expect("create root dir");
    let file_root = root.join("not_a_dir.txt");
    fs::write(&file_root, "x").expect("write file");

    let output = cli_command("root-is-file")
        .args([
            "--cli",
            "--root",
            file_root.to_string_lossy().as_ref(),
            "--limit",
            "5",
        ])
        .output()
        .expect("run cli");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("root is not a directory"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_006_help_describes_cli_usability_options() {
    let output = cli_command("help-options")
        .arg("--help")
        .output()
        .expect("run cli help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "--interactive",
        "--absolute",
        "--print0",
        "--fail-no-match",
        "--type",
        "--regex",
        "--case-sensitive",
        "--source",
        "--ignore-file",
        "--no-ignore",
        "--progress",
    ] {
        assert!(
            stdout.contains(expected),
            "help missing {expected}: {stdout}"
        );
    }
    assert!(stdout.contains("Print paths without opening the GUI"));
}

#[test]
fn tc_006_interactive_requires_cli_mode() {
    let output = cli_command("interactive-requires-cli")
        .arg("--interactive")
        .output()
        .expect("run cli");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--cli"), "unexpected stderr: {stderr}");
}

#[test]
fn tc_006_absolute_and_print0_control_path_framing() {
    let root = test_root("absolute-print0");
    fs::create_dir_all(&root).expect("create root");
    let file = root.join("alpha.txt");
    fs::write(&file, "alpha").expect("write file");

    let output = cli_command("absolute-print0")
        .args([
            "--cli",
            "alpha",
            "--root",
            root.to_string_lossy().as_ref(),
            "--absolute",
            "--print0",
        ])
        .output()
        .expect("run cli");

    assert!(output.status.success());
    assert_eq!(output.stdout.last(), Some(&0));
    assert_eq!(output.stdout.iter().filter(|byte| **byte == 0).count(), 1);
    let text = String::from_utf8_lossy(&output.stdout[..output.stdout.len() - 1]);
    let actual = fs::canonicalize(text.as_ref()).expect("canonicalize output");
    assert_eq!(actual, fs::canonicalize(file).expect("canonicalize file"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_006_fail_no_match_changes_only_the_exit_status() {
    let root = test_root("fail-no-match");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("alpha.txt"), "alpha").expect("write file");

    let output = cli_command("fail-no-match")
        .args([
            "--cli",
            "missing",
            "--root",
            root.to_string_lossy().as_ref(),
            "--fail-no-match",
        ])
        .output()
        .expect("run cli");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_006_type_filter_distinguishes_files_and_folders() {
    let root = test_root("type-filter");
    fs::create_dir_all(root.join("folder")).expect("create folder");
    fs::write(root.join("file.txt"), "file").expect("write file");

    let files = cli_command("type-files")
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--type",
            "file",
        ])
        .output()
        .expect("run files cli");
    let folders = cli_command("type-folders")
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--type",
            "folder",
        ])
        .output()
        .expect("run folders cli");

    assert!(files.status.success());
    assert_eq!(String::from_utf8_lossy(&files.stdout).trim(), "file.txt");
    assert!(folders.status.success());
    assert_eq!(String::from_utf8_lossy(&folders.stdout).trim(), "folder");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_006_regex_and_case_sensitive_options_reach_shared_search() {
    let root = test_root("regex-case");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("Alpha123.txt"), "alpha").expect("write file");

    let regex = cli_command("regex")
        .args([
            "--cli",
            "Alpha[0-9]+",
            "--root",
            root.to_string_lossy().as_ref(),
            "--regex",
        ])
        .output()
        .expect("run regex cli");
    let case_sensitive = cli_command("case-sensitive")
        .args([
            "--cli",
            "alpha",
            "--root",
            root.to_string_lossy().as_ref(),
            "--case-sensitive",
        ])
        .output()
        .expect("run case-sensitive cli");

    assert!(regex.status.success());
    assert_eq!(
        String::from_utf8_lossy(&regex.stdout).trim(),
        "Alpha123.txt"
    );
    assert!(case_sensitive.status.success());
    assert!(case_sensitive.stdout.is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_006_source_controls_filelist_and_walker_selection() {
    let root = test_root("source-selection");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("listed.txt"), "listed").expect("write listed");
    fs::write(root.join("walked.txt"), "walked").expect("write walked");
    fs::write(root.join("FileList.txt"), "listed.txt\n").expect("write filelist");

    let automatic = cli_command("source-auto")
        .args(["--cli", "walked", "--root", root.to_string_lossy().as_ref()])
        .output()
        .expect("run auto cli");
    let walker = cli_command("source-walker")
        .args([
            "--cli",
            "walked",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
        ])
        .output()
        .expect("run walker cli");

    assert!(automatic.status.success());
    assert!(automatic.stdout.is_empty());
    assert!(walker.status.success());
    assert_eq!(String::from_utf8_lossy(&walker.stdout).trim(), "walked.txt");

    let missing_root = test_root("source-filelist-missing");
    fs::create_dir_all(&missing_root).expect("create missing root");
    let required = cli_command("source-filelist-missing")
        .args([
            "--cli",
            "--root",
            missing_root.to_string_lossy().as_ref(),
            "--source",
            "filelist",
        ])
        .output()
        .expect("run required filelist cli");
    assert!(!required.status.success());
    assert!(String::from_utf8_lossy(&required.stderr).contains("FileList"));

    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&missing_root);
}

#[test]
fn tc_006_explicit_ignore_file_and_no_ignore_are_supported_and_conflict() {
    let root = test_root("ignore-options");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("visible.txt"), "visible").expect("write visible");
    fs::write(root.join("hidden.tmp"), "hidden").expect("write hidden");
    let ignore = root.join("custom.ignore");
    fs::write(&ignore, "tmp\n").expect("write ignore");

    let filtered = cli_command("ignore-file")
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--ignore-file",
            ignore.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("run ignore cli");
    assert!(filtered.status.success());
    let filtered_text = String::from_utf8_lossy(&filtered.stdout);
    assert!(filtered_text.contains("visible.txt"));
    assert!(!filtered_text.contains("hidden.tmp"));

    let conflict = cli_command("ignore-conflict")
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--ignore-file",
            ignore.to_string_lossy().as_ref(),
            "--no-ignore",
        ])
        .output()
        .expect("run conflicting cli");
    assert!(!conflict.status.success());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_006_progress_is_written_to_stderr_only() {
    let root = test_root("progress");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("alpha.txt"), "alpha").expect("write file");

    let output = cli_command("progress")
        .args([
            "--cli",
            "alpha",
            "--root",
            root.to_string_lossy().as_ref(),
            "--progress",
        ])
        .output()
        .expect("run cli");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "alpha.txt");
    assert!(String::from_utf8_lossy(&output.stderr).contains("Indexing"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_gnu_build_links_windows_icon_into_final_exe() {
    let build_dir = std::path::Path::new("/tmp/flistwalker-build");
    let directives = windows_resource_build::cargo_directives_for_windows_resource_bin(
        "gnu",
        windows_resource_build::WINDOWS_GUI_BIN_NAME,
        build_dir,
    );

    assert_eq!(directives.len(), 1);
    assert_eq!(
        directives[0],
        format!(
            "cargo:rustc-link-arg-bin=flistwalker={}",
            build_dir.join("resource.o").display()
        )
    );
}

#[test]
fn non_gnu_build_does_not_emit_bin_specific_resource_linking() {
    let directives = windows_resource_build::cargo_directives_for_windows_resource_bin(
        "msvc",
        windows_resource_build::WINDOWS_GUI_BIN_NAME,
        std::path::Path::new("/tmp/flistwalker-build"),
    );

    assert!(directives.is_empty());
}
