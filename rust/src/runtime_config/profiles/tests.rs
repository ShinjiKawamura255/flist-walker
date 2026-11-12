use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("flistwalker-config-profiles-{name}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn cli(&self) -> PathBuf {
        self.0.join(CLI_RUNTIME_CONFIG_FILE_NAME)
    }
    fn gui(&self) -> PathBuf {
        runtime_config_file_path_in(&self.0)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cli_copy_preserves_source_bytes_unknown_keys_and_developer_settings() {
    let fixture = Fixture::new("copy");
    let original = r#"{"walker_max_entries":42,"future":{"keep":true},"developer":{"adaptive_walk_initial_limit":2}}"#;
    fs::write(fixture.gui(), original).unwrap();
    let config = load_cli_config_locked(&fixture.cli());
    assert_eq!(config.walker_max_entries, 42);
    assert_eq!(fs::read_to_string(fixture.gui()).unwrap(), original);
    let copy: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.cli()).unwrap()).unwrap();
    assert_eq!(copy["future"]["keep"], true);
    assert_eq!(copy["developer"]["adaptive_walk_initial_limit"], 2);
    assert!(copy["emacs_keybindings_enabled"].is_boolean());
}

#[test]
fn existing_cli_config_wins_and_does_not_change_gui() {
    let fixture = Fixture::new("priority");
    fs::write(fixture.gui(), r#"{"walker_max_entries":42}"#).unwrap();
    fs::write(fixture.cli(), r#"{"walker_max_entries":84}"#).unwrap();
    assert_eq!(
        load_cli_config_locked(&fixture.cli()).walker_max_entries,
        84
    );
    assert_eq!(
        fs::read_to_string(fixture.gui()).unwrap(),
        r#"{"walker_max_entries":42}"#
    );
}

#[test]
fn invalid_cli_config_is_preserved_without_inheriting_gui() {
    let fixture = Fixture::new("invalid-cli");
    fs::write(fixture.gui(), r#"{"walker_max_entries":42}"#).unwrap();
    fs::write(fixture.cli(), "broken CLI JSON").unwrap();
    let _ = load_cli_config_locked(&fixture.cli());
    assert_eq!(
        fs::read_to_string(fixture.cli()).unwrap(),
        "broken CLI JSON"
    );
    assert_eq!(
        fs::read_to_string(fixture.gui()).unwrap(),
        r#"{"walker_max_entries":42}"#
    );
}

#[test]
fn invalid_gui_config_is_preserved_without_creating_cli() {
    let fixture = Fixture::new("invalid-gui");
    for original in ["broken GUI JSON", r#"{"walker_max_entries":"wrong type"}"#] {
        fs::write(fixture.gui(), original).unwrap();
        let _ = load_cli_config_locked(&fixture.cli());
        assert!(!fixture.cli().exists());
        assert_eq!(fs::read_to_string(fixture.gui()).unwrap(), original);
    }
}

#[test]
fn legacy_source_is_copied_not_moved_and_new_location_has_priority() {
    let fixture = Fixture::new("legacy");
    let legacy = fixture.0.join("old-config.json");
    fs::write(&legacy, r#"{"walker_max_entries":42}"#).unwrap();
    let sources = [fixture.gui(), legacy.clone()];
    let loaded = copy_first_config_locked(&fixture.cli(), &sources)
        .unwrap()
        .unwrap();
    assert_eq!(loaded.walker_max_entries, 42);
    assert_eq!(
        fs::read_to_string(&legacy).unwrap(),
        r#"{"walker_max_entries":42}"#
    );
    let other_cli = fixture.0.join("other-cli.json");
    fs::write(fixture.gui(), r#"{"walker_max_entries":84}"#).unwrap();
    let loaded = copy_first_config_locked(&other_cli, &sources)
        .unwrap()
        .unwrap();
    assert_eq!(loaded.walker_max_entries, 84);
}

#[test]
fn invalid_new_location_does_not_silently_use_stale_legacy_settings() {
    let fixture = Fixture::new("invalid-priority");
    let legacy = fixture.0.join("old-config.json");
    fs::write(fixture.gui(), "broken").unwrap();
    fs::write(&legacy, r#"{"walker_max_entries":42}"#).unwrap();
    assert!(copy_first_config_locked(&fixture.cli(), &[fixture.gui(), legacy]).is_err());
    assert!(!fixture.cli().exists());
}

#[test]
fn missing_sources_do_not_create_or_modify_gui_config() {
    let fixture = Fixture::new("missing");
    assert!(copy_first_config_locked(&fixture.cli(), &[fixture.gui()])
        .unwrap()
        .is_none());
    let _ = seed_runtime_config_locked(&fixture.cli());
    assert!(fixture.cli().is_file());
    assert!(!fixture.gui().exists());
}

#[test]
fn failed_cli_copy_preserves_original_gui_config() {
    let fixture = Fixture::new("failed-copy");
    let original = r#"{"walker_max_entries":42}"#;
    fs::write(fixture.gui(), original).unwrap();
    fs::create_dir(fixture.cli()).unwrap();
    assert!(copy_first_config_locked(&fixture.cli(), &[fixture.gui()]).is_err());
    assert_eq!(fs::read_to_string(fixture.gui()).unwrap(), original);
}

#[test]
fn concurrent_cli_bootstraps_recheck_winner_after_lock() {
    let fixture = Fixture::new("concurrent");
    fs::write(fixture.gui(), r#"{"walker_max_entries":42}"#).unwrap();
    let lock = acquire_sidecar_lock(&fixture.cli(), Duration::from_secs(5)).unwrap();
    let path = fixture.cli();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let _lock = acquire_sidecar_lock(&path, Duration::from_secs(5)).unwrap();
        load_cli_config_locked(&path)
    });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    fs::write(fixture.cli(), r#"{"walker_max_entries":84}"#).unwrap();
    drop(lock);
    assert_eq!(worker.join().unwrap().walker_max_entries, 84);
    assert_eq!(
        fs::read_to_string(fixture.gui()).unwrap(),
        r#"{"walker_max_entries":42}"#
    );
}

#[test]
fn gui_and_cli_settings_remain_independently_editable() {
    let fixture = Fixture::new("independent-edits");
    fs::write(fixture.gui(), r#"{"walker_max_entries":42}"#).unwrap();
    let _ = load_cli_config_locked(&fixture.cli());
    let cli_before = fs::read(fixture.cli()).unwrap();
    let gui = RuntimeConfig {
        walker_max_entries: 84,
        ..RuntimeConfig::default()
    };
    gui.save_to_path(&fixture.gui()).unwrap();
    assert_eq!(fs::read(fixture.cli()).unwrap(), cli_before);
    assert_eq!(
        load_cli_config_locked(&fixture.cli()).walker_max_entries,
        42
    );
    let cli = RuntimeConfig {
        walker_max_entries: 126,
        ..RuntimeConfig::default()
    };
    cli.save_to_path(&fixture.cli()).unwrap();
    assert_eq!(
        load_migrate_or_seed_runtime_config_locked(&fixture.gui()).walker_max_entries,
        84
    );
    assert_eq!(
        load_cli_config_locked(&fixture.cli()).walker_max_entries,
        126
    );
}

#[test]
fn cli_copy_waits_for_gui_writer_and_reads_its_committed_config() {
    let fixture = Fixture::new("gui-lock");
    fs::write(fixture.gui(), r#"{"walker_max_entries":42}"#).unwrap();
    let gui_lock = acquire_sidecar_lock(&fixture.gui(), Duration::from_secs(5)).unwrap();
    let path = fixture.cli();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _lock = acquire_sidecar_lock(&path, Duration::from_secs(5)).unwrap();
        started_tx.send(()).unwrap();
        load_cli_config_locked(&path)
    });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    fs::write(fixture.gui(), r#"{"walker_max_entries":84}"#).unwrap();
    drop(gui_lock);
    assert_eq!(worker.join().unwrap().walker_max_entries, 84);
    assert_eq!(
        fs::read_to_string(fixture.gui()).unwrap(),
        r#"{"walker_max_entries":84}"#
    );
}
