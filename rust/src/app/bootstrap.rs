use super::{
    spawn_action_worker, spawn_filelist_worker, spawn_index_worker, spawn_kind_resolver_worker,
    spawn_preview_worker, spawn_search_worker, spawn_sort_metadata_worker, spawn_update_worker,
    ActionWorkerBus, AppRuntimeState, AppShellState, CacheStateBundle, EntryKindCacheState,
    FeatureStateBundle, FileListManager, FileListWorkerBus, FlistWalkerApp, HashSet,
    HighlightCacheState, IndexBuildResult, IndexCoordinator, IndexRequest, IndexResponse,
    IndexSource, KindWorkerBus, LaunchSettings, PreviewCacheState, PreviewWorkerBus, QueryState,
    Receiver, ResultSortMode, RootBrowserState, RuntimeUiState, SavedTabState, SearchCoordinator,
    SearchRequest, SearchResponse, Sender, SortMetadataCacheState, SortWorkerBus, TabSessionState,
    UpdateWorkerBus, WorkerBus, WorkerRuntime,
};
use crate::app::state::{UpdateManager, UpdateState};
use crate::ignore_list::load_ignore_terms_from_current_exe;
use crate::path_utils::normalize_windows_path_buf;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

type WorkerBootstrapParts = (
    Sender<SearchRequest>,
    Receiver<SearchResponse>,
    WorkerBus,
    Sender<IndexRequest>,
    Receiver<IndexResponse>,
    Arc<Mutex<HashMap<u64, u64>>>,
    WorkerRuntime,
);

type LaunchSeedParts = (
    PathBuf,
    usize,
    String,
    VecDeque<String>,
    Vec<PathBuf>,
    Option<PathBuf>,
    bool,
    bool,
    f32,
    Arc<Vec<String>>,
    UpdateState,
);

pub(super) struct AppWorkerBootstrap {
    search_tx: Sender<SearchRequest>,
    search_rx: Receiver<SearchResponse>,
    worker_bus: WorkerBus,
    index_tx: Sender<IndexRequest>,
    index_rx: Receiver<IndexResponse>,
    latest_index_request_ids: Arc<Mutex<HashMap<u64, u64>>>,
    worker_runtime: WorkerRuntime,
}

pub(super) struct AppLaunchSeed {
    root: PathBuf,
    limit: usize,
    query: String,
    query_history: VecDeque<String>,
    saved_roots: Vec<PathBuf>,
    default_root: Option<PathBuf>,
    show_preview: bool,
    ignore_list_enabled: bool,
    preview_panel_width: f32,
    ignore_list_terms: Arc<Vec<String>>,
    update_state: UpdateState,
}

impl AppWorkerBootstrap {
    pub(super) fn into_parts(self) -> WorkerBootstrapParts {
        (
            self.search_tx,
            self.search_rx,
            self.worker_bus,
            self.index_tx,
            self.index_rx,
            self.latest_index_request_ids,
            self.worker_runtime,
        )
    }
}

impl AppLaunchSeed {
    pub(super) fn into_parts(self) -> LaunchSeedParts {
        (
            self.root,
            self.limit,
            self.query,
            self.query_history,
            self.saved_roots,
            self.default_root,
            self.show_preview,
            self.ignore_list_enabled,
            self.preview_panel_width,
            self.ignore_list_terms,
            self.update_state,
        )
    }
}

impl FlistWalkerApp {
    pub(super) fn build_new(root: PathBuf, limit: usize, query: String) -> Self {
        let launch = LaunchSettings {
            show_preview: true,
            ignore_list_enabled: true,
            preview_panel_width: Self::DEFAULT_PREVIEW_PANEL_WIDTH,
            ..LaunchSettings::default()
        };
        Self::new_with_launch(root, limit, query, launch, None)
    }

    pub(super) fn build_from_launch(
        root: PathBuf,
        limit: usize,
        query: String,
        root_explicit: bool,
    ) -> Self {
        let launch = Self::load_launch_settings();
        let restore_tabs_enabled = Self::restore_tabs_enabled();
        let saved_last_root = launch
            .last_root
            .as_ref()
            .and_then(|p| p.canonicalize().ok())
            .map(normalize_windows_path_buf)
            .filter(|p| p.is_dir());
        let saved_default = launch
            .default_root
            .as_ref()
            .and_then(|p| p.canonicalize().ok())
            .map(normalize_windows_path_buf)
            .filter(|p| p.is_dir());
        let restore_session = if restore_tabs_enabled && !root_explicit && query.trim().is_empty() {
            Self::sanitize_saved_tabs(&launch.restore_tabs, launch.restore_active_tab)
        } else {
            None
        };
        let chosen_root = Self::choose_startup_root(
            root,
            root_explicit,
            restore_tabs_enabled,
            restore_session.as_ref(),
            saved_last_root,
            saved_default,
        );
        let mut app = Self::new_with_launch(chosen_root, limit, query, launch, restore_session);
        app.request_startup_update_check();
        app
    }

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
            ignore_list_enabled: launch.ignore_list_enabled,
            preview_panel_width: launch
                .preview_panel_width
                .max(Self::MIN_PREVIEW_PANEL_WIDTH),
            ignore_list_terms: Arc::new(load_ignore_terms_from_current_exe()),
            update_state: UpdateState {
                skipped_target_version: launch.skipped_update_target_version.clone(),
                suppress_check_failure_dialog: launch.suppress_update_check_failure_dialog,
                ..UpdateState::default()
            },
        }
    }

    fn new_with_launch(
        root: PathBuf,
        limit: usize,
        query: String,
        launch: LaunchSettings,
        restore_session: Option<(Vec<SavedTabState>, usize)>,
    ) -> Self {
        let (
            search_tx,
            search_rx,
            worker_bus,
            index_tx,
            index_rx,
            latest_index_request_ids,
            worker_runtime,
        ) = Self::bootstrap_workers().into_parts();
        let (
            root,
            limit,
            query,
            query_history,
            saved_roots,
            default_root,
            show_preview,
            ignore_list_enabled,
            preview_panel_width,
            ignore_list_terms,
            update_state,
        ) = Self::launch_seed(root, limit, query, &launch).into_parts();
        let mut app = Self {
            shell: AppShellState {
                runtime: AppRuntimeState {
                    root,
                    limit,
                    query_state: QueryState::new(query, query_history),
                    use_filelist: true,
                    use_regex: false,
                    ignore_case: true,
                    ignore_list_terms,
                    include_files: true,
                    include_dirs: true,
                    index: IndexBuildResult {
                        entries: Vec::new(),
                        source: IndexSource::None,
                    },
                    all_entries: Arc::new(Vec::new()),
                    entries: Arc::new(Vec::new()),
                    base_results: Vec::new(),
                    results: Vec::new(),
                    result_sort_mode: ResultSortMode::Score,
                    pinned_paths: HashSet::new(),
                    current_row: Some(0),
                    preview: String::new(),
                    notice: String::new(),
                    status_line: "Initializing...".to_string(),
                },
                search: SearchCoordinator::new(search_tx, search_rx),
                worker_bus,
                indexing: IndexCoordinator::new(index_tx, index_rx, latest_index_request_ids),
                ui: RuntimeUiState::new(show_preview, ignore_list_enabled, preview_panel_width),
                cache: CacheStateBundle {
                    preview: PreviewCacheState::default(),
                    highlight: HighlightCacheState::with_scope_ignore_case(true),
                    entry_kind: EntryKindCacheState::default(),
                    sort_metadata: SortMetadataCacheState::default(),
                },
                tabs: TabSessionState::default(),
                features: FeatureStateBundle {
                    root_browser: RootBrowserState {
                        #[cfg(test)]
                        browse_dialog_result: None,
                        saved_roots,
                        default_root,
                    },
                    filelist: FileListManager::default(),
                    update: UpdateManager::from_state(update_state),
                },
                worker_runtime: Some(worker_runtime),
            },
        };
        if let Some(path) = Self::window_trace_path() {
            Self::append_window_trace("app_initialized", &format!("path={}", path.display()));
        }
        if let Some((tabs, active_tab)) = restore_session {
            app.initialize_tabs_from_saved(tabs, active_tab);
        } else {
            app.initialize_tabs();
            app.request_index_refresh();
        }
        app
    }
}
