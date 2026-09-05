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

fn workspace_test_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("cli-contract-fixtures")
        .join(format!("fff-rs-cli-{name}-{nonce}"))
}

fn bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_flistwalker"))
}

fn fw_bin_path() -> PathBuf {
    let mut path = bin_path();
    path.set_file_name(if cfg!(windows) { "fw.exe" } else { "fw" });
    path
}

fn fw_command_with_settings(name: &str) -> Command {
    let settings_root = test_root(&format!("{name}-settings"));
    fs::create_dir_all(&settings_root).expect("create fw settings root");
    let mut command = Command::new(fw_bin_path());
    command
        .env_remove("RUST_LOG")
        .env("HOME", &settings_root)
        .env("USERPROFILE", &settings_root)
        .env("LOCALAPPDATA", &settings_root)
        .env("APPDATA", &settings_root);
    command
}

fn cli_command_with_settings(name: &str) -> (Command, PathBuf) {
    let settings_root = test_root(&format!("{name}-settings"));
    fs::create_dir_all(&settings_root).expect("create settings root");
    let mut command = Command::new(bin_path());
    command
        .env_remove("RUST_LOG")
        .env("HOME", &settings_root)
        .env("USERPROFILE", &settings_root)
        .env("LOCALAPPDATA", &settings_root)
        .env("APPDATA", &settings_root);
    let settings_dir = if cfg!(windows) {
        settings_root.join("flistwalker")
    } else {
        settings_root.join(".flistwalker")
    };
    (command, settings_dir)
}

fn cli_command(name: &str) -> Command {
    cli_command_with_settings(name).0
}

fn cli_command_in_settings(settings_root: &std::path::Path) -> Command {
    let mut command = Command::new(bin_path());
    command
        .env_remove("RUST_LOG")
        .env("HOME", settings_root)
        .env("USERPROFILE", settings_root)
        .env("LOCALAPPDATA", settings_root)
        .env("APPDATA", settings_root);
    command
}

fn write_persisted_roots(
    settings_dir: &std::path::Path,
    default_root: Option<&std::path::Path>,
    saved_roots: &[PathBuf],
) {
    fs::create_dir_all(settings_dir).expect("create settings directory");
    if let Some(default_root) = default_root {
        let default_root = serde_json::to_string(&default_root.to_string_lossy().to_string())
            .expect("serialize default root");
        let ui_state = format!(
            r#"{{"last_root":null,"default_root":{default_root},"show_preview":null,"ignore_list_enabled":true,"preview_panel_width":null,"query_history":[],"results_panel_width":null,"tabs":[],"active_tab":null,"window":null,"skipped_update_target_version":null,"suppress_update_check_failure_dialog":false}}"#
        );
        fs::write(settings_dir.join(".flistwalker_ui_state.json"), ui_state)
            .expect("write UI state");
    }
    let saved_roots = saved_roots
        .iter()
        .map(|path| path.to_string_lossy())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        settings_dir.join(".flistwalker_roots.txt"),
        if saved_roots.is_empty() {
            String::new()
        } else {
            format!("{saved_roots}\n")
        },
    )
    .expect("write saved roots");
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
fn tc_193_fw_uses_short_command_name_and_implicit_cli_mode() {
    let version = fw_command_with_settings("fw-version")
        .arg("--version")
        .output()
        .expect("run fw version");
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        format!("fw {}", env!("CARGO_PKG_VERSION"))
    );

    let root = test_root("fw-implicit-cli");
    fs::create_dir_all(&root).expect("create fw fixture root");
    fs::write(root.join("main.rs"), "main").expect("write fw fixture");
    fs::write(root.join("other.txt"), "other").expect("write fw fixture");

    let fw = fw_command_with_settings("fw-implicit-cli")
        .args([
            "main",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "--type",
            "file",
        ])
        .output()
        .expect("run implicit fw CLI");
    let universal = cli_command("fw-universal-parity")
        .args([
            "--cli",
            "main",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "--type",
            "file",
        ])
        .output()
        .expect("run universal CLI parity");

    assert!(
        fw.status.success(),
        "{}",
        String::from_utf8_lossy(&fw.stderr)
    );
    assert_eq!(fw.stdout, universal.stdout);
    assert_eq!(fw.stderr, universal.stderr);
    assert_eq!(fw.status.code(), universal.status.code());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn tc_193_fw_dispatches_hidden_restart_before_cli_argument_parsing() {
    let restart = fw_command_with_settings("fw-hidden-restart")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .arg("--flistwalker-internal-update-restart")
        .output()
        .expect("run fw internal restart");
    assert!(restart.status.success());
    assert!(restart.stdout.is_empty());
    assert!(restart.stderr.is_empty());

    let disabled_update = fw_command_with_settings("fw-disabled-update")
        .env("FLISTWALKER_DISABLE_SELF_UPDATE", "1")
        .arg("--update")
        .output()
        .expect("run disabled fw update");
    assert_eq!(disabled_update.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&disabled_update.stderr).trim(),
        "Automatic updates are disabled."
    );
}

#[test]
fn tc_174_named_roots_and_search_presets_round_trip_without_reserving_query_words() {
    let root = test_root("named-preset");
    let settings_root = test_root("named-preset-settings");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&settings_root).expect("create settings root");
    fs::write(root.join("alpha.rs"), "alpha").expect("write alpha");
    fs::write(root.join("preset.txt"), "reserved word").expect("write preset");

    let added = cli_command_in_settings(&settings_root)
        .args([
            "--cli",
            "--add-named-root",
            &format!("repo={}", root.display()),
        ])
        .output()
        .expect("add named root");
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let saved = cli_command_in_settings(&settings_root)
        .args([
            "--cli",
            "alpha",
            "--named-root",
            "repo",
            "--type",
            "file",
            "--save-preset",
            "rust",
        ])
        .output()
        .expect("save preset");
    assert!(
        saved.status.success(),
        "{}",
        String::from_utf8_lossy(&saved.stderr)
    );

    let applied = cli_command_in_settings(&settings_root)
        .args(["--cli", "--preset", "rust"])
        .output()
        .expect("apply preset");
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let applied_stdout = String::from_utf8_lossy(&applied.stdout);
    assert!(applied_stdout.contains("alpha.rs"), "{applied_stdout}");
    assert!(!applied_stdout.contains("preset.txt"), "{applied_stdout}");

    let named = cli_command_in_settings(&settings_root)
        .args(["--cli", "--list-named-roots"])
        .output()
        .expect("list named roots");
    assert!(String::from_utf8_lossy(&named.stdout).starts_with("repo\t"));
    let presets = cli_command_in_settings(&settings_root)
        .args(["--cli", "--list-presets"])
        .output()
        .expect("list presets");
    assert_eq!(String::from_utf8_lossy(&presets.stdout).trim(), "rust");

    let ordinary_query = cli_command_in_settings(&settings_root)
        .args(["--cli", "preset", "--root", root.to_string_lossy().as_ref()])
        .output()
        .expect("search reserved-looking word");
    assert!(ordinary_query.status.success());
    assert!(String::from_utf8_lossy(&ordinary_query.stdout).contains("preset.txt"));

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(settings_root);
}

#[test]
fn tc_175_cli_applies_field_scoped_query_without_changing_plain_query_behavior() {
    let root = test_root("field-query");
    fs::create_dir_all(root.join("src/config")).expect("create fixture");
    fs::write(root.join("src/config/archive.tar.gz"), "archive").expect("write archive");
    fs::write(root.join("src/config/main.rs"), "main").expect("write main");
    fs::write(root.join("src/readme.txt"), "readme").expect("write readme");

    let run = |query: &str| {
        cli_command("field-query")
            .args([
                "--cli",
                query,
                "--root",
                root.to_string_lossy().as_ref(),
                "--source",
                "walker",
                "--type",
                "file",
            ])
            .output()
            .unwrap_or_else(|error| panic!("run {query}: {error}"))
    };

    let field = run("dir:config ext:rs");
    assert!(
        field.status.success(),
        "{}",
        String::from_utf8_lossy(&field.stderr)
    );
    let stdout = String::from_utf8_lossy(&field.stdout);
    assert!(stdout.contains("main.rs"), "{stdout}");
    assert!(!stdout.contains("archive.tar.gz"), "{stdout}");
    assert!(!stdout.contains("readme.txt"), "{stdout}");

    let plain = run("main");
    assert!(
        plain.status.success(),
        "{}",
        String::from_utf8_lossy(&plain.stderr)
    );
    assert!(String::from_utf8_lossy(&plain.stdout).contains("main.rs"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn tc_175_cli_ext_query_excludes_extension_named_directories() {
    let root = test_root("field-extension-directory");
    fs::create_dir_all(root.join("generated.rs")).expect("create extension-named directory");
    fs::write(root.join("main.rs"), "main").expect("write Rust fixture");

    let output = cli_command("field-extension-directory")
        .args([
            "--cli",
            "ext:rs",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
        ])
        .output()
        .expect("run extension query");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("main.rs"), "{stdout}");
    assert!(!stdout.contains("generated.rs"), "{stdout}");

    let _ = fs::remove_dir_all(root);
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
fn tc_169_update_commands_are_headless_and_documented_in_english() {
    let help = cli_command("update-help")
        .arg("--help")
        .output()
        .expect("run help");
    assert!(help.status.success());
    let stdout = String::from_utf8_lossy(&help.stdout);
    assert!(stdout.contains("--check-update"));
    assert!(stdout.contains("Check for a newer release without installing it"));
    assert!(stdout.contains("--update"));
    assert!(stdout.contains("Check for and install the latest supported release"));

    let aliased_update = cli_command("update-cli-alias")
        .env("FLISTWALKER_DISABLE_SELF_UPDATE", "1")
        .args(["--cli", "--update"])
        .output()
        .expect("run update through a --cli alias");
    assert_eq!(aliased_update.status.code(), Some(1));
    assert!(aliased_update.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&aliased_update.stderr).trim(),
        "Automatic updates are disabled."
    );

    let headless_restart = cli_command("update-headless-restart")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .arg("--flistwalker-internal-update-restart")
        .output()
        .expect("run internal post-update restart without a display server");
    assert!(headless_restart.status.success());
    assert!(headless_restart.stdout.is_empty());
    assert!(headless_restart.stderr.is_empty());

    let disabled_check = cli_command("update-check-disabled")
        .env("FLISTWALKER_DISABLE_SELF_UPDATE", "1")
        .arg("--check-update")
        .output()
        .expect("run disabled update check");
    assert!(disabled_check.status.success());
    assert_eq!(
        String::from_utf8_lossy(&disabled_check.stdout).trim(),
        "Update checks are disabled."
    );
    assert!(disabled_check.stderr.is_empty());

    let disabled_update = cli_command("update-install-disabled")
        .env("FLISTWALKER_DISABLE_SELF_UPDATE", "1")
        .arg("--update")
        .output()
        .expect("run disabled update install");
    assert_eq!(disabled_update.status.code(), Some(1));
    assert!(disabled_update.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&disabled_update.stderr).trim(),
        "Automatic updates are disabled."
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

#[test]
fn tc_163_batch_sort_uses_shared_full_match_sort_before_limit() {
    let root = test_root("sort-before-limit");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("zeta.txt"), "z").expect("write zeta");
    fs::write(root.join("alpha.txt"), "a").expect("write alpha");
    fs::write(root.join("large.txt"), "123456789").expect("write large");

    let name = cli_command("sort-name")
        .args([
            "--cli",
            "txt",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "--sort",
            "name-asc",
            "--limit",
            "1",
        ])
        .output()
        .expect("run name-sort CLI");
    let size = cli_command("sort-size")
        .args([
            "--cli",
            "txt",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "--sort",
            "size-desc",
            "--limit",
            "1",
        ])
        .output()
        .expect("run size-sort CLI");
    let name_desc = cli_command("sort-name-desc")
        .args([
            "--cli",
            "txt",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "--sort",
            "name-desc",
            "--limit",
            "1",
        ])
        .output()
        .expect("run descending name-sort CLI");
    let size_asc = cli_command("sort-size-asc")
        .args([
            "--cli",
            "txt",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "--sort",
            "size-asc",
            "--limit",
            "1",
        ])
        .output()
        .expect("run ascending size-sort CLI");
    let zero_limit = cli_command("sort-limit-zero")
        .args([
            "--cli",
            "txt",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "--sort",
            "name-asc",
            "--limit",
            "0",
        ])
        .output()
        .expect("run zero-limit CLI");

    assert!(name.status.success());
    assert_eq!(String::from_utf8_lossy(&name.stdout).trim(), "alpha.txt");
    assert!(size.status.success());
    assert_eq!(String::from_utf8_lossy(&size.stdout).trim(), "large.txt");
    assert!(name_desc.status.success());
    assert_eq!(
        String::from_utf8_lossy(&name_desc.stdout).trim(),
        "zeta.txt"
    );
    assert!(size_asc.status.success());
    assert_eq!(
        String::from_utf8_lossy(&size_asc.stdout).trim(),
        "alpha.txt"
    );
    assert!(zero_limit.status.success());
    assert!(zero_limit.stdout.is_empty());

    for sort in [
        "score",
        "name-asc",
        "name-desc",
        "modified-desc",
        "modified-asc",
        "created-desc",
        "created-asc",
        "size-desc",
        "size-asc",
    ] {
        let output = cli_command(&format!("sort-accept-{sort}"))
            .args([
                "--cli",
                "txt",
                "--root",
                root.to_string_lossy().as_ref(),
                "--source",
                "walker",
                "--sort",
                sort,
                "--limit",
                "1",
            ])
            .output()
            .expect("run accepted sort mode");
        assert!(output.status.success(), "sort mode {sort} was not accepted");
    }

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_163_batch_root_selectors_use_persisted_roots_and_report_usage_errors() {
    let default_root = test_root("default-root");
    let saved_root = test_root("saved-root");
    fs::create_dir_all(&default_root).expect("create default root");
    fs::create_dir_all(&saved_root).expect("create saved root");
    fs::write(default_root.join("default.txt"), "default").expect("write default file");
    fs::write(saved_root.join("saved.txt"), "saved").expect("write saved file");

    let (mut default_command, default_settings) = cli_command_with_settings("default-root");
    write_persisted_roots(
        &default_settings,
        Some(&default_root),
        std::slice::from_ref(&saved_root),
    );
    let default_output = default_command
        .args([
            "--cli",
            "default",
            "--use-default-root",
            "--source",
            "walker",
        ])
        .output()
        .expect("run default-root CLI");

    let (mut saved_command, saved_settings) = cli_command_with_settings("saved-root");
    write_persisted_roots(
        &saved_settings,
        Some(&default_root),
        std::slice::from_ref(&saved_root),
    );
    let saved_output = saved_command
        .args(["--cli", "saved", "--saved-root", "1", "--source", "walker"])
        .output()
        .expect("run saved-root CLI");

    let missing_default = cli_command("missing-default")
        .args(["--cli", "--use-default-root"])
        .output()
        .expect("run missing-default CLI");
    let invalid_index = cli_command("invalid-saved-index")
        .args(["--cli", "--saved-root", "0"])
        .output()
        .expect("run invalid saved-root CLI");
    let root_default_conflict = cli_command("root-default-conflict")
        .args([
            "--cli",
            "--root",
            default_root.to_string_lossy().as_ref(),
            "--use-default-root",
        ])
        .output()
        .expect("run root/default conflict CLI");
    let root_saved_conflict = cli_command("root-saved-conflict")
        .args([
            "--cli",
            "--root",
            default_root.to_string_lossy().as_ref(),
            "--saved-root",
            "1",
        ])
        .output()
        .expect("run root/saved conflict CLI");
    let default_saved_conflict = cli_command("default-saved-conflict")
        .args(["--cli", "--use-default-root", "--saved-root", "1"])
        .output()
        .expect("run default/saved conflict CLI");

    assert!(default_output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&default_output.stdout).trim(),
        "default.txt"
    );
    assert!(saved_output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&saved_output.stdout).trim(),
        "saved.txt"
    );
    assert_eq!(missing_default.status.code(), Some(2));
    assert_eq!(invalid_index.status.code(), Some(2));
    assert_eq!(root_default_conflict.status.code(), Some(2));
    assert_eq!(root_saved_conflict.status.code(), Some(2));
    assert_eq!(default_saved_conflict.status.code(), Some(2));

    let _ = fs::remove_dir_all(&default_root);
    let _ = fs::remove_dir_all(&saved_root);
}

#[test]
fn tc_163_list_saved_roots_is_exclusive_and_preserves_framing() {
    let first = test_root("list-saved-first");
    let second = test_root("list-saved-second");
    let (mut command, settings_dir) = cli_command_with_settings("list-saved-roots");
    write_persisted_roots(&settings_dir, None, &[first.clone(), second.clone()]);
    let human = command
        .args(["--cli", "--list-saved-roots"])
        .output()
        .expect("run saved-roots list");

    let (mut nul_command, nul_settings_dir) = cli_command_with_settings("list-saved-roots-nul");
    write_persisted_roots(&nul_settings_dir, None, &[first.clone(), second.clone()]);
    let nul = nul_command
        .args(["--cli", "--list-saved-roots", "--print0"])
        .output()
        .expect("run NUL saved-roots list");

    let conflict = cli_command("list-saved-roots-conflict")
        .args(["--cli", "query", "--list-saved-roots"])
        .output()
        .expect("run conflicting list CLI");
    let (mut progress_conflict_command, progress_conflict_settings_dir) =
        cli_command_with_settings("list-saved-roots-progress-conflict");
    write_persisted_roots(
        &progress_conflict_settings_dir,
        None,
        &[first.clone(), second.clone()],
    );
    let progress_conflict = progress_conflict_command
        .args(["--cli", "--list-saved-roots", "--progress"])
        .output()
        .expect("run progress-conflicting list CLI");
    let missing = test_root("list-saved-missing");
    let (mut missing_command, missing_settings_dir) =
        cli_command_with_settings("list-saved-missing");
    write_persisted_roots(&missing_settings_dir, None, std::slice::from_ref(&missing));
    let missing_output = missing_command
        .args(["--cli", "--list-saved-roots"])
        .output()
        .expect("run saved-roots list with missing root");

    assert!(human.status.success());
    assert!(
        settings_dir.join(".flistwalker_config.json").is_file(),
        "saved-root listing must initialize runtime config"
    );
    assert_eq!(
        String::from_utf8_lossy(&human.stdout),
        format!("1\t{}\n2\t{}\n", first.display(), second.display())
    );
    assert!(nul.status.success());
    assert_eq!(
        String::from_utf8_lossy(&nul.stdout),
        format!("{}\0{}\0", first.display(), second.display())
    );
    assert_eq!(conflict.status.code(), Some(2));
    assert_eq!(progress_conflict.status.code(), Some(2));
    assert!(progress_conflict.stdout.is_empty());
    assert!(String::from_utf8_lossy(&progress_conflict.stderr)
        .contains("--list-saved-roots cannot be combined with search options"));
    assert!(
        !progress_conflict_settings_dir
            .join(".flistwalker_config.json")
            .exists(),
        "invalid list-saved-roots arguments must not bootstrap runtime config"
    );
    assert!(missing_output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&missing_output.stdout),
        format!("1\t{}\n", missing.display())
    );
}

#[test]
fn tc_164_action_print_preserves_output_and_action_all_requires_non_print_mode() {
    let root = test_root("action-print");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("alpha.txt"), "alpha").expect("write file");
    fs::write(root.join("beta.txt"), "beta").expect("write file");

    let print = cli_command("action-print")
        .args([
            "--cli",
            "alpha",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "--action",
            "print",
        ])
        .output()
        .expect("run print action CLI");
    let invalid_all = cli_command("action-all-print")
        .args(["--cli", "--action-all"])
        .output()
        .expect("run invalid action-all CLI");
    let absolute = cli_command("action-open-absolute")
        .args([
            "--cli",
            "alpha",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "--action",
            "open",
            "--absolute",
        ])
        .output()
        .expect("run invalid absolute action CLI");
    let print0 = cli_command("action-reveal-print0")
        .args([
            "--cli",
            "alpha",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "--action",
            "reveal",
            "--print0",
        ])
        .output()
        .expect("run invalid print0 action CLI");
    let implicit_multi = cli_command("action-open-implicit-multi")
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "--action",
            "open",
        ])
        .output()
        .expect("run implicit multi action CLI");

    assert!(print.status.success());
    assert_eq!(String::from_utf8_lossy(&print.stdout).trim(), "alpha.txt");
    assert_eq!(invalid_all.status.code(), Some(2));
    assert_eq!(absolute.status.code(), Some(2));
    assert_eq!(print0.status.code(), Some(2));
    assert_eq!(implicit_multi.status.code(), Some(1));
    assert!(implicit_multi.stdout.is_empty());
    assert!(String::from_utf8_lossy(&implicit_multi.stderr).contains("require --action-all"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_165_batch_create_filelist_requires_explicit_overwrite_and_keeps_stdout_empty() {
    let root = test_root("create-filelist");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("alpha.txt"), "alpha").expect("write file");

    let (mut created_command, created_settings_dir) = cli_command_with_settings("create-filelist");
    let created = created_command
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--create-filelist",
        ])
        .output()
        .expect("create FileList");
    let original = fs::read_to_string(root.join("FileList.txt")).expect("read created FileList");
    let refused = cli_command("create-filelist-refused")
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--create-filelist",
        ])
        .output()
        .expect("refuse overwrite");
    let invalid_query = cli_command("create-filelist-query")
        .args([
            "--cli",
            "alpha",
            "--root",
            root.to_string_lossy().as_ref(),
            "--create-filelist",
        ])
        .output()
        .expect("reject query combination");

    assert!(created.status.success());
    assert!(
        created_settings_dir
            .join(".flistwalker_config.json")
            .is_file(),
        "FileList creation must initialize runtime config"
    );
    assert!(created.stdout.is_empty());
    assert!(created
        .stderr
        .windows(b"committed".len())
        .any(|part| part == b"committed"));
    assert!(original.contains("alpha.txt"));
    assert_eq!(refused.status.code(), Some(1));
    assert!(refused.stdout.is_empty());
    assert_eq!(
        fs::read_to_string(root.join("FileList.txt")).expect("read unchanged"),
        original
    );
    assert_eq!(invalid_query.status.code(), Some(2));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc_165_batch_create_filelist_wires_overwrite_ancestors_and_saved_roots() {
    // Regression guard: the real CLI intentionally walks to the filesystem root.
    // Keep this subprocess fixture under the workspace so coverage/sandbox runs
    // never enumerate a developer's profile directory.
    let parent = workspace_test_root("create-filelist-ancestor");
    let root = parent.join("child");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("alpha.txt"), "alpha").expect("write file");
    let parent_filelist = parent.join("FileList.txt");
    fs::write(&parent_filelist, "before\n").expect("write ancestor FileList");

    let initial = cli_command("create-filelist-initial")
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--create-filelist",
        ])
        .output()
        .expect("create root FileList");
    let without_propagation =
        fs::read_to_string(&parent_filelist).expect("read unchanged ancestor");
    let overwrite = cli_command("create-filelist-overwrite")
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--create-filelist",
            "--overwrite-filelist",
            "--propagate-ancestors",
        ])
        .output()
        .expect("overwrite and propagate FileList");
    let propagated = fs::read_to_string(&parent_filelist).expect("read propagated ancestor");

    let saved_root = test_root("create-filelist-saved");
    fs::create_dir_all(&saved_root).expect("create saved root");
    fs::write(saved_root.join("saved.txt"), "saved").expect("write saved file");
    let (mut saved_command, settings_dir) = cli_command_with_settings("create-filelist-saved");
    write_persisted_roots(
        &settings_dir,
        Some(&saved_root),
        std::slice::from_ref(&saved_root),
    );
    let saved = saved_command
        .args(["--cli", "--use-default-root", "--create-filelist"])
        .output()
        .expect("create default saved-root FileList");
    let overwrite_only = cli_command("overwrite-requires-create")
        .args(["--cli", "--overwrite-filelist"])
        .output()
        .expect("overwrite requires create");
    let propagate_only = cli_command("propagate-requires-create")
        .args(["--cli", "--propagate-ancestors"])
        .output()
        .expect("propagate requires create");

    assert!(initial.status.success());
    assert_eq!(without_propagation, "before\n");
    assert!(overwrite.status.success());
    assert!(
        propagated.contains("child/FileList.txt") || propagated.contains("child\\FileList.txt")
    );
    assert!(saved.status.success());
    assert!(saved_root.join("FileList.txt").exists());
    assert_eq!(overwrite_only.status.code(), Some(2));
    assert_eq!(propagate_only.status.code(), Some(2));

    let _ = fs::remove_dir_all(&parent);
    let _ = fs::remove_dir_all(&saved_root);
}

// macOS filesystems reject the invalid byte sequence used by this Linux
// byte-preservation contract, even though Rust exposes Unix OsString APIs.
#[cfg(target_os = "linux")]
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
        "--action",
        "--action-all",
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
fn tc_172_color_help_and_batch_output_control_are_explicit() {
    let help = cli_command("color-help")
        .arg("--help")
        .output()
        .expect("run CLI help");
    assert!(help.status.success());
    let stdout = String::from_utf8_lossy(&help.stdout);
    assert!(stdout.contains("--color [<COLOR>]"), "{stdout}");
    assert!(stdout.contains("auto, always, never"), "{stdout}");

    let root = test_root("color-batch");
    fs::create_dir_all(&root).expect("create color root");
    fs::write(root.join("match.txt"), "match").expect("write color fixture");

    let always = cli_command("color-always")
        .args([
            "--cli",
            "at",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "--color",
            "always",
        ])
        .output()
        .expect("run forced-color batch");
    assert!(always.status.success());
    assert_eq!(always.stdout, b"m\x1b[38;5;11mat\x1b[0mch.txt\n");

    let auto = cli_command("color-auto")
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
        ])
        .output()
        .expect("run auto-color batch");
    assert!(auto.status.success());
    assert_eq!(auto.stdout, b"match.txt\n");

    let default = cli_command("color-default")
        .args([
            "--cli",
            "at",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
        ])
        .output()
        .expect("run default-color batch");
    assert!(default.status.success());
    assert_eq!(default.stdout, b"match.txt\n");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn tc_163_interactive_rejects_batch_only_exit_and_progress_options() {
    for option in ["--fail-no-match", "--progress"] {
        let output = cli_command("interactive-batch-only-conflict")
            .args(["--cli", "--interactive", option])
            .output()
            .expect("run conflicting interactive CLI");

        assert_eq!(output.status.code(), Some(2), "option {option}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("--interactive"),
            "unexpected stderr: {stderr}"
        );
        assert!(stderr.contains(option), "unexpected stderr: {stderr}");
    }
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

    let interactive_required = cli_command("interactive-source-filelist-missing-regression")
        .args([
            "--cli",
            "--interactive",
            "--root",
            missing_root.to_string_lossy().as_ref(),
            "--source",
            "filelist",
        ])
        .output()
        .expect("run interactive required FileList preflight");
    assert!(!interactive_required.status.success());
    let stderr = String::from_utf8_lossy(&interactive_required.stderr);
    assert!(stderr.contains("FileList"), "stderr={stderr}");
    assert!(
        !stderr.contains("terminal stdin and stderr"),
        "required FileList must fail before terminal setup: {stderr}"
    );

    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&missing_root);
}

#[test]
fn tc_180_cli_max_depth_limits_walker_and_filelist_and_rejects_zero() {
    let root = test_root("max-depth");
    let settings = test_root("max-depth-preset-settings");
    let child = root.join("child");
    let grand = child.join("grand");
    fs::create_dir_all(&grand).expect("create nested dirs");
    fs::write(root.join("top.txt"), "x").expect("write top");
    fs::write(child.join("child.txt"), "x").expect("write child");
    fs::write(grand.join("deep.txt"), "x").expect("write deep");
    fs::write(
        root.join("FileList.txt"),
        "top.txt\nchild/child.txt\nchild/grand/deep.txt\n",
    )
    .expect("write FileList");

    for source in ["walker", "filelist", "auto"] {
        let output = cli_command(&format!("max-depth-{source}"))
            .args([
                "--cli",
                "--root",
                root.to_string_lossy().as_ref(),
                "--source",
                source,
                "--max-depth",
                "2",
            ])
            .output()
            .expect("run max-depth CLI");
        assert!(output.status.success(), "source={source}: {output:?}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("top.txt"), "source={source}: {stdout}");
        assert!(stdout.contains("child.txt"), "source={source}: {stdout}");
        assert!(!stdout.contains("deep.txt"), "source={source}: {stdout}");
    }

    let zero = cli_command("max-depth-zero")
        .args(["--cli", "--max-depth", "0"])
        .output()
        .expect("run invalid max-depth CLI");
    assert_eq!(zero.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&zero.stderr).contains("--max-depth"));

    fs::create_dir_all(&settings).expect("create preset settings");
    let saved = cli_command_in_settings(&settings)
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "--max-depth",
            "2",
            "--save-preset",
            "depth-two",
        ])
        .output()
        .expect("save max-depth preset");
    assert!(saved.status.success(), "{saved:?}");

    let applied = cli_command_in_settings(&settings)
        .args(["--cli", "--preset", "depth-two"])
        .output()
        .expect("apply max-depth preset");
    assert!(applied.status.success(), "{applied:?}");
    let applied_stdout = String::from_utf8_lossy(&applied.stdout);
    assert!(applied_stdout.contains("child.txt"));
    assert!(!applied_stdout.contains("deep.txt"));

    let conflict = cli_command_in_settings(&settings)
        .args(["--cli", "--preset", "depth-two", "--max-depth", "3"])
        .output()
        .expect("reject preset max-depth override");
    assert_eq!(conflict.status.code(), Some(2));

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(settings);
}

#[test]
fn tc_006_explicit_ignore_file_and_no_ignore_are_supported_and_conflict() {
    let root = test_root("ignore-options");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("visible.txt"), "visible").expect("write visible");
    fs::write(root.join("hidden.tmp"), "hidden").expect("write hidden");
    let ignore = root.join("custom.ignore");
    fs::write(&ignore, "\u{feff}tmp\r\n").expect("write ignore");

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
fn tc_176_default_sidecar_filters_walker_and_filelist_and_rejects_invalid_utf8() {
    let stage = test_root("default-ignore-stage");
    let root = stage.join("root");
    let settings = stage.join("settings");
    fs::create_dir_all(&root).expect("create root");
    fs::create_dir_all(&settings).expect("create settings");
    fs::write(root.join("visible.txt"), "visible").expect("write visible");
    fs::write(root.join("hidden.tmp"), "hidden").expect("write hidden");
    fs::write(root.join("FileList.txt"), "visible.txt\nhidden.tmp\n").expect("write FileList");

    let source_exe = bin_path();
    let staged_exe = stage.join(source_exe.file_name().expect("binary filename"));
    fs::copy(&source_exe, &staged_exe).expect("stage CLI binary");
    let ignore = stage.join("flistwalker.ignore.txt");
    fs::write(&ignore, "\u{feff}hidden\r\n").expect("write default ignore sidecar");

    let command = || {
        let mut command = Command::new(&staged_exe);
        command
            .env_remove("RUST_LOG")
            .env("HOME", &settings)
            .env("USERPROFILE", &settings)
            .env("LOCALAPPDATA", &settings)
            .env("APPDATA", &settings);
        command
    };

    for (source, query) in [("walker", ""), ("filelist", "txt|tmp")] {
        let output = command()
            .args([
                "--cli",
                query,
                "--root",
                root.to_string_lossy().as_ref(),
                "--source",
                source,
                "--type",
                "file",
            ])
            .output()
            .unwrap_or_else(|error| panic!("run staged {source}: {error}"));
        assert!(
            output.status.success(),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("visible.txt"), "{source}: {stdout}");
        assert!(!stdout.contains("hidden.tmp"), "{source}: {stdout}");
    }

    fs::write(&ignore, [0xff, 0xfe, 0xfd]).expect("write invalid UTF-8 sidecar");
    let invalid = command()
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
        ])
        .output()
        .expect("run invalid sidecar");
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8_lossy(&invalid.stderr).contains("failed to read ignore file"),
        "{}",
        String::from_utf8_lossy(&invalid.stderr)
    );

    let _ = fs::remove_dir_all(stage);
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
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Indexing"));
    assert!(stderr.contains("Indexed 1 candidate"), "{stderr}");
    assert!(stderr.contains("Matched 1 path"), "{stderr}");

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
fn tc_193_gnu_build_links_windows_resources_into_both_executables() {
    let build_dir = std::path::Path::new("/tmp/flistwalker-build");
    let directives =
        windows_resource_build::cargo_directives_for_all_windows_bins("gnu", build_dir);

    assert_eq!(directives.len(), 2);
    assert!(directives
        .iter()
        .any(|directive| directive.contains("-bin=flistwalker=")));
    assert!(directives
        .iter()
        .any(|directive| directive.contains("-bin=fw=")));
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

#[test]
fn tc_170_help_describes_external_command_batching() {
    let output = cli_command("exec-help")
        .arg("--help")
        .output()
        .expect("run cli help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in ["-x", "--exec", "--exec-max-args", "--dry-run", "{}"] {
        assert!(
            stdout.contains(expected),
            "help missing {expected}: {stdout}"
        );
    }
}

#[test]
fn tc_170_invalid_exec_template_is_rejected_before_runtime_bootstrap() {
    let (mut command, settings_dir) = cli_command_with_settings("invalid-exec-template");
    let output = command
        .args(["--cli", "-x", "tool"])
        .output()
        .expect("run invalid exec template");

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("standalone {} placeholder"));
    assert!(!settings_dir.join(".flistwalker_config.json").exists());
}

#[test]
fn tc_170_zero_matches_do_not_spawn_external_command() {
    let root = test_root("exec-zero");
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("alpha.txt"), "alpha").expect("write file");

    let output = cli_command("exec-zero")
        .args([
            "--cli",
            "no-such-match",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "-x",
            "definitely-not-a-real-flistwalker-command",
            "--",
            "{}",
        ])
        .output()
        .expect("run zero-result exec");

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("failed to start"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tc_170_exec_dry_run_reports_all_results_and_greedy_batch_count() {
    let root = test_root("exec-dry-run");
    fs::create_dir_all(&root).expect("create root");
    for index in 0..5 {
        fs::write(root.join(format!("item-{index}.txt")), "x").expect("write file");
    }

    let output = cli_command("exec-dry-run")
        .args([
            "--cli",
            "--root",
            root.to_string_lossy().as_ref(),
            "--source",
            "walker",
            "--type",
            "file",
            "--limit",
            "5",
            "--exec-max-args",
            "2",
            "--dry-run",
            "-x",
            "definitely-not-a-real-flistwalker-command",
            "{}",
        ])
        .output()
        .expect("run exec dry-run");

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Dry run: 5 paths in 3 batches"), "{stderr}");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tc_170_exec_runs_every_result_in_bounded_batches() {
    let root = test_root("exec-real");
    fs::create_dir_all(&root).expect("create root");
    for index in 0..5 {
        fs::write(root.join(format!("item-{index}.txt")), "x").expect("write file");
    }

    let mut command = cli_command("exec-real");
    command.args([
        "--cli",
        "--root",
        root.to_string_lossy().as_ref(),
        "--source",
        "walker",
        "--type",
        "file",
        "--limit",
        "5",
        "--exec-max-args",
        "2",
        "-x",
    ]);
    #[cfg(windows)]
    command.args(["cmd.exe", "/D", "/C", "rem", "{}"]);
    #[cfg(unix)]
    command.args(["true", "{}"]);
    let output = command.output().expect("run real exec batches");

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Command completed: 5 paths in 3 batches"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tc_170_exec_rejects_output_and_builtin_action_options() {
    for extra in [["--absolute", ""], ["--print0", ""], ["--action", "open"]] {
        let mut command = cli_command("exec-conflict");
        command.arg("--cli");
        command.arg(extra[0]);
        if !extra[1].is_empty() {
            command.arg(extra[1]);
        }
        let output = command
            .args(["-x", "tool", "{}"])
            .output()
            .expect("run conflicting exec CLI");

        assert_eq!(output.status.code(), Some(2), "option {}", extra[0]);
        assert!(output.stdout.is_empty());
    }
}

#[cfg(any(unix, windows))]
#[test]
fn follow_links_cli_search_presets_and_filelist_creation_use_the_same_option() {
    let fixture = test_root("follow-links-contract");
    let root = fixture.join("root");
    let outside = fixture.join("outside");
    let settings = fixture.join("settings");
    fs::create_dir_all(&root).expect("root");
    fs::create_dir_all(&outside).expect("outside");
    fs::create_dir_all(&settings).expect("settings");
    fs::write(outside.join("linked.txt"), "content").expect("file");
    let link = root.join("alias");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).expect("directory symlink");
    #[cfg(windows)]
    {
        let output = Command::new("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .expect("directory junction");
        assert!(output.status.success(), "{output:?}");
    }
    for (flags, expected) in [
        (vec![], false),
        (vec!["--follow-links"], true),
        (vec!["--follow-links", "--max-depth", "1"], false),
    ] {
        let output = cli_command_in_settings(&settings)
            .args(["--cli", "--root"])
            .arg(&root)
            .args(["--source", "walker", "--type", "file"])
            .args(flags)
            .output()
            .expect("search");
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8_lossy(&output.stdout).replace('\\', "/");
        assert_eq!(stdout.contains("alias/linked.txt"), expected, "{stdout}");
    }
    let saved = cli_command_in_settings(&settings)
        .args(["--cli", "--root"])
        .arg(&root)
        .args([
            "--source",
            "walker",
            "--follow-links",
            "--save-preset",
            "links",
        ])
        .output()
        .expect("save preset");
    assert!(saved.status.success(), "{saved:?}");
    let applied = cli_command_in_settings(&settings)
        .args(["--cli", "--preset", "links"])
        .output()
        .expect("apply preset");
    assert!(applied.status.success(), "{applied:?}");
    assert!(String::from_utf8_lossy(&applied.stdout).contains("linked.txt"));
    let conflict = cli_command_in_settings(&settings)
        .args(["--cli", "--preset", "links", "--follow-links"])
        .output()
        .expect("reject preset override");
    assert_eq!(conflict.status.code(), Some(2));
    let created = cli_command_in_settings(&settings)
        .args(["--cli", "--root"])
        .arg(&root)
        .args(["--create-filelist", "--follow-links"])
        .output()
        .expect("create FileList");
    assert!(created.status.success(), "{created:?}");
    let list = fs::read_to_string(root.join("FileList.txt")).expect("FileList");
    assert!(
        list.replace('\\', "/").contains("alias/linked.txt"),
        "{list}"
    );
    // Explicit FileList records are candidates regardless of traversal mode.
    let filelist = cli_command_in_settings(&settings)
        .args(["--cli", "--root"])
        .arg(&root)
        .args(["--source", "filelist"])
        .output()
        .expect("search FileList");
    assert!(filelist.status.success(), "{filelist:?}");
    assert!(String::from_utf8_lossy(&filelist.stdout).contains("linked.txt"));
    let _ = fs::remove_dir_all(fixture);
}
