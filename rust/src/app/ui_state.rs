use super::{SavedWindowGeometry, TabDragState};
use eframe::egui;
use std::time::Instant;

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

#[allow(dead_code)]
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

    pub(super) fn show_preview(&self) -> bool {
        self.show_preview
    }

    pub(super) fn set_show_preview(&mut self, show_preview: bool) {
        self.show_preview = show_preview;
    }

    pub(super) fn preview_panel_width(&self) -> f32 {
        self.preview_panel_width
    }

    pub(super) fn set_preview_panel_width(&mut self, width: f32) {
        self.preview_panel_width = width;
    }

    pub(super) fn preview_resize_in_progress(&self) -> bool {
        self.preview_resize_in_progress
    }

    pub(super) fn set_preview_resize_in_progress(&mut self, value: bool) {
        self.preview_resize_in_progress = value;
    }

    pub(super) fn scroll_to_current(&self) -> bool {
        self.scroll_to_current
    }

    pub(super) fn set_scroll_to_current(&mut self, value: bool) {
        self.scroll_to_current = value;
    }

    pub(super) fn root_dropdown_highlight(&self) -> Option<usize> {
        self.root_dropdown_highlight
    }

    pub(super) fn set_root_dropdown_highlight(&mut self, value: Option<usize>) {
        self.root_dropdown_highlight = value;
    }

    pub(super) fn query_input_id(&self) -> egui::Id {
        self.query_input_id
    }

    pub(super) fn focus_query_requested(&self) -> bool {
        self.focus_query_requested
    }

    pub(super) fn unfocus_query_requested(&self) -> bool {
        self.unfocus_query_requested
    }

    pub(super) fn request_focus_query(&mut self) {
        self.focus_query_requested = true;
    }

    pub(super) fn request_unfocus_query(&mut self) {
        self.unfocus_query_requested = true;
    }

    pub(super) fn clear_focus_query_request(&mut self) {
        self.focus_query_requested = false;
    }

    pub(super) fn clear_unfocus_query_request(&mut self) {
        self.unfocus_query_requested = false;
    }

    pub(super) fn pending_render_commands_mut(&mut self) -> &mut Vec<super::render::RenderCommand> {
        &mut self.pending_render_commands
    }

    pub(super) fn take_pending_render_commands(
        &mut self,
    ) -> Vec<super::render::RenderCommand> {
        std::mem::take(&mut self.pending_render_commands)
    }
}
