use super::*;
use crate::runtime_config::{set_process_runtime_config, DeveloperRuntimeConfig, RuntimeConfig};
use std::time::{SystemTime, UNIX_EPOCH};
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
    assert_eq!(classified, (EntryKind::file(), false));
    #[cfg(not(windows))]
    assert_eq!(classified, (EntryKind::file(), true));
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
    };
    let mut metrics = WalkerMetrics::new(WalkerBackend::Adaptive);
    metrics.entries_emitted = 11;
    metrics.batches_sent = 2;
    metrics.dirs_read = 5;

    let summary = walker_metrics_summary(&req, &metrics, "finished");
    write_walker_metrics_summary(&summary, &log_path.to_string_lossy());

    let text = std::fs::read_to_string(&log_path).expect("read metrics log");
    assert!(text.contains("event=metrics"));
    assert!(text.contains("backend=adaptive"));
    assert!(text.contains("entries_emitted=11"));

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
    assert_eq!(default_adaptive_max_limit_from_logical_cores(4), 2);
    assert_eq!(default_adaptive_max_limit_from_logical_cores(16), 8);
    assert_eq!(default_adaptive_max_limit_from_logical_cores(64), 8);
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
    };
    let shutdown = AtomicBool::new(false);
    let latest_request_ids = Mutex::new(HashMap::from([(req.tab_id, req.request_id + 1)]));

    let result = stream_walker_index(&tx_res, &req, &root, &shutdown, &latest_request_ids);

    assert_eq!(result, Err("superseded".to_string()));
    set_process_runtime_config(RuntimeConfig::default());
    let _ = std::fs::remove_dir_all(&root);
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
    let (tx_req, rx_res, handles) = spawn_index_worker(shutdown.clone(), latest_request_ids);
    tx_req
        .send(IndexRequest {
            request_id,
            tab_id,
            root: root.clone(),
            use_filelist: false,
            include_files: true,
            include_dirs: true,
        })
        .expect("send request");

    assert!(matches!(
        rx_res.recv().expect("started response"),
        IndexResponse::Started {
            request_id: 41,
            source: IndexSource::Walker,
        }
    ));
    assert!(matches!(
        rx_res.recv().expect("batch response"),
        IndexResponse::Batch { request_id: 41, .. }
    ));
    assert!(matches!(
        rx_res.recv().expect("finished response"),
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
        "Walker backend comparison std_read_dir_ms={:.3} adaptive_ms={:.3} std_count={} adaptive_count={} adaptive_dirs_read={} adaptive_errors={} adaptive_max_inflight={} adaptive_throttle_events={} adaptive_limit_min={} adaptive_limit_max={} adaptive_limit_final={} adaptive_read_dir_avg_us={} adaptive_read_dir_max_us={}",
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

#[test]
#[ignore = "perf measurement; run explicitly"]
fn perf_walker_classification_is_faster_than_eager_metadata_resolution() {
    let root = test_root("perf");
    let _ = std::fs::remove_dir_all(&root);
    let dataset = root.join("dataset");
    std::fs::create_dir_all(&dataset).expect("create dataset");
    for i in 0..20_000usize {
        let dir = dataset.join(format!("dir-{i}"));
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(dir.join("main.rs"), "fn main() {}").expect("write file");
    }

    let mut eager_best = Duration::MAX;
    let mut fast_best = Duration::MAX;
    let iterations = 3usize;
    let mut eager_count = 0usize;
    let mut fast_count = 0usize;

    for _ in 0..iterations {
        let eager_start = Instant::now();
        eager_count = count_eager_metadata_entries(&root);
        eager_best = eager_best.min(eager_start.elapsed());

        let fast_start = Instant::now();
        fast_count = count_std_walker_entries(&root);
        fast_best = fast_best.min(fast_start.elapsed());
    }

    let eager_ms = eager_best.as_secs_f64() * 1000.0;
    let fast_ms = fast_best.as_secs_f64() * 1000.0;
    let speedup = if fast_ms > 0.0 {
        eager_ms / fast_ms
    } else {
        f64::INFINITY
    };

    eprintln!(
        "Walker perf control_baseline eager_metadata_ms={eager_ms:.3} fast_classify_ms={fast_ms:.3} speedup={speedup:.2}x eager_count={eager_count} fast_count={fast_count}"
    );

    assert_eq!(eager_count, fast_count);
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
