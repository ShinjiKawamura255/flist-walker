use super::worker::browse::{picker_start_location, BrowseService};
use super::FlistWalkerApp;
use crate::path_utils::{normalize_path_for_display, normalize_windows_path_buf};
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct BrowseState {
    pub(super) service: Option<BrowseService>,
    pending: Option<(u64, BrowseTarget)>,
}

enum BrowseTarget {
    Tab {
        id: u64,
        root: PathBuf,
        new_tab: bool,
    },
    Manage {
        generation: u64,
        input: String,
    },
    Preset {
        input: String,
    },
    NamedRoot {
        input: String,
    },
}

impl FlistWalkerApp {
    pub(super) fn cancel_root_browse(&mut self) {
        // Keep worker admission occupied until its response settles. Cancellation
        // cannot spawn replacement threads while an OS metadata call is blocked.
        self.shell.features.root_browser.browse.pending = None;
        if let Some(service) = &self.shell.features.root_browser.browse.service {
            service.cancel();
        }
        if self.shell.runtime.notice == "Preparing folder picker..." {
            self.set_notice("");
        }
    }

    fn start_root_browse(&mut self, target: BrowseTarget, input: &str) {
        if self.shell.features.root_browser.browse.service.is_none() {
            let Some(runtime) = self.shell.worker_runtime.as_mut() else {
                return;
            };
            let (service, handle) =
                BrowseService::spawn_with(runtime.shutdown_token(), picker_start_location);
            runtime.push("browse-location", handle);
            self.shell.features.root_browser.browse.service = Some(service);
        }
        let root = if input.trim().is_empty() {
            self.shell.runtime.root.clone()
        } else {
            PathBuf::from(input.trim())
        };
        match self
            .shell
            .features
            .root_browser
            .browse
            .service
            .as_mut()
            .unwrap()
            .start(root)
        {
            Ok(id) => {
                self.shell.features.root_browser.browse.pending = Some((id, target));
                self.set_notice("Preparing folder picker...");
            }
            Err(error) => self.set_notice(error),
        }
    }

    pub(super) fn browse_for_root(&mut self) {
        self.browse_for_tab(false);
    }
    pub(super) fn browse_for_root_in_new_tab(&mut self) {
        self.browse_for_tab(true);
    }
    fn browse_for_tab(&mut self, new_tab: bool) {
        let Some(id) = self.current_tab_id() else {
            return;
        };
        self.start_root_browse(
            BrowseTarget::Tab {
                id,
                root: self.shell.runtime.root.clone(),
                new_tab,
            },
            "",
        );
    }

    pub(super) fn browse_for_manage_root_list(&mut self) {
        if self.reject_manage_root_list_mutation_while_saving() {
            return;
        }
        let manage = &self.shell.features.root_browser.manage_list;
        let input = manage.input_path.clone();
        let target = BrowseTarget::Manage {
            generation: manage.dialog_generation,
            input: input.clone(),
        };
        self.start_root_browse(target, &input);
    }

    pub(super) fn browse_for_preset_editor_root(&mut self) {
        let editor = &self.shell.features.presets.picker.editor;
        if self.shell.worker_bus.catalog.in_progress || !editor.open {
            return;
        }
        let input = editor.root_path.clone();
        self.start_root_browse(
            BrowseTarget::Preset {
                input: input.clone(),
            },
            &input,
        );
    }

    pub(super) fn browse_for_named_root_editor_path(&mut self) {
        let editor = &self.shell.features.presets.picker.named_roots.editor;
        if self.shell.worker_bus.catalog.in_progress || !editor.open {
            return;
        }
        let input = editor.path.clone();
        self.start_root_browse(
            BrowseTarget::NamedRoot {
                input: input.clone(),
            },
            &input,
        );
    }

    fn root_browse_target_current(&self, target: &BrowseTarget) -> bool {
        match target {
            BrowseTarget::Tab { id, root, .. } => {
                self.current_tab_id() == Some(*id) && &self.shell.runtime.root == root
            }
            BrowseTarget::Manage { generation, input } => {
                let manage = &self.shell.features.root_browser.manage_list;
                manage.open
                    && manage.dialog_generation == *generation
                    && manage.input_path == *input
                    && !manage.remove_mode
                    && manage.editing_index.is_none()
                    && !self.settings_commit_in_progress()
            }
            BrowseTarget::Preset { input } => {
                let editor = &self.shell.features.presets.picker.editor;
                editor.open
                    && editor.root_path == *input
                    && !self.shell.worker_bus.catalog.in_progress
            }
            BrowseTarget::NamedRoot { input } => {
                let editor = &self.shell.features.presets.picker.named_roots.editor;
                editor.open && editor.path == *input && !self.shell.worker_bus.catalog.in_progress
            }
        }
    }

    pub(super) fn poll_root_browse(&mut self) {
        if super::process_shutdown_requested()
            || self.shell.features.update.state.close_requested_for_install
            || self.shell.features.update.state.close_after_update_terminal
        {
            self.cancel_root_browse();
        }
        let Some(service) = &mut self.shell.features.root_browser.browse.service else {
            return;
        };
        let Some((id, location)) = service.poll() else {
            return;
        };
        if self.shell.runtime.notice == "Preparing folder picker..." {
            self.set_notice("");
        }
        let Some((expected, target)) = self.shell.features.root_browser.browse.pending.take()
        else {
            return;
        };
        if id != expected || !self.root_browse_target_current(&target) {
            return;
        }
        // Only filesystem preparation runs on the worker. The established native
        // modal stays on its platform-required UI thread; no Rust I/O precedes it.
        let result = location
            .map_err(str::to_string)
            .and_then(|location| self.select_root_via_dialog(&location));
        match (target, result) {
            (BrowseTarget::Tab { new_tab, .. }, Ok(Some(path))) => {
                if new_tab {
                    self.create_new_tab();
                }
                self.apply_root_change(path);
            }
            (BrowseTarget::Manage { .. }, Ok(Some(path))) => {
                let input = normalize_windows_path_buf(path)
                    .to_string_lossy()
                    .to_string();
                self.shell.features.root_browser.manage_list.input_path = input.clone();
                self.request_manage_root_validation(super::RootValidationIntent::Add, input);
            }
            (BrowseTarget::Preset { .. }, Ok(Some(path))) => {
                let editor = &mut self.shell.features.presets.picker.editor;
                editor.root_path = normalize_path_for_display(&path);
                editor.error.clear();
            }
            (BrowseTarget::NamedRoot { .. }, Ok(Some(path))) => {
                let editor = &mut self.shell.features.presets.picker.named_roots.editor;
                editor.path = normalize_path_for_display(&path);
                editor.error.clear();
            }
            (target, Err(error)) => {
                let message = format!("Browse failed: {error}");
                match target {
                    BrowseTarget::Preset { .. } => {
                        self.shell.features.presets.picker.editor.error = message
                    }
                    BrowseTarget::NamedRoot { .. } => {
                        self.shell.features.presets.picker.named_roots.editor.error = message
                    }
                    BrowseTarget::Manage { .. } => {
                        self.shell.features.root_browser.manage_list.notice = message
                    }
                    BrowseTarget::Tab { .. } => self.set_notice(message),
                }
            }
            (_, Ok(None)) => self.set_notice("Folder picker canceled"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{settle_root_browse, test_root};
    use std::sync::{atomic::AtomicBool, mpsc, Arc};
    use std::time::{Duration, Instant};

    #[test]
    fn browse_ready_response_does_not_open_picker_during_close_frame() {
        for closing in ["native", "install", "deferred update"] {
            let mut app = FlistWalkerApp::new(test_root("browse-close-frame"), 50, String::new());
            let (service, requests, responses) = BrowseService::controlled_for_test();
            app.shell.features.root_browser.browse.service = Some(service);
            app.browse_for_root();
            let request = requests.recv_timeout(Duration::from_secs(5)).unwrap();
            responses.send(request).unwrap();
            let mut input = eframe::egui::RawInput::default();
            match closing {
                "native" => input
                    .viewports
                    .get_mut(&eframe::egui::ViewportId::ROOT)
                    .unwrap()
                    .events
                    .push(eframe::egui::ViewportEvent::Close),
                "install" => app.shell.features.update.state.close_requested_for_install = true,
                _ => app.shell.features.update.state.close_after_update_terminal = true,
            }
            let context = eframe::egui::Context::default();
            let _ = context.run_ui(input, |ui| {
                app.run_update_cycle(ui);
            });
            assert!(
                app.shell
                    .features
                    .root_browser
                    .last_browse_dialog_root
                    .is_none(),
                "picker opened during {closing}"
            );
            assert!(
                app.shell.features.root_browser.browse.pending.is_none(),
                "Browse owner survived {closing}"
            );
        }
    }

    fn delayed_app() -> (FlistWalkerApp, mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (entered, started) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let mut app = FlistWalkerApp::new(test_root("delayed-browse"), 50, String::new());
        let (service, handle) =
            BrowseService::spawn_with(Arc::new(AtomicBool::new(false)), move |path| {
                entered.send(()).unwrap();
                let _ = gate.recv_timeout(Duration::from_secs(5));
                path.to_path_buf()
            });
        app.shell
            .worker_runtime
            .as_mut()
            .unwrap()
            .push("browse-location", handle);
        app.shell.features.root_browser.browse.service = Some(service);
        app.shell.features.root_browser.browse_dialog_result = Some(Ok(Some("selected".into())));
        (app, started, release)
    }

    #[test]
    fn browse_delayed_probe_keeps_frames_responsive_and_discards_old_tab() {
        let (mut app, started, release) = delayed_app();
        let context = eframe::egui::Context::default();
        // Initialize font/layout resources before the interaction under test.
        // A cold egui frame is startup work, not time spent awaiting Browse I/O.
        let _ = context.run_ui(Default::default(), |ui| app.run_ui_frame(ui));
        context.memory_mut(|memory| memory.request_focus(app.shell.ui.query_input_id));
        let begin = Instant::now();
        app.browse_for_root();
        let dispatch_elapsed = begin.elapsed();
        assert!(
            dispatch_elapsed < Duration::from_millis(100),
            "Browse dispatch took {dispatch_elapsed:?}"
        );
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let input = eframe::egui::RawInput {
            events: vec![eframe::egui::Event::Text("responsive".into())],
            ..Default::default()
        };
        let begin = Instant::now();
        let _ = context.run_ui(input, |ui| app.run_ui_frame(ui));
        let frame_elapsed = begin.elapsed();
        assert!(
            frame_elapsed < Duration::from_millis(100),
            "Browse-blocked interaction frame took {frame_elapsed:?}"
        );
        assert_eq!(app.shell.runtime.query_state.query, "responsive");
        assert!(app
            .shell
            .features
            .root_browser
            .last_browse_dialog_root
            .is_none());
        app.create_new_tab();
        let current = app.current_tab_id();
        release.send(()).unwrap();
        settle_root_browse(&mut app);
        assert_eq!(app.current_tab_id(), current);
        assert!(app
            .shell
            .features
            .root_browser
            .last_browse_dialog_root
            .is_none());
    }

    #[test]
    fn browse_closed_and_reopened_editor_rejects_equal_old_draft() {
        let (mut app, started, release) = delayed_app();
        app.start_add_preset();
        app.browse_for_preset_editor_root();
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let input = app.shell.features.presets.picker.editor.root_path.clone();
        app.cancel_preset_edit();
        app.start_add_preset();
        assert_eq!(app.shell.features.presets.picker.editor.root_path, input);
        app.browse_for_preset_editor_root();
        assert!(app.shell.runtime.notice.contains("already in progress"));
        release.send(()).unwrap();
        settle_root_browse(&mut app);
        assert_eq!(app.shell.features.presets.picker.editor.root_path, input);
        assert!(app
            .shell
            .features
            .root_browser
            .last_browse_dialog_root
            .is_none());
    }

    #[test]
    fn browse_editing_path_while_probe_runs_preserves_latest_input() {
        let (mut app, started, release) = delayed_app();
        app.start_add_named_root();
        app.browse_for_named_root_editor_path();
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        app.shell.features.presets.picker.named_roots.editor.path = "new draft".into();
        release.send(()).unwrap();
        settle_root_browse(&mut app);
        assert_eq!(
            app.shell.features.presets.picker.named_roots.editor.path,
            "new draft"
        );
        assert!(app
            .shell
            .features
            .root_browser
            .last_browse_dialog_root
            .is_none());
    }
}
