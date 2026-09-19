use super::{FlistWalkerApp, PreviewRequest, PreviewResponse};
use crate::ui_model::{PagedTextPreview, PreviewPageError, PreviewPageState};
use std::path::Path;
use std::sync::Arc;

pub(super) struct PagedPreviewView {
    pub(super) error: Option<PreviewPageError>,
    pub(super) busy: bool,
    pub(super) display_generation: u64,
    tab_id: Option<u64>,
    path: Option<std::path::PathBuf>,
    request_id: Option<u64>,
    pub(super) color_enabled: bool,
}

impl Default for PagedPreviewView {
    fn default() -> Self {
        Self {
            error: None,
            busy: false,
            display_generation: 0,
            tab_id: None,
            path: None,
            request_id: None,
            color_enabled: true,
        }
    }
}

const PREVIEW_BUILD_OR_RESPONSE_MAX_BYTES: usize = 8 * 1024 * 1024;
const PREVIEW_RENDER_SCRATCH_MAX_BYTES: usize = 8 * 1024 * 1024;
const PREVIEW_TOTAL_MAX_BYTES: usize = 96 * 1024 * 1024;

fn preview_payload_accounted_bytes(
    resident_bytes: usize,
    retirement_bytes: usize,
    worker_input_bytes: usize,
) -> usize {
    resident_bytes
        .saturating_add(retirement_bytes)
        .saturating_add(worker_input_bytes)
        .saturating_add(PREVIEW_BUILD_OR_RESPONSE_MAX_BYTES + PREVIEW_RENDER_SCRATCH_MAX_BYTES)
}

#[cfg(test)]
fn preview_payload_within_total_budget(
    resident_bytes: usize,
    retirement_bytes: usize,
    worker_input_bytes: usize,
) -> bool {
    preview_payload_accounted_bytes(resident_bytes, retirement_bytes, worker_input_bytes)
        <= PREVIEW_TOTAL_MAX_BYTES
}

impl FlistWalkerApp {
    pub(super) fn enforce_preview_payload_budget(
        &mut self,
        incoming: Option<&Arc<PagedTextPreview>>,
        replacing_active: bool,
    ) -> bool {
        let active = if replacing_active {
            None
        } else {
            self.shell.runtime.preview_document.as_ref()
        };
        let mut parked = self
            .parked_preview_request
            .as_ref()
            .and_then(|request| request.document.as_ref())
            .into_iter()
            .collect::<Vec<_>>();
        parked.extend(
            self.deferred_latest_preview_request
                .as_ref()
                .and_then(|request| request.document.as_ref()),
        );
        parked.extend(
            self.shell
                .worker_bus
                .preview
                .latest_request
                .as_ref()
                .and_then(|request| request.document.as_ref()),
        );
        if !self
            .shell
            .tabs
            .enforce_preview_resident_budget(active, incoming, &parked)
        {
            return false;
        }
        // A continuation can hold both its old input document and a new build.
        // The new build and response mailbox are mutually exclusive because
        // the next request is not dispatched until this response settles.
        self.preview_accounted_payload_bytes(incoming, replacing_active) <= PREVIEW_TOTAL_MAX_BYTES
    }

    pub(super) fn preview_accounted_payload_bytes(
        &self,
        incoming: Option<&Arc<PagedTextPreview>>,
        replacing_active: bool,
    ) -> usize {
        let active = if replacing_active {
            None
        } else {
            self.shell.runtime.preview_document.as_ref()
        };
        let mut parked = self
            .parked_preview_request
            .as_ref()
            .and_then(|request| request.document.as_ref())
            .into_iter()
            .collect::<Vec<_>>();
        parked.extend(
            self.deferred_latest_preview_request
                .as_ref()
                .and_then(|request| request.document.as_ref()),
        );
        parked.extend(
            self.shell
                .worker_bus
                .preview
                .latest_request
                .as_ref()
                .and_then(|request| request.document.as_ref()),
        );
        preview_payload_accounted_bytes(
            self.shell
                .tabs
                .preview_resident_bytes(active, incoming, &parked),
            self.shell.runtime.preview_retirement_bytes(),
            self.shell.worker_bus.preview.worker_input_document_bytes,
        )
    }

    pub(super) fn restore_paged_preview_view_for_active(&mut self) {
        let Some(request_id) = self.shell.worker_bus.preview.pending_request_id else {
            return;
        };
        if !self.shell.worker_bus.preview.in_progress {
            return;
        }
        let Some(tab_id) = self.current_tab_id() else {
            return;
        };
        if self.shell.tabs.preview_request_tab(request_id) != Some(tab_id) {
            return;
        }
        let Some(path) = self
            .shell
            .runtime
            .current_row
            .and_then(|row| self.shell.runtime.results.get(row))
            .map(|(path, _)| path.clone())
        else {
            return;
        };
        self.paged_preview_view.tab_id = Some(tab_id);
        self.paged_preview_view.path = Some(path);
        self.paged_preview_view.request_id = Some(request_id);
        self.paged_preview_view.busy = true;
    }

    pub(super) fn paged_preview_response_matches_active(&self, response: &PreviewResponse) -> bool {
        self.paged_preview_view.request_id == Some(response.request_id)
            && self.paged_preview_view.tab_id == self.current_tab_id()
            && self.paged_preview_view.path.as_deref() == Some(response.path.as_path())
    }

    pub(super) fn clear_paged_preview(&mut self) {
        self.deferred_more_intent = None;
        self.paged_preview_view.error = None;
        self.paged_preview_view.busy = false;
        self.paged_preview_view.tab_id = None;
        self.paged_preview_view.path = None;
        self.paged_preview_view.request_id = None;
    }

    pub(super) fn prepare_paged_preview_initial(&mut self, path: &Path, request_id: u64) {
        self.paged_preview_view.error = None;
        self.paged_preview_view.busy = true;
        self.paged_preview_view.tab_id = self.current_tab_id();
        self.paged_preview_view.path = Some(path.to_path_buf());
        self.paged_preview_view.request_id = Some(request_id);
        self.paged_preview_view.display_generation =
            self.paged_preview_view.display_generation.saturating_add(1);
    }

    pub(super) fn request_paged_preview_more(&mut self) {
        if self.paged_preview_view.busy {
            return;
        }
        let Some(document) = self.paged_preview_for_current().cloned() else {
            return;
        };
        if document.state() != PreviewPageState::More
            || self
                .paged_preview_view
                .error
                .is_some_and(permanent_page_error)
        {
            return;
        }
        let Some(tab_id) = self.current_tab_id() else {
            return;
        };
        let path = document.header.path.clone();
        if self.parked_preview_request.is_some() || self.deferred_preview_response.is_some() {
            self.deferred_more_intent = Some((tab_id, path));
            self.paged_preview_view.busy = true;
            return;
        }
        let request_id = self.shell.worker_bus.preview.begin_request();
        self.bind_preview_request_to_current_tab(request_id);
        let request = PreviewRequest {
            request_id,
            path: path.clone(),
            is_dir: false,
            document: Some(document),
        };
        if !self.queue_preview_request(request) {
            self.fail_preview_worker();
            return;
        }
        self.paged_preview_view.busy = true;
        self.paged_preview_view.error = None;
        self.shell.runtime.set_preview_page_error(None);
        self.paged_preview_view.tab_id = Some(tab_id);
        self.paged_preview_view.path = Some(path);
        self.paged_preview_view.request_id = Some(request_id);
    }

    pub(super) fn reload_paged_preview(&mut self) {
        self.request_preview_for_current();
    }

    pub(super) fn apply_paged_preview_response(&mut self, response: &PreviewResponse) {
        if self.paged_preview_view.request_id != Some(response.request_id)
            || self.paged_preview_view.tab_id != self.current_tab_id()
            || self.paged_preview_view.path.as_deref() != Some(response.path.as_path())
        {
            return;
        }
        self.paged_preview_view.busy = false;
        self.paged_preview_view.request_id = None;
        match (&response.document, response.page_error) {
            (Some(document), _) => {
                self.shell
                    .runtime
                    .set_preview_document(Arc::clone(document))
                    .expect("preview response replacement admitted before apply");
                self.paged_preview_view.error = None;
            }
            (None, Some(error)) => {
                self.paged_preview_view.error = Some(error);
                if !response.is_more {
                    self.shell
                        .runtime
                        .set_preview(format!("<preview {}>", page_error_label(error)));
                }
                self.shell.runtime.set_preview_page_error(Some(error));
            }
            (None, None) => self.clear_paged_preview(),
        }
    }

    pub(super) fn paged_preview_for_current(&self) -> Option<&Arc<PagedTextPreview>> {
        if !self.shell.ui.show_preview {
            return None;
        }
        let path = self
            .shell
            .runtime
            .current_row
            .and_then(|row| self.shell.runtime.results.get(row))
            .map(|(path, _)| path.as_path())?;
        if self.shell.runtime.preview_stale {
            return None;
        }
        let document = self.shell.runtime.preview_document.as_ref()?;
        (document.header.path == path).then_some(document)
    }
}

pub(super) fn permanent_page_error(error: PreviewPageError) -> bool {
    matches!(
        error,
        PreviewPageError::Binary
            | PreviewPageError::DecodeFailed
            | PreviewPageError::Changed
            | PreviewPageError::LimitReached
            | PreviewPageError::OnDemandSkipped
            | PreviewPageError::Empty
    )
}

pub(super) fn page_error_label(error: PreviewPageError) -> &'static str {
    match error {
        PreviewPageError::Empty => "empty file",
        PreviewPageError::Binary => "binary content",
        PreviewPageError::DecodeFailed => "text decoding failed",
        PreviewPageError::PermissionDenied => "permission denied",
        PreviewPageError::NotFound => "file not found",
        PreviewPageError::OnDemandSkipped => "on-demand file skipped",
        PreviewPageError::ReadFailed => "read failed",
        PreviewPageError::Changed => "file changed; reload to start over",
        PreviewPageError::LimitReached => "display limit reached",
        PreviewPageError::Canceled => "canceled",
    }
}

#[cfg(test)]
mod budget_tests {
    use super::preview_payload_within_total_budget;

    #[test]
    fn total_payload_admission_respects_exact_96_mib_boundary() {
        const MIB: usize = 1024 * 1024;
        assert!(preview_payload_within_total_budget(
            32 * MIB,
            40 * MIB,
            8 * MIB
        ));
        assert!(!preview_payload_within_total_budget(
            32 * MIB,
            40 * MIB + 1,
            8 * MIB
        ));
        assert!(!preview_payload_within_total_budget(
            32 * MIB,
            40 * MIB,
            8 * MIB + 1
        ));
        assert!(!preview_payload_within_total_budget(
            32 * MIB + 1,
            40 * MIB,
            8 * MIB
        ));
    }
}
