use super::*;
use crate::runtime_config::{set_process_runtime_config, DeveloperRuntimeConfig, RuntimeConfig};
use crate::walker_runtime::{
    adaptive_shared_frontier_soft_limit, classify_walker_entry,
    default_adaptive_max_limit_from_logical_cores, next_limit_from_throughput, resolve_entry_kind,
    walk_adaptive, walk_adaptive_filtered, walk_adaptive_filtered_deferred,
    walk_adaptive_filtered_unbounded, walk_adaptive_filtered_with_frontier_limits,
    walk_adaptive_filtered_with_frontier_limits_and_max_depth, walk_adaptive_with_max_depth,
    walker_runtime_settings, LimitDirection, WalkerBackend,
};
use std::sync::atomic::AtomicUsize;
use std::sync::Condvar;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing_subscriber::EnvFilter;

fn init_test_tracing() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off"));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .without_time()
            .with_test_writer()
            .try_init();
    });
}

fn test_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("flistwalker-workers-{name}-{nonce}"))
}

fn test_response_mailboxes() -> Arc<Mutex<HashMap<u64, Arc<IndexResponseMailbox>>>> {
    Arc::new(Mutex::new(HashMap::new()))
}

fn establish_prequeued_mailbox_invariant(
    mailboxes: &Arc<Mutex<HashMap<u64, Arc<IndexResponseMailbox>>>>,
    request_id: u64,
) {
    // The production coordinator registers request ownership and its mailbox before queueing.
    // Direct worker tests bypass that coordinator, so they establish the invariant explicitly.
    mailboxes
        .lock()
        .expect("mailboxes")
        .insert(request_id, Arc::new(IndexResponseMailbox::new()));
}

fn recv_index_response(
    mailboxes: &Arc<Mutex<HashMap<u64, Arc<IndexResponseMailbox>>>>,
    request_id: u64,
    timeout: Duration,
) -> IndexResponse {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(response) = mailboxes
            .lock()
            .ok()
            .and_then(|mailboxes| mailboxes.get(&request_id).cloned())
            .and_then(|mailbox| mailbox.try_recv())
        {
            return response;
        }
        assert!(Instant::now() < deadline, "index response timed out");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn tc_206_full_mailbox_publish_stops_when_generation_is_superseded() {
    let request_id = 41;
    let tab_id = 7;
    let mailbox = Arc::new(IndexResponseMailbox::with_data_capacity(1));
    mailbox
        .try_publish(IndexResponse::Batch {
            request_id,
            entries: Vec::new(),
        })
        .expect("fill mailbox data lane");
    let latest_request_ids = Arc::new(Mutex::new(HashMap::from([(tab_id, request_id)])));
    let sink = MailboxResponseSink {
        request_id,
        tab_id,
        mailbox,
        shutdown: Arc::new(AtomicBool::new(false)),
        latest_request_ids: Arc::clone(&latest_request_ids),
    };
    let handle = thread::spawn(move || {
        sink.send(IndexResponse::Batch {
            request_id,
            entries: Vec::new(),
        })
    });
    thread::sleep(Duration::from_millis(10));
    latest_request_ids
        .lock()
        .expect("latest requests")
        .insert(tab_id, request_id + 1);
    assert!(handle.join().expect("join blocked publisher").is_err());
}

#[test]
fn classify_walker_entry_keeps_regular_file_fast_path_known() {
    let root = test_root("file");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create dir");
    let path = root.join("main.rs");
    std::fs::write(&path, "fn main() {}").expect("write file");
    let file_type = std::fs::symlink_metadata(&path)
        .expect("metadata")
        .file_type();

    let classified =
        classify_walker_entry(&path, file_type, true, true).expect("classify walker entry");

    assert_eq!(classified, (EntryKind::file(), true));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn classify_walker_entry_defers_windows_shortcut_when_both_filters_enabled() {
    let root = test_root("shortcut");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create dir");
    let path = root.join("app.lnk");
    std::fs::write(&path, "shortcut").expect("write file");
    let file_type = std::fs::symlink_metadata(&path)
        .expect("metadata")
        .file_type();

    let classified =
        classify_walker_entry(&path, file_type, true, true).expect("classify walker entry");

    #[cfg(windows)]
    assert_eq!(classified, (EntryKind::link_unknown(), false));
    #[cfg(not(windows))]
    assert_eq!(classified, (EntryKind::file(), true));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn missing_path_resolves_to_terminal_other_kind() {
    let path = test_root("missing-terminal").join("missing-entry");

    assert_eq!(resolve_entry_kind(&path), Some(EntryKind::other()));
    assert!(!EntryKind::other().needs_resolution());
}

#[cfg(unix)]
#[test]
fn classify_walker_entry_marks_symlink_before_resolving_target_kind() {
    use std::os::unix::fs::symlink;

    let root = test_root("symlink-fast-path");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create dir");
    let target = root.join("target.txt");
    let path = root.join("target-link");
    std::fs::write(&target, "target").expect("write target");
    symlink(&target, &path).expect("create symlink");
    let file_type = std::fs::symlink_metadata(&path)
        .expect("metadata")
        .file_type();

    let classified =
        classify_walker_entry(&path, file_type, true, true).expect("classify walker entry");

    assert_eq!(classified, (EntryKind::link_unknown(), false));
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn classify_walker_entry_does_not_promote_unix_socket_to_link() {
    use std::os::unix::net::UnixListener;

    // AF_UNIX paths have a platform-specific short length limit (SUN_LEN on
    // macOS). Keep the socket path independent from the long test fixture
    // root used by the other worker tests.
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("flistwalker-socket-{nonce}"));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).expect("bind unix socket");
    let file_type = std::fs::symlink_metadata(&path)
        .expect("metadata")
        .file_type();

    assert!(classify_walker_entry(&path, file_type, true, true).is_none());

    drop(listener);
    let _ = std::fs::remove_file(&path);
}

#[cfg(unix)]
#[test]
fn broken_symlink_resolves_to_link_with_unknown_target_kind() {
    use std::os::unix::fs::symlink;

    let root = test_root("broken-symlink");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create dir");
    let path = root.join("broken-link");
    symlink(root.join("missing"), &path).expect("create broken symlink");

    assert_eq!(resolve_entry_kind(&path), Some(EntryKind::link_unknown()));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn walker_runtime_settings_use_adaptive_by_default() {
    let config = RuntimeConfig {
        walker_max_entries: 123,
        developer: DeveloperRuntimeConfig {
            walker_metrics: true,
            walker_metrics_log_path: "metrics.log".to_string(),
            walker_adaptive_initial_limit: Some(3),
            walker_adaptive_max_limit: Some(6),
        },
        ..RuntimeConfig::default()
    };

    let settings = walker_runtime_settings(&config);

    assert_eq!(settings.backend, WalkerBackend::Adaptive);
    assert_eq!(settings.adaptive_initial_limit, 3);
    assert_eq!(settings.adaptive_max_limit, 6);
    assert_eq!(settings.metrics_log_path, "metrics.log");
    assert_eq!(settings.max_entries, 123);
    assert!(settings.metrics_enabled);
}

#[test]
fn walker_runtime_settings_always_uses_adaptive_backend() {
    let config = RuntimeConfig {
        ..RuntimeConfig::default()
    };

    let settings = walker_runtime_settings(&config);

    assert_eq!(settings.backend, WalkerBackend::Adaptive);
}

#[test]
fn walker_metrics_summary_can_be_written_to_file() {
    let root = test_root("metrics-log");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    let log_path = root.join("walker-metrics.log");
    let req = IndexRequest {
        request_id: 7,
        tab_id: 3,
        root: root.clone(),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    };
    let mut metrics = WalkerMetrics::new(WalkerBackend::Adaptive);
    metrics.entries_emitted = 11;
    metrics.batches_sent = 2;
    metrics.dirs_read = 5;
    metrics.adaptive_limit_change_count = 7;
    metrics.adaptive_limit_avg = 2.25;
    metrics.child_dir_publish_batches = 3;
    metrics.max_queued_dirs = 17;
    metrics.shared_frontier_soft_limit = 256;
    metrics.frontier_saturation_fallbacks = 4;
    metrics.frontier_soft_limit_bypasses = 2;
    metrics.open_directory_frame_budget = 64;
    metrics.max_open_directory_frames = 7;

    let summary = walker_metrics_summary(&req, &metrics, "finished");
    write_walker_metrics_summary(&summary, &log_path.to_string_lossy());

    let text = std::fs::read_to_string(&log_path).expect("read metrics log");
    assert!(text.contains("event=metrics"));
    assert!(text.contains("backend=adaptive"));
    assert!(text.contains("entries_emitted=11"));
    assert!(text.contains("adaptive_limit_avg=2.250"));
    assert!(text.contains("adaptive_limit_change_count=7"));
    assert!(text.contains("child_dir_publish_batches=3"));
    assert!(text.contains("max_queued_dirs=17"));
    assert!(text.contains("shared_frontier_soft_limit=256"));
    assert!(text.contains("frontier_saturation_fallbacks=4"));
    assert!(text.contains("frontier_soft_limit_bypasses=2"));
    assert!(text.contains("open_directory_frame_budget=64"));
    assert!(text.contains("max_open_directory_frames=7"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn walker_runtime_settings_uses_explicit_adaptive_limits_without_walker_threads_clamp() {
    let config = RuntimeConfig {
        developer: DeveloperRuntimeConfig {
            walker_adaptive_initial_limit: Some(9),
            walker_adaptive_max_limit: Some(99),
            ..DeveloperRuntimeConfig::default()
        },
        ..RuntimeConfig::default()
    };

    let settings = walker_runtime_settings(&config);

    assert_eq!(settings.adaptive_max_limit, 99);
    assert_eq!(settings.adaptive_initial_limit, 9);
}

#[test]
fn walker_runtime_settings_clamp_adaptive_limits_to_single_thread() {
    let config = RuntimeConfig {
        developer: DeveloperRuntimeConfig {
            walker_adaptive_initial_limit: Some(8),
            walker_adaptive_max_limit: Some(1),
            ..DeveloperRuntimeConfig::default()
        },
        ..RuntimeConfig::default()
    };

    let settings = walker_runtime_settings(&config);

    assert_eq!(settings.adaptive_max_limit, 1);
    assert_eq!(settings.adaptive_initial_limit, 1);
}

#[test]
fn walker_runtime_settings_default_adaptive_initial_limit_is_half_of_max() {
    let config = RuntimeConfig {
        developer: DeveloperRuntimeConfig {
            walker_adaptive_max_limit: Some(8),
            ..DeveloperRuntimeConfig::default()
        },
        ..RuntimeConfig::default()
    };

    let settings = walker_runtime_settings(&config);

    assert_eq!(settings.adaptive_max_limit, 8);
    assert_eq!(settings.adaptive_initial_limit, 4);
}

#[test]
fn default_adaptive_max_limit_caps_at_eight_and_uses_half_logical_cores() {
    assert_eq!(default_adaptive_max_limit_from_logical_cores(1), 1);
    assert_eq!(default_adaptive_max_limit_from_logical_cores(2), 1);
    assert_eq!(default_adaptive_max_limit_from_logical_cores(3), 2);
    assert_eq!(default_adaptive_max_limit_from_logical_cores(4), 2);
    assert_eq!(default_adaptive_max_limit_from_logical_cores(5), 3);
    assert_eq!(default_adaptive_max_limit_from_logical_cores(15), 8);
    assert_eq!(default_adaptive_max_limit_from_logical_cores(16), 8);
    assert_eq!(default_adaptive_max_limit_from_logical_cores(64), 8);
}

#[test]
fn next_limit_from_throughput_moves_only_on_meaningful_change() {
    assert_eq!(
        next_limit_from_throughput(4, 8, None, 64, 90, 64, 100),
        (5, Some(LimitDirection::Increase))
    );
    assert_eq!(
        next_limit_from_throughput(4, 8, None, 64, 110, 64, 100),
        (3, Some(LimitDirection::Decrease))
    );
    assert_eq!(
        next_limit_from_throughput(4, 8, None, 64, 98, 64, 100),
        (4, None)
    );
    assert_eq!(
        next_limit_from_throughput(8, 8, None, 64, 90, 64, 100),
        (8, None)
    );
    assert_eq!(
        next_limit_from_throughput(1, 8, None, 64, 110, 64, 100),
        (1, None)
    );
}

#[test]
fn next_limit_from_throughput_follows_successful_probe_direction() {
    assert_eq!(
        next_limit_from_throughput(3, 8, Some(LimitDirection::Decrease), 64, 90, 64, 100),
        (2, Some(LimitDirection::Decrease))
    );
    assert_eq!(
        next_limit_from_throughput(5, 8, Some(LimitDirection::Increase), 64, 98, 64, 100),
        (6, Some(LimitDirection::Increase))
    );
}

#[test]
fn next_limit_from_throughput_reverses_failed_probe_direction() {
    assert_eq!(
        next_limit_from_throughput(5, 8, Some(LimitDirection::Increase), 64, 110, 64, 100),
        (4, Some(LimitDirection::Decrease))
    );
    assert_eq!(
        next_limit_from_throughput(3, 8, Some(LimitDirection::Decrease), 64, 110, 64, 100),
        (4, Some(LimitDirection::Increase))
    );
}

#[test]
fn next_limit_from_throughput_preserves_direction_at_bounds() {
    assert_eq!(
        next_limit_from_throughput(8, 8, Some(LimitDirection::Increase), 64, 90, 64, 100),
        (8, Some(LimitDirection::Increase))
    );
    assert_eq!(
        next_limit_from_throughput(1, 8, Some(LimitDirection::Decrease), 64, 90, 64, 100),
        (1, Some(LimitDirection::Decrease))
    );
}

#[test]
fn adaptive_walker_emits_entries_and_records_control_metrics() {
    let root = test_root("adaptive-basic");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("dir")).expect("create dir");
    std::fs::write(root.join("dir").join("main.rs"), "fn main() {}").expect("write file");

    let mut paths = Vec::new();
    let metrics = walk_adaptive(
        &root,
        2,
        2,
        |entry| {
            paths.push(entry.path);
            true
        },
        || false,
    );

    assert!(paths.iter().any(|path| path.ends_with("dir")));
    assert!(paths.iter().any(|path| path.ends_with("main.rs")));
    assert!(metrics.dirs_read >= 1);
    assert!(metrics.max_inflight_read_dirs >= 1);
    assert!(metrics.adaptive_limit_final >= 1);
    assert!(metrics.adaptive_limit_avg >= 1.0);
    assert!(metrics.adaptive_limit_avg <= metrics.adaptive_limit_max as f64);
    assert!(metrics.adaptive_limit_change_count <= metrics.dirs_read);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_can_stop_from_consumer_callback() {
    let root = test_root("adaptive-stop");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    for i in 0..100usize {
        std::fs::write(root.join(format!("file-{i}.txt")), "x").expect("write file");
    }

    let mut count = 0usize;
    let _metrics = walk_adaptive(
        &root,
        2,
        2,
        |_entry| {
            count = count.saturating_add(1);
            count < 3
        },
        || false,
    );

    assert!(count <= 4);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_returns_superseded_when_canceled_before_entry() {
    set_process_runtime_config(RuntimeConfig {
        developer: DeveloperRuntimeConfig {
            walker_adaptive_initial_limit: Some(1),
            walker_adaptive_max_limit: Some(1),
            ..DeveloperRuntimeConfig::default()
        },
        ..RuntimeConfig::default()
    });
    let root = test_root("adaptive-canceled-before-entry");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    std::fs::write(root.join("main.rs"), "fn main() {}").expect("write file");

    let (tx_res, _rx_res) = mpsc::channel();
    let req = IndexRequest {
        request_id: 10,
        tab_id: 3,
        root: root.clone(),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    };
    let shutdown = AtomicBool::new(false);
    let latest_request_ids = Mutex::new(HashMap::from([(req.tab_id, req.request_id + 1)]));

    let result = stream_walker_index(&tx_res, &req, &root, &shutdown, &latest_request_ids);

    assert_eq!(result, Err("superseded".to_string()));
    set_process_runtime_config(RuntimeConfig::default());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn filelist_stream_returns_superseded_when_canceled_before_entry() {
    let root = test_root("filelist-canceled-before-entry");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    let filelist = root.join("FileList.txt");
    std::fs::write(&filelist, "main.rs\n").expect("write FileList");

    let (tx_res, _rx_res) = mpsc::channel();
    let req = IndexRequest {
        request_id: 11,
        tab_id: 4,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    };
    let shutdown = AtomicBool::new(false);
    let latest_request_ids = Mutex::new(HashMap::from([(req.tab_id, req.request_id + 1)]));

    let result = stream_filelist_index(
        &tx_res,
        &req,
        &root,
        filelist,
        &shutdown,
        &latest_request_ids,
    );

    assert_eq!(result, Err("superseded".to_string()));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn filelist_stream_uses_larger_batches() {
    let root = test_root("filelist-large-batch");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    let filelist = root.join("FileList.txt");
    let text = (0..1025usize)
        .map(|i| format!("entry-{i}.txt\n"))
        .collect::<String>();
    std::fs::write(&filelist, text).expect("write filelist");

    let (tx_res, rx_res) = mpsc::channel();
    let req = IndexRequest {
        request_id: 22,
        tab_id: 5,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    };
    let shutdown = AtomicBool::new(false);
    let latest_request_ids = Mutex::new(HashMap::from([(req.tab_id, req.request_id)]));

    let result = stream_filelist_index(
        &tx_res,
        &req,
        &root,
        filelist,
        &shutdown,
        &latest_request_ids,
    );

    assert!(matches!(result, Ok(IndexSource::FileList(_))));
    let responses = rx_res.try_iter().collect::<Vec<_>>();
    assert!(matches!(
        responses.first(),
        Some(IndexResponse::Started {
            request_id: 22,
            source: IndexSource::FileList(_),
        })
    ));
    let batches = responses
        .iter()
        .filter_map(|response| match response {
            IndexResponse::Batch { entries, .. } => Some(entries.len()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(batches.iter().sum::<usize>(), 1025);
    assert_eq!(batches.iter().max(), Some(&1024));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn filelist_stream_applies_nested_override_after_initial_batches() {
    let root = test_root("filelist-nested-replace");
    let _ = std::fs::remove_dir_all(&root);
    let child = root.join("child");
    std::fs::create_dir_all(&child).expect("create child");
    std::fs::write(root.join("keep.txt"), "x").expect("write keep");
    std::fs::write(child.join("old.txt"), "x").expect("write old");
    std::fs::write(child.join("new.txt"), "x").expect("write new");
    let root_filelist = root.join("FileList.txt");
    let mut root_filelist_text = (0..2_050usize)
        .map(|index| format!("bulk-{index}.txt\n"))
        .collect::<String>();
    root_filelist_text.push_str("keep.txt\nchild\nchild/old.txt\nchild/filelist.txt\n");
    std::fs::write(&root_filelist, root_filelist_text).expect("write root filelist");
    std::thread::sleep(Duration::from_millis(1100));
    std::fs::write(child.join("filelist.txt"), "new.txt\n").expect("write child filelist");

    let (tx_res, rx_res) = mpsc::channel();
    let req = IndexRequest {
        request_id: 23,
        tab_id: 5,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    };
    let shutdown = AtomicBool::new(false);
    let latest_request_ids = Mutex::new(HashMap::from([(req.tab_id, req.request_id)]));

    let result = stream_filelist_index(
        &tx_res,
        &req,
        &root,
        root_filelist,
        &shutdown,
        &latest_request_ids,
    );

    assert!(matches!(result, Ok(IndexSource::FileList(_))));
    let responses = rx_res.try_iter().collect::<Vec<_>>();
    let replacement_start = responses
        .iter()
        .position(|response| matches!(response, IndexResponse::ReplaceAll { .. }))
        .expect("replace all response");
    let replacement_batches = responses[replacement_start..]
        .iter()
        .filter_map(|response| match response {
            IndexResponse::ReplaceAll { entries, .. } | IndexResponse::Batch { entries, .. } => {
                Some(entries)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(replacement_batches
        .iter()
        .all(|entries| entries.len() <= FILELIST_BATCH_SIZE));
    let replaced_paths = replacement_batches
        .into_iter()
        .flat_map(|entries| entries.iter().map(|entry| entry.path.clone()))
        .collect::<Vec<_>>();
    assert!(replaced_paths.contains(&root.join("bulk-2049.txt")));
    assert!(replaced_paths.contains(&root.join("keep.txt")));
    assert!(replaced_paths.contains(&child.join("new.txt")));
    assert!(!replaced_paths.contains(&child.join("old.txt")));
    assert!(!replaced_paths.contains(&child));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn tc_152_stale_index_request_cancels_before_root_resolution() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let latest_request_ids = Arc::new(Mutex::new(HashMap::from([(7, 2)])));
    let root_resolve_calls = Arc::new(AtomicUsize::new(0));
    let resolve_root: Arc<dyn Fn(&Path) -> PathBuf + Send + Sync> = {
        let root_resolve_calls = Arc::clone(&root_resolve_calls);
        Arc::new(move |root| {
            root_resolve_calls.fetch_add(1, Ordering::SeqCst);
            root.to_path_buf()
        })
    };
    let mailboxes = test_response_mailboxes();
    let (tx, _rx, _returned_mailboxes, handles) = spawn_index_worker_with(
        Arc::clone(&shutdown),
        latest_request_ids,
        Arc::clone(&mailboxes),
        resolve_root,
    );
    establish_prequeued_mailbox_invariant(&mailboxes, 1);
    tx.send(IndexRequest {
        request_id: 1,
        tab_id: 7,
        root: PathBuf::from("stale-root"),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    })
    .expect("send stale index request");
    assert!(matches!(
        recv_index_response(&mailboxes, 1, Duration::from_secs(1)),
        IndexResponse::Canceled { request_id: 1 }
    ));
    assert_eq!(root_resolve_calls.load(Ordering::SeqCst), 0);
    shutdown.store(true, Ordering::Relaxed);
    drop(tx);
    for handle in handles {
        handle.join().expect("join index worker");
    }
}

#[test]
fn tc_152_native_filelist_request_starts_and_finishes_within_deadline_regression() {
    let root = test_root("native-filelist-terminal-deadline");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    std::fs::write(root.join("entry.txt"), "entry").expect("write entry");
    std::fs::write(root.join("FileList.txt"), "entry.txt\n").expect("write FileList");

    let shutdown = Arc::new(AtomicBool::new(false));
    let request_id = 1;
    let tab_id = 7;
    let latest_request_ids = Arc::new(Mutex::new(HashMap::from([(tab_id, request_id)])));
    let (tx, _rx, mailboxes, handles) =
        spawn_index_worker(Arc::clone(&shutdown), latest_request_ids);
    establish_prequeued_mailbox_invariant(&mailboxes, request_id);
    tx.send(IndexRequest {
        request_id,
        tab_id,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    })
    .expect("send FileList index request");

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut started = false;
    let mut finished = false;
    let mut indexed_entry = false;
    while !finished {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .expect("tiny local FileList request exceeded terminal deadline");
        match recv_index_response(&mailboxes, request_id, remaining) {
            IndexResponse::Started {
                request_id: response_id,
                source: IndexSource::FileList(_),
            } => {
                assert_eq!(response_id, request_id);
                started = true;
            }
            IndexResponse::Batch {
                request_id: response_id,
                entries,
            } => {
                assert_eq!(response_id, request_id);
                indexed_entry |= entries.iter().any(|entry| {
                    entry
                        .path
                        .file_name()
                        .is_some_and(|name| name == "entry.txt")
                });
            }
            IndexResponse::Finished {
                request_id: response_id,
                source: IndexSource::FileList(_),
            } => {
                assert_eq!(response_id, request_id);
                finished = true;
            }
            IndexResponse::Started { source, .. } => {
                panic!("unexpected start source: {}", index_source_kind(&source))
            }
            IndexResponse::Finished { source, .. } => {
                panic!("unexpected finish source: {}", index_source_kind(&source))
            }
            IndexResponse::Canceled { .. } => panic!("FileList request was canceled"),
            IndexResponse::Failed { error, .. } => panic!("FileList request failed: {error}"),
            IndexResponse::ReplaceAll { .. } => panic!("unexpected replacement response"),
            IndexResponse::Truncated { .. } => panic!("unexpected truncation response"),
        }
    }
    assert!(started, "FileList source must be visible before completion");
    assert!(
        indexed_entry,
        "FileList entry must be emitted before completion"
    );

    shutdown.store(true, Ordering::Relaxed);
    drop(tx);
    for handle in handles {
        handle.join().expect("join index worker");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn tc_152_filelist_restore_index_regression_cancels_before_filelist_start() {
    let root = test_root("filelist-restore-stale-after-root-resolution");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    std::fs::write(root.join("FileList.txt"), "entry.txt\n").expect("write FileList");

    let shutdown = Arc::new(AtomicBool::new(false));
    let latest_request_ids = Arc::new(Mutex::new(HashMap::from([(7, 1)])));
    let latest_for_resolver = Arc::clone(&latest_request_ids);
    let resolve_root: Arc<dyn Fn(&Path) -> PathBuf + Send + Sync> = Arc::new(move |path| {
        latest_for_resolver
            .lock()
            .expect("lock latest request ids")
            .insert(7, 2);
        path.to_path_buf()
    });
    let mailboxes = test_response_mailboxes();
    let (tx, _rx, _returned_mailboxes, handles) = spawn_index_worker_with(
        Arc::clone(&shutdown),
        latest_request_ids,
        Arc::clone(&mailboxes),
        resolve_root,
    );
    establish_prequeued_mailbox_invariant(&mailboxes, 1);
    tx.send(IndexRequest {
        request_id: 1,
        tab_id: 7,
        root: root.clone(),
        use_filelist: true,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    })
    .expect("send stale FileList request");

    assert!(matches!(
        recv_index_response(&mailboxes, 1, Duration::from_secs(1)),
        IndexResponse::Canceled { request_id: 1 }
    ));

    shutdown.store(true, Ordering::Relaxed);
    drop(tx);
    for handle in handles {
        handle.join().expect("join index worker");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn tc_152_index_workers_bound_total_to_four() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let latest_request_ids = Arc::new(Mutex::new(HashMap::from([
        (1, 1),
        (2, 2),
        (3, 3),
        (4, 4),
        (5, 5),
    ])));
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (started_tx, started_rx) = mpsc::channel();
    let resolve_root: Arc<dyn Fn(&Path) -> PathBuf + Send + Sync> = {
        let gate = Arc::clone(&gate);
        Arc::new(move |root| {
            started_tx.send(()).expect("signal root resolution");
            let (lock, ready) = &*gate;
            let mut open = lock.lock().expect("lock gate");
            while !*open {
                open = ready.wait(open).expect("wait gate");
            }
            root.to_path_buf()
        })
    };
    let mailboxes = test_response_mailboxes();
    let (tx, _rx, _returned_mailboxes, handles) = spawn_index_worker_with(
        Arc::clone(&shutdown),
        latest_request_ids,
        Arc::clone(&mailboxes),
        resolve_root,
    );
    let request = |request_id| IndexRequest {
        request_id,
        tab_id: request_id,
        root: PathBuf::from(format!("missing-root-{request_id}")),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    };
    for request_id in 1..=5 {
        establish_prequeued_mailbox_invariant(&mailboxes, request_id);
    }
    tx.send(request(1)).expect("send first index request");
    tx.send(request(2)).expect("send second index request");
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("first index worker started");
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("second index worker started");
    tx.send(request(3)).expect("fill first queue slot");
    tx.send(request(4)).expect("fill second queue slot");
    assert!(matches!(
        tx.try_send(request(5)),
        Err(mpsc::TrySendError::Full(_))
    ));
    assert_eq!(tx.load().queued, 2);
    assert_eq!(tx.load().inflight, 2);
    assert_eq!(tx.load().capacity, 2);

    let (lock, ready) = &*gate;
    *lock.lock().expect("lock gate") = true;
    ready.notify_all();
    drop(tx);
    for handle in handles {
        handle.join().expect("join index worker");
    }
}

#[test]
fn tc_206_closed_request_mailboxes_do_not_terminate_resident_index_workers_regression() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let latest_request_ids = Arc::new(Mutex::new(HashMap::from([(1, 1), (2, 2), (3, 3)])));
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let (started_tx, started_rx) = mpsc::channel();
    let resolve_root: Arc<dyn Fn(&Path) -> PathBuf + Send + Sync> = {
        let gate = Arc::clone(&gate);
        Arc::new(move |root| {
            started_tx.send(()).expect("signal root resolution");
            let (lock, ready) = &*gate;
            let mut open = lock.lock().expect("lock gate");
            while !*open {
                open = ready.wait(open).expect("wait gate");
            }
            root.to_path_buf()
        })
    };
    let mailboxes = test_response_mailboxes();
    let (tx, _rx, _returned_mailboxes, handles) = spawn_index_worker_with(
        Arc::clone(&shutdown),
        latest_request_ids,
        Arc::clone(&mailboxes),
        resolve_root,
    );
    let request = |request_id| IndexRequest {
        request_id,
        tab_id: request_id,
        root: PathBuf::from(format!("missing-root-{request_id}")),
        use_filelist: false,
        include_files: true,
        include_dirs: true,
        max_depth: crate::indexer::MaxDepth::unlimited(),
    };
    for request_id in 1..=3 {
        establish_prequeued_mailbox_invariant(&mailboxes, request_id);
    }

    tx.send(request(1)).expect("send first index request");
    tx.send(request(2)).expect("send second index request");
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("first worker acquired request mailbox");
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("second worker acquired request mailbox");
    {
        let mailboxes = mailboxes.lock().expect("mailboxes");
        mailboxes.get(&1).expect("first mailbox").close();
        mailboxes.get(&2).expect("second mailbox").close();
    }
    tx.send(request(3))
        .expect("queue request after both request mailboxes close");

    let (lock, ready) = &*gate;
    *lock.lock().expect("lock gate") = true;
    ready.notify_all();

    assert!(matches!(
        recv_index_response(&mailboxes, 3, Duration::from_secs(1)),
        IndexResponse::Started {
            request_id: 3,
            source: IndexSource::Walker,
        }
    ));

    shutdown.store(true, Ordering::Relaxed);
    drop(tx);
    for handle in handles {
        handle.join().expect("join index worker");
    }
}

#[test]
fn tc_153_index_shutdown_drains_accepted_queue_with_terminal_cancellation() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let root_resolve_calls = Arc::new(AtomicUsize::new(0));
    let resolve_root: Arc<dyn Fn(&Path) -> PathBuf + Send + Sync> = {
        let root_resolve_calls = Arc::clone(&root_resolve_calls);
        Arc::new(move |root| {
            root_resolve_calls.fetch_add(1, Ordering::SeqCst);
            root.to_path_buf()
        })
    };
    let mailboxes = test_response_mailboxes();
    let (tx, _rx, _returned_mailboxes, handles) = spawn_index_worker_with(
        Arc::clone(&shutdown),
        Arc::new(Mutex::new(HashMap::new())),
        Arc::clone(&mailboxes),
        resolve_root,
    );
    shutdown.store(true, Ordering::Relaxed);
    for request_id in 1..=4 {
        establish_prequeued_mailbox_invariant(&mailboxes, request_id);
        tx.send(IndexRequest {
            request_id,
            tab_id: request_id,
            root: PathBuf::from(format!("shutdown-root-{request_id}")),
            use_filelist: false,
            include_files: true,
            include_dirs: true,
            max_depth: crate::indexer::MaxDepth::unlimited(),
        })
        .expect("accept index request before channel close");
    }
    drop(tx);

    let mut settled = Vec::new();
    for request_id in 1..=4 {
        match recv_index_response(&mailboxes, request_id, Duration::from_secs(1)) {
            IndexResponse::Canceled { request_id } => settled.push(request_id),
            _ => panic!("shutdown must emit only terminal cancellation"),
        }
    }
    settled.sort_unstable();
    assert_eq!(settled, vec![1, 2, 3, 4]);
    assert_eq!(root_resolve_calls.load(Ordering::SeqCst), 0);
    for handle in handles {
        handle.join().expect("join index worker");
    }
}

#[test]
fn index_worker_trace_smoke_emits_canonical_fields() {
    init_test_tracing();
    set_process_runtime_config(RuntimeConfig::default());
    let root = test_root("trace-smoke");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create dir");
    std::fs::write(root.join("main.rs"), "fn main() {}").expect("write file");

    let shutdown = Arc::new(AtomicBool::new(false));
    let latest_request_ids = Arc::new(Mutex::new(HashMap::new()));
    let request_id = 41u64;
    let tab_id = 7u64;
    latest_request_ids
        .lock()
        .expect("latest ids lock")
        .insert(tab_id, request_id);
    let (tx_req, _rx_res, mailboxes, handles) =
        spawn_index_worker(shutdown.clone(), latest_request_ids);
    establish_prequeued_mailbox_invariant(&mailboxes, request_id);
    tx_req
        .send(IndexRequest {
            request_id,
            tab_id,
            root: root.clone(),
            use_filelist: false,
            include_files: true,
            include_dirs: true,
            max_depth: crate::indexer::MaxDepth::unlimited(),
        })
        .expect("send request");

    assert!(matches!(
        recv_index_response(&mailboxes, request_id, Duration::from_secs(1)),
        IndexResponse::Started {
            request_id: 41,
            source: IndexSource::Walker,
        }
    ));
    assert!(matches!(
        recv_index_response(&mailboxes, request_id, Duration::from_secs(1)),
        IndexResponse::Batch { request_id: 41, .. }
    ));
    assert!(matches!(
        recv_index_response(&mailboxes, request_id, Duration::from_secs(1)),
        IndexResponse::Finished {
            request_id: 41,
            source: IndexSource::Walker,
        }
    ));

    drop(tx_req);
    shutdown.store(true, Ordering::Relaxed);
    for handle in handles {
        handle.join().expect("join index worker");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_matches_std_read_dir_count_on_basic_tree() {
    let root = test_root("adaptive-std-count");
    let _ = std::fs::remove_dir_all(&root);
    let dataset = root.join("dataset");
    std::fs::create_dir_all(dataset.join("a")).expect("create a");
    std::fs::create_dir_all(dataset.join("b")).expect("create b");
    std::fs::write(dataset.join("a").join("main.rs"), "fn main() {}").expect("write main");
    std::fs::write(dataset.join("b").join("lib.rs"), "pub fn lib() {}").expect("write lib");

    let std_count = count_std_walker_entries(&root);

    let mut adaptive_count = 0usize;
    let _metrics = walk_adaptive(
        &root,
        2,
        2,
        |entry| {
            if classify_walker_entry(&entry.path, entry.file_type, true, true).is_some() {
                adaptive_count = adaptive_count.saturating_add(1);
            }
            true
        },
        || false,
    );

    assert_eq!(adaptive_count, std_count);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_classification_counts_match_include_modes() {
    let root = test_root("adaptive-include-modes");
    let _ = std::fs::remove_dir_all(&root);
    let dataset = root.join("dataset");
    std::fs::create_dir_all(dataset.join("a").join("nested")).expect("create a/nested");
    std::fs::create_dir_all(dataset.join("b")).expect("create b");
    std::fs::write(dataset.join("root.txt"), "root").expect("write root file");
    std::fs::write(dataset.join("a").join("a.txt"), "a").expect("write a file");
    std::fs::write(
        dataset.join("a").join("nested").join("nested.txt"),
        "nested",
    )
    .expect("write nested file");

    for (include_files, include_dirs, expected) in [
        (true, false, 3usize),
        (false, true, 3usize),
        (true, true, 6usize),
    ] {
        let mut count = 0usize;
        walk_adaptive(
            &dataset,
            2,
            2,
            |entry| {
                if classify_walker_entry(&entry.path, entry.file_type, include_files, include_dirs)
                    .is_some()
                {
                    count = count.saturating_add(1);
                }
                true
            },
            || false,
        );

        assert_eq!(
            count, expected,
            "include_files={include_files} include_dirs={include_dirs}"
        );
    }

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_filter_suppresses_excluded_regular_entries_without_skipping_recursion() {
    let root = test_root("adaptive-producer-filter");
    let _ = std::fs::remove_dir_all(&root);
    let dataset = root.join("dataset");
    std::fs::create_dir_all(dataset.join("a").join("nested")).expect("create a/nested");
    std::fs::create_dir_all(dataset.join("b")).expect("create b");
    std::fs::write(dataset.join("root.txt"), "root").expect("write root file");
    std::fs::write(dataset.join("a").join("a.txt"), "a").expect("write a file");
    std::fs::write(
        dataset.join("a").join("nested").join("nested.txt"),
        "nested",
    )
    .expect("write nested file");

    let mut file_paths = Vec::new();
    walk_adaptive_filtered(
        &dataset,
        2,
        2,
        true,
        false,
        |entry| {
            assert!(entry.file_type.is_file());
            file_paths.push(entry.path);
            true
        },
        || false,
    );
    assert_eq!(file_paths.len(), 3);
    assert!(file_paths.iter().any(|path| path.ends_with("nested.txt")));

    let mut dir_paths = Vec::new();
    walk_adaptive_filtered(
        &dataset,
        2,
        2,
        false,
        true,
        |entry| {
            assert!(entry.file_type.is_dir());
            dir_paths.push(entry.path);
            true
        },
        || false,
    );
    assert_eq!(dir_paths.len(), 3);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_single_worker_filter_preserves_nested_file_recursion() {
    let root = test_root("adaptive-serial-filter");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("nested")).expect("create nested dir");
    std::fs::write(root.join("root.txt"), "root").expect("write root file");
    std::fs::write(root.join("nested").join("nested.txt"), "nested").expect("write nested file");

    let mut paths = Vec::new();
    let metrics = walk_adaptive_filtered(
        &root,
        1,
        1,
        true,
        false,
        |entry| {
            assert!(entry.file_type.is_file());
            paths.push(entry.path);
            true
        },
        || false,
    );

    assert_eq!(paths.len(), 2);
    assert!(paths.iter().any(|path| path.ends_with("nested.txt")));
    assert_eq!(metrics.max_inflight_read_dirs, 1);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_publishes_wide_child_frontier_in_batches() {
    let root = test_root("adaptive-wide-frontier");
    let _ = std::fs::remove_dir_all(&root);
    create_wide_frontier_fixture(&root, 128);

    let metrics = walk_adaptive(&root, 2, 2, |_entry| true, || false);

    assert!(metrics.child_dir_publish_batches >= 1);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_holds_shared_frontier_soft_limit_for_wide_shallow_trees() {
    const MAX_WORKERS: usize = 4;
    const CHILD_COUNT: usize = 1024;

    let root = test_root("adaptive-bounded-frontier");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    for i in 0..CHILD_COUNT {
        std::fs::create_dir_all(root.join(format!("dir-{i:04}"))).expect("create child dir");
    }

    let mut count = 0usize;
    let metrics = walk_adaptive(
        &root,
        MAX_WORKERS,
        // Keep one worker active while the root publishes its wide child set so
        // saturation is a fixture invariant rather than an OS scheduling race.
        1,
        |_entry| {
            count = count.saturating_add(1);
            true
        },
        || false,
    );

    assert_eq!(count, CHILD_COUNT);
    assert!(
        metrics.max_queued_dirs <= adaptive_shared_frontier_soft_limit(MAX_WORKERS),
        "shared frontier peak {} exceeded soft limit {}",
        metrics.max_queued_dirs,
        adaptive_shared_frontier_soft_limit(MAX_WORKERS)
    );
    assert_eq!(
        metrics.shared_frontier_soft_limit,
        adaptive_shared_frontier_soft_limit(MAX_WORKERS)
    );
    assert!(metrics.frontier_saturation_fallbacks > 0);
    assert_eq!(metrics.frontier_soft_limit_bypasses, 0);
    assert!(metrics.max_open_directory_frames <= metrics.open_directory_frame_budget);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_should_stop_after_frontier_saturation_returns_promptly() {
    const MAX_WORKERS: usize = 4;
    const CHILD_COUNT: usize = 1024;

    let root = test_root("adaptive-saturated-should-stop");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    for i in 0..CHILD_COUNT {
        std::fs::create_dir_all(root.join(format!("dir-{i:04}"))).expect("create child dir");
    }

    let stop = AtomicBool::new(false);
    let started = Instant::now();
    let mut count = 0usize;
    // Start with one active worker so the root must saturate the frontier before it can be drained.
    let metrics = walk_adaptive(
        &root,
        MAX_WORKERS,
        1,
        |_entry| {
            count = count.saturating_add(1);
            if count == 400 {
                stop.store(true, Ordering::Relaxed);
            }
            true
        },
        || stop.load(Ordering::Relaxed),
    );

    assert_eq!(count, 400);
    assert!(metrics.frontier_saturation_fallbacks > 0);
    assert!(metrics.max_queued_dirs <= adaptive_shared_frontier_soft_limit(MAX_WORKERS));
    assert!(started.elapsed() < Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_frontier_saturation_preserves_max_depth() {
    const MAX_WORKERS: usize = 4;
    const TOP_DIR_COUNT: usize = 512;

    let root = test_root("adaptive-saturated-max-depth");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    for i in 0..TOP_DIR_COUNT {
        let nested = root.join(format!("dir-{i:04}")).join("nested");
        std::fs::create_dir_all(&nested).expect("create nested dir");
        std::fs::write(nested.join("too-deep.txt"), "x").expect("write deep file");
    }

    let mut paths = Vec::new();
    let metrics = walk_adaptive_with_max_depth(
        &root,
        MAX_WORKERS,
        2,
        true,
        true,
        crate::indexer::MaxDepth::limited(2).expect("valid depth"),
        |entry| {
            paths.push(entry.path);
            true
        },
        || false,
    );

    assert_eq!(paths.len(), TOP_DIR_COUNT * 2);
    assert!(paths.iter().all(|path| !path.ends_with("too-deep.txt")));
    assert!(metrics.frontier_saturation_fallbacks > 0);
    assert!(metrics.max_queued_dirs <= adaptive_shared_frontier_soft_limit(MAX_WORKERS));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_bounds_open_frames_and_bypasses_soft_limit_for_deep_wide_tree() {
    const MAX_WORKERS: usize = 4;
    const SOFT_LIMIT: usize = 8;
    const LOCAL_FRAME_LIMIT: usize = 2;
    const TOP_DIR_COUNT: usize = 32;
    const NESTED_DIR_COUNT: usize = 32;

    let root = test_root("adaptive-frame-budget-bypass");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    for i in 0..TOP_DIR_COUNT {
        let top = root.join(format!("top-{i:02}"));
        std::fs::create_dir_all(&top).expect("create top dir");
        for j in 0..NESTED_DIR_COUNT {
            std::fs::create_dir_all(top.join(format!("nested-{j:02}"))).expect("create nested dir");
        }
    }

    let mut count = 0usize;
    let metrics = walk_adaptive_filtered_with_frontier_limits(
        &root,
        MAX_WORKERS,
        2,
        true,
        true,
        SOFT_LIMIT,
        LOCAL_FRAME_LIMIT,
        |_entry| {
            count = count.saturating_add(1);
            true
        },
        || false,
    );

    assert_eq!(count, TOP_DIR_COUNT + TOP_DIR_COUNT * NESTED_DIR_COUNT);
    assert_eq!(metrics.read_dir_errors, 0);
    assert_eq!(metrics.shared_frontier_soft_limit, SOFT_LIMIT);
    assert!(metrics.frontier_soft_limit_bypasses > 0);
    assert!(metrics.max_queued_dirs > SOFT_LIMIT);
    assert!(metrics.max_open_directory_frames <= metrics.open_directory_frame_budget);
    assert_eq!(
        metrics.open_directory_frame_budget,
        MAX_WORKERS * LOCAL_FRAME_LIMIT
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_soft_limit_bypass_preserves_max_depth() {
    const MAX_WORKERS: usize = 4;
    const TOP_DIR_COUNT: usize = 32;
    const NESTED_DIR_COUNT: usize = 32;

    let root = test_root("adaptive-bypass-max-depth");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    for i in 0..TOP_DIR_COUNT {
        let top = root.join(format!("top-{i:02}"));
        for j in 0..NESTED_DIR_COUNT {
            let nested = top.join(format!("nested-{j:02}"));
            std::fs::create_dir_all(&nested).expect("create nested dir");
            std::fs::write(nested.join("too-deep.txt"), "x").expect("write deep file");
        }
    }

    let mut paths = Vec::new();
    let metrics = walk_adaptive_filtered_with_frontier_limits_and_max_depth(
        &root,
        MAX_WORKERS,
        2,
        true,
        true,
        crate::indexer::MaxDepth::limited(2).expect("valid depth"),
        8,
        1,
        |entry| {
            paths.push(entry.path);
            true
        },
        || false,
    );

    assert_eq!(
        paths.len(),
        TOP_DIR_COUNT + TOP_DIR_COUNT * NESTED_DIR_COUNT
    );
    assert!(paths.iter().all(|path| !path.ends_with("too-deep.txt")));
    assert!(metrics.frontier_soft_limit_bypasses > 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_should_stop_after_soft_limit_bypass_returns_promptly() {
    const MAX_WORKERS: usize = 4;
    const TOP_DIR_COUNT: usize = 32;
    const NESTED_DIR_COUNT: usize = 32;

    let root = test_root("adaptive-bypass-should-stop");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    for i in 0..TOP_DIR_COUNT {
        let top = root.join(format!("top-{i:02}"));
        for j in 0..NESTED_DIR_COUNT {
            std::fs::create_dir_all(top.join(format!("nested-{j:02}"))).expect("create nested dir");
        }
    }

    let stop = AtomicBool::new(false);
    let started = Instant::now();
    let mut count = 0usize;
    let metrics = walk_adaptive_filtered_with_frontier_limits(
        &root,
        MAX_WORKERS,
        2,
        true,
        true,
        8,
        1,
        |_entry| {
            count = count.saturating_add(1);
            if count == 400 {
                stop.store(true, Ordering::Relaxed);
            }
            true
        },
        || stop.load(Ordering::Relaxed),
    );

    assert_eq!(count, 400);
    assert!(metrics.frontier_soft_limit_bypasses > 0);
    assert!(started.elapsed() < Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_incremental_publish_emits_parent_before_child() {
    let root = test_root("adaptive-parent-before-child");
    let _ = std::fs::remove_dir_all(&root);
    create_wide_frontier_fixture(&root, 64);

    let mut emitted_dirs = std::collections::HashSet::new();
    walk_adaptive(
        &root,
        4,
        2,
        |entry| {
            if entry.file_type.is_dir() {
                emitted_dirs.insert(entry.path);
            } else if entry.file_type.is_file() {
                let parent = entry.path.parent().expect("file parent");
                assert!(
                    emitted_dirs.contains(parent),
                    "child emitted before parent: {}",
                    entry.path.display()
                );
            }
            true
        },
        || false,
    );

    assert_eq!(emitted_dirs.len(), 64);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_callback_cancel_after_batch_publish_returns_promptly() {
    let root = test_root("adaptive-batched-callback-cancel");
    let _ = std::fs::remove_dir_all(&root);
    create_wide_frontier_fixture(&root, 96);

    let started = Instant::now();
    let mut count = 0usize;
    let metrics = walk_adaptive(
        &root,
        4,
        2,
        |_entry| {
            count = count.saturating_add(1);
            count < 40
        },
        || false,
    );

    assert_eq!(count, 40);
    assert!(metrics.child_dir_publish_batches >= 1);
    assert!(started.elapsed() < Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn adaptive_walker_should_stop_after_batch_publish_returns_promptly() {
    let root = test_root("adaptive-batched-should-stop");
    let _ = std::fs::remove_dir_all(&root);
    create_wide_frontier_fixture(&root, 96);

    let stop = AtomicBool::new(false);
    let started = Instant::now();
    let mut count = 0usize;
    let metrics = walk_adaptive(
        &root,
        4,
        2,
        |_entry| {
            count = count.saturating_add(1);
            if count == 40 {
                stop.store(true, Ordering::Relaxed);
            }
            true
        },
        || stop.load(Ordering::Relaxed),
    );

    assert_eq!(count, 40);
    assert!(metrics.child_dir_publish_batches >= 1);
    assert!(started.elapsed() < Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&root);
}

fn create_wide_frontier_fixture(root: &Path, child_count: usize) {
    std::fs::create_dir_all(root).expect("create root");
    for i in 0..child_count {
        let dir = root.join(format!("dir-{i:03}"));
        std::fs::create_dir_all(&dir).expect("create child dir");
        std::fs::write(dir.join("entry.txt"), "x").expect("write child file");
    }
}

#[cfg(windows)]
#[test]
fn adaptive_walker_folder_filter_keeps_shortcuts_for_deferred_kind_resolution() {
    let root = test_root("adaptive-folder-shortcut");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create root");
    let shortcut = root.join("target.lnk");
    std::fs::write(&shortcut, "shortcut fixture").expect("write shortcut fixture");

    let mut emitted = Vec::new();
    walk_adaptive_filtered(
        &root,
        2,
        2,
        false,
        true,
        |entry| {
            emitted.push(entry.path);
            true
        },
        || false,
    );

    assert_eq!(emitted, vec![shortcut]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
#[ignore = "perf measurement; run explicitly"]
fn perf_adaptive_walker_reports_local_dataset_metrics() {
    let root = test_root("perf-adaptive-compare");
    let _ = std::fs::remove_dir_all(&root);
    let dataset = root.join("dataset");
    std::fs::create_dir_all(&dataset).expect("create dataset");
    for i in 0..10_000usize {
        let dir = dataset.join(format!("dir-{i}"));
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(dir.join("main.rs"), "fn main() {}").expect("write file");
    }

    let std_start = Instant::now();
    let std_count = count_std_walker_entries(&root);
    let std_elapsed = std_start.elapsed();

    let adaptive_start = Instant::now();
    let mut adaptive_count = 0usize;
    let adaptive_metrics = walk_adaptive(
        &root,
        2,
        2,
        |entry| {
            if classify_walker_entry(&entry.path, entry.file_type, true, true).is_some() {
                adaptive_count = adaptive_count.saturating_add(1);
            }
            true
        },
        || false,
    );
    let adaptive_elapsed = adaptive_start.elapsed();

    eprintln!(
        "Walker backend comparison std_read_dir_ms={:.3} adaptive_ms={:.3} std_count={} adaptive_count={} adaptive_dirs_read={} adaptive_errors={} adaptive_max_inflight={} adaptive_throttle_events={} adaptive_limit_min={} adaptive_limit_max={} adaptive_limit_final={} adaptive_limit_change_count={} adaptive_limit_avg={:.3} adaptive_read_dir_avg_us={} adaptive_read_dir_max_us={}",
        std_elapsed.as_secs_f64() * 1000.0,
        adaptive_elapsed.as_secs_f64() * 1000.0,
        std_count,
        adaptive_count,
        adaptive_metrics.dirs_read,
        adaptive_metrics.read_dir_errors,
        adaptive_metrics.max_inflight_read_dirs,
        adaptive_metrics.throttle_events,
        adaptive_metrics.adaptive_limit_min,
        adaptive_metrics.adaptive_limit_max,
        adaptive_metrics.adaptive_limit_final,
        adaptive_metrics.adaptive_limit_change_count,
        adaptive_metrics.adaptive_limit_avg,
        if adaptive_metrics.dirs_read == 0 {
            0
        } else {
            adaptive_metrics.read_dir_total_us / adaptive_metrics.dirs_read as u128
        },
        adaptive_metrics.read_dir_max_us,
    );

    assert_eq!(std_count, adaptive_count);
    assert!(adaptive_metrics.max_inflight_read_dirs <= 2);

    let _ = std::fs::remove_dir_all(&root);
}

#[derive(Clone, Debug)]
struct AdaptivePerfObservation {
    elapsed: Duration,
    first_file: Option<Duration>,
    callbacks: usize,
    count: usize,
    metrics: AdaptiveWalkerMetrics,
}

#[derive(Clone, Copy, Debug)]
enum AdaptivePerfVariant {
    UnfilteredIncremental,
    FilteredIncremental,
    FilteredIncrementalUnbounded,
    FilteredDeferred,
}

#[test]
#[ignore = "perf measurement matrix; run explicitly with --release"]
fn perf_adaptive_walker_release_matrix() {
    const REPETITIONS: usize = 8;
    const MAX_WORKERS: usize = 4;
    const INITIAL_LIMIT: usize = 2;

    let root = test_root("perf-adaptive-matrix");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create matrix root");

    let cases = [
        build_adaptive_perf_dir_heavy_case(&root),
        build_adaptive_perf_file_heavy_case(&root),
        build_adaptive_perf_mixed_case(&root),
    ];
    let modes = [
        ("files", true, false),
        ("folders", false, true),
        ("both", true, true),
    ];

    for (shape, dataset, expected_files, expected_dirs) in &cases {
        for (mode, include_files, include_dirs) in modes {
            let expected = usize::from(include_files)
                .saturating_mul(*expected_files)
                .saturating_add(usize::from(include_dirs).saturating_mul(*expected_dirs));

            let _baseline_warmup = measure_adaptive_perf_once(
                dataset,
                include_files,
                include_dirs,
                MAX_WORKERS,
                INITIAL_LIMIT,
                AdaptivePerfVariant::UnfilteredIncremental,
            );
            let _filtered_warmup = measure_adaptive_perf_once(
                dataset,
                include_files,
                include_dirs,
                MAX_WORKERS,
                INITIAL_LIMIT,
                AdaptivePerfVariant::FilteredIncremental,
            );
            let mut baseline_observations = Vec::with_capacity(REPETITIONS);
            let mut filtered_observations = Vec::with_capacity(REPETITIONS);
            for iteration in 0..REPETITIONS {
                let measure_baseline = || {
                    measure_adaptive_perf_once(
                        dataset,
                        include_files,
                        include_dirs,
                        MAX_WORKERS,
                        INITIAL_LIMIT,
                        AdaptivePerfVariant::UnfilteredIncremental,
                    )
                };
                let measure_filtered = || {
                    measure_adaptive_perf_once(
                        dataset,
                        include_files,
                        include_dirs,
                        MAX_WORKERS,
                        INITIAL_LIMIT,
                        AdaptivePerfVariant::FilteredIncremental,
                    )
                };
                if iteration.is_multiple_of(2) {
                    baseline_observations.push(measure_baseline());
                    filtered_observations.push(measure_filtered());
                } else {
                    filtered_observations.push(measure_filtered());
                    baseline_observations.push(measure_baseline());
                }
            }

            for observation in baseline_observations
                .iter()
                .chain(filtered_observations.iter())
            {
                assert_eq!(
                    observation.count, expected,
                    "shape={shape} mode={mode} adaptive count mismatch"
                );
                assert_eq!(observation.metrics.read_dir_errors, 0);
            }
            for observation in &filtered_observations {
                assert_eq!(
                    observation.callbacks, expected,
                    "shape={shape} mode={mode} producer emitted an excluded regular entry"
                );
            }
            baseline_observations.sort_unstable_by_key(|observation| observation.elapsed);
            filtered_observations.sort_unstable_by_key(|observation| observation.elapsed);
            let baseline_median = &baseline_observations[REPETITIONS / 2];
            let filtered_median = &filtered_observations[REPETITIONS / 2];
            let baseline_median_seconds = median_elapsed_seconds(&baseline_observations);
            let filtered_median_seconds = median_elapsed_seconds(&filtered_observations);
            let filtered_min = filtered_observations[0].elapsed;
            let filtered_max = filtered_observations[REPETITIONS - 1].elapsed;
            let speedup = baseline_median_seconds / filtered_median_seconds.max(f64::MIN_POSITIVE);
            let filtered_first_file_median = if include_files {
                format!(
                    "{:.0}",
                    median_first_file_seconds(&filtered_observations) * 1_000_000.0
                )
            } else {
                "none".to_string()
            };

            eprintln!(
                "Adaptive walker matrix profile={} shape={} mode={} repetitions={} entries={} dirs_read={} baseline_callbacks={} filtered_callbacks={} baseline_median_ms={:.3} filtered_median_ms={:.3} speedup={:.3}x filtered_min_ms={:.3} filtered_max_ms={:.3} filtered_first_file_median_us={} max_inflight={} throttle_events={} limit_min={} limit_max={} limit_final={} limit_changes={} limit_avg={:.3}",
                if cfg!(debug_assertions) { "debug" } else { "release" },
                shape,
                mode,
                REPETITIONS,
                filtered_median.count,
                filtered_median.metrics.dirs_read,
                baseline_median.callbacks,
                filtered_median.callbacks,
                baseline_median_seconds * 1000.0,
                filtered_median_seconds * 1000.0,
                speedup,
                filtered_min.as_secs_f64() * 1000.0,
                filtered_max.as_secs_f64() * 1000.0,
                filtered_first_file_median,
                filtered_median.metrics.max_inflight_read_dirs,
                filtered_median.metrics.throttle_events,
                filtered_median.metrics.adaptive_limit_min,
                filtered_median.metrics.adaptive_limit_max,
                filtered_median.metrics.adaptive_limit_final,
                filtered_median.metrics.adaptive_limit_change_count,
                filtered_median.metrics.adaptive_limit_avg,
            );
        }
    }

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
#[ignore = "scheduling perf measurement matrix; run explicitly with --release"]
fn perf_adaptive_walker_scheduling_release_matrix() {
    const REPETITIONS: usize = 8;
    const MAX_WORKERS: usize = 4;
    const INITIAL_LIMIT: usize = 2;

    let root = test_root("perf-adaptive-scheduling");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create scheduling root");
    let cases = [
        build_adaptive_perf_dir_heavy_case(&root),
        build_adaptive_perf_mixed_case(&root),
    ];
    let modes = [("files", true, false), ("both", true, true)];

    for (shape, dataset, expected_files, expected_dirs) in &cases {
        for (mode, include_files, include_dirs) in modes {
            let expected = usize::from(include_files)
                .saturating_mul(*expected_files)
                .saturating_add(usize::from(include_dirs).saturating_mul(*expected_dirs));
            let _deferred_warmup = measure_adaptive_perf_once(
                dataset,
                include_files,
                include_dirs,
                MAX_WORKERS,
                INITIAL_LIMIT,
                AdaptivePerfVariant::FilteredDeferred,
            );
            let _incremental_warmup = measure_adaptive_perf_once(
                dataset,
                include_files,
                include_dirs,
                MAX_WORKERS,
                INITIAL_LIMIT,
                AdaptivePerfVariant::FilteredIncremental,
            );
            let mut deferred = Vec::with_capacity(REPETITIONS);
            let mut incremental = Vec::with_capacity(REPETITIONS);
            for iteration in 0..REPETITIONS {
                let measure_deferred = || {
                    measure_adaptive_perf_once(
                        dataset,
                        include_files,
                        include_dirs,
                        MAX_WORKERS,
                        INITIAL_LIMIT,
                        AdaptivePerfVariant::FilteredDeferred,
                    )
                };
                let measure_incremental = || {
                    measure_adaptive_perf_once(
                        dataset,
                        include_files,
                        include_dirs,
                        MAX_WORKERS,
                        INITIAL_LIMIT,
                        AdaptivePerfVariant::FilteredIncremental,
                    )
                };
                if iteration.is_multiple_of(2) {
                    deferred.push(measure_deferred());
                    incremental.push(measure_incremental());
                } else {
                    incremental.push(measure_incremental());
                    deferred.push(measure_deferred());
                }
            }

            for observation in deferred.iter().chain(incremental.iter()) {
                assert_eq!(observation.count, expected);
                assert_eq!(observation.metrics.read_dir_errors, 0);
                assert_eq!(
                    observation.metrics.shared_frontier_soft_limit,
                    adaptive_shared_frontier_soft_limit(MAX_WORKERS)
                );
                assert!(
                    observation.metrics.max_queued_dirs
                        <= adaptive_shared_frontier_soft_limit(MAX_WORKERS)
                );
                assert_eq!(observation.metrics.frontier_soft_limit_bypasses, 0);
            }
            assert!(deferred
                .iter()
                .all(|observation| observation.metrics.child_dir_publish_batches == 0));
            assert!(incremental
                .iter()
                .all(|observation| observation.metrics.child_dir_publish_batches > 0));
            deferred.sort_unstable_by_key(|observation| observation.elapsed);
            incremental.sort_unstable_by_key(|observation| observation.elapsed);
            let deferred_median_seconds = median_elapsed_seconds(&deferred);
            let incremental_median_seconds = median_elapsed_seconds(&incremental);
            let elapsed_speedup =
                deferred_median_seconds / incremental_median_seconds.max(f64::MIN_POSITIVE);
            let deferred_first_file_seconds = median_first_file_seconds(&deferred);
            let incremental_first_file_seconds = median_first_file_seconds(&incremental);
            let first_file_speedup =
                deferred_first_file_seconds / incremental_first_file_seconds.max(f64::MIN_POSITIVE);
            let incremental_median = &incremental[REPETITIONS / 2];

            eprintln!(
                "Adaptive scheduling matrix profile={} shape={} mode={} repetitions={} entries={} deferred_median_ms={:.3} incremental_median_ms={:.3} elapsed_speedup={:.3}x deferred_first_file_us={:.0} incremental_first_file_us={:.0} first_file_speedup={:.3}x publish_batches={} max_queued_dirs={}",
                if cfg!(debug_assertions) { "debug" } else { "release" },
                shape,
                mode,
                REPETITIONS,
                incremental_median.count,
                deferred_median_seconds * 1000.0,
                incremental_median_seconds * 1000.0,
                elapsed_speedup,
                deferred_first_file_seconds * 1_000_000.0,
                incremental_first_file_seconds * 1_000_000.0,
                first_file_speedup,
                incremental_median.metrics.child_dir_publish_batches,
                incremental_median.metrics.max_queued_dirs,
            );
        }
    }

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
#[ignore = "frontier perf measurement matrix; run explicitly with --release"]
fn perf_adaptive_walker_frontier_release_matrix() {
    const REPETITIONS: usize = 8;
    const MAX_WORKERS: usize = 4;
    const INITIAL_LIMIT: usize = 2;

    let root = test_root("perf-adaptive-frontier");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create frontier root");
    let cases = [
        build_adaptive_perf_dir_heavy_case(&root),
        build_adaptive_perf_mixed_case(&root),
    ];
    let modes = [("files", true, false), ("both", true, true)];

    for (shape, dataset, expected_files, expected_dirs) in &cases {
        for (mode, include_files, include_dirs) in modes {
            let expected = usize::from(include_files)
                .saturating_mul(*expected_files)
                .saturating_add(usize::from(include_dirs).saturating_mul(*expected_dirs));
            let _unbounded_warmup = measure_adaptive_perf_once(
                dataset,
                include_files,
                include_dirs,
                MAX_WORKERS,
                INITIAL_LIMIT,
                AdaptivePerfVariant::FilteredIncrementalUnbounded,
            );
            let _bounded_warmup = measure_adaptive_perf_once(
                dataset,
                include_files,
                include_dirs,
                MAX_WORKERS,
                INITIAL_LIMIT,
                AdaptivePerfVariant::FilteredIncremental,
            );
            let mut unbounded = Vec::with_capacity(REPETITIONS);
            let mut bounded = Vec::with_capacity(REPETITIONS);
            for iteration in 0..REPETITIONS {
                let measure_unbounded = || {
                    measure_adaptive_perf_once(
                        dataset,
                        include_files,
                        include_dirs,
                        MAX_WORKERS,
                        INITIAL_LIMIT,
                        AdaptivePerfVariant::FilteredIncrementalUnbounded,
                    )
                };
                let measure_bounded = || {
                    measure_adaptive_perf_once(
                        dataset,
                        include_files,
                        include_dirs,
                        MAX_WORKERS,
                        INITIAL_LIMIT,
                        AdaptivePerfVariant::FilteredIncremental,
                    )
                };
                if iteration.is_multiple_of(2) {
                    unbounded.push(measure_unbounded());
                    bounded.push(measure_bounded());
                } else {
                    bounded.push(measure_bounded());
                    unbounded.push(measure_unbounded());
                }
            }

            for observation in unbounded.iter().chain(bounded.iter()) {
                assert_eq!(observation.count, expected);
                assert_eq!(observation.metrics.read_dir_errors, 0);
            }
            let soft_limit = adaptive_shared_frontier_soft_limit(MAX_WORKERS);
            assert!(bounded
                .iter()
                .all(|observation| observation.metrics.max_queued_dirs <= soft_limit));
            assert!(bounded
                .iter()
                .all(|observation| observation.metrics.frontier_soft_limit_bypasses == 0));
            unbounded.sort_unstable_by_key(|observation| observation.elapsed);
            bounded.sort_unstable_by_key(|observation| observation.elapsed);
            let unbounded_median_seconds = median_elapsed_seconds(&unbounded);
            let bounded_median_seconds = median_elapsed_seconds(&bounded);
            let elapsed_speedup =
                unbounded_median_seconds / bounded_median_seconds.max(f64::MIN_POSITIVE);
            let unbounded_first_file_seconds = median_first_file_seconds(&unbounded);
            let bounded_first_file_seconds = median_first_file_seconds(&bounded);
            let first_file_speedup =
                unbounded_first_file_seconds / bounded_first_file_seconds.max(f64::MIN_POSITIVE);
            let unbounded_median = &unbounded[REPETITIONS / 2];
            let bounded_median = &bounded[REPETITIONS / 2];

            eprintln!(
                "Adaptive frontier matrix profile={} shape={} mode={} repetitions={} entries={} soft_limit={} unbounded_peak={} soft_limited_peak={} unbounded_median_ms={:.3} soft_limited_median_ms={:.3} elapsed_speedup={:.3}x unbounded_first_file_us={:.0} soft_limited_first_file_us={:.0} first_file_speedup={:.3}x",
                if cfg!(debug_assertions) { "debug" } else { "release" },
                shape,
                mode,
                REPETITIONS,
                bounded_median.count,
                soft_limit,
                unbounded_median.metrics.max_queued_dirs,
                bounded_median.metrics.max_queued_dirs,
                unbounded_median_seconds * 1000.0,
                bounded_median_seconds * 1000.0,
                elapsed_speedup,
                unbounded_first_file_seconds * 1_000_000.0,
                bounded_first_file_seconds * 1_000_000.0,
                first_file_speedup,
            );
        }
    }

    let _ = std::fs::remove_dir_all(&root);
}

fn median_elapsed_seconds(observations: &[AdaptivePerfObservation]) -> f64 {
    let upper = observations.len() / 2;
    if observations.len().is_multiple_of(2) {
        (observations[upper - 1].elapsed.as_secs_f64() + observations[upper].elapsed.as_secs_f64())
            / 2.0
    } else {
        observations[upper].elapsed.as_secs_f64()
    }
}

fn median_first_file_seconds(observations: &[AdaptivePerfObservation]) -> f64 {
    let mut durations = observations
        .iter()
        .map(|observation| observation.first_file.expect("first file observation"))
        .collect::<Vec<_>>();
    durations.sort_unstable();
    let upper = durations.len() / 2;
    if durations.len().is_multiple_of(2) {
        (durations[upper - 1].as_secs_f64() + durations[upper].as_secs_f64()) / 2.0
    } else {
        durations[upper].as_secs_f64()
    }
}

fn measure_adaptive_perf_once(
    root: &Path,
    include_files: bool,
    include_dirs: bool,
    max_workers: usize,
    initial_limit: usize,
    variant: AdaptivePerfVariant,
) -> AdaptivePerfObservation {
    let started = Instant::now();
    let mut first_file = None;
    let mut callbacks = 0usize;
    let mut count = 0usize;
    let mut on_entry = |entry: AdaptiveWalkerEntry| {
        callbacks = callbacks.saturating_add(1);
        if first_file.is_none() && entry.file_type.is_file() {
            first_file = Some(started.elapsed());
        }
        if classify_walker_entry(&entry.path, entry.file_type, include_files, include_dirs)
            .is_some()
        {
            count = count.saturating_add(1);
        }
        true
    };
    let metrics = match variant {
        AdaptivePerfVariant::UnfilteredIncremental => {
            walk_adaptive(root, max_workers, initial_limit, &mut on_entry, || false)
        }
        AdaptivePerfVariant::FilteredIncremental => walk_adaptive_filtered(
            root,
            max_workers,
            initial_limit,
            include_files,
            include_dirs,
            &mut on_entry,
            || false,
        ),
        AdaptivePerfVariant::FilteredIncrementalUnbounded => walk_adaptive_filtered_unbounded(
            root,
            max_workers,
            initial_limit,
            include_files,
            include_dirs,
            &mut on_entry,
            || false,
        ),
        AdaptivePerfVariant::FilteredDeferred => walk_adaptive_filtered_deferred(
            root,
            max_workers,
            initial_limit,
            include_files,
            include_dirs,
            &mut on_entry,
            || false,
        ),
    };

    AdaptivePerfObservation {
        elapsed: started.elapsed(),
        first_file,
        callbacks,
        count,
        metrics,
    }
}

fn build_adaptive_perf_dir_heavy_case(root: &Path) -> (&'static str, PathBuf, usize, usize) {
    let dataset = root.join("dir-heavy");
    std::fs::create_dir_all(&dataset).expect("create dir-heavy dataset");
    let dir_count = 2_048usize;
    for i in 0..dir_count {
        let dir = dataset.join(format!("dir-{i:04}"));
        std::fs::create_dir_all(&dir).expect("create dir-heavy dir");
        std::fs::write(dir.join("entry.txt"), "x").expect("write dir-heavy file");
    }
    ("dir-heavy", dataset, dir_count, dir_count)
}

fn build_adaptive_perf_file_heavy_case(root: &Path) -> (&'static str, PathBuf, usize, usize) {
    let dataset = root.join("file-heavy");
    std::fs::create_dir_all(&dataset).expect("create file-heavy dataset");
    let dir_count = 64usize;
    let files_per_dir = 128usize;
    for i in 0..dir_count {
        let dir = dataset.join(format!("dir-{i:02}"));
        std::fs::create_dir_all(&dir).expect("create file-heavy dir");
        for j in 0..files_per_dir {
            std::fs::write(dir.join(format!("entry-{j:03}.txt")), "x")
                .expect("write file-heavy file");
        }
    }
    (
        "file-heavy",
        dataset,
        dir_count.saturating_mul(files_per_dir),
        dir_count,
    )
}

fn build_adaptive_perf_mixed_case(root: &Path) -> (&'static str, PathBuf, usize, usize) {
    let dataset = root.join("mixed");
    std::fs::create_dir_all(&dataset).expect("create mixed dataset");
    let top_dirs = 128usize;
    let child_dirs = 8usize;
    let files_per_child = 2usize;
    for i in 0..top_dirs {
        let top = dataset.join(format!("top-{i:03}"));
        std::fs::create_dir_all(&top).expect("create mixed top dir");
        for j in 0..child_dirs {
            let child = top.join(format!("child-{j:02}"));
            std::fs::create_dir_all(&child).expect("create mixed child dir");
            for k in 0..files_per_child {
                std::fs::write(child.join(format!("entry-{k}.txt")), "x")
                    .expect("write mixed file");
            }
        }
    }
    (
        "mixed",
        dataset,
        top_dirs
            .saturating_mul(child_dirs)
            .saturating_mul(files_per_child),
        top_dirs.saturating_add(top_dirs.saturating_mul(child_dirs)),
    )
}

#[test]
#[ignore = "perf measurement; run explicitly"]
fn perf_walker_classification_is_faster_than_eager_metadata_resolution() {
    let root = test_root("perf");
    let _ = std::fs::remove_dir_all(&root);
    let dataset = root.join("dataset");
    std::fs::create_dir_all(&dataset).expect("create dataset");
    // Keep the fixture shallow and file-heavy so the shared recursive read_dir cost
    // does not drown out the per-entry classification behavior under measurement.
    for i in 0..128usize {
        let dir = dataset.join(format!("dir-{i}"));
        std::fs::create_dir_all(&dir).expect("create dir");
        for j in 0..256usize {
            std::fs::write(dir.join(format!("entry-{j}.rs")), "fn main() {}").expect("write file");
        }
    }
    #[cfg(unix)]
    let link_target = dataset.join("dir-0").join("main.rs");
    for i in 0..128usize {
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&link_target, dataset.join(format!("link-{i}"))).expect("create symlink");
        }
        #[cfg(windows)]
        {
            std::fs::write(dataset.join(format!("link-{i}.lnk")), "shortcut")
                .expect("write shortcut");
        }
    }

    let expected_count = count_eager_metadata_entries(&root);
    assert_eq!(expected_count, count_std_walker_entries(&root));

    let iterations = 7usize;
    let mut eager_samples = Vec::with_capacity(iterations);
    let mut fast_samples = Vec::with_capacity(iterations);
    for iteration in 0..iterations {
        let mut measure_eager = || {
            let started = Instant::now();
            assert_eq!(count_eager_metadata_entries(&root), expected_count);
            eager_samples.push(started.elapsed());
        };
        let mut measure_fast = || {
            let started = Instant::now();
            assert_eq!(count_std_walker_entries(&root), expected_count);
            fast_samples.push(started.elapsed());
        };
        if iteration % 2 == 0 {
            measure_eager();
            measure_fast();
        } else {
            measure_fast();
            measure_eager();
        }
    }

    eager_samples.sort_unstable();
    fast_samples.sort_unstable();
    let eager_median = eager_samples[iterations / 2];
    let fast_median = fast_samples[iterations / 2];
    let eager_ms = eager_median.as_secs_f64() * 1000.0;
    let fast_ms = fast_median.as_secs_f64() * 1000.0;
    let speedup = if fast_ms > 0.0 {
        eager_ms / fast_ms
    } else {
        f64::INFINITY
    };

    eprintln!(
        "Walker perf control_baseline samples={iterations} eager_metadata_median_ms={eager_ms:.3} fast_classify_median_ms={fast_ms:.3} speedup={speedup:.2}x entries={expected_count}"
    );

    assert!(
        speedup >= 1.25,
        "walker fast classification did not beat the control baseline enough: {speedup:.2}x"
    );
    let _ = std::fs::remove_dir_all(&root);
}

fn count_std_walker_entries(root: &Path) -> usize {
    let mut count = 0usize;
    visit_std_walker_entries(root, &mut |path, file_type| {
        if classify_walker_entry(path, file_type, true, true).is_some() {
            count = count.saturating_add(1);
        }
    });
    count
}

fn count_eager_metadata_entries(root: &Path) -> usize {
    let mut count = 0usize;
    visit_std_walker_entries(root, &mut |path, _file_type| {
        if resolve_entry_kind(path).is_some() {
            count = count.saturating_add(1);
        }
    });
    count
}

fn visit_std_walker_entries(root: &Path, on_entry: &mut impl FnMut(&Path, FileType)) {
    let Ok(read_dir) = std::fs::read_dir(root) else {
        return;
    };
    for child in read_dir.flatten() {
        let Ok(file_type) = child.file_type() else {
            continue;
        };
        let path = child.path();
        on_entry(&path, file_type);
        if file_type.is_dir() && !file_type.is_symlink() {
            visit_std_walker_entries(&path, on_entry);
        }
    }
}
