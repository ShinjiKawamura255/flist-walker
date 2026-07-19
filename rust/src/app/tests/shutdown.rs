use super::*;

#[test]
fn process_shutdown_flag_can_be_set_and_cleared() {
    clear_process_shutdown_request();
    assert!(!process_shutdown_requested());
    request_process_shutdown();
    assert!(process_shutdown_requested());
    clear_process_shutdown_request();
    assert!(!process_shutdown_requested());
}

#[test]
fn worker_runtime_join_all_with_timeout_returns_joined_when_workers_finish() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut runtime = WorkerRuntime::new(Arc::clone(&shutdown));
    runtime.push("worker-a", thread::spawn(|| {}));
    runtime.push("worker-b", thread::spawn(|| {}));

    let summary = runtime.join_all_with_timeout(Duration::from_millis(500));

    assert_eq!(summary.total, 2);
    assert_eq!(summary.joined, 2);
    assert!(summary.pending.is_empty());
}

#[test]
fn worker_runtime_join_all_with_timeout_returns_early_on_timeout() {
    let shutdown = Arc::new(AtomicBool::new(false));
    let mut runtime = WorkerRuntime::new(Arc::clone(&shutdown));
    runtime.push(
        "slow-worker",
        thread::spawn(|| {
            thread::sleep(Duration::from_millis(200));
        }),
    );

    let summary = runtime.join_all_with_timeout(Duration::from_millis(10));

    assert_eq!(summary.total, 1);
    assert_eq!(summary.joined, 0);
    assert_eq!(summary.pending, vec!["slow-worker".to_string()]);
}

#[test]
fn regression_gui_close_uses_short_worker_join_timeout_budget() {
    assert!(FlistWalkerApp::worker_join_timeout() <= Duration::from_millis(250));
}

#[test]
fn tc_153_runtime_registers_direct_bounded_worker_handles() {
    let root = test_root("tc-153-direct-worker-handles");
    fs::create_dir_all(&root).expect("create root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    let names = app
        .shell
        .worker_runtime
        .as_ref()
        .expect("worker runtime")
        .worker_names();
    assert!(names.contains(&"action-0".to_string()));
    assert!(names.contains(&"action-1".to_string()));
    assert!(names.contains(&"index-0".to_string()));
    assert!(names.contains(&"index-1".to_string()));
    assert!(!names.contains(&"action".to_string()));

    let summary = app
        .shutdown_workers_with_timeout(Duration::from_millis(250), "tc-153")
        .expect("shutdown summary");
    assert_eq!(summary.joined, summary.total);
    assert!(summary.pending.is_empty());
    let _ = fs::remove_dir_all(&root);
}
