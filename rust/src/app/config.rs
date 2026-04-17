use super::FlistWalkerApp;
use std::time::Duration;

impl FlistWalkerApp {
    pub(super) const PREVIEW_CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;
    pub(super) const HIGHLIGHT_CACHE_MAX: usize = 256;
    pub(super) const SORT_METADATA_CACHE_MAX: usize = 4096;
    pub(super) const TAB_DRAG_START_DISTANCE: f32 = 6.0;
    pub(super) const QUERY_HISTORY_MAX: usize = 100;
    pub(super) const QUERY_HISTORY_IDLE_DELAY: Duration = Duration::from_millis(400);
    pub(super) const INCREMENTAL_SEARCH_REFRESH_INTERVAL: Duration =
        Duration::from_millis(300);
    pub(super) const INCREMENTAL_SEARCH_REFRESH_INTERVAL_DURING_INDEX: Duration =
        Duration::from_millis(1500);
    pub(super) const INCREMENTAL_SEARCH_MIN_DELTA_DURING_INDEX: usize = 2048;
    pub(super) const PAGE_MOVE_ROWS: isize = 10;
    pub(super) const DEFAULT_PREVIEW_PANEL_WIDTH: f32 = 440.0;
    pub(super) const MIN_RESULTS_PANEL_WIDTH: f32 = 220.0;
    pub(super) const MIN_PREVIEW_PANEL_WIDTH: f32 = 220.0;
    pub(super) const ROOT_SELECTOR_POPUP_ID: &'static str = "root-selector-popup";
    pub(super) const INDEX_MAX_CONCURRENT: usize = 2;
    pub(super) const INDEX_MAX_QUEUE: usize = 4;
    pub(super) const UI_STATE_SAVE_INTERVAL: Duration = Duration::from_millis(500);
    pub(super) const WINDOW_GEOMETRY_SETTLE_INTERVAL: Duration = Duration::from_millis(350);
    pub(super) const MEMORY_SAMPLE_INTERVAL: Duration = Duration::from_millis(1000);
    // Regression guard: app close should not stall on background workers once
    // shutdown has been requested and all request senders have been dropped.
    pub(super) const WORKER_JOIN_TIMEOUT: Duration = Duration::from_millis(250);
    pub(super) const SHRINK_MIN_CAPACITY: usize = 4096;
    pub(super) const SEARCH_HINTS_TOOLTIP: &'static str = "\
Search hints:
- トークンは AND 条件（例: main py）
- abc|foo|bar : OR 条件（スペースなしの | で連結）
- 'term : 完全一致トークン（例: 'main.py）
- !term : 除外トークン（例: main !test）
- ^term : 先頭一致を優先（例: ^src）
- term$ : 末尾一致を優先（例: .rs$）";
}
