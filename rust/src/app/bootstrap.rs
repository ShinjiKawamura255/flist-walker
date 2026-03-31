use super::*;

pub(super) struct AppWorkerBootstrap {
    pub(super) search_tx: Sender<SearchRequest>,
    pub(super) search_rx: Receiver<SearchResponse>,
    pub(super) preview_tx: Sender<PreviewRequest>,
    pub(super) preview_rx: Receiver<PreviewResponse>,
    pub(super) action_tx: Sender<ActionRequest>,
    pub(super) action_rx: Receiver<ActionResponse>,
    pub(super) sort_tx: Sender<SortMetadataRequest>,
    pub(super) sort_rx: Receiver<SortMetadataResponse>,
    pub(super) kind_tx: Sender<KindResolveRequest>,
    pub(super) kind_rx: Receiver<KindResolveResponse>,
    pub(super) filelist_tx: Sender<FileListRequest>,
    pub(super) filelist_rx: Receiver<FileListResponse>,
    pub(super) update_tx: Sender<UpdateRequest>,
    pub(super) update_rx: Receiver<UpdateResponse>,
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
            preview_tx,
            preview_rx,
            action_tx,
            action_rx,
            sort_tx,
            sort_rx,
            kind_tx,
            kind_rx,
            filelist_tx,
            filelist_rx,
            update_tx,
            update_rx,
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
            root: Self::normalize_windows_path(root),
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
