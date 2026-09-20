use super::*;
use crate::fs_atomic::acquire_sidecar_lock;
use crate::persistence::paths::{
    migrate_or_legacy_path, migrate_or_legacy_saved_roots_path, saved_roots_file_path_in,
    ui_state_file_path_in,
};
use serde_json::json;
use std::env;
use std::fs;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
fn public_persistence_round_trip_preserves_existing_document_fields() {
    let base = temp_dir("public-round-trip");
    fs::create_dir_all(&base).expect("create base");
    let state_path = base.join("ui-state.json");
    let roots_path = base.join("roots.txt");
    let last_root = base.join("last-root");
    fs::write(
        &state_path,
        json!({
            "last_root": last_root,
            "query_history": ["old"],
            "unknown_future_field": {"keep": true},
            "show_preview": false
        })
        .to_string(),
    )
    .expect("seed state");
    fs::write(&roots_path, "saved-root\n").expect("seed roots");

    let writer = crate::persistence::AsyncHistoryPersistence::new(state_path.clone(), false);
    writer.enqueue_history(vec!["new".into()]).expect("enqueue");
    writer.flush(Duration::from_secs(2)).expect("flush");
    writer.shutdown(Duration::from_secs(2)).expect("shutdown");
    let loaded = crate::persistence::load_persisted_roots_and_history_from_paths(
        &state_path,
        &roots_path,
        false,
    );
    assert_eq!(
        loaded.last_root,
        Some(normalize_windows_path_buf(last_root))
    );
    assert_eq!(loaded.saved_roots, vec![PathBuf::from("saved-root")]);
    assert_eq!(loaded.query_history, vec!["old", "new"]);
    let document: Value =
        serde_json::from_str(&fs::read_to_string(&state_path).expect("read document"))
            .expect("parse document");
    assert_eq!(document["unknown_future_field"], json!({"keep": true}));
    assert_eq!(document["show_preview"], false);
    fs::remove_dir_all(base).expect("remove fixture");
}

#[test]
fn default_persistence_paths_are_isolated_in_unit_tests() {
    assert!(ui_state_file_path().is_none());
    assert!(crate::persistence::saved_roots_file_path().is_none());
    assert!(crate::persistence::AsyncHistoryPersistence::new_default().is_none());
    assert_eq!(
        crate::persistence::load_persisted_roots_and_history(),
        crate::persistence::PersistedRootsAndHistory::default()
    );
}

#[test]
fn public_persisted_reader_preserves_full_document_validation() {
    let base = temp_dir("wire-schema-compatibility");
    fs::create_dir_all(&base).expect("create base");
    let state_path = base.join("ui-state.json");
    let roots_path = base.join("roots.txt");
    fs::write(&roots_path, "saved-root\n").expect("write roots");
    let valid = json!({
        "last_root": "last-root",
        "default_root": "default-root",
        "query_history": ["first", "second"],
        "unknown_future_field": {"keep": true}
    });
    fs::write(&state_path, valid.to_string()).expect("write valid state");
    let loaded = crate::persistence::load_persisted_roots_and_history_from_paths(
        &state_path,
        &roots_path,
        false,
    );
    assert_eq!(loaded.last_root, Some(PathBuf::from("last-root")));
    assert_eq!(loaded.default_root, Some(PathBuf::from("default-root")));
    assert_eq!(loaded.query_history, vec!["first", "second"]);
    assert_eq!(loaded.saved_roots, vec![PathBuf::from("saved-root")]);

    let mut malformed_window = valid.clone();
    malformed_window["window"] = json!({"width": 100.0});
    let mut malformed_tabs = valid;
    malformed_tabs["tabs"] = json!([{
        "root": "root", "use_filelist": false, "use_regex": false,
        "include_files": true, "include_dirs": true, "query": "",
        "tab_accent": "unknown"
    }]);
    for document in [
        malformed_window.to_string(),
        malformed_tabs.to_string(),
        "{".into(),
    ] {
        fs::write(&state_path, document).expect("write malformed state");
        let loaded = crate::persistence::load_persisted_roots_and_history_from_paths(
            &state_path,
            &roots_path,
            false,
        );
        assert_eq!(loaded.last_root, None);
        assert_eq!(loaded.default_root, None);
        assert!(loaded.query_history.is_empty());
        assert_eq!(loaded.saved_roots, vec![PathBuf::from("saved-root")]);
    }
    fs::remove_dir_all(&base).expect("remove fixture");
}

#[test]
fn ui_state_file_path_in_joins_base_directory() {
    let base = PathBuf::from("/tmp/flistwalker-settings");
    assert_eq!(
        ui_state_file_path_in(&base),
        base.join(".flistwalker_ui_state.json")
    );
}

#[test]
fn saved_roots_file_path_in_joins_base_directory() {
    let base = PathBuf::from("/tmp/flistwalker-settings");
    assert_eq!(
        saved_roots_file_path_in(&base),
        base.join(".flistwalker_roots.txt")
    );
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    env::temp_dir().join(format!("flistwalker-session-{name}-{nonce}"))
}

#[test]
fn migrate_or_legacy_ui_state_path_prefers_current_and_moves_legacy_when_missing() {
    let base = temp_dir("ui-state");
    let legacy_base = base.join("legacy");
    let current_base = base.join("current");
    fs::create_dir_all(&legacy_base).expect("create legacy");
    fs::create_dir_all(&current_base).expect("create current");
    let current_path = ui_state_file_path_in(&current_base);
    let legacy_path = ui_state_file_path_in(&legacy_base);
    fs::write(&legacy_path, "{\"ignore_list_enabled\":false}").expect("write legacy");

    let resolved = migrate_or_legacy_path(&current_path, std::slice::from_ref(&legacy_path));
    assert_eq!(resolved, current_path);
    assert!(current_path.exists());
    assert!(!legacy_path.exists());

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn migrate_or_legacy_saved_roots_path_leaves_existing_current_file_untouched() {
    let base = temp_dir("saved-roots");
    let legacy_base = base.join("legacy");
    let current_base = base.join("current");
    fs::create_dir_all(&legacy_base).expect("create legacy");
    fs::create_dir_all(&current_base).expect("create current");
    let current_path = saved_roots_file_path_in(&current_base);
    let legacy_path = saved_roots_file_path_in(&legacy_base);
    fs::write(&legacy_path, "legacy-root").expect("write legacy");
    fs::write(&current_path, "current-root").expect("write current");

    let resolved = migrate_or_legacy_saved_roots_path(&current_path);
    assert_eq!(resolved, current_path);
    assert!(current_path.exists());
    assert!(legacy_path.exists());

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn migrate_or_legacy_path_skips_missing_legacy_and_uses_next_one() {
    let base = temp_dir("migration-priority");
    let current_base = base.join("current");
    let missing_legacy_base = base.join("missing-legacy");
    let legacy_base = base.join("legacy");
    fs::create_dir_all(&current_base).expect("create current");
    fs::create_dir_all(&legacy_base).expect("create legacy");
    let current_path = ui_state_file_path_in(&current_base);
    let missing_legacy_path = ui_state_file_path_in(&missing_legacy_base);
    let legacy_path = ui_state_file_path_in(&legacy_base);
    fs::write(&legacy_path, "{\"ignore_list_enabled\":false}").expect("write legacy");

    let resolved =
        migrate_or_legacy_path(&current_path, &[missing_legacy_path, legacy_path.clone()]);
    assert_eq!(resolved, current_path);
    assert!(current_path.exists());
    assert!(!legacy_path.exists());

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_persistence_merges_two_writers_and_preserves_unknown_json_fields() {
    let base = temp_dir("two-writers");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    fs::write(
        &path,
        json!({
            "unknown_top": {"keep": true},
            "window": {"x": 1.0, "unknown_nested": "keep"},
            "query_history": []
        })
        .to_string(),
    )
    .expect("seed state");
    let writer_a = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(50),
    );
    let writer_b = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(50),
    );

    writer_a.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"window": {"width": 800.0}})),
        vec!["alpha".into()],
    );
    writer_a
        .flush(Duration::from_secs(1))
        .expect("flush writer a");
    writer_b.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"show_preview": false})),
        vec!["beta".into()],
    );
    writer_b
        .flush(Duration::from_secs(1))
        .expect("flush writer b");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["unknown_top"]["keep"], true);
    assert_eq!(written["window"]["unknown_nested"], "keep");
    assert_eq!(written["window"]["width"], 800.0);
    assert_eq!(written["query_history"], json!(["alpha", "beta"]));
    writer_a
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer a");
    writer_b
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer b");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_persistence_keeps_ordered_a_b_a_deltas_as_b_a() {
    let base = temp_dir("history-burst");
    let path = base.join("ui-state.json");
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(50),
    );

    writer.enqueue_patch_for_test(UiStatePatch::default(), vec!["A".into()]);
    writer.enqueue_patch_for_test(UiStatePatch::default(), vec!["B".into()]);
    writer.enqueue_patch_for_test(UiStatePatch::default(), vec!["A".into()]);
    writer
        .flush(Duration::from_secs(1))
        .expect("flush history burst");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["query_history"], json!(["B", "A"]));
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_persistence_deduplicates_and_caps_history_at_100() {
    let base = temp_dir("history-cap");
    let path = base.join("ui-state.json");
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(50),
    );

    writer.enqueue_patch_for_test(
        UiStatePatch::default(),
        (0..101).map(|index| format!("q-{index}")).collect(),
    );
    writer.enqueue_patch_for_test(UiStatePatch::default(), vec!["q-50".into()]);
    writer
        .flush(Duration::from_secs(1))
        .expect("flush capped history");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    let history = written["query_history"].as_array().expect("history array");
    assert_eq!(history.len(), 100);
    assert_eq!(history.first(), Some(&json!("q-1")));
    assert_eq!(history.last(), Some(&json!("q-50")));
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_persistence_coalesces_patch_leaves_last_write_wins() {
    let base = temp_dir("patch-leaves");
    let path = base.join("ui-state.json");
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(50),
    );

    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"show_preview": false, "window": {"width": 800.0}})),
        Vec::new(),
    );
    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"show_preview": true, "window": {"height": 600.0}})),
        Vec::new(),
    );
    writer
        .flush(Duration::from_secs(1))
        .expect("flush patch leaves");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["show_preview"], true);
    assert_eq!(written["window"]["width"], 800.0);
    assert_eq!(written["window"]["height"], 600.0);
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_persistence_retries_lock_timeout_without_losing_generations() {
    let base = temp_dir("retry");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    let lock = acquire_sidecar_lock(&path, Duration::from_millis(10)).expect("hold lock");
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(10),
    );

    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"first": 1})),
        vec!["A".into()],
    );
    assert!(writer.flush(Duration::from_millis(200)).is_err());
    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"second": 2})),
        vec!["B".into()],
    );
    drop(lock);
    writer
        .flush(Duration::from_secs(1))
        .expect("retry after release");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["first"], 1);
    assert_eq!(written["second"], 2);
    assert_eq!(written["query_history"], json!(["A", "B"]));
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_persistence_disabled_history_is_a_load_and_save_noop() {
    let base = temp_dir("history-disabled");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    fs::write(
        &path,
        json!({"query_history": ["old"], "unknown": true}).to_string(),
    )
    .expect("seed state");
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        true,
        Duration::from_millis(50),
    );

    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"show_preview": false})),
        vec!["new".into()],
    );
    writer
        .flush(Duration::from_secs(1))
        .expect("flush disabled history");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["query_history"], json!(["old"]));
    assert_eq!(written["unknown"], true);
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_168_persistence_enqueue_does_not_wait_for_a_held_lock() {
    let base = temp_dir("frame-latency");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    let lock = acquire_sidecar_lock(&path, Duration::from_millis(10)).expect("hold lock");
    let writer =
        AsyncHistoryPersistence::new_with_lock_timeout(path.clone(), false, Duration::from_secs(1));

    let started = Instant::now();
    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"frame": "fast"})),
        Vec::new(),
    );
    assert!(started.elapsed() < Duration::from_millis(200));
    drop(lock);
    writer
        .flush(Duration::from_secs(1))
        .expect("flush after lock release");

    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_168_detached_ui_state_writer_flushes_outside_frame_waiting_for_lock_release() {
    let base = temp_dir("detached-flush");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    let lock = acquire_sidecar_lock(&path, Duration::from_millis(10)).expect("hold lock");

    let started = Instant::now();
    enqueue_ui_state_patch(
        path.clone(),
        UiStatePatch::from_json(json!({"frame": "enqueued"})),
        Vec::new(),
        false,
    );
    assert!(started.elapsed() < Duration::from_millis(200));
    drop(lock);
    flush_ui_state_persistence(&path, Duration::from_secs(1));

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["frame"], "enqueued");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_observed_settings_commit_reports_saved_root_failure_without_changing_ui_state() {
    let base = temp_dir("observed-settings-failure");
    let ui_state_path = base.join("ui-state.json");
    let invalid_roots_target = base.join("roots-as-directory");
    fs::create_dir_all(&invalid_roots_target).expect("create invalid target directory");
    fs::write(
        &ui_state_path,
        json!({"default_root": "old-root", "unknown": true}).to_string(),
    )
    .expect("seed state");
    let before = fs::read(&ui_state_path).expect("read state before commit");

    let response = enqueue_settings_commit(
        ui_state_path.clone(),
        false,
        SettingsCommitRequest {
            request_id: 41,
            patch: UiStatePatch::from_json(json!({"default_root": "new-root"})),
            saved_roots: Some((invalid_roots_target, "new-root\n".to_string())),
        },
    )
    .expect("enqueue observed commit")
    .recv_timeout(Duration::from_secs(1))
    .expect("observed response");

    assert_eq!(response.request_id, 41);
    assert!(response.result.is_err());
    assert_eq!(
        fs::read(&ui_state_path).expect("read state after failure"),
        before
    );
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_observed_settings_commit_rolls_back_saved_roots_when_ui_state_write_fails() {
    let base = temp_dir("observed-settings-rollback");
    let ui_state_path = base.join("ui-state.json");
    let roots_path = base.join("roots.txt");
    fs::create_dir_all(&base).expect("create base");
    fs::write(&ui_state_path, "{\"unknown\":true}").expect("seed UI state");
    fs::write(&roots_path, "old-root\n").expect("seed roots");
    let mut writes = Vec::new();

    let result = commit_settings_with_writer(
        &ui_state_path,
        &[],
        false,
        Duration::from_secs(1),
        SettingsCommitRequest {
            request_id: 42,
            patch: UiStatePatch::from_json(json!({"default_root": "new-root"})),
            saved_roots: Some((roots_path.clone(), "new-root\n".to_string())),
        },
        |path, bytes| {
            writes.push(path.to_path_buf());
            if path == ui_state_path {
                assert_eq!(fs::read_to_string(&roots_path).unwrap(), "new-root\n");
                Err(std::io::Error::other("injected UI-state write failure"))
            } else {
                write_bytes_atomic(path, bytes)
            }
        },
    );

    assert!(result
        .err()
        .unwrap()
        .to_string()
        .contains("injected UI-state write failure"));
    assert_eq!(
        writes,
        vec![
            roots_path.clone(),
            ui_state_path.clone(),
            roots_path.clone()
        ]
    );
    assert_eq!(
        fs::read_to_string(&ui_state_path).unwrap(),
        "{\"unknown\":true}"
    );
    assert_eq!(
        fs::read_to_string(&roots_path).expect("read rolled-back roots"),
        "old-root\n"
    );
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_unreadable_existing_document_is_not_an_empty_merge_base() {
    let base = temp_dir("unreadable-merge-base");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&path).unwrap();
    let error = build_ui_state_document(&path, &[], None, false)
        .expect_err("existing unreadable targets must fail before preparing a replacement");
    assert_ne!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(path.is_dir());
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn tc_167_invalid_document_blocks_settings_commit_before_any_write() {
    for bytes in [b"{\"unknown\":".as_slice(), b"", b"null", b"[]", b"\xff"] {
        let base = temp_dir("invalid-settings-document");
        fs::create_dir_all(&base).unwrap();
        let path = base.join("ui-state.json");
        let roots = base.join("roots.txt");
        fs::write(&path, bytes).unwrap();
        fs::write(&roots, "old-root\n").unwrap();
        let mut writes = 0;
        let result = commit_settings_with_writer(
            &path,
            &[],
            false,
            Duration::from_secs(1),
            SettingsCommitRequest {
                request_id: 45,
                patch: UiStatePatch::from_json(json!({"show_preview": false})),
                saved_roots: Some((roots.clone(), "new-root\n".into())),
            },
            |target, bytes| {
                writes += 1;
                write_bytes_atomic(target, bytes)
            },
        );
        assert!(
            result.is_err(),
            "invalid existing bytes must be preserved: {bytes:?}"
        );
        assert_eq!(writes, 0);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read_to_string(&roots).unwrap(), "old-root\n");
        fs::remove_dir_all(base).unwrap();
    }
}

#[test]
fn tc_167_invalid_document_retry_preserves_history_until_external_repair() {
    let base = temp_dir("invalid-history-document");
    fs::create_dir_all(&base).unwrap();
    let path = base.join("ui-state.json");
    let original = b"{\"query_history\":[\"old\"],\"unknown\":";
    fs::write(&path, original).unwrap();
    let writer = AsyncHistoryPersistence::new(path.clone(), false);
    writer.enqueue_history(vec![" A ".into()]).unwrap();
    let error = writer
        .flush(Duration::from_secs(2))
        .expect_err("invalid JSON must fail");
    assert!(error.contains("UI-state"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), original);
    writer
        .enqueue_history(vec!["B".into(), "A".into()])
        .unwrap();
    {
        let _lock = acquire_sidecar_lock(&path, Duration::from_secs(2)).unwrap();
        fs::write(
            &path,
            r#"{"query_history":["old"],"unknown":{"keep":true}}"#,
        )
        .unwrap();
    }
    writer.flush(Duration::from_secs(2)).unwrap();
    writer.shutdown(Duration::from_secs(2)).unwrap();
    let document: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(document["query_history"], json!(["old", "B", "A"]));
    assert_eq!(document["unknown"]["keep"], true);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn tc_167_settings_rollback_failures_preserve_original_and_both_restore_errors() {
    let base = temp_dir("settings-rollback-errors");
    fs::create_dir_all(&base).unwrap();
    let path = base.join("ui-state.json");
    let roots = base.join("roots.txt");
    fs::write(&path, "{\"show_preview\":true}").unwrap();
    fs::write(&roots, "old-root\n").unwrap();
    let mut writes = Vec::new();
    let result = commit_settings_with_writer(
        &path,
        &[],
        false,
        Duration::from_secs(1),
        SettingsCommitRequest {
            request_id: 46,
            patch: UiStatePatch::from_json(json!({"show_preview": false})),
            saved_roots: Some((roots.clone(), "new-root\n".into())),
        },
        |target, bytes| {
            writes.push(target.to_path_buf());
            match writes.len() {
                1 => write_bytes_atomic(target, bytes),
                2 => crate::fs_atomic::write_bytes_atomic_with_sync_for_test(target, bytes, |_| {
                    Err(std::io::Error::other("injected durability failure"))
                }),
                3 => Err(std::io::Error::other("injected UI restore failure")),
                4 => Err(std::io::Error::other("injected roots restore failure")),
                _ => panic!("unexpected write"),
            }
        },
    );
    let error = result
        .err()
        .expect("rollback failures must not report success")
        .to_string();
    for message in [
        "injected durability failure",
        "UI-state rollback failed",
        "injected UI restore failure",
        "saved-roots rollback failed",
        "injected roots restore failure",
    ] {
        assert!(error.contains(message), "missing {message}: {error}");
    }
    assert_eq!(
        writes,
        vec![roots.clone(), path.clone(), path.clone(), roots.clone()]
    );
    let document: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(document["show_preview"], false);
    assert_eq!(fs::read_to_string(&roots).unwrap(), "new-root\n");
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn tc_167_post_replace_sync_failure_rolls_back_saved_roots() {
    let base = temp_dir("observed-settings-post-replace-roots");
    let ui_state_path = base.join("ui-state.json");
    let roots_path = base.join("roots.txt");
    fs::create_dir_all(&base).expect("create base");
    fs::write(
        &ui_state_path,
        json!({"default_root": "old-root"}).to_string(),
    )
    .expect("seed UI state");
    fs::write(&roots_path, "old-root\n").expect("seed roots");
    let before_ui = fs::read(&ui_state_path).expect("read prior UI state");
    let calls = std::sync::atomic::AtomicUsize::new(0);

    let result = commit_settings_with_writer(
        &ui_state_path,
        &[],
        false,
        Duration::from_secs(1),
        SettingsCommitRequest {
            request_id: 43,
            patch: UiStatePatch::from_json(json!({"default_root": "new-root"})),
            saved_roots: Some((roots_path.clone(), "new-root\n".to_string())),
        },
        |path, bytes| {
            if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                crate::fs_atomic::write_bytes_atomic_with_sync_for_test(path, bytes, |_| {
                    Err(std::io::Error::other("injected directory sync failure"))
                })
            } else {
                crate::fs_atomic::write_bytes_atomic(path, bytes)
            }
        },
    );

    let error = match result {
        Ok(_) => panic!("post-replace sync failure must be reported"),
        Err(error) => error,
    };
    assert!(error
        .to_string()
        .contains("destination was replaced but durability sync failed"));
    assert_eq!(fs::read(&ui_state_path).expect("read UI state"), before_ui);
    assert_eq!(
        fs::read_to_string(&roots_path).expect("read restored roots"),
        "old-root\n"
    );
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_post_replace_ui_state_sync_failure_rolls_back_both_files() {
    let base = temp_dir("observed-settings-post-replace-ui");
    let ui_state_path = base.join("ui-state.json");
    let roots_path = base.join("roots.txt");
    fs::create_dir_all(&base).expect("create base");
    fs::write(
        &ui_state_path,
        json!({"default_root": "old-root"}).to_string(),
    )
    .expect("seed UI state");
    fs::write(&roots_path, "old-root\n").expect("seed roots");
    let before_ui = fs::read(&ui_state_path).expect("read prior UI state");
    let calls = std::sync::atomic::AtomicUsize::new(0);

    let result = commit_settings_with_writer(
        &ui_state_path,
        &[],
        false,
        Duration::from_secs(1),
        SettingsCommitRequest {
            request_id: 44,
            patch: UiStatePatch::from_json(json!({"default_root": "new-root"})),
            saved_roots: Some((roots_path.clone(), "new-root\n".to_string())),
        },
        |path, bytes| {
            if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1 {
                crate::fs_atomic::write_bytes_atomic_with_sync_for_test(path, bytes, |_| {
                    Err(std::io::Error::other("injected directory sync failure"))
                })
            } else {
                crate::fs_atomic::write_bytes_atomic(path, bytes)
            }
        },
    );

    let error = match result {
        Ok(_) => panic!("post-replace sync failure must be reported"),
        Err(error) => error,
    };
    assert!(error
        .to_string()
        .contains("destination was replaced but durability sync failed"));
    assert_eq!(
        fs::read(&ui_state_path).expect("read restored UI state"),
        before_ui
    );
    assert_eq!(
        fs::read_to_string(&roots_path).expect("read restored roots"),
        "old-root\n"
    );
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_168_observed_settings_commit_enqueue_does_not_wait_for_ui_state_lock() {
    let base = temp_dir("observed-settings-frame-latency");
    let ui_state_path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    let lock = acquire_sidecar_lock(&ui_state_path, Duration::from_millis(10)).expect("hold lock");

    let started = Instant::now();
    let response = enqueue_settings_commit(
        ui_state_path.clone(),
        false,
        SettingsCommitRequest {
            request_id: 42,
            patch: UiStatePatch::from_json(json!({"default_root": base})),
            saved_roots: None,
        },
    )
    .expect("enqueue observed commit");
    assert!(started.elapsed() < Duration::from_millis(200));

    drop(lock);
    let response = response
        .recv_timeout(Duration::from_secs(2))
        .expect("observed response");
    assert_eq!(response.request_id, 42);
    response.result.expect("commit after lock release");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn public_persisted_roots_and_history_api_honors_history_disabled() {
    let base = temp_dir("public-read-api");
    let ui_state_path = base.join("ui-state.json");
    let saved_roots_path = base.join("roots.txt");
    fs::create_dir_all(&base).expect("create base");
    fs::write(
        &ui_state_path,
        json!({
            "last_root": "C:/last",
            "default_root": "C:/default",
            "query_history": ["one", "two"]
        })
        .to_string(),
    )
    .expect("write ui state");
    fs::write(&saved_roots_path, "C:/saved\nC:/saved\nC:/other\n").expect("write roots");

    let enabled = crate::persistence::load_persisted_roots_and_history_from_paths(
        &ui_state_path,
        &saved_roots_path,
        false,
    );
    assert_eq!(enabled.query_history, vec!["one", "two"]);
    assert_eq!(enabled.saved_roots.len(), 2);
    assert_eq!(
        enabled.default_root,
        Some(normalize_windows_path_buf(PathBuf::from("C:/default")))
    );

    let disabled = crate::persistence::load_persisted_roots_and_history_from_paths(
        &ui_state_path,
        &saved_roots_path,
        true,
    );
    assert!(disabled.query_history.is_empty());
    assert_eq!(disabled.saved_roots, enabled.saved_roots);
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_latest_json_history_is_globally_normalized_before_unrelated_patch() {
    let base = temp_dir("latest-history-normalization");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    fs::write(
        &path,
        json!({"query_history": [" first ", "second", "first", "", "third"]}).to_string(),
    )
    .expect("seed history");
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(50),
    );

    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"show_preview": false})),
        Vec::new(),
    );
    writer.flush(Duration::from_secs(1)).expect("flush history");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(
        written["query_history"],
        json!(["second", "first", "third"])
    );
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn public_async_history_enqueue_is_a_noop_when_history_is_disabled() {
    let base = temp_dir("public-async-history-disabled");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    fs::write(&path, json!({"query_history": ["existing"]}).to_string()).expect("seed history");
    let writer = crate::persistence::AsyncHistoryPersistence::new(path.clone(), true);

    writer
        .enqueue_history(vec!["ignored".into()])
        .expect("disabled enqueue");
    writer
        .flush(Duration::from_secs(1))
        .expect("disabled flush");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["query_history"], json!(["existing"]));
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_seeded_gui_history_does_not_replay_over_latest_external_history() {
    let base = temp_dir("seeded-history");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    fs::write(&path, json!({"query_history": ["local"]}).to_string()).expect("seed local");
    seed_persisted_history_snapshot(path.clone(), &["local".to_string()]);
    fs::write(
        &path,
        json!({"query_history": ["local", "external"]}).to_string(),
    )
    .expect("simulate external write");

    enqueue_ui_state_patch(
        path.clone(),
        UiStatePatch::from_json(json!({"show_preview": false})),
        vec!["local".into()],
        false,
    );
    flush_ui_state_persistence(&path, Duration::from_secs(1));

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["query_history"], json!(["local", "external"]));
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_generation_arriving_during_blocked_commit_is_flushed_next_without_loss() {
    let base = temp_dir("generation-during-commit");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    let lock = acquire_sidecar_lock(&path, Duration::from_millis(10)).expect("hold lock");
    let writer = AsyncHistoryPersistence::new_with_lock_timeout(
        path.clone(),
        false,
        Duration::from_millis(80),
    );

    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"first": 1})),
        vec!["A".into()],
    );
    std::thread::sleep(Duration::from_millis(15));
    writer.enqueue_patch_for_test(
        UiStatePatch::from_json(json!({"second": 2})),
        vec!["B".into()],
    );
    drop(lock);
    writer
        .flush(Duration::from_secs(1))
        .expect("flush generations");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["first"], 1);
    assert_eq!(written["second"], 2);
    assert_eq!(written["query_history"], json!(["A", "B"]));
    writer
        .shutdown(Duration::from_secs(1))
        .expect("shutdown writer");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn tc_167_child_process_history_writer_helper() {
    let Ok(path) = env::var("FLISTWALKER_PERSISTENCE_CHILD_PATH") else {
        return;
    };
    let delta = env::var("FLISTWALKER_PERSISTENCE_CHILD_DELTA").expect("child delta");
    let writer = AsyncHistoryPersistence::new(PathBuf::from(path), false);
    writer.enqueue_history(vec![delta]).expect("child enqueue");
    writer.flush(Duration::from_secs(2)).expect("child flush");
    writer
        .shutdown(Duration::from_secs(2))
        .expect("child shutdown");
}

#[test]
fn tc_167_two_process_writers_preserve_alternating_history() {
    let base = temp_dir("two-process-writers");
    let path = base.join("ui-state.json");
    fs::create_dir_all(&base).expect("create base");
    let test_exe = env::current_exe().expect("current test executable");
    let helper = concat!(
        module_path!(),
        "::tc_167_child_process_history_writer_helper"
    );
    let helper = helper
        .strip_prefix("flist_walker::")
        .expect("crate-qualified helper");

    let parent_writer = AsyncHistoryPersistence::new(path.clone(), false);
    parent_writer
        .enqueue_history(vec!["A".into()])
        .expect("parent enqueue A");
    parent_writer
        .flush(Duration::from_secs(2))
        .expect("parent flush A");

    let output = Command::new(&test_exe)
        .arg("--exact")
        .arg(helper)
        .env("FLISTWALKER_PERSISTENCE_CHILD_PATH", &path)
        .env("FLISTWALKER_PERSISTENCE_CHILD_DELTA", "B")
        .output()
        .expect("run child writer");
    assert!(output.status.success(), "child writer B failed: {output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "expected exactly one child helper test: {output:?}"
    );

    parent_writer
        .enqueue_history(vec!["A".into()])
        .expect("parent enqueue final A");
    parent_writer
        .flush(Duration::from_secs(2))
        .expect("parent flush final A");
    parent_writer
        .shutdown(Duration::from_secs(2))
        .expect("parent shutdown");

    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read state")).expect("parse state");
    assert_eq!(written["query_history"], json!(["B", "A"]));
    let _ = fs::remove_dir_all(&base);
}
