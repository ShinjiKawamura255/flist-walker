use super::*;
use crate::fs_atomic::acquire_sidecar_lock;
use serde_json::json;
use std::env;
use std::fs;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
fn ui_state_file_path_in_joins_base_directory() {
    let base = PathBuf::from("/tmp/flistwalker-settings");
    assert_eq!(
        FlistWalkerApp::ui_state_file_path_in(&base),
        base.join(".flistwalker_ui_state.json")
    );
}

#[test]
fn saved_roots_file_path_in_joins_base_directory() {
    let base = PathBuf::from("/tmp/flistwalker-settings");
    assert_eq!(
        FlistWalkerApp::saved_roots_file_path_in(&base),
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
    let current_path = FlistWalkerApp::ui_state_file_path_in(&current_base);
    let legacy_path = FlistWalkerApp::ui_state_file_path_in(&legacy_base);
    fs::write(&legacy_path, "{\"ignore_list_enabled\":false}").expect("write legacy");

    let resolved =
        FlistWalkerApp::migrate_or_legacy_path(&current_path, std::slice::from_ref(&legacy_path));
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
    let current_path = FlistWalkerApp::saved_roots_file_path_in(&current_base);
    let legacy_path = FlistWalkerApp::saved_roots_file_path_in(&legacy_base);
    fs::write(&legacy_path, "legacy-root").expect("write legacy");
    fs::write(&current_path, "current-root").expect("write current");

    let resolved = FlistWalkerApp::migrate_or_legacy_saved_roots_path(&current_path);
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
    let current_path = FlistWalkerApp::ui_state_file_path_in(&current_base);
    let missing_legacy_path = FlistWalkerApp::ui_state_file_path_in(&missing_legacy_base);
    let legacy_path = FlistWalkerApp::ui_state_file_path_in(&legacy_base);
    fs::write(&legacy_path, "{\"ignore_list_enabled\":false}").expect("write legacy");

    let resolved = FlistWalkerApp::migrate_or_legacy_path(
        &current_path,
        &[missing_legacy_path, legacy_path.clone()],
    );
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
    let helper = "app::session::tests::tc_167_child_process_history_writer_helper";

    let parent_writer = AsyncHistoryPersistence::new(path.clone(), false);
    parent_writer
        .enqueue_history(vec!["A".into()])
        .expect("parent enqueue A");
    parent_writer
        .flush(Duration::from_secs(2))
        .expect("parent flush A");

    let status = Command::new(&test_exe)
        .arg("--exact")
        .arg(helper)
        .env("FLISTWALKER_PERSISTENCE_CHILD_PATH", &path)
        .env("FLISTWALKER_PERSISTENCE_CHILD_DELTA", "B")
        .status()
        .expect("run child writer");
    assert!(status.success(), "child writer B failed");

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
