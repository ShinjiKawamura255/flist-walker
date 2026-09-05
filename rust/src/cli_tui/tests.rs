use super::*;
use crate::actions::{
    execute_authorized_action_request, AuthorizedActionBackend, AuthorizedActionGuard,
    AuthorizedActionMode, AuthorizedActionRequest,
};
use crate::indexer::{
    execute_filelist_write_plan, plan_filelist_write_cancellable, FileListWriteOptions,
    FileListWriteStatus,
};
use crate::search::SearchPrefixCache;
use crate::ui_model::build_preview_text_with_kind;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::thread;
use unicode_width::UnicodeWidthChar;

fn settle_filelist_discovery_for_test(
    state: &mut TuiState,
    request_id: u64,
    root: &Path,
    settlement: FileListDiscoverySettlement,
) -> Option<TuiExit> {
    let (index_tx, _index_rx) = mpsc::channel();
    settle_filelist_discovery(
        state,
        request_id,
        root,
        settlement,
        &index_tx,
        &TuiIndexFreshness::new(),
        &TuiActionFreshness::new(),
    )
}

#[test]
fn tc_169_tui_update_notice_is_english_and_manual_only() {
    assert_eq!(
        format_tui_update_notice("0.20.0"),
        "Update available: v0.20.0 — Run flistwalker --update after exiting"
    );
}

#[test]
fn tc_177_regression_tui_path_rendering_never_uses_raw_os_strings() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut production_sources = vec![source_root.join("cli_tui.rs")];
    production_sources.extend(
        fs::read_dir(source_root.join("cli_tui"))
            .expect("read cli_tui source directory")
            .map(|entry| entry.expect("read cli_tui source entry").path())
            .filter(|path| {
                path.extension().is_some_and(|extension| extension == "rs")
                    && path.file_name().is_some_and(|name| name != "tests.rs")
            }),
    );

    for path in production_sources {
        let source = fs::read_to_string(&path).expect("read production TUI source");
        assert!(
            !source.contains(".display()") && !source.contains("to_string_lossy()"),
            "{} bypasses the shared TUI path display boundary",
            path.display()
        );
    }
}

#[test]
#[cfg(target_os = "windows")]
fn tc_177_regression_tui_root_surfaces_strip_drive_and_unc_extended_prefixes() {
    let drive_root = PathBuf::from(r"\\?\D:\work\flistwalker");
    let unc_root = PathBuf::from(r"\\?\UNC\server\share\project");
    let freshness = TuiActionFreshness::new();
    let mut state = TuiState::new("");
    state.root = drive_root.clone();

    assert_eq!(
        missing_required_filelist_message(&drive_root),
        r"FileList was required but none was found in D:\work\flistwalker"
    );
    assert!(state
        .current_options_summary()
        .contains(r"Root: D:\work\flistwalker"));
    state.prepare_refresh();
    assert_eq!(state.status, r"Refreshing D:\work\flistwalker...");
    state.prepare_root_switch(&freshness, unc_root.clone());
    assert_eq!(state.status, r"Switching root to \\server\share\project...");

    let mut output = Vec::new();
    render_root_picker(
        &mut output,
        &RootPicker { selected: 1 },
        &[drive_root, unc_root],
        true,
        120,
        8,
    )
    .expect("render roots");
    let rendered = String::from_utf8_lossy(&output);
    assert!(rendered.contains(r"  D:\work\flistwalker"));
    assert!(rendered.contains(r"> \\server\share\project"));
    assert!(!rendered.contains(r"\\?\"));
}
use crate::runtime_config::{DeveloperRuntimeConfig, RuntimeConfig};
use std::cell::RefCell;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

struct TestTempDir {
    path: PathBuf,
}

impl TestTempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "flistwalker-cli-tui-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::create_dir_all(&path).expect("create temporary test directory");
        Self { path }
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[derive(Clone)]
struct FakeTerminalOps {
    calls: Rc<RefCell<Vec<&'static str>>>,
    fail_on: Option<&'static str>,
}

impl FakeTerminalOps {
    fn call(&self, name: &'static str) -> io::Result<()> {
        self.calls.borrow_mut().push(name);
        if self.fail_on == Some(name) {
            Err(io::Error::other(format!("failed at {name}")))
        } else {
            Ok(())
        }
    }
}

impl TerminalOps for FakeTerminalOps {
    fn enable_raw_mode(&mut self) -> io::Result<()> {
        self.call("enable_raw")
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        self.call("disable_raw")
    }

    fn enter_alternate<W: Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        self.call("enter_alternate")
    }

    fn leave_alternate<W: Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        self.call("leave_alternate")
    }

    fn hide_cursor<W: Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        self.call("hide_cursor")
    }

    fn show_cursor<W: Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        self.call("show_cursor")
    }

    fn enable_paste<W: Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        self.call("enable_paste")
    }

    fn disable_paste<W: Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        self.call("disable_paste")
    }
}

#[test]
fn tc_006_interactive_query_editing_marks_search_dirty() {
    let mut state = TuiState::new("");
    state.dirty = false;

    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)
        ),
        KeyAction::Continue
    ));
    assert_eq!(state.query, "a");
    assert!(state.last_query_change.is_some());
    assert!(state.dirty);
}

#[test]
fn tc_006_interactive_enter_returns_selected_path() {
    let mut state = TuiState::new("");
    state.results = Arc::new(vec![(PathBuf::from("selected.txt"), 1.0)]);

    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        ),
        KeyAction::Select
    ));
    assert_eq!(selected_paths(&state), vec![PathBuf::from("selected.txt")]);
}

#[test]
fn tc_006_escape_cancels_without_selecting() {
    let mut state = TuiState::new("");
    assert!(matches!(
        handle_key(&mut state, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        KeyAction::Cancel
    ));
}

#[test]
fn tc_006_tab_toggles_multiple_pins() {
    let mut state = TuiState::new("");
    state.results = Arc::new(vec![
        (PathBuf::from("one.txt"), 1.0),
        (PathBuf::from("two.txt"), 1.0),
    ]);
    assert!(matches!(
        handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        KeyAction::Continue
    ));
    state.selected = 1;
    handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(
        selected_paths(&state),
        vec![PathBuf::from("one.txt"), PathBuf::from("two.txt")]
    );
}

#[test]
fn tc_162_tui_emacs_navigation_pin_and_select_follow_runtime_toggle() {
    let mut enabled = TuiState::new("");
    enabled.results = Arc::new(
        (0..8)
            .map(|index| (PathBuf::from(format!("{index}.txt")), 1.0))
            .collect(),
    );
    enabled.viewport_rows = 3;

    handle_key(
        &mut enabled,
        KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
    );
    assert_eq!(enabled.selected, 1);
    handle_key(
        &mut enabled,
        KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
    );
    assert_eq!(enabled.selected, 0);
    handle_key(
        &mut enabled,
        KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL),
    );
    assert_eq!(enabled.selected, 3);
    handle_key(
        &mut enabled,
        KeyEvent::new(KeyCode::Char('v'), KeyModifiers::ALT),
    );
    assert_eq!(enabled.selected, 0);
    handle_key(
        &mut enabled,
        KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL),
    );
    assert_eq!(enabled.pinned, vec![PathBuf::from("0.txt")]);
    assert!(matches!(
        handle_key(
            &mut enabled,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
        ),
        KeyAction::Select
    ));

    let mut disabled = TuiState::new("");
    disabled.emacs_keybindings_enabled = false;
    disabled.results = enabled.results.clone();
    handle_key(
        &mut disabled,
        KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
    );
    handle_key(
        &mut disabled,
        KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL),
    );
    assert_eq!(disabled.selected, 0);
    assert!(disabled.pinned.is_empty());
    assert!(matches!(
        handle_key(
            &mut disabled,
            KeyEvent::new(KeyCode::Char('m'), KeyModifiers::CONTROL),
        ),
        KeyAction::Continue
    ));
    disabled.query = "keep".to_string();
    disabled.query_cursor = disabled.query.chars().count();
    disabled.history_enabled = true;
    handle_key(
        &mut disabled,
        KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
    );
    handle_key(
        &mut disabled,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );
    assert_eq!(disabled.query, "keep");
    assert!(disabled.history.is_none());
    handle_key(
        &mut disabled,
        KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
    );
    handle_key(
        &mut disabled,
        KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
    );
    assert_eq!(disabled.selected, 1);
    assert_eq!(disabled.pinned, vec![PathBuf::from("1.txt")]);
}

#[test]
fn tc_162_tui_tab_pin_move_setting_applies_to_tab_backtab_and_ctrl_i() {
    let mut state = TuiState::new("");
    state.tab_pin_moves_to_next_row = true;
    state.results = Arc::new(
        (0..3)
            .map(|index| (PathBuf::from(format!("{index}.txt")), 1.0))
            .collect(),
    );

    handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(state.pinned, vec![PathBuf::from("0.txt")]);
    assert_eq!(state.selected, 1);

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
    );
    assert_eq!(
        state.pinned,
        vec![PathBuf::from("0.txt"), PathBuf::from("1.txt")]
    );
    assert_eq!(state.selected, 2);

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL),
    );
    assert_eq!(state.pinned.last(), Some(&PathBuf::from("2.txt")));
    assert_eq!(state.selected, 2);
}

#[test]
fn tc_162_tui_emacs_query_editing_uses_the_same_runtime_toggle() {
    let mut enabled = TuiState::new("alpha beta");
    enabled.query_cursor = 5;
    handle_key(
        &mut enabled,
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
    );
    assert_eq!(enabled.query, "alpha");
    handle_key(
        &mut enabled,
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
    );
    assert_eq!(enabled.query, "alpha beta");

    handle_key(
        &mut enabled,
        KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
    );
    assert_eq!(enabled.query_cursor, 0);
    handle_key(
        &mut enabled,
        KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
    );
    assert_eq!(enabled.query_cursor, enabled.query.chars().count());

    enabled.query = "alpha/beta".to_string();
    enabled.query_cursor = enabled.query.chars().count();
    handle_key(
        &mut enabled,
        KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
    );
    assert_eq!(enabled.query, "alpha/");
    assert_eq!(enabled.kill_buffer, "beta");

    let mut disabled = TuiState::new("alpha beta");
    disabled.emacs_keybindings_enabled = false;
    disabled.query_cursor = 5;
    handle_key(
        &mut disabled,
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
    );
    assert_eq!(disabled.query, "alpha beta");
    assert_eq!(disabled.query_cursor, 5);
}

#[test]
fn tc_162_result_refresh_preserves_the_selected_path() {
    let mut state = TuiState::new("");
    state.results = Arc::new(vec![
        (PathBuf::from("one.txt"), 1.0),
        (PathBuf::from("two.txt"), 0.5),
    ]);
    state.selected = 1;

    state.set_results(
        vec![
            (PathBuf::from("zero.txt"), 2.0),
            (PathBuf::from("two.txt"), 1.5),
        ],
        None,
    );

    assert_eq!(state.selected, 1);
    assert_eq!(state.results[state.selected].0, PathBuf::from("two.txt"));
}

#[test]
fn tc_162_hidden_pins_remain_part_of_the_final_selection() {
    let mut state = TuiState::new("");
    state.results = Arc::new(vec![(PathBuf::from("pinned.txt"), 1.0)]);
    handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    state.results = Arc::new(vec![(PathBuf::from("visible.txt"), 1.0)]);
    state.selected = 0;

    assert_eq!(selected_paths(&state), vec![PathBuf::from("pinned.txt")]);
}

#[test]
fn tc_162_enter_without_a_selection_does_not_exit() {
    let mut state = TuiState::new("");

    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        ),
        KeyAction::Continue
    ));
    assert_eq!(state.status, "No selection");
}

#[test]
fn tc_162_query_editor_inserts_at_the_cursor() {
    let mut state = TuiState::new("ab");

    handle_key(&mut state, KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT),
    );

    assert_eq!(state.query, "aXb");
}

#[test]
fn tc_162_stale_search_response_is_ignored_by_request_id() {
    let mut state = TuiState::new("new");
    state.root = PathBuf::from("root");
    state.active_search_request_id = Some(2);
    state.results = Arc::new(vec![(PathBuf::from("current.txt"), 1.0)]);
    let search_options = state.runtime_options.search_options(state.sort_mode);

    apply_search_response(
        &mut state,
        1,
        Path::new("root"),
        "new",
        search_options,
        vec![(PathBuf::from("stale.txt"), 2.0)],
        None,
    );
    assert_eq!(state.results[0].0, PathBuf::from("current.txt"));

    apply_search_response(
        &mut state,
        2,
        Path::new("root"),
        "new",
        search_options,
        vec![(PathBuf::from("latest.txt"), 3.0)],
        None,
    );
    assert_eq!(state.results[0].0, PathBuf::from("latest.txt"));

    state.active_search_request_id = Some(3);
    apply_search_response(
        &mut state,
        3,
        Path::new("other-root"),
        "new",
        search_options,
        vec![(PathBuf::from("wrong-root.txt"), 4.0)],
        None,
    );
    assert_eq!(state.results[0].0, PathBuf::from("latest.txt"));
}

#[test]
fn tc_162_index_failure_keeps_tui_recoverable() {
    let mut state = TuiState::new("");
    state.active_index_request = Some((1, PathBuf::from("root")));

    apply_worker_response(
        &mut state,
        WorkerResponse::IndexFailed {
            request_id: 1,
            root: PathBuf::from("root"),
            has_root_filelist: false,
            error: "broken FileList".to_string(),
        },
    )
    .expect("index failure is surfaced in status");

    assert!(state.status.contains("broken FileList"));
    assert!(state.active_index_request.is_none());
    assert!(state.root_filelist_known);
    assert!(!state.root_filelist_exists);
    assert!(matches!(
        handle_key(&mut state, KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)),
        KeyAction::OpenFileList
    ));
}

#[test]
fn tc_162_walker_failure_emits_index_failed_without_finished() {
    let missing_root = TestTempDir::new("walker-failure").path.join("missing");
    let request = IndexRequest {
        request_id: 7,
        root: missing_root,
        include_files: true,
        include_dirs: true,
        source: TuiSource::Walker,
        filelist_discovery: FileListDiscoveryOwnership::WorkerOwned,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    };
    let mut responses = Vec::new();

    process_index_request(request, &|| false, |response| responses.push(response));

    assert!(matches!(
        responses.as_slice(),
        [WorkerResponse::IndexFailed { request_id: 7, .. }]
    ));
}

#[test]
fn tc_162_stale_filelist_discovery_emits_no_response_and_latest_request_proceeds_regression() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let temp = TestTempDir::new("stale-filelist-discovery");
    fs::write(temp.path.join("neighbor.txt"), "neighbor").expect("write fixture");
    let request = |request_id| IndexRequest {
        request_id,
        root: temp.path.clone(),
        include_files: true,
        include_dirs: false,
        source: TuiSource::Auto,
        filelist_discovery: FileListDiscoveryOwnership::WorkerOwned,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    };
    let checks = AtomicUsize::new(0);
    let mut stale_responses = Vec::new();

    process_index_request(
        request(70),
        &|| checks.fetch_add(1, Ordering::SeqCst) > 1,
        |response| stale_responses.push(response),
    );

    assert!(stale_responses.is_empty());

    let mut latest_responses = Vec::new();
    process_index_request(request(71), &|| false, |response| {
        latest_responses.push(response)
    });
    assert!(latest_responses.iter().any(|response| matches!(
        response,
        WorkerResponse::IndexedFinished { request_id: 71, .. }
    )));
}

#[test]
fn tc_162_initial_filelist_discovery_is_consumed_without_rescan_regression() {
    let temp = TestTempDir::new("initial-filelist-discovery-owned");
    fs::write(temp.path.join("listed.txt"), "listed").expect("write listed fixture");
    let discovered = temp.path.join("startup-discovered.txt");
    fs::write(&discovered, "listed.txt\n").expect("write injected FileList");
    let request = IndexRequest {
        request_id: 72,
        root: temp.path.clone(),
        include_files: true,
        include_dirs: false,
        source: TuiSource::FileList,
        filelist_discovery: FileListDiscoveryOwnership::Completed(Some(discovered)),
        max_depth: crate::indexer::MaxDepth::unlimited(),
    };
    let mut responses = Vec::new();

    process_index_request(request, &|| false, |response| responses.push(response));

    assert!(responses.iter().any(|response| matches!(
        response,
        WorkerResponse::IndexedBatch { entries, .. }
            if entries.iter().any(|path| path.ends_with("listed.txt"))
    )));
    assert!(responses.iter().any(|response| matches!(
        response,
        WorkerResponse::IndexedFinished {
            request_id: 72,
            has_root_filelist: true,
            ..
        }
    )));
}

#[test]
fn tc_162_startup_discovery_ownership_is_source_specific_regression() {
    use std::cell::Cell;

    let root = Path::new("fixture");
    for source in [TuiSource::Auto, TuiSource::Walker] {
        let calls = Cell::new(0);
        let ownership = initial_filelist_discovery_with(root, source, |_| {
            calls.set(calls.get() + 1);
            None
        })
        .expect("worker-owned source");
        assert!(matches!(ownership, FileListDiscoveryOwnership::WorkerOwned));
        assert_eq!(calls.get(), 0, "{source:?} must not discover on main");
    }

    let expected = root.join("FileList.txt");
    let calls = Cell::new(0);
    let ownership = initial_filelist_discovery_with(root, TuiSource::FileList, |_| {
        calls.set(calls.get() + 1);
        Some(expected.clone())
    })
    .expect("required FileList preflight");
    assert!(matches!(
        ownership,
        FileListDiscoveryOwnership::Completed(Some(path)) if path == expected
    ));
    assert_eq!(calls.get(), 1);

    let error = initial_filelist_discovery_with(root, TuiSource::FileList, |_| None)
        .expect_err("required FileList must fail before terminal ownership");
    assert!(error.to_string().contains("FileList was required"));
}

#[test]
fn tc_162_explicit_walker_performs_zero_filelist_discovery_regression() {
    let temp = TestTempDir::new("explicit-walker-no-filelist-discovery");
    fs::write(temp.path.join("walked.txt"), "walked").expect("write walker fixture");
    fs::write(temp.path.join("FileList.txt"), "walked.txt\n").expect("write ignored FileList");
    let request = IndexRequest {
        request_id: 73,
        root: temp.path.clone(),
        include_files: true,
        include_dirs: false,
        source: TuiSource::Walker,
        filelist_discovery: FileListDiscoveryOwnership::WorkerOwned,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    };
    let mut responses = Vec::new();

    process_index_request(request, &|| false, |response| responses.push(response));

    assert!(responses.iter().any(|response| matches!(
        response,
        WorkerResponse::IndexedFinished {
            request_id: 73,
            has_root_filelist: false,
            ..
        }
    )));

    let mut state = TuiState::new("");
    state.root = temp.path.clone();
    state.runtime_options.source = TuiSource::Walker;
    state.active_index_request = Some((73, temp.path.clone()));
    apply_worker_response(
        &mut state,
        WorkerResponse::IndexedFinished {
            request_id: 73,
            root: temp.path.clone(),
            has_root_filelist: false,
        },
    )
    .expect("apply explicit Walker finish");
    assert!(!state.root_filelist_known);
}

#[test]
fn tc_162_tui_walker_uses_runtime_adaptive_limits_and_reports_cap_before_finish() {
    let temp = TestTempDir::new("walker-runtime-limits");
    for name in ["one.txt", "two.txt", "three.txt"] {
        fs::write(temp.path.join(name), name).expect("write walker fixture");
    }
    let request = IndexRequest {
        request_id: 8,
        root: temp.path.clone(),
        include_files: true,
        include_dirs: false,
        source: TuiSource::Walker,
        filelist_discovery: FileListDiscoveryOwnership::WorkerOwned,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    };
    let config = RuntimeConfig {
        walker_max_entries: 1,
        developer: DeveloperRuntimeConfig {
            walker_adaptive_initial_limit: Some(1),
            walker_adaptive_max_limit: Some(1),
            ..DeveloperRuntimeConfig::default()
        },
        ..RuntimeConfig::default()
    };
    let mut responses = Vec::new();

    process_index_request_with_config(request, &config, &|| false, |response| {
        responses.push(response)
    });

    let emitted = responses
        .iter()
        .map(|response| match response {
            WorkerResponse::IndexedBatch { entries, .. } => entries.len(),
            _ => 0,
        })
        .sum::<usize>();
    let truncated = responses
        .iter()
        .position(|response| {
            matches!(
                response,
                WorkerResponse::IndexTruncated {
                    request_id: 8,
                    limit: 1,
                    ..
                }
            )
        })
        .expect("truncation response");
    let finished = responses
        .iter()
        .position(|response| {
            matches!(
                response,
                WorkerResponse::IndexedFinished { request_id: 8, .. }
            )
        })
        .expect("finished response");

    assert_eq!(emitted, 1);
    assert!(truncated < finished);
}

#[test]
fn tc_180_tui_index_request_applies_max_depth() {
    let temp = TestTempDir::new("max-depth");
    let child = temp.path.join("child");
    let grandchild = child.join("grandchild");
    fs::create_dir_all(&grandchild).expect("create depth fixture");
    fs::write(child.join("visible.txt"), "visible").expect("write visible");
    fs::write(grandchild.join("hidden.txt"), "hidden").expect("write hidden");
    let request = IndexRequest {
        request_id: 180,
        root: temp.path.clone(),
        include_files: true,
        include_dirs: true,
        source: TuiSource::Walker,
        filelist_discovery: FileListDiscoveryOwnership::WorkerOwned,
        max_depth: crate::indexer::MaxDepth::limited(2).expect("valid depth"),
    };
    let mut responses = Vec::new();

    process_index_request(request, &|| false, |response| responses.push(response));

    let emitted = responses
        .iter()
        .filter_map(|response| match response {
            WorkerResponse::IndexedBatch { entries, .. } => Some(entries),
            _ => None,
        })
        .flatten()
        .collect::<Vec<_>>();
    assert!(emitted.iter().any(|path| path.ends_with("visible.txt")));
    assert!(!emitted.iter().any(|path| path.ends_with("hidden.txt")));
}

#[test]
fn tc_162_candidate_batches_append_without_cloning_existing_paths() {
    let mut candidates = CandidateBatches::default();
    candidates.push(vec![PathBuf::from("first.txt")]);
    let search_snapshot = candidates.snapshot();
    let first_batch = Arc::clone(&search_snapshot[0].entries);

    candidates.push(vec![PathBuf::from("second.txt")]);

    assert_eq!(candidates.len(), 2);
    assert!(Arc::ptr_eq(&first_batch, &candidates.snapshot()[0].entries));
}

#[test]
fn tc_162_worker_response_drain_respects_per_tick_budget() {
    let (tx, rx) = mpsc::channel();
    for value in 0..=MAX_WORKER_RESPONSES_PER_TICK {
        tx.send(value).expect("queue response");
    }

    let drained = take_ready_responses(&rx, MAX_WORKER_RESPONSES_PER_TICK);

    assert_eq!(drained.len(), MAX_WORKER_RESPONSES_PER_TICK);
    assert_eq!(rx.try_recv(), Ok(MAX_WORKER_RESPONSES_PER_TICK));
}

#[test]
fn tc_162_query_editor_supports_delete_home_end_and_unicode_paste() {
    let mut state = TuiState::new("ab");
    handle_key(&mut state, KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
    insert_paste(&mut state, "界🙂");
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
    );
    handle_key(&mut state, KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
    );

    assert_eq!(state.query, "界🙂");
    assert_eq!(state.query_cursor, 2);
}

#[test]
fn tc_162_page_navigation_uses_dynamic_viewport_rows() {
    let mut state = TuiState::new("");
    state.results = Arc::new(
        (0..20)
            .map(|index| (PathBuf::from(format!("{index}.txt")), 1.0))
            .collect(),
    );
    state.viewport_rows = 5;

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
    );
    assert_eq!(state.selected, 5);
    assert_eq!(state.offset, 1);
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
    );
    assert_eq!(state.selected, 0);
    assert_eq!(state.offset, 0);
}

#[test]
fn tc_162_unicode_clipping_uses_terminal_column_width() {
    assert_eq!(clip_to_width("a界b", 3), "a界");
    assert_eq!(clip_to_width("a界b", 2), "a");
    assert_eq!(clip_to_width("e\u{301}x", 1), "e\u{301}");
    assert_eq!(clip_to_width("a\u{1b}b", 3), "a�b");

    let mut state = TuiState::new("abcdefghijk");
    state.query_cursor = 10;
    let query_line = query_line_for_width(&state, 8);
    assert!(query_line.contains('│'));
    assert!(
        query_line
            .chars()
            .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
            .sum::<usize>()
            <= 8
    );
}

#[test]
fn tc_162_preview_toggle_collapse_and_reexpansion_preserve_preference() {
    let mut state = TuiState::new("");
    state.results = Arc::new(vec![(PathBuf::from("selected.txt"), 1.0)]);

    assert!(update_preview_visibility(
        &mut state,
        PREVIEW_MIN_WIDTH,
        PREVIEW_MIN_HEIGHT
    ));
    assert!(state.preview_visible);
    assert!(state.preview_preferred);

    let request = state
        .next_preview_request()
        .expect("visible preview request");
    assert_eq!(request.path, PathBuf::from("selected.txt"));
    assert_eq!(state.preview, "Loading preview...");

    assert!(!update_preview_visibility(
        &mut state,
        PREVIEW_MIN_WIDTH - 1,
        PREVIEW_MIN_HEIGHT
    ));
    assert!(!state.preview_visible);
    assert!(state.preview_preferred);
    assert!(state.preview.is_empty());

    assert!(update_preview_visibility(
        &mut state,
        PREVIEW_MIN_WIDTH,
        PREVIEW_MIN_HEIGHT
    ));
    assert!(state.preview_visible);
    assert!(state.preview_preferred);
    let expanded_request = state.next_preview_request().expect("re-expanded request");
    assert_ne!(expanded_request.request_id, request.request_id);

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT),
    );
    assert!(!state.preview_preferred);
    assert!(!state.preview_visible);
    assert!(state.preview.is_empty());
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT),
    );
    assert!(state.preview_preferred);
    assert!(update_preview_visibility(
        &mut state,
        PREVIEW_MIN_WIDTH,
        PREVIEW_MIN_HEIGHT
    ));
    assert!(state.preview_visible);
    assert!(state.next_preview_request().is_some());
}

#[test]
fn tc_162_preview_response_requires_matching_request_root_and_path() {
    let mut state = TuiState::new("");
    state.root = PathBuf::from("root-a");
    state.results = Arc::new(vec![
        (PathBuf::from("root-a/one.txt"), 1.0),
        (PathBuf::from("root-a/two.txt"), 1.0),
    ]);
    update_preview_visibility(&mut state, PREVIEW_MIN_WIDTH, PREVIEW_MIN_HEIGHT);
    let request = state.next_preview_request().expect("preview request");

    apply_preview_response(
        &mut state,
        request.request_id,
        Path::new("root-b"),
        &request.path,
        "wrong root".to_string(),
    );
    assert_eq!(state.preview, "Loading preview...");

    apply_preview_response(
        &mut state,
        request.request_id.wrapping_add(1),
        &request.root,
        &request.path,
        "wrong id".to_string(),
    );
    assert_eq!(state.preview, "Loading preview...");

    state.move_selection(1);
    apply_preview_response(
        &mut state,
        request.request_id,
        &request.root,
        &request.path,
        "stale path".to_string(),
    );
    assert_eq!(state.preview, "Loading preview...");

    let request = state
        .next_preview_request()
        .expect("replacement preview request");
    assert_eq!(state.preview, "Loading preview...");
    assert_ne!(request.request_id, 1);
    apply_preview_response(
        &mut state,
        request.request_id,
        &request.root,
        &request.path,
        "fresh preview".to_string(),
    );
    assert_eq!(state.preview, "fresh preview");
}

#[test]
fn tc_162_preview_request_clears_content_without_selection() {
    let mut state = TuiState::new("");
    update_preview_visibility(&mut state, PREVIEW_MIN_WIDTH, PREVIEW_MIN_HEIGHT);
    state.preview = "stale".to_string();
    state.active_preview_request = Some(PreviewRequestIdentity {
        request_id: 9,
        root: PathBuf::from("root"),
        path: PathBuf::from("root/old.txt"),
    });

    assert!(state.next_preview_request().is_none());
    assert!(state.preview.is_empty());
    assert!(state.active_preview_request.is_none());
}

#[test]
fn tc_162_preview_uses_shared_text_builder_for_file_binary_and_error() {
    let root = std::env::temp_dir().join(format!(
        "flistwalker-cli-preview-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("create preview fixture");
    let text = root.join("text.txt");
    let binary = root.join("binary.bin");
    std::fs::write(&text, "preview text").expect("write text fixture");
    std::fs::write(&binary, [0, 159, 146, 150]).expect("write binary fixture");

    assert!(build_preview_text_with_kind(&root, true).contains("Directory:"));
    assert!(build_preview_text_with_kind(&text, false).contains("preview text"));
    assert!(build_preview_text_with_kind(&binary, false).contains("<binary or unreadable file>"));
    assert!(
        build_preview_text_with_kind(&root.join("missing.txt"), false)
            .contains("<binary or unreadable file>")
    );

    std::fs::remove_dir_all(root).expect("remove preview fixture");
}

#[test]
fn tc_162_preview_pane_clips_unicode_and_control_text() {
    let mut state = TuiState::new("");
    state.preview = "界界\u{1b}x\nsecond".to_string();
    let mut output = Vec::new();

    render_preview_pane(&mut output, &state, 60, 100, PREVIEW_MIN_HEIGHT)
        .expect("render preview pane");

    let rendered = String::from_utf8_lossy(&output);
    assert!(rendered.contains("Preview"));
    assert!(rendered.contains('�'));
    assert!(!rendered.contains("\u{1b}x"));
}

#[test]
fn tc_162_delayed_preview_worker_cleanup_uses_the_bounded_wait() {
    let (done_tx, done_rx) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("flistwalker-cli-preview-delayed-test".to_string())
        .spawn(move || {
            thread::sleep(Duration::from_millis(500));
            let _ = done_tx.send(());
        })
        .expect("start delayed preview worker");

    let started = Instant::now();
    finish_worker(handle, done_rx);
    assert!(
        started.elapsed() < Duration::from_millis(450),
        "preview cleanup exceeded the bounded wait: {:?}",
        started.elapsed()
    );
}

fn history_state(entries: &[&str], query: &str) -> TuiState {
    let mut state = TuiState::new(query);
    state.history_enabled = true;
    state.history_entries = entries.iter().map(|entry| (*entry).to_string()).collect();
    state.viewport_rows = 1;
    state
}

#[test]
fn tc_162_history_overlay_orders_recent_entries_and_filters_fuzzily() {
    let mut state = history_state(&["old", "alpha", "beta"], "draft");
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );
    let history = state.history.as_ref().expect("history overlay");
    assert_eq!(history.draft_query, "draft");
    assert_eq!(history.results, vec!["draft", "beta", "alpha", "old"]);

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE),
    );
    let history = state.history.as_ref().expect("filtered history overlay");
    assert_eq!(history.filter, "p");
    assert_eq!(history.results, vec!["alpha"]);
}

#[test]
fn tc_162_history_filter_supports_enabled_emacs_editing_and_disabled_noop() {
    let mut enabled = history_state(&["alpha beta", "alpha"], "draft");
    handle_key(
        &mut enabled,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );
    insert_paste(&mut enabled, "alpha beta");
    handle_key(
        &mut enabled,
        KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
    );
    assert_eq!(
        enabled.history.as_ref().expect("history overlay").filter,
        "alpha "
    );
    handle_key(
        &mut enabled,
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
    );
    assert_eq!(
        enabled.history.as_ref().expect("history overlay").filter,
        "alpha beta"
    );
    handle_key(
        &mut enabled,
        KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
    );
    assert_eq!(
        enabled
            .history
            .as_ref()
            .expect("history overlay")
            .filter_cursor,
        0
    );

    let mut disabled = history_state(&["alpha"], "draft");
    handle_key(
        &mut disabled,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );
    insert_paste(&mut disabled, "alpha");
    disabled.emacs_keybindings_enabled = false;
    handle_key(
        &mut disabled,
        KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
    );
    assert_eq!(
        disabled.history.as_ref().expect("history overlay").filter,
        "alpha"
    );
}

#[test]
fn tc_162_history_overlay_accept_cancel_navigation_and_paste_contract() {
    let mut state = history_state(&["one", "two", "three"], "draft");
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
    );
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT),
    );
    assert!(
        state
            .history
            .as_ref()
            .expect("history overlay")
            .filter
            .is_empty(),
        "side-effect chords must not edit the history filter"
    );
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
    );
    assert_eq!(state.history.as_ref().expect("history overlay").selected, 1);
    handle_key(&mut state, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(state.history.as_ref().expect("history overlay").selected, 0);
    insert_paste(&mut state, "tw");
    assert_eq!(
        state.history.as_ref().expect("history overlay").results,
        vec!["two"]
    );
    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        ),
        KeyAction::HistoryApplied
    ));
    assert!(state.history.is_none());
    assert_eq!(state.query, "two");
    assert!(state.last_query_change.is_some());

    state.query = "draft again".to_string();
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );
    insert_paste(&mut state, "x");
    handle_key(&mut state, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(state.history.is_none());
    assert_eq!(state.query, "draft again");

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
    );
    assert!(state.history.is_none());
    assert_eq!(state.query, "draft again");
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );
    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
        ),
        KeyAction::Cancel
    ));
}

#[test]
fn tc_162_history_disabled_ctrl_r_is_a_silent_noop() {
    let mut state = TuiState::new("draft");
    state.status = "Ready".to_string();
    state.dirty = false;

    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL)
        ),
        KeyAction::Continue
    ));
    assert!(state.history.is_none());
    assert_eq!(state.query, "draft");
    assert_eq!(state.status, "Ready");
    assert!(!state.dirty);
    assert!(enqueue_history_delta(None, " trimmed ").is_ok());
}

#[test]
fn tc_162_history_open_commits_draft_as_the_most_recent_delta() {
    let mut state = history_state(&["first", "draft", "second"], " draft ");

    let action = handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );

    assert!(matches!(action, KeyAction::HistoryOpened(Some(ref query)) if query == "draft"));
    assert_eq!(state.history_entries, vec!["first", "second", "draft"]);
    assert_eq!(
        state
            .history
            .as_ref()
            .expect("history overlay")
            .results
            .first(),
        Some(&"draft".to_string())
    );
}

#[test]
fn tc_162_help_overlay_has_precedence_and_ctrl_g_only_closes_it() {
    let mut state = history_state(&["prior"], "draft");
    state.pinned.push(PathBuf::from("pinned.txt"));
    state.preview_preferred = true;

    handle_key(&mut state, KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    assert_eq!(state.help, Some(HelpContext::Normal));
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
    );
    handle_key(&mut state, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT),
    );
    assert_eq!(state.query, "draft");
    assert_eq!(state.pinned, vec![PathBuf::from("pinned.txt")]);
    assert!(state.preview_preferred);

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
    );
    assert!(state.help.is_none());
    assert_eq!(state.query, "draft");
    assert_eq!(state.pinned, vec![PathBuf::from("pinned.txt")]);

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
    );
    assert!(state.query.is_empty());
    assert!(state.pinned.is_empty());
}

#[test]
fn tc_162_help_from_history_restores_history_and_ctrl_c_exits_tui() {
    let mut state = history_state(&["prior"], "draft");
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );
    insert_paste(&mut state, "pr");
    let filter = state
        .history
        .as_ref()
        .expect("history overlay")
        .filter
        .clone();

    handle_key(&mut state, KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    assert_eq!(state.help, Some(HelpContext::History));
    insert_paste(&mut state, "ignored");
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    );
    assert!(state.help.is_none());
    assert_eq!(
        state.history.as_ref().expect("history overlay").filter,
        filter
    );

    handle_key(&mut state, KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
        ),
        KeyAction::Cancel
    ));
}

#[test]
fn tc_162_help_and_history_overlays_clear_the_full_terminal() {
    let history = HistoryOverlay {
        draft_query: String::new(),
        filter: String::new(),
        filter_cursor: 0,
        results: vec!["entry".to_string()],
        selected: 0,

        offset: 0,
    };
    let mut history_output = Vec::new();
    render_history_overlay(&mut history_output, &history, true, 40, 8, true)
        .expect("render history");
    let mut help_output = Vec::new();
    render_help_overlay(&mut help_output, HelpContext::Normal, true, 40, 8).expect("render help");

    for output in [&history_output, &help_output] {
        assert!(
            output.windows(4).any(|window| window == b"\x1b[2J"),
            "overlay must clear terminal before rendering"
        );
    }
}

#[test]
fn tc_162_help_overlay_matches_emacs_runtime_config() {
    let mut enabled_output = Vec::new();
    render_help_overlay(&mut enabled_output, HelpContext::Normal, true, 100, 10)
        .expect("render enabled help");
    let enabled_text = String::from_utf8_lossy(&enabled_output);
    assert!(enabled_text.contains("Ctrl+N"));
    assert!(enabled_text.contains("Ctrl+G"));
    assert!(enabled_text.contains("Ctrl+R"));

    let mut disabled_output = Vec::new();
    render_help_overlay(&mut disabled_output, HelpContext::Normal, false, 100, 10)
        .expect("render disabled help");
    let disabled_text = String::from_utf8_lossy(&disabled_output);
    assert!(disabled_text.contains("Emacs shortcuts disabled"));
    assert!(!disabled_text.contains("Ctrl+N"));
    assert!(!disabled_text.contains("Ctrl+G"));
    assert!(!disabled_text.contains("Ctrl+R"));

    let options = OptionsOverlay {
        draft: TuiRuntimeOptions {
            include_files: true,
            include_dirs: true,
            regex: false,
            ignore_case: false,
            ignore_enabled: true,
            source: TuiSource::Walker,
        },
        selected: 0,
    };
    let history = HistoryOverlay {
        draft_query: String::new(),
        filter: String::new(),
        filter_cursor: 0,
        results: Vec::new(),
        selected: 0,
        offset: 0,
    };
    let mut overlay_outputs = vec![Vec::new(); 5];
    render_options_overlay(&mut overlay_outputs[0], &options, false, 120, 8)
        .expect("render disabled options");
    render_sort_picker(
        &mut overlay_outputs[1],
        &SortPicker { selected: 0 },
        false,
        120,
        8,
    )
    .expect("render disabled sort");
    render_root_picker(
        &mut overlay_outputs[2],
        &RootPicker { selected: 0 },
        &[PathBuf::from("root")],
        false,
        120,
        8,
    )
    .expect("render disabled roots");
    render_filelist_confirmation(
        &mut overlay_outputs[3],
        &FileListConfirmation::Mode {
            propagate_to_ancestors: false,
        },
        false,
        120,
        8,
    )
    .expect("render disabled FileList confirmation");
    render_history_overlay(&mut overlay_outputs[4], &history, false, 120, 8, true)
        .expect("render disabled history");
    for output in overlay_outputs {
        assert!(!String::from_utf8_lossy(&output).contains("Ctrl+G"));
    }
}

#[test]
fn tc_162_f2_options_and_f3_sort_overlays_have_precedence_without_side_effects() {
    let mut state = TuiState::new("draft");
    state.results = Arc::new(vec![(PathBuf::from("selected.txt"), 1.0)]);
    state.pinned.push(PathBuf::from("pinned.txt"));

    handle_key(&mut state, KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
    assert!(state.options_overlay.is_some());
    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)
        ),
        KeyAction::Continue
    ));
    assert_eq!(state.query, "draft");
    assert_eq!(state.pinned, vec![PathBuf::from("pinned.txt")]);
    handle_key(&mut state, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    handle_key(&mut state, KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE));
    assert!(state.sort_picker.is_some());
    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        ),
        KeyAction::Continue
    ));
    assert_eq!(state.sort_mode, SearchSortMode::Score);
}

#[test]
fn tc_162_tui_sort_picker_has_all_nine_shared_modes_and_query_resets_score() {
    assert_eq!(SORT_MODES.len(), 9);
    assert_eq!(SORT_MODES[0], SearchSortMode::Score);
    assert_eq!(SORT_MODES[8], SearchSortMode::SizeAsc);
    let mut state = TuiState::new("draft");
    state.sort_mode = SearchSortMode::SizeDesc;

    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
    );

    assert_eq!(state.sort_mode, SearchSortMode::Score);
}

#[test]
fn tc_162_tui_options_reindex_only_for_scope_or_source_changes() {
    let base = TuiRuntimeOptions::from_startup(&CliTuiOptions {
        initial_query: String::new(),
        limit: 10,
        max_depth: crate::indexer::MaxDepth::unlimited(),
        absolute: false,
        print0: false,
        include_files: true,
        include_dirs: true,
        use_filelist: true,
        require_filelist: false,
        regex: false,
        ignore_case: false,
        ignore_enabled: true,
        ignore_terms: vec!["ignored".to_string()],
        sort_mode: SearchSortMode::Score,
        color_enabled: true,
    });
    let mut search_only = base;
    search_only.regex = true;
    search_only.ignore_case = true;
    search_only.ignore_enabled = false;
    assert!(!option_change_requires_reindex(base, search_only));
    let mut reindex = base;
    reindex.include_files = false;
    assert!(option_change_requires_reindex(base, reindex));
    let mut source = base;
    source.source = TuiSource::Walker;
    assert!(option_change_requires_reindex(base, source));
}

#[test]
fn tc_162_options_never_disable_both_files_and_folders() {
    let mut overlay = OptionsOverlay {
        draft: TuiRuntimeOptions {
            include_files: true,
            include_dirs: false,
            regex: false,
            ignore_case: false,
            ignore_enabled: true,
            source: TuiSource::Auto,
        },
        selected: 0,
    };
    toggle_option(&mut overlay);
    assert!(overlay.draft.include_files);
    overlay.draft.include_dirs = true;
    toggle_option(&mut overlay);
    assert!(!overlay.draft.include_files);
    overlay.selected = 1;
    toggle_option(&mut overlay);
    assert!(overlay.draft.include_dirs);
}

#[test]
fn tc_162_stale_index_responses_are_discarded_by_identity() {
    let mut state = TuiState::new("");
    state.active_index_request = Some((2, PathBuf::from("root-b")));
    apply_worker_response(
        &mut state,
        WorkerResponse::IndexTruncated {
            request_id: 1,
            root: PathBuf::from("root-a"),
            limit: 3,
        },
    )
    .expect("stale truncation ignored");
    assert_eq!(state.index_truncated_limit, None);
    apply_worker_response(
        &mut state,
        WorkerResponse::IndexedBatch {
            request_id: 1,
            root: PathBuf::from("root-a"),
            entries: vec![PathBuf::from("stale.txt")],
        },
    )
    .expect("stale response ignored");
    assert_eq!(state.entries.len(), 0);
    apply_worker_response(
        &mut state,
        WorkerResponse::IndexedBatch {
            request_id: 2,
            root: PathBuf::from("root-b"),
            entries: vec![PathBuf::from("fresh.txt")],
        },
    )
    .expect("fresh response accepted");
    assert_eq!(state.entries.len(), 1);
    assert_eq!(
        state.entries.snapshot()[0].as_ref(),
        [PathBuf::from("fresh.txt")]
    );
    apply_worker_response(
        &mut state,
        WorkerResponse::IndexTruncated {
            request_id: 2,
            root: PathBuf::from("root-b"),
            limit: 5,
        },
    )
    .expect("fresh truncation accepted");
    apply_worker_response(
        &mut state,
        WorkerResponse::IndexedFinished {
            request_id: 2,
            root: PathBuf::from("root-b"),
            has_root_filelist: false,
        },
    )
    .expect("fresh finish accepted");
    assert!(state.status.contains("Walker capped at 5 entries"));
    assert_eq!(state.index_truncated_limit, Some(5));

    state.root = PathBuf::from("root-b");
    state.active_search_request_id = Some(9);
    let options = state.runtime_options.search_options(state.sort_mode);
    apply_worker_response(
        &mut state,
        WorkerResponse::Searched {
            request_id: 9,
            root: PathBuf::from("root-b"),
            query: String::new(),
            options,
            results: Arc::new(vec![(PathBuf::from("fresh.txt"), 1.0)]),
            error: None,
        },
    )
    .expect("search response accepted");
    assert!(state.status.contains("Walker capped at 5 entries"));
}

#[test]
fn tc_162_tui_search_applies_ignore_in_worker_snapshot_and_sorts_before_limit() {
    let entries = Arc::new(vec![CandidateBatch::from(vec![
        PathBuf::from("root/zeta.txt"),
        PathBuf::from("root/ignored.txt"),
        PathBuf::from("root/alpha.txt"),
    ])]);
    let mut cache = SearchPrefixCache::default();
    let request = SearchRequest {
        request_id: 1,
        query: String::new(),
        entries: Arc::clone(&entries),
        root: PathBuf::from("root"),
        limit: 2,
        options: SearchOptions {
            regex: false,
            ignore_case: false,
            ignore_enabled: true,
            sort_mode: SearchSortMode::NameAsc,
        },
        ignore_terms: Arc::new(vec!["ignored".to_string()]),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let mut snapshot_cache = TuiSearchSnapshotCache::default();
    let (results, error) = search(&request, &mut cache, &mut snapshot_cache);
    assert!(error.is_none());
    assert_eq!(
        results.iter().map(|(path, _)| path).collect::<Vec<_>>(),
        vec![
            &PathBuf::from("root/alpha.txt"),
            &PathBuf::from("root/zeta.txt")
        ],
        "Name sort must run over all non-ignored matches before limit"
    );

    let unignored = SearchRequest {
        options: SearchOptions {
            ignore_enabled: false,
            ..request.options
        },
        ..request
    };
    let (results, error) = search(&unignored, &mut cache, &mut snapshot_cache);
    assert!(error.is_none());
    assert_eq!(results.len(), 2);
    assert!(results
        .iter()
        .any(|(path, _)| path.ends_with("ignored.txt")));
}

#[test]
fn tui_search_reuses_candidate_projection_and_warms_prefix_cache_for_same_snapshot() {
    let paths = (0..1_000)
        .map(|index| {
            if index < 25 {
                PathBuf::from(format!("root/abcde-item-{index:04}.rs"))
            } else {
                PathBuf::from(format!("root/zzzz-item-{index:04}.rs"))
            }
        })
        .collect::<Vec<_>>();
    let entries = Arc::new(vec![CandidateBatch::from(paths)]);
    let make_request = |request_id, query: &str| SearchRequest {
        request_id,
        query: query.to_string(),
        entries: Arc::clone(&entries),
        root: PathBuf::from("root"),
        limit: 100,
        options: SearchOptions {
            regex: false,
            ignore_case: true,
            ignore_enabled: false,
            sort_mode: SearchSortMode::Score,
        },
        ignore_terms: Arc::new(Vec::new()),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let mut prefix_cache = SearchPrefixCache::default();
    let mut snapshot_cache = TuiSearchSnapshotCache::default();

    let (seed, seed_error) = search_with_stats(
        &make_request(1, "abc"),
        &mut prefix_cache,
        &mut snapshot_cache,
    );
    assert!(seed_error.is_none());
    assert_eq!(seed.evaluated_candidate_count, 1_000);

    let (warm, warm_error) = search_with_stats(
        &make_request(2, "abcd"),
        &mut prefix_cache,
        &mut snapshot_cache,
    );
    assert!(warm_error.is_none());
    assert_eq!(
        warm.results.iter().map(|item| &item.0).collect::<Vec<_>>(),
        seed.results.iter().map(|item| &item.0).collect::<Vec<_>>()
    );
    assert_eq!(warm.evaluated_candidate_count, 25);
    assert_eq!(snapshot_cache.build_count(), 1);
}

#[test]
fn newer_tui_search_request_cancels_the_previous_request() {
    let mut state = TuiState::new("first");
    let first = state.next_search_request(PathBuf::from("root"), 100);
    state.query = "second".to_string();
    let second = state.next_search_request(PathBuf::from("root"), 100);

    assert!(first.cancel.load(Ordering::Acquire));
    assert!(!second.cancel.load(Ordering::Acquire));
}

#[test]
fn canceled_tui_candidate_projection_is_stopped_and_not_cached() {
    let request = SearchRequest {
        request_id: 1,
        query: "module".to_string(),
        entries: Arc::new(vec![CandidateBatch::from(
            (0..10_000)
                .map(|index| PathBuf::from(format!("root/module-{index:05}.rs")))
                .collect::<Vec<_>>(),
        )]),
        root: PathBuf::from("root"),
        limit: 100,
        options: SearchOptions {
            regex: false,
            ignore_case: true,
            ignore_enabled: false,
            sort_mode: SearchSortMode::Score,
        },
        ignore_terms: Arc::new(Vec::new()),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let checks = std::sync::atomic::AtomicUsize::new(0);
    let cancellation = || checks.fetch_add(1, Ordering::Relaxed) >= 1;
    let mut prefix_cache = SearchPrefixCache::default();
    let mut snapshot_cache = TuiSearchSnapshotCache::default();

    let result = search_with_stats_cancellable(
        &request,
        &mut prefix_cache,
        &mut snapshot_cache,
        &cancellation,
    );

    assert!(result.is_none());
    assert_eq!(snapshot_cache.build_count(), 0);
}

#[test]
fn request_cancel_skips_only_that_tui_search_while_shutdown_stops_the_worker() {
    let shutdown = AtomicBool::new(false);
    let request_cancel = AtomicBool::new(true);
    assert_eq!(
        search_publish_decision(&shutdown, &request_cancel),
        SearchPublishDecision::SkipRequest
    );

    shutdown.store(true, Ordering::Release);
    assert_eq!(
        search_publish_decision(&shutdown, &request_cancel),
        SearchPublishDecision::StopWorker
    );
}

#[test]
fn tc_163_disabled_startup_ignore_can_be_reenabled_without_reloading_terms() {
    let startup = CliTuiOptions {
        initial_query: String::new(),
        limit: 10,
        max_depth: crate::indexer::MaxDepth::unlimited(),
        absolute: false,
        print0: false,
        include_files: true,
        include_dirs: true,
        use_filelist: true,
        require_filelist: false,
        regex: false,
        ignore_case: true,
        ignore_enabled: false,
        ignore_terms: vec!["ignored".to_string()],
        sort_mode: SearchSortMode::Score,
        color_enabled: true,
    };
    let mut runtime = TuiRuntimeOptions::from_startup(&startup);
    assert!(!runtime.ignore_enabled);

    runtime.ignore_enabled = true;
    let request = SearchRequest {
        request_id: 1,
        query: String::new(),
        entries: Arc::new(vec![CandidateBatch::from(vec![
            PathBuf::from("root/visible.txt"),
            PathBuf::from("root/ignored.txt"),
        ])]),
        root: PathBuf::from("root"),
        limit: 10,
        options: runtime.search_options(SearchSortMode::Score),
        ignore_terms: Arc::new(startup.ignore_terms),
        cancel: Arc::new(AtomicBool::new(false)),
    };

    let (results, error) = search(
        &request,
        &mut SearchPrefixCache::default(),
        &mut TuiSearchSnapshotCache::default(),
    );

    assert!(error.is_none());
    assert_eq!(results.len(), 1);
    assert!(results[0].0.ends_with("visible.txt"));
}

#[test]
fn tc_162_newer_index_identity_supersedes_an_in_progress_request() {
    let freshness = TuiIndexFreshness::new();
    freshness.activate(1);
    assert!(freshness.is_current(1));

    freshness.activate(2);
    assert!(
        !freshness.is_current(1),
        "walker/FileList cancellation closure must stop the superseded request"
    );
    assert!(freshness.is_current(2));
}

#[test]
fn tc_162_applied_options_reset_sort_only_when_the_draft_changes() {
    let mut state = TuiState::new("query");
    state.sort_mode = SearchSortMode::SizeDesc;
    state.options_overlay = Some(OptionsOverlay {
        draft: state.runtime_options,
        selected: 0,
    });
    handle_options_key(
        &mut state,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    );
    assert_eq!(state.sort_mode, SearchSortMode::SizeDesc);

    state.sort_mode = SearchSortMode::SizeDesc;
    let mut changed = state.runtime_options;
    changed.ignore_case = !changed.ignore_case;
    state.options_overlay = Some(OptionsOverlay {
        draft: changed,
        selected: 0,
    });
    handle_options_key(
        &mut state,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    );
    assert_eq!(state.sort_mode, SearchSortMode::Score);
    assert!(state.active_search_request_id.is_none());
}

#[test]
fn tc_162_source_transition_clears_source_scoped_state_before_reindex() {
    let mut state = TuiState::new("");
    state.root = PathBuf::from("root");
    state.pinned.push(PathBuf::from("root/pinned.txt"));
    state.preview = "old preview".to_string();
    state.active_preview_request = Some(PreviewRequestIdentity {
        request_id: 3,
        root: state.root.clone(),
        path: PathBuf::from("root/old.txt"),
    });
    state.active_action_request = Some((4, PathBuf::from("root/old.txt")));
    state.source_changed_on_apply = true;
    let action_freshness = TuiActionFreshness::new();
    action_freshness.activate(4, &state.root);

    prepare_source_transition(&mut state, &action_freshness, Path::new("root"));

    assert!(state.pinned.is_empty());
    assert!(state.preview.is_empty());
    assert!(state.active_preview_request.is_none());
    assert!(state.active_action_request.is_none());
    assert!(!state.source_changed_on_apply);
    assert!(!action_freshness.is_current(4, Path::new("root")));
}

#[test]
fn tc_162_every_reindex_clears_current_preview_and_pending_search_without_clearing_pins() {
    let mut state = TuiState::new("");
    state.root = PathBuf::from("root");
    state.results = Arc::new(vec![(PathBuf::from("root/current.txt"), 1.0)]);
    state.pinned.push(PathBuf::from("root/pinned.txt"));
    state.preview = "stale preview".to_string();
    state.active_preview_request = Some(PreviewRequestIdentity {
        request_id: 1,
        root: state.root.clone(),
        path: PathBuf::from("root/current.txt"),
    });
    state.active_search_request_id = Some(2);

    state.next_index_request(state.root.clone());

    assert!(state.results.is_empty());
    assert!(state.preview.is_empty());
    assert!(state.active_preview_request.is_none());
    assert!(state.active_search_request_id.is_none());
    assert_eq!(state.pinned, vec![PathBuf::from("root/pinned.txt")]);
}

#[test]
fn tc_162_root_picker_precedence_empty_state_and_small_viewport_are_safe() {
    let mut state = TuiState::new("query");
    state.results = Arc::new(vec![(PathBuf::from("current.txt"), 1.0)]);
    handle_key(&mut state, KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE));
    assert!(state.root_picker.is_some());
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
    );
    assert_eq!(state.query, "query");
    assert!(state.root_picker.is_some());
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    );
    assert!(state.root_picker.is_none());

    let roots = (0..6)
        .map(|index| PathBuf::from(format!("root-{index}")))
        .collect::<Vec<_>>();
    let mut output = Vec::new();
    render_root_picker(
        &mut output,
        &RootPicker { selected: 5 },
        &roots,
        true,
        80,
        4,
    )
    .expect("render roots");
    assert!(String::from_utf8_lossy(&output).contains("> root-5"));
    let mut output = Vec::new();
    render_root_picker(&mut output, &RootPicker { selected: 0 }, &[], true, 80, 4)
        .expect("render empty roots");
    assert!(String::from_utf8_lossy(&output).contains("No saved roots"));
}

#[test]
fn tc_162_root_switch_clears_old_scope_before_new_index_and_preserves_query_options_history() {
    let mut state = TuiState::new("keep query");
    state.root = PathBuf::from("old-root");
    state.history_enabled = true;
    state.history_entries = vec!["history".to_string()];
    state.runtime_options.regex = true;
    state.results = Arc::new(vec![(PathBuf::from("old-root/current.txt"), 1.0)]);
    state.pinned.push(PathBuf::from("old-root/pinned.txt"));
    state.preview = "old preview".to_string();
    state.active_search_request_id = Some(5);
    let freshness = TuiActionFreshness::new();
    freshness.activate(7, Path::new("old-root"));
    state.active_action_request = Some((7, PathBuf::from("old-root/current.txt")));

    state.prepare_root_switch(&freshness, PathBuf::from("new-root"));
    state.next_index_request(state.root.clone());

    assert_eq!(state.root, PathBuf::from("new-root"));
    assert!(state.results.is_empty());
    assert!(state.pinned.is_empty());
    assert!(state.preview.is_empty());
    assert!(state.active_search_request_id.is_none());
    assert!(state.active_action_request.is_none());
    assert_eq!(state.query, "keep query");
    assert!(state.runtime_options.regex);
    assert_eq!(state.history_entries, vec!["history"]);
    assert!(!freshness.is_current(7, Path::new("old-root")));
}

#[test]
fn tc_162_root_picker_selects_the_highlighted_root_and_refresh_keeps_pins() {
    let mut state = TuiState::new("");
    state.root = PathBuf::from("old-root");
    state.saved_roots = vec![PathBuf::from("first"), PathBuf::from("second")];
    state.root_picker = Some(RootPicker { selected: 1 });
    assert!(matches!(
        handle_root_picker_key(&mut state, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        KeyAction::SwitchRoot(ref root) if root == Path::new("second")
    ));
    state.pinned.push(PathBuf::from("old-root/pinned.txt"));
    state.results = Arc::new(vec![(PathBuf::from("old-root/current.txt"), 1.0)]);
    state.next_index_request(state.root.clone());
    assert_eq!(state.pinned, vec![PathBuf::from("old-root/pinned.txt")]);
    assert!(state.results.is_empty());
}

#[test]
fn tc_162_options_overlay_keeps_headings_and_renders_items_below_them() {
    let overlay = OptionsOverlay {
        draft: TuiRuntimeOptions::from_startup(&CliTuiOptions {
            initial_query: String::new(),
            limit: 1,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            absolute: false,
            print0: false,
            include_files: true,
            include_dirs: true,
            use_filelist: true,
            require_filelist: false,
            regex: false,
            ignore_case: false,
            ignore_enabled: true,
            ignore_terms: Vec::new(),
            sort_mode: SearchSortMode::Score,
            color_enabled: true,
        }),
        selected: 0,
    };
    let mut output = Vec::new();
    render_options_overlay(&mut output, &overlay, true, 80, 5).expect("render options");
    let rendered = String::from_utf8_lossy(&output);
    assert!(rendered.contains("Options"));
    assert!(rendered.contains("Enter apply"));
    assert!(rendered.contains("\x1b[3;1H> Files:"), "{rendered:?}");
}

#[test]
fn tc_162_paste_is_confined_to_history_and_never_leaks_through_modal_overlays() {
    let mut state = TuiState::new("query");
    state.options_overlay = Some(OptionsOverlay {
        draft: state.runtime_options,
        selected: 0,
    });
    insert_paste(&mut state, " leaked");
    assert_eq!(state.query, "query");
    state.options_overlay = None;
    state.sort_picker = Some(SortPicker { selected: 0 });
    insert_paste(&mut state, " leaked");
    assert_eq!(state.query, "query");
    state.sort_picker = None;
    state.root_picker = Some(RootPicker { selected: 0 });
    insert_paste(&mut state, " leaked");
    assert_eq!(state.query, "query");
    state.root_picker = None;

    state.history_enabled = true;
    state.history_entries = vec!["history".to_string()];
    state.begin_history();
    insert_paste(&mut state, "hi");
    assert_eq!(state.history.as_ref().expect("history").filter, "hi");
}

#[test]
fn tc_162_root_switch_and_refresh_reset_sort_and_pending_search() {
    let freshness = TuiActionFreshness::new();
    let mut state = TuiState::new("");
    state.root = PathBuf::from("old-root");
    state.sort_mode = SearchSortMode::SizeDesc;
    state.active_search_request_id = Some(4);
    state.prepare_root_switch(&freshness, PathBuf::from("new-root"));
    assert_eq!(state.sort_mode, SearchSortMode::Score);
    assert!(state.active_search_request_id.is_none());

    state.sort_mode = SearchSortMode::NameDesc;
    state.active_search_request_id = Some(5);
    state.prepare_refresh();
    state.next_index_request(state.root.clone());
    assert_eq!(state.sort_mode, SearchSortMode::Score);
    assert!(state.active_search_request_id.is_none());
}

#[test]
fn tc_162_active_root_relative_output_is_prepared_after_terminal_cleanup() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let guard = TerminalGuard::start(
        FakeTerminalOps {
            calls: Rc::clone(&calls),
            fail_on: None,
        },
        Vec::<u8>::new(),
    )
    .expect("terminal setup");
    let active_root = PathBuf::from("active-root");
    let path = active_root.join("selected.txt");
    let selected = run_terminal_operation(guard, |_writer| Ok((path, active_root.clone())))
        .expect("terminal operation");
    calls.borrow_mut().push("stdout_output");
    assert_eq!(
        output_path_bytes(&selected.0, &selected.1, true, false),
        b"selected.txt"
    );
    let disable_raw = calls
        .borrow()
        .iter()
        .position(|call| *call == "disable_raw")
        .expect("raw cleanup");
    let stdout_output = calls
        .borrow()
        .iter()
        .position(|call| *call == "stdout_output")
        .expect("stdout output");
    assert!(disable_raw < stdout_output);
}

#[test]
fn tc_162_small_overlays_keep_source_and_size_selection_visible() {
    assert_eq!(overlay_window_start(5, 6, 2), 4);
    assert_eq!(overlay_window_start(8, 9, 2), 7);
    let options = OptionsOverlay {
        draft: TuiRuntimeOptions::from_startup(&CliTuiOptions {
            initial_query: String::new(),
            limit: 1,
            max_depth: crate::indexer::MaxDepth::unlimited(),
            absolute: false,
            print0: false,
            include_files: true,
            include_dirs: true,
            use_filelist: true,
            require_filelist: false,
            regex: false,
            ignore_case: false,
            ignore_enabled: true,
            ignore_terms: Vec::new(),
            sort_mode: SearchSortMode::Score,
            color_enabled: true,
        }),
        selected: 5,
    };
    let mut output = Vec::new();
    render_options_overlay(&mut output, &options, true, 80, 4).expect("render options");
    assert!(String::from_utf8_lossy(&output).contains("> Source:"));
    let mut output = Vec::new();
    render_sort_picker(&mut output, &SortPicker { selected: 8 }, true, 80, 4).expect("render sort");
    assert!(String::from_utf8_lossy(&output).contains("> Size (Small)"));
}

#[derive(Default)]
struct RecordingTuiActionBackend {
    calls: Mutex<Vec<(AuthorizedActionMode, PathBuf)>>,
    fail: bool,
}

impl AuthorizedActionBackend for RecordingTuiActionBackend {
    fn execute_or_open(&self, path: &Path) -> Result<()> {
        self.calls
            .lock()
            .expect("record action")
            .push((AuthorizedActionMode::ExecuteOrOpen, path.to_path_buf()));
        if self.fail {
            anyhow::bail!("raw executor path and failure detail")
        }
        Ok(())
    }

    fn reveal(&self, path: &Path) -> Result<()> {
        self.calls
            .lock()
            .expect("record action")
            .push((AuthorizedActionMode::Reveal, path.to_path_buf()));
        if self.fail {
            anyhow::bail!("raw executor path and failure detail")
        }
        Ok(())
    }
}

fn action_fixture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "flistwalker-tui-action-{name}-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    std::fs::create_dir_all(root.join("folder")).expect("create action fixture");
    let current = root.join("current.txt");
    let pinned = root.join("folder").join("pinned.txt");
    std::fs::write(&current, "current").expect("write current");
    std::fs::write(&pinned, "pinned").expect("write pinned");
    (root, current, pinned)
}

#[test]
fn tc_164_tui_actions_snapshot_only_the_current_row_not_pins() {
    let (root, current, pinned) = action_fixture("current-only");
    let mut state = TuiState::new("");
    state.root = root.clone();
    state.results = Arc::new(vec![(current.clone(), 1.0)]);
    state.pinned.push(pinned.clone());
    let freshness = TuiActionFreshness::new();
    let request = state
        .next_action_request(
            AuthorizedActionMode::ExecuteOrOpen,
            &freshness,
            Arc::new(AtomicBool::new(false)),
        )
        .expect("action request");
    assert_eq!(request.request.selected_targets, vec![current.clone()]);

    let backend = RecordingTuiActionBackend::default();
    let report = execute_authorized_action_request(&request.request, &freshness, &backend);
    assert_eq!(report.outcome, AuthorizedActionOutcome::Completed);
    let calls = backend.calls.lock().expect("calls");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, AuthorizedActionMode::ExecuteOrOpen);
    assert!(calls[0].1.ends_with("current.txt"));
    drop(calls);
    assert!(!backend
        .calls
        .lock()
        .expect("calls")
        .iter()
        .any(|(_, path)| path == &pinned));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tc_164_tui_reveal_is_current_only_and_preauthorization_blocks_zero_calls() {
    let (root, current, _) = action_fixture("reveal-and-block");
    let freshness = TuiActionFreshness::new();
    freshness.activate(1, &root);
    let reveal = AuthorizedActionRequest::new_with_cancellation(
        1,
        root.clone(),
        vec![current.clone()],
        AuthorizedActionMode::Reveal,
        Arc::new(AtomicBool::new(false)),
    );
    let backend = RecordingTuiActionBackend::default();
    let report = execute_authorized_action_request(&reveal, &freshness, &backend);
    assert_eq!(report.outcome, AuthorizedActionOutcome::Completed);
    let calls = backend.calls.lock().expect("calls");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, AuthorizedActionMode::Reveal);
    assert!(calls[0]
        .1
        .ends_with(root.file_name().expect("fixture root name")));

    freshness.activate(2, &root);
    let outside = root
        .parent()
        .expect("fixture parent")
        .join("outside-action.txt");
    std::fs::write(&outside, "outside").expect("write outside");
    let blocked = AuthorizedActionRequest::new_with_cancellation(
        2,
        root.clone(),
        vec![outside.clone()],
        AuthorizedActionMode::ExecuteOrOpen,
        Arc::new(AtomicBool::new(false)),
    );
    let blocked_backend = RecordingTuiActionBackend::default();
    let report = execute_authorized_action_request(&blocked, &freshness, &blocked_backend);
    assert_eq!(report.outcome, AuthorizedActionOutcome::Blocked);
    assert!(blocked_backend.calls.lock().expect("calls").is_empty());
    let _ = std::fs::remove_file(outside);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tc_164_tui_action_stale_cancel_and_executor_errors_are_safe() {
    let (root, current, _) = action_fixture("stale-cancel-error");
    let freshness = TuiActionFreshness::new();
    freshness.activate(1, &root);
    let cancellation = Arc::new(AtomicBool::new(false));
    let request = AuthorizedActionRequest::new_with_cancellation(
        1,
        root.clone(),
        vec![current.clone()],
        AuthorizedActionMode::ExecuteOrOpen,
        Arc::clone(&cancellation),
    );
    freshness.activate(2, &root);
    let backend = RecordingTuiActionBackend::default();
    let report = execute_authorized_action_request(&request, &freshness, &backend);
    assert_eq!(report.outcome, AuthorizedActionOutcome::Superseded);
    assert!(backend.calls.lock().expect("calls").is_empty());

    freshness.activate(3, &root);
    cancellation.store(true, Ordering::Release);
    let canceled = AuthorizedActionRequest::new_with_cancellation(
        3,
        root.clone(),
        vec![current.clone()],
        AuthorizedActionMode::ExecuteOrOpen,
        Arc::clone(&cancellation),
    );
    let report = execute_authorized_action_request(&canceled, &freshness, &backend);
    assert_eq!(report.outcome, AuthorizedActionOutcome::Canceled);
    assert!(backend.calls.lock().expect("calls").is_empty());

    let failing_backend = RecordingTuiActionBackend {
        calls: Mutex::default(),
        fail: true,
    };
    let active = AuthorizedActionRequest::new_with_cancellation(
        4,
        root.clone(),
        vec![current.clone()],
        AuthorizedActionMode::ExecuteOrOpen,
        Arc::new(AtomicBool::new(false)),
    );
    freshness.activate(4, &root);
    let report = execute_authorized_action_request(&active, &freshness, &failing_backend);
    assert_eq!(report.outcome, AuthorizedActionOutcome::Failed);
    assert_eq!(tui_action_status(&report), "Action failed: executor failed");
    assert!(!tui_action_status(&report).contains("raw executor"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tc_162_tui_action_keys_are_current_only_and_disabled_in_overlays() {
    let mut state = TuiState::new("");
    state.results = Arc::new(vec![(PathBuf::from("current.txt"), 1.0)]);
    state.pinned.push(PathBuf::from("pinned.txt"));
    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)
        ),
        KeyAction::DispatchAction(AuthorizedActionMode::ExecuteOrOpen)
    ));
    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)
        ),
        KeyAction::DispatchAction(AuthorizedActionMode::Reveal)
    ));

    state.history_enabled = true;
    handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    );
    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)
        ),
        KeyAction::Continue
    ));
    state.open_help();
    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)
        ),
        KeyAction::Continue
    ));
}

#[test]
fn tc_162_history_overlay_renderer_clips_control_text() {
    let mut history = HistoryOverlay {
        draft_query: String::new(),
        filter: "\u{1b}x".to_string(),
        filter_cursor: 2,
        results: vec!["界\u{1b}x".to_string()],
        selected: 0,
        offset: 0,
    };
    refresh_history_results(&mut history, &["界\u{1b}x".to_string()]);
    let mut output = Vec::new();
    render_history_overlay(&mut output, &history, true, 12, 6, true)
        .expect("render history overlay");
    let rendered = String::from_utf8_lossy(&output);
    assert!(rendered.contains("History"));
    assert!(rendered.contains('�'));
    assert!(!rendered.contains("\u{1b}x"));
}

#[test]
fn tc_162_tui_frame_is_wrapped_in_synchronized_terminal_update() {
    let mut output = Vec::new();

    write_synchronized_frame(&mut output, b"frame").expect("write synchronized frame");

    let rendered = String::from_utf8(output).expect("terminal output is UTF-8");
    let begin = rendered
        .find("\x1b[?2026h")
        .expect("begin synchronized update");
    let frame = rendered.find("frame").expect("frame payload");
    let end = rendered
        .find("\x1b[?2026l")
        .expect("end synchronized update");
    assert!(begin < frame && frame < end, "{rendered:?}");
}

#[test]
fn tc_172_color_never_omits_highlight_escape_sequences() {
    force_tui_color_output(true);
    let positions = [0].into_iter().collect();
    let mut colored = Vec::new();
    print_highlighted(&mut colored, 0, "> ", "match", &positions, 20, true)
        .expect("render colored match");
    let colored = String::from_utf8(colored).expect("colored frame is UTF-8");
    assert!(colored.contains("\x1b[38;"), "{colored:?}");

    let mut plain = Vec::new();
    print_highlighted(&mut plain, 0, "> ", "match", &positions, 20, false)
        .expect("render plain match");
    let plain = String::from_utf8(plain).expect("plain frame is UTF-8");
    assert!(!plain.contains("\x1b[38;"), "{plain:?}");
    assert!(!plain.contains("\x1b[0m"), "{plain:?}");
}

#[test]
fn tc_162_tty_policy_requires_stdin_and_stderr_only() {
    assert!(interactive_terminal_supported(true, true));
    assert!(!interactive_terminal_supported(false, true));
    assert!(!interactive_terminal_supported(true, false));
}

#[test]
fn tc_162_terminal_guard_restores_only_successful_setup_steps() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let result = TerminalGuard::start(
        FakeTerminalOps {
            calls: Rc::clone(&calls),
            fail_on: Some("hide_cursor"),
        },
        Vec::<u8>::new(),
    );

    assert!(result.is_err());
    assert_eq!(
        calls.borrow().as_slice(),
        [
            "enable_raw",
            "enter_alternate",
            "hide_cursor",
            "leave_alternate",
            "disable_raw",
        ]
    );
}

#[test]
fn tc_162_terminal_guard_restores_in_reverse_order_during_unwind() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let unwind_calls = Rc::clone(&calls);
    let result = catch_unwind(AssertUnwindSafe(move || {
        let _guard = TerminalGuard::start(
            FakeTerminalOps {
                calls: unwind_calls,
                fail_on: None,
            },
            Vec::<u8>::new(),
        )
        .expect("terminal setup");
        panic!("simulated runtime failure");
    }));

    assert!(result.is_err());
    assert_eq!(
        calls.borrow().as_slice(),
        [
            "enable_raw",
            "enter_alternate",
            "hide_cursor",
            "enable_paste",
            "disable_paste",
            "show_cursor",
            "leave_alternate",
            "disable_raw",
        ]
    );
}

#[test]
fn tc_162_runtime_error_restores_terminal_before_propagation() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let guard = TerminalGuard::start(
        FakeTerminalOps {
            calls: Rc::clone(&calls),
            fail_on: None,
        },
        Vec::<u8>::new(),
    )
    .expect("terminal setup");

    let result: Result<()> =
        run_terminal_operation(guard, |_writer| anyhow::bail!("simulated draw/read error"));

    assert!(result.is_err());
    assert_eq!(
        calls.borrow().as_slice(),
        [
            "enable_raw",
            "enter_alternate",
            "hide_cursor",
            "enable_paste",
            "disable_paste",
            "show_cursor",
            "leave_alternate",
            "disable_raw",
        ]
    );
}

#[test]
fn tc_162_selected_output_is_emitted_only_after_terminal_cleanup() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let guard = TerminalGuard::start(
        FakeTerminalOps {
            calls: Rc::clone(&calls),
            fail_on: None,
        },
        Vec::<u8>::new(),
    )
    .expect("terminal setup");

    let selected = run_terminal_operation(guard, |_writer| Ok(vec![PathBuf::from("selected.txt")]))
        .expect("terminal operation");
    calls.borrow_mut().push("stdout_output");

    assert_eq!(selected, vec![PathBuf::from("selected.txt")]);
    assert_eq!(calls.borrow().last(), Some(&"stdout_output"));
    let disable_raw = calls
        .borrow()
        .iter()
        .position(|call| *call == "disable_raw")
        .expect("raw cleanup");
    let stdout_output = calls
        .borrow()
        .iter()
        .position(|call| *call == "stdout_output")
        .expect("stdout output");
    assert!(disable_raw < stdout_output);
}

#[test]
fn tc_166_filelist_confirmation_requires_explicit_scope_and_overwrite_consent() {
    let mut state = TuiState::new("draft");
    state.root = PathBuf::from("fixture-root");
    state.root_filelist_known = true;
    state.root_filelist_exists = true;

    assert!(matches!(
        handle_key(&mut state, KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)),
        KeyAction::OpenFileList
    ));
    state.open_filelist_confirmation();
    insert_paste(&mut state, " must-not-leak");
    assert_eq!(state.query, "draft");
    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        ),
        KeyAction::Continue
    ));
    assert!(matches!(
        state.filelist_confirmation,
        Some(FileListConfirmation::Overwrite { .. })
    ));
    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        ),
        KeyAction::StartFileList {
            propagate_to_ancestors: false,
            allow_root_overwrite: true,
        }
    ));
    assert!(state.filelist_confirmation.is_none());
}

#[test]
fn tc_166_filelist_uses_fresh_walker_snapshot_not_partial_tui_entries() {
    let temp = TestTempDir::new("filelist-fresh-walker");
    let nested = temp.path.join("nested");
    fs::create_dir_all(&nested).expect("create nested directory");
    fs::write(temp.path.join("visible.txt"), "visible").expect("write visible file");
    fs::write(nested.join("inside.txt"), "inside").expect("write nested file");
    fs::write(temp.path.join("FileList.txt"), "stale-entry\n").expect("write old FileList");

    let mut state = TuiState::new("");
    state.root = temp.path.clone();
    state.runtime_options.include_files = true;
    state.runtime_options.include_dirs = false;
    state.entries.push(vec![temp.path.join("visible.txt")]);
    let request = state.next_filelist_request(false, true);

    let entries = build_filelist_snapshot(&request.root, &|| false).expect("fresh walker snapshot");
    assert!(entries.iter().any(|entry| entry.ends_with("visible.txt")));
    assert!(entries.iter().any(|entry| entry.ends_with("nested")));
    assert!(entries.iter().any(|entry| entry.ends_with("inside.txt")));
    assert!(
        !entries.iter().any(|entry| entry.ends_with("FileList.txt")),
        "the root FileList must not list itself"
    );

    let plan = plan_filelist_write_cancellable(
        &request.root,
        &entries,
        FileListWriteOptions {
            allow_root_overwrite: request.allow_root_overwrite,
            propagate_to_ancestors: request.propagate_to_ancestors,
        },
        &|| false,
    )
    .expect("write plan");
    let report = execute_filelist_write_plan(&plan, &|| false);
    assert_eq!(report.status, FileListWriteStatus::Completed);
    let text = fs::read_to_string(temp.path.join("FileList.txt")).expect("read FileList");
    assert!(text.contains("visible.txt"));
    assert!(text.contains("nested"));
    assert!(text.contains("inside.txt"));
    assert!(!text.contains("FileList.txt"));
}

#[test]
fn tc_166_filelist_fresh_walk_cancellation_is_a_clean_report() {
    let temp = TestTempDir::new("filelist-fresh-walk-cancel");
    fs::write(temp.path.join("candidate.txt"), "candidate").expect("write candidate");
    let cancelled = AtomicBool::new(true);

    let report = build_filelist_snapshot(&temp.path, &|| cancelled.load(Ordering::Acquire))
        .expect_err("cancelled walk must not reach planning");

    assert_eq!(report.status, FileListWriteStatus::Canceled);
    assert_eq!(report.exit_code(), 130);
    assert!(report.committed.is_empty());
    assert!(report.failed.is_empty());
    assert!(!temp.path.join("FileList.txt").exists());
}

#[test]
fn tc_166_filelist_requires_completed_index_and_intent_priority_is_sticky() {
    let mut state = TuiState::new("");
    let discovery = state
        .open_filelist_if_ready()
        .expect("unknown existence starts lazy discovery");
    assert!(state.filelist_confirmation.is_none());
    assert_eq!(state.status, "Checking FileList...");
    settle_filelist_discovery_for_test(
        &mut state,
        discovery.request_id,
        &discovery.root,
        FileListDiscoverySettlement::Canceled,
    );
    assert!(state.active_filelist.is_none());
    assert!(!state.root_filelist_known);
    state.root_filelist_known = true;
    assert!(state.open_filelist_if_ready().is_none());
    assert!(state.filelist_confirmation.is_some());
    state.filelist_confirmation = None;
    let request = state.next_filelist_request(false, false);
    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)
        ),
        KeyAction::Cancel
    ));
    state.record_filelist_intent(PendingFileListIntent::SelectOutput);
    assert_eq!(
        state.pending_filelist_intent,
        Some(PendingFileListIntent::SelectOutput)
    );
    state.record_filelist_intent(PendingFileListIntent::SwitchRoot(PathBuf::from("first")));
    assert_eq!(
        state.pending_filelist_intent,
        Some(PendingFileListIntent::SwitchRoot(PathBuf::from("first")))
    );
    state.record_filelist_intent(PendingFileListIntent::SwitchRoot(PathBuf::from("latest")));
    assert_eq!(
        state.pending_filelist_intent,
        Some(PendingFileListIntent::SwitchRoot(PathBuf::from("latest")))
    );
    state.record_filelist_intent(PendingFileListIntent::CancelExit);
    assert_eq!(
        state.pending_filelist_intent,
        Some(PendingFileListIntent::CancelExit)
    );
    state.record_filelist_intent(PendingFileListIntent::SwitchRoot(PathBuf::from("ignored")));
    state.record_filelist_intent(PendingFileListIntent::SelectOutput);
    assert_eq!(
        state.pending_filelist_intent,
        Some(PendingFileListIntent::CancelExit)
    );
    assert!(request.cancel.load(Ordering::Acquire));
}

#[test]
fn tc_166_walker_f6_lazy_discovery_confirms_before_snapshot_regression() {
    let temp = TestTempDir::new("walker-f6-lazy-discovery");
    let filelist = temp.path.join("FileList.txt");
    fs::write(&filelist, "kept.txt\n").expect("write existing FileList");
    let mut state = TuiState::new("");
    state.root = temp.path.clone();
    state.runtime_options.source = TuiSource::Walker;

    let request = state
        .open_filelist_if_ready()
        .expect("unknown Walker state starts discovery");
    let worker = spawn_filelist_discovery_worker(request).expect("spawn discovery");
    let result = worker.result.recv().expect("discovery result");
    worker.join();
    let FileListWorkerResult::DiscoveryFinished {
        request_id,
        root,
        discovered,
        canceled,
    } = result
    else {
        panic!("expected discovery result");
    };
    let settlement = if canceled {
        FileListDiscoverySettlement::Canceled
    } else {
        FileListDiscoverySettlement::Completed(discovered)
    };
    settle_filelist_discovery_for_test(&mut state, request_id, &root, settlement);
    assert!(state.root_filelist_exists);
    assert!(matches!(
        state.filelist_confirmation,
        Some(FileListConfirmation::Mode { .. })
    ));
    assert_eq!(
        fs::read_to_string(&filelist).expect("existing FileList remains untouched"),
        "kept.txt\n"
    );
    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        ),
        KeyAction::Continue
    ));
    assert!(matches!(
        state.filelist_confirmation,
        Some(FileListConfirmation::Overwrite { .. })
    ));
}

#[test]
fn tc_166_walker_f6_absent_and_canceled_discovery_settle_regression() {
    let temp = TestTempDir::new("walker-f6-lazy-absent");
    let mut state = TuiState::new("");
    state.root = temp.path.clone();
    state.runtime_options.source = TuiSource::Walker;
    let request = state.open_filelist_if_ready().expect("start discovery");
    settle_filelist_discovery_for_test(
        &mut state,
        request.request_id,
        &request.root,
        FileListDiscoverySettlement::Completed(None),
    );
    assert!(state.root_filelist_known);
    assert!(!state.root_filelist_exists);
    assert!(matches!(
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        ),
        KeyAction::StartFileList {
            allow_root_overwrite: false,
            ..
        }
    ));

    state.root_filelist_known = false;
    let request = state.open_filelist_if_ready().expect("restart discovery");
    state.cancel_active_filelist();
    assert!(request.cancel.load(Ordering::Acquire));
    let worker = spawn_filelist_discovery_worker(request).expect("spawn canceled discovery");
    let result = worker.result.recv().expect("canceled discovery result");
    worker.join();
    let FileListWorkerResult::DiscoveryFinished {
        request_id,
        root,
        discovered,
        canceled,
    } = result
    else {
        panic!("expected canceled discovery result");
    };
    assert!(canceled);
    let settlement = if canceled {
        FileListDiscoverySettlement::Canceled
    } else {
        FileListDiscoverySettlement::Completed(discovered)
    };
    settle_filelist_discovery_for_test(&mut state, request_id, &root, settlement);
    assert!(state.active_filelist.is_none());
    assert!(!state.root_filelist_known);
    assert_eq!(state.status, "FileList check canceled");
}

#[test]
fn tc_166_f4_root_switch_intent_settles_discovery_once_for_all_outcomes_regression() {
    for outcome in ["success", "cancel", "failure"] {
        let mut state = TuiState::new("");
        state.root = PathBuf::from("before");
        let next_root = PathBuf::from(format!("after-{outcome}"));
        state.saved_roots = vec![next_root.clone()];
        let discovery = state.open_filelist_if_ready().expect("start discovery");

        assert!(matches!(
            handle_key(&mut state, KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE)),
            KeyAction::Continue
        ));
        assert!(state.root_picker.is_some());
        let KeyAction::SwitchRoot(selected_root) = handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        ) else {
            panic!("root picker must select a root");
        };
        assert_eq!(selected_root, next_root);
        state.record_filelist_intent(PendingFileListIntent::SwitchRoot(selected_root));
        assert!(discovery.cancel.load(Ordering::Acquire));

        let settlement = match outcome {
            "success" => {
                FileListDiscoverySettlement::Completed(Some(discovery.root.join("FileList.txt")))
            }
            "cancel" => FileListDiscoverySettlement::Canceled,
            _ => FileListDiscoverySettlement::Failed("injected failure".to_string()),
        };
        let (index_tx, index_rx) = mpsc::channel();
        let exit = settle_filelist_discovery(
            &mut state,
            discovery.request_id,
            &discovery.root,
            settlement,
            &index_tx,
            &TuiIndexFreshness::new(),
            &TuiActionFreshness::new(),
        );
        assert!(exit.is_none());
        assert_eq!(state.root, next_root);
        assert_eq!(index_rx.try_recv().expect("root reindex").root, next_root);
        assert!(index_rx.try_recv().is_err());
        assert!(state.active_filelist.is_none());
        assert!(state.pending_filelist_intent.is_none());
        assert!(state.filelist_confirmation.is_none());

        // A later creation settlement must not replay the consumed root switch.
        assert!(settle_filelist(
            &mut state,
            FileListSettlement::Canceled,
            &index_tx,
            &TuiIndexFreshness::new(),
            &TuiActionFreshness::new(),
        )
        .is_none());
        assert_eq!(state.root, next_root);
        assert!(index_rx.try_recv().is_err());
    }
}

#[test]
fn tc_166_select_output_intent_never_leaks_past_discovery_regression() {
    for outcome in ["success", "cancel", "failure"] {
        let mut state = TuiState::new("query");
        state.root = PathBuf::from("root");
        state.results = Arc::new(vec![(PathBuf::from("picked.txt"), 1.0)]);
        let discovery = state.open_filelist_if_ready().expect("start discovery");
        assert!(matches!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
            ),
            KeyAction::Select
        ));
        state.record_filelist_intent(PendingFileListIntent::SelectOutput);

        let settlement = match outcome {
            "success" => FileListDiscoverySettlement::Completed(None),
            "cancel" => FileListDiscoverySettlement::Canceled,
            _ => FileListDiscoverySettlement::Failed("injected failure".to_string()),
        };
        let selected = settle_filelist_discovery_for_test(
            &mut state,
            discovery.request_id,
            &discovery.root,
            settlement,
        );
        assert!(matches!(
            selected,
            Some(TuiExit::Selected { paths, .. }) if paths == vec![PathBuf::from("picked.txt")]
        ));
        assert!(state.pending_filelist_intent.is_none());
        assert!(state.active_filelist.is_none());
        assert!(state.filelist_confirmation.is_none());

        let (index_tx, _index_rx) = mpsc::channel::<IndexRequest>();
        assert!(settle_filelist(
            &mut state,
            FileListSettlement::Canceled,
            &index_tx,
            &TuiIndexFreshness::new(),
            &TuiActionFreshness::new(),
        )
        .is_none());
    }
}

#[test]
fn tc_166_stale_discovery_response_preserves_new_request_and_intent_regression() {
    let mut state = TuiState::new("");
    state.root = PathBuf::from("first");
    let stale = state.open_filelist_if_ready().expect("first discovery");
    state.active_filelist = None;
    let current = state
        .open_filelist_if_ready()
        .expect("replacement discovery");
    let next_root = PathBuf::from("next");
    state.record_filelist_intent(PendingFileListIntent::SwitchRoot(next_root.clone()));

    assert!(settle_filelist_discovery_for_test(
        &mut state,
        stale.request_id,
        &stale.root,
        FileListDiscoverySettlement::Canceled,
    )
    .is_none());
    assert_eq!(
        state
            .active_filelist
            .as_ref()
            .map(|active| active.request_id),
        Some(current.request_id)
    );
    assert_eq!(
        state.pending_filelist_intent,
        Some(PendingFileListIntent::SwitchRoot(next_root.clone()))
    );

    let (index_tx, index_rx) = mpsc::channel();
    assert!(settle_filelist_discovery(
        &mut state,
        current.request_id,
        &current.root,
        FileListDiscoverySettlement::Canceled,
        &index_tx,
        &TuiIndexFreshness::new(),
        &TuiActionFreshness::new(),
    )
    .is_none());
    assert_eq!(state.root, next_root);
    assert_eq!(
        index_rx.try_recv().expect("replacement root request").root,
        next_root
    );
    assert!(state.pending_filelist_intent.is_none());
}

#[test]
fn tc_166_filelist_failure_does_not_resume_select_or_root_but_cancel_exits_one() {
    let (index_tx, _index_rx) = mpsc::channel();
    let freshness = TuiIndexFreshness::new();

    let actions = TuiActionFreshness::new();
    let mut state = TuiState::new("");
    state.pending_filelist_intent = Some(PendingFileListIntent::SelectOutput);
    assert!(settle_filelist(
        &mut state,
        FileListSettlement::Failed("rollback failed".to_string()),
        &index_tx,
        &freshness,
        &actions,
    )
    .is_none());
    state.pending_filelist_intent = Some(PendingFileListIntent::CancelExit);
    assert!(matches!(
        settle_filelist(
            &mut state,
            FileListSettlement::Failed("rollback failed".to_string()),
            &index_tx,
            &freshness,
            &actions,
        ),
        Some(TuiExit::Failed(_))
    ));

    state.root = PathBuf::from("before");
    state.pending_filelist_intent = Some(PendingFileListIntent::SwitchRoot(PathBuf::from("after")));
    assert!(settle_filelist(
        &mut state,
        FileListSettlement::Failed("rollback failed".to_string()),
        &index_tx,
        &freshness,
        &actions,
    )
    .is_none());
    assert_eq!(state.root, PathBuf::from("before"));
}

#[test]
fn tc_166_filelist_worker_join_never_detaches_a_delayed_transaction() {
    let temp = TestTempDir::new("filelist-join");
    let marker = temp.path.join("FileList.txt");
    let (result_tx, result_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let finished = Arc::new(AtomicBool::new(false));
    let worker_finished = Arc::clone(&finished);
    let handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        fs::write(&marker, "committed\n").expect("write delayed FileList marker");
        worker_finished.store(true, Ordering::Release);
        let _ = result_tx.send(FileListWorkerResult::Failed {
            request_id: 1,
            root: PathBuf::from("fixture"),
            error: "injected missing response path".to_string(),
        });
        let _ = done_tx.send(());
    });
    let worker = ActiveFileListWorker {
        cancel: Arc::new(AtomicBool::new(false)),
        result: result_rx,
        done: done_rx,
        handle: Some(handle),
    };
    let started = Instant::now();
    worker.join();
    assert!(
        started.elapsed() >= Duration::from_millis(250),
        "FileList worker must not use the generic bounded-detach cleanup"
    );
    assert!(finished.load(Ordering::Acquire));
    let bytes_at_return = fs::read(temp.path.join("FileList.txt")).expect("read marker");
    thread::sleep(Duration::from_millis(80));
    assert_eq!(
        fs::read(temp.path.join("FileList.txt")).expect("read marker after return"),
        bytes_at_return,
        "no FileList write may occur after the transaction worker has been joined"
    );
}

#[test]
fn tc_166_filelist_missing_or_panicked_worker_never_resumes_success_intents() {
    let (result_tx, result_rx) = mpsc::channel::<FileListWorkerResult>();
    drop(result_tx);
    let (done_tx, done_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        drop(done_tx);
        panic!("injected FileList worker panic");
    });
    let worker = ActiveFileListWorker {
        cancel: Arc::new(AtomicBool::new(false)),
        result: result_rx,
        done: done_rx,
        handle: Some(handle),
    };
    while !worker.is_finished() {
        thread::yield_now();
    }
    assert!(matches!(
        worker.result.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
    worker.join();

    let (index_tx, _index_rx) = mpsc::channel();
    let freshness = TuiIndexFreshness::new();
    let actions = TuiActionFreshness::new();
    let mut state = TuiState::new("");
    state.pending_filelist_intent = Some(PendingFileListIntent::SelectOutput);
    assert!(settle_filelist(
        &mut state,
        FileListSettlement::Failed("FileList worker disconnected".to_string()),
        &index_tx,
        &freshness,
        &actions,
    )
    .is_none());
    assert!(state.status.contains("failed"));
}

#[test]
fn alignment_tui_help_remains_discoverable_in_a_narrow_viewport() {
    assert_eq!(normal_help_line(SearchSortMode::Score, 7), "F1 Help");
    assert!(normal_help_line(SearchSortMode::Score, 35).starts_with("F1 Help"));
}

#[test]
fn alignment_tui_preview_supersession_cancels_old_request() {
    let mut state = TuiState::new("");
    state.results = Arc::new(vec![
        (PathBuf::from("first"), 1.0),
        (PathBuf::from("second"), 0.0),
    ]);
    state.preview_visible = true;
    let first = state.next_preview_request().unwrap();
    state.selected = 1;
    let second = state.next_preview_request().unwrap();
    assert!(first.cancel.load(Ordering::Acquire));
    assert!(!second.cancel.load(Ordering::Acquire));
    state.clear_preview();
    assert!(second.cancel.load(Ordering::Acquire));
}

#[test]
fn alignment_real_tui_workers_guard_index_and_result_payloads_before_publication() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("flist-tui-guard-{nonce}"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("sample.txt"), "sample").unwrap();
    let workers = TuiWorkerSet::start().unwrap();
    let mut state = TuiState::new("");
    state.root = root.clone();
    state.runtime_options.source = TuiSource::Walker;
    state
        .dispatch_current_index(workers.index_tx(), workers.index_freshness().as_ref())
        .unwrap();
    loop {
        let response = workers
            .response_rx()
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        let finished = matches!(response, WorkerResponse::IndexedFinished { .. });
        if let WorkerResponse::IndexedSharedBatch { batch, .. } = &response {
            let owner = batch._owner.as_ref().expect("unguarded batch");
            assert!(
                Arc::strong_count(owner) >= 2,
                "recycler owner must exist before UI publication"
            );
            assert!(Arc::strong_count(&batch.entries) >= 2);
        }
        apply_worker_response(&mut state, response).unwrap();
        if finished {
            break;
        }
    }
    assert!(!state.entries.batches.is_empty());
    workers
        .search_tx()
        .send(state.next_search_request(root.clone(), 10))
        .unwrap();
    let response = workers
        .response_rx()
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    let WorkerResponse::Searched { results, .. } = &response else {
        panic!("expected search response")
    };
    assert!(Arc::strong_count(results) >= 2);
    apply_worker_response(&mut state, response).unwrap();
    assert_eq!(state.results.len(), 1);
    state.next_index_request(root.clone());
    assert!(state.results.is_empty());
    drop(state);
    workers.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}
