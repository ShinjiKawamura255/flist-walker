use super::*;
use crate::path_utils::normalize_windows_path_buf;

pub(super) struct AppWorkerBootstrap {
    pub(super) search_tx: Sender<SearchRequest>,
    pub(super) search_rx: Receiver<SearchResponse>,
    pub(super) worker_bus: WorkerBus,
    pub(super) index_tx: Sender<IndexRequest>,
    pub(super) index_rx: Receiver<IndexResponse>,
    pub(super) latest_index_request_ids: Arc<Mutex<HashMap<u64, u64>>>,
    pub(super) worker_runtime: WorkerRuntime,
}

pub(super) struct AppLaunchSeed {
    pub(super) root: PathBuf,
    pub(super) limit: usize,
    pub(super) query: String,
    pub(super) query_history: VecDeque<String>,
    pub(super) saved_roots: Vec<PathBuf>,
    pub(super) default_root: Option<PathBuf>,
    pub(super) show_preview: bool,
    pub(super) preview_panel_width: f32,
    pub(super) update_state: UpdateState,
}

impl FlistWalkerApp {
    pub(super) fn bootstrap_workers() -> AppWorkerBootstrap {
        let worker_shutdown = Arc::new(AtomicBool::new(false));
        let mut worker_runtime = WorkerRuntime::new(Arc::clone(&worker_shutdown));
        let (search_tx, search_rx, search_handle) =
            spawn_search_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push("search", search_handle);
        let (preview_tx, preview_rx, preview_handle) =
            spawn_preview_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push("preview", preview_handle);
        let (action_tx, action_rx, action_handle) =
            spawn_action_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push("action", action_handle);
        let (sort_tx, sort_rx, sort_handle) =
            spawn_sort_metadata_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push("sort-metadata", sort_handle);
        let (kind_tx, kind_rx, kind_handle) =
            spawn_kind_resolver_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push("kind-resolver", kind_handle);
        let (filelist_tx, filelist_rx, filelist_handle) =
            spawn_filelist_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push("filelist", filelist_handle);
        let (update_tx, update_rx, update_handle) =
            spawn_update_worker(Arc::clone(&worker_shutdown));
        worker_runtime.push("update", update_handle);
        let latest_index_request_ids = Arc::new(Mutex::new(HashMap::new()));
        let (index_tx, index_rx, index_handles) = spawn_index_worker(
            Arc::clone(&worker_shutdown),
            Arc::clone(&latest_index_request_ids),
        );
        for (idx, handle) in index_handles.into_iter().enumerate() {
            worker_runtime.push(format!("index-{idx}"), handle);
        }

        AppWorkerBootstrap {
            search_tx,
            search_rx,
            worker_bus: WorkerBus {
                preview: PreviewWorkerBus {
                    tx: preview_tx,
                    rx: preview_rx,
                    next_request_id: 1,
                    pending_request_id: None,
                    in_progress: false,
                },
                action: ActionWorkerBus {
                    tx: action_tx,
                    rx: action_rx,
                    next_request_id: 1,
                    pending_request_id: None,
                    in_progress: false,
                },
                sort: SortWorkerBus {
                    tx: sort_tx,
                    rx: sort_rx,
                    next_request_id: 1,
                    pending_request_id: None,
                    in_progress: false,
                },
                kind: KindWorkerBus {
                    tx: kind_tx,
                    rx: kind_rx,
                },
                filelist: FileListWorkerBus {
                    tx: filelist_tx,
                    rx: filelist_rx,
                },
                update: UpdateWorkerBus {
                    tx: update_tx,
                    rx: update_rx,
                },
            },
            index_tx,
            index_rx,
            latest_index_request_ids,
            worker_runtime,
        }
    }

    pub(super) fn launch_seed(
        root: PathBuf,
        limit: usize,
        query: String,
        launch: &LaunchSettings,
    ) -> AppLaunchSeed {
        AppLaunchSeed {
            root: normalize_windows_path_buf(root),
            limit: limit.clamp(1, 1000),
            query,
            query_history: launch.query_history.iter().cloned().collect(),
            saved_roots: Self::load_saved_roots(),
            default_root: launch.default_root.clone(),
            show_preview: launch.show_preview,
            preview_panel_width: launch
                .preview_panel_width
                .max(Self::MIN_PREVIEW_PANEL_WIDTH),
            update_state: UpdateState {
                skipped_target_version: launch.skipped_update_target_version.clone(),
                suppress_check_failure_dialog: launch.suppress_update_check_failure_dialog,
                ..UpdateState::default()
            },
        }
    }
}
