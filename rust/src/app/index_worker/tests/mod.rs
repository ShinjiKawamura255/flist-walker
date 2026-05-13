use super::*;
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
fn index_worker_trace_smoke_emits_canonical_fields() {
    init_test_tracing();
    let root = test_root("trace-smoke");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create dir");
    std::fs::write(root.join("main.rs"), "fn main() {}").expect("write file");

    let shutdown = Arc::new(AtomicBool::new(false));
    let latest_request_ids = Arc::new(Mutex::new(HashMap::new()));
    let (tx_req, rx_res, handles) = spawn_index_worker(shutdown.clone(), latest_request_ids);
    let request_id = 41u64;
    let tab_id = 7u64;
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
        eager_count = 0;
        for entry in WalkDir::new(&root)
            .parallelism(Parallelism::Serial)
            .skip_hidden(false)
            .follow_links(false)
            .min_depth(1)
            .into_iter()
            .flatten()
        {
            if resolve_entry_kind(&entry.path()).is_some() {
                eager_count = eager_count.saturating_add(1);
            }
        }
        eager_best = eager_best.min(eager_start.elapsed());

        let fast_start = Instant::now();
        fast_count = 0;
        for entry in WalkDir::new(&root)
            .parallelism(Parallelism::Serial)
            .skip_hidden(false)
            .follow_links(false)
            .min_depth(1)
            .into_iter()
            .flatten()
        {
            if classify_walker_entry(&entry.path(), entry.file_type(), true, true).is_some() {
                fast_count = fast_count.saturating_add(1);
            }
        }
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
