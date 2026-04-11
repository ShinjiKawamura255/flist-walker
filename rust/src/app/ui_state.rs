use super::*;

pub(super) struct RuntimeUiState {
    pub(super) pending_copy_shortcut: bool,
    pub(super) root_dropdown_highlight: Option<usize>,
    pub(super) scroll_to_current: bool,
    pub(super) preview_resize_in_progress: bool,
    pub(super) focus_query_requested: bool,
    pub(super) unfocus_query_requested: bool,
    pub(super) show_preview: bool,
    pub(super) preview_panel_width: f32,
    pub(super) window_geometry: Option<SavedWindowGeometry>,
    pub(super) pending_window_geometry: Option<SavedWindowGeometry>,
    pub(super) last_window_geometry_change: Instant,
    pub(super) ui_state_dirty: bool,
    pub(super) last_ui_state_save: Instant,
    pub(super) last_memory_sample: Instant,
    pub(super) memory_usage_bytes: Option<u64>,
    pub(super) ime_composition_active: bool,
    pub(super) prev_space_down: bool,
    pub(super) query_input_id: egui::Id,
    pub(super) tab_drag_state: Option<TabDragState>,
    pub(super) pending_render_commands: Vec<super::render::RenderCommand>,
}

impl RuntimeUiState {
    pub(super) fn new(show_preview: bool, preview_panel_width: f32) -> Self {
        Self {
            pending_copy_shortcut: false,
            root_dropdown_highlight: None,
            scroll_to_current: true,
            preview_resize_in_progress: false,
            focus_query_requested: true,
            unfocus_query_requested: false,
            show_preview,
            preview_panel_width,
            window_geometry: None,
            pending_window_geometry: None,
            last_window_geometry_change: Instant::now(),
            ui_state_dirty: false,
            last_ui_state_save: Instant::now(),
            last_memory_sample: Instant::now(),
            memory_usage_bytes: None,
            ime_composition_active: false,
            prev_space_down: false,
            query_input_id: egui::Id::new("query-input"),
            tab_drag_state: None,
            pending_render_commands: Vec::new(),
        }
    }
}
