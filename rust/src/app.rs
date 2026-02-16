use crate::actions::execute_or_open;
use crate::indexer::{build_index_with_metadata, write_filelist, IndexBuildResult, IndexSource};
use crate::search::search_entries;
use crate::ui_model::{build_preview_text, display_path, has_visible_match, match_positions_for_path};
use anyhow::Result;
use eframe::egui;
use std::collections::HashSet;
use std::path::PathBuf;

pub struct FastFileFinderApp {
    root: PathBuf,
    limit: usize,
    query: String,
    use_filelist: bool,
    use_regex: bool,
    include_files: bool,
    include_dirs: bool,
    index: IndexBuildResult,
    entries: Vec<PathBuf>,
    results: Vec<(PathBuf, f64)>,
    pinned_paths: HashSet<PathBuf>,
    current_row: Option<usize>,
    preview: String,
    status: String,
}

impl FastFileFinderApp {
    pub fn new(root: PathBuf, limit: usize, query: String) -> Self {
        let mut app = Self {
            root,
            limit: limit.clamp(1, 1000),
            query,
            use_filelist: true,
            use_regex: false,
            include_files: true,
            include_dirs: true,
            index: IndexBuildResult {
                entries: Vec::new(),
                source: IndexSource::None,
            },
            entries: Vec::new(),
            results: Vec::new(),
            pinned_paths: HashSet::new(),
            current_row: None,
            preview: String::new(),
            status: "Initializing...".to_string(),
        };
        let _ = app.refresh_index();
        app
    }

    fn refresh_index(&mut self) -> Result<()> {
        self.index = build_index_with_metadata(
            &self.root,
            self.use_filelist,
            self.include_files,
            self.include_dirs,
        )?;
        self.entries = self.index.entries.clone();
        self.update_results();
        Ok(())
    }

    fn update_results(&mut self) {
        if self.query.trim().is_empty() {
            self.results = self
                .entries
                .iter()
                .take(self.limit)
                .cloned()
                .map(|p| (p, 0.0))
                .collect();
        } else {
            self.results = search_entries(&self.query, &self.entries, self.limit, self.use_regex)
                .into_iter()
                .filter(|(p, _)| has_visible_match(p, &self.root, &self.query))
                .collect();
        }

        if self.results.is_empty() {
            self.current_row = None;
            self.preview.clear();
        } else {
            if self.current_row.is_none() {
                self.current_row = Some(0);
            }
            self.set_preview_from_current();
        }

        let clipped = self.results.len() >= self.limit;
        let clip_text = if clipped {
            format!(" (limit {} reached)", self.limit)
        } else {
            String::new()
        };
        let pinned = if self.pinned_paths.is_empty() {
            String::new()
        } else {
            format!(" | Pinned: {}", self.pinned_paths.len())
        };

        self.status = format!(
            "Entries: {} | Results: {}{}{}",
            self.entries.len(),
            self.results.len(),
            clip_text,
            pinned
        );
    }

    fn set_preview_from_current(&mut self) {
        if let Some(row) = self.current_row {
            if let Some((path, _)) = self.results.get(row) {
                self.preview = build_preview_text(path);
                return;
            }
        }
        self.preview.clear();
    }

    fn move_row(&mut self, delta: isize) {
        if self.results.is_empty() {
            return;
        }
        let row = self.current_row.unwrap_or(0) as isize;
        let next = (row + delta).clamp(0, self.results.len() as isize - 1) as usize;
        self.current_row = Some(next);
        self.set_preview_from_current();
    }

    fn toggle_pin_and_move(&mut self, delta: isize) {
        if let Some(row) = self.current_row {
            if let Some((path, _)) = self.results.get(row) {
                if self.pinned_paths.contains(path) {
                    self.pinned_paths.remove(path);
                } else {
                    self.pinned_paths.insert(path.clone());
                }
            }
        }
        self.move_row(delta);
        self.update_results();
    }

    fn selected_paths(&self) -> Vec<PathBuf> {
        if !self.pinned_paths.is_empty() {
            let mut out: Vec<PathBuf> = self.pinned_paths.iter().cloned().collect();
            out.sort();
            return out;
        }
        self.current_row
            .and_then(|row| self.results.get(row).map(|(p, _)| vec![p.clone()]))
            .unwrap_or_default()
    }

    fn execute_selected(&mut self) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }

        for path in &paths {
            if let Err(err) = execute_or_open(path) {
                self.status = format!("Action failed: {}", err);
                return;
            }
        }

        if paths.len() == 1 {
            self.status = format!("Action: {}", paths[0].display());
        } else {
            self.status = format!("Action: launched {} items", paths.len());
        }
    }

    fn copy_selected_paths(&mut self, ctx: &egui::Context) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let text = paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        ctx.output_mut(|o| o.copied_text = text);
        if paths.len() == 1 {
            self.status = format!("Copied path: {}", paths[0].display());
        } else {
            self.status = format!("Copied {} paths to clipboard", paths.len());
        }
    }

    fn clear_pinned(&mut self) {
        self.pinned_paths.clear();
        self.update_results();
        self.status = "Cleared pinned selections".to_string();
    }

    fn create_filelist(&mut self) {
        match build_index_with_metadata(
            &self.root,
            false,
            self.include_files,
            self.include_dirs,
        )
        .and_then(|snapshot| {
            let count = snapshot.entries.len();
            write_filelist(&self.root, &snapshot.entries, "FileList.txt")
                .map(|p| (p, count))
        }) {
            Ok((path, count)) => {
                self.status = format!("Created {}: {} entries", path.display(), count);
                if self.use_filelist {
                    let _ = self.refresh_index();
                }
            }
            Err(err) => {
                self.status = format!("Create File List failed: {}", err);
            }
        }
    }

    fn source_text(&self) -> String {
        match &self.index.source {
            IndexSource::FileList(path) => format!("Source: FileList ({})", path.file_name().and_then(|s| s.to_str()).unwrap_or("FileList.txt")),
            IndexSource::Walker => "Source: Walker".to_string(),
            IndexSource::None => "Source: None".to_string(),
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown))
            || ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::N))
        {
            self.move_row(1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp))
            || ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::P))
        {
            self.move_row(-1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Tab) && !i.modifiers.shift) {
            self.toggle_pin_and_move(1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Tab) && i.modifiers.shift) {
            self.toggle_pin_and_move(-1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Enter))
            || ctx.input(|i| i.modifiers.ctrl && (i.key_pressed(egui::Key::J) || i.key_pressed(egui::Key::M)))
        {
            self.execute_selected();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::C)) {
            self.copy_selected_paths(ctx);
        }
    }
}

impl eframe::App for FastFileFinderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ctx);

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Root:");
                let mut root_text = self.root.to_string_lossy().to_string();
                ui.add_enabled(
                    false,
                    egui::TextEdit::singleline(&mut root_text).desired_width(f32::INFINITY),
                );
                if ui.button("Browse...").clicked() {
                    if let Ok(Some(dir)) = native_dialog::FileDialog::new()
                        .set_location(&self.root)
                        .show_open_single_dir()
                    {
                        self.root = dir;
                        if let Err(err) = self.refresh_index() {
                            self.status = format!("Indexing failed: {}", err);
                        }
                    }
                }
            });

            ui.horizontal(|ui| {
                let mut changed = false;
                changed |= ui.checkbox(&mut self.use_filelist, "Use FileList").changed();
                ui.checkbox(&mut self.use_regex, "Regex");
                changed |= ui.checkbox(&mut self.include_files, "Files").changed();
                changed |= ui.checkbox(&mut self.include_dirs, "Folders").changed();
                if !self.include_files && !self.include_dirs {
                    self.include_files = true;
                }
                ui.separator();
                ui.label(self.source_text());
                if changed {
                    if let Err(err) = self.refresh_index() {
                        self.status = format!("Indexing failed: {}", err);
                    }
                }
            });

            let response = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .desired_width(f32::INFINITY)
                    .hint_text("Type to fuzzy-search files/folders..."),
            );
            if response.changed() {
                self.update_results();
            }

            ui.horizontal(|ui| {
                if ui.button("Open / Execute").clicked() {
                    self.execute_selected();
                }
                if ui.button("Copy Path(s)").clicked() {
                    self.copy_selected_paths(ctx);
                }
                if ui.button("Clear Selected").clicked() {
                    self.clear_pinned();
                }
                if ui.button("Create File List").clicked() {
                    self.create_filelist();
                }
                if ui.button("Refresh Index").clicked() {
                    if let Err(err) = self.refresh_index() {
                        self.status = format!("Indexing failed: {}", err);
                    }
                }
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(&self.status);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |cols| {
                cols[0].heading("Results");
                egui::ScrollArea::vertical().show(&mut cols[0], |ui| {
                    let mut clicked_row: Option<usize> = None;
                    let mut execute_row: Option<usize> = None;
                    for (i, (path, _score)) in self.results.iter().enumerate() {
                        let is_current = self.current_row == Some(i);
                        let is_pinned = self.pinned_paths.contains(path);
                        let marker_current = if is_current { "▶" } else { "·" };
                        let marker_pin = if is_pinned { "◆" } else { "·" };
                        let display = display_path(path, &self.root);
                        let positions = match_positions_for_path(path, &self.root, &self.query);

                        let mut job = egui::text::LayoutJob::default();
                        job.append(
                            &format!("{} {} ", marker_current, marker_pin),
                            0.0,
                            egui::TextFormat {
                                color: if is_current { egui::Color32::LIGHT_BLUE } else { egui::Color32::GRAY },
                                ..Default::default()
                            },
                        );
                        let kind = if path.is_dir() { "DIR " } else { "FILE" };
                        job.append(
                            kind,
                            0.0,
                            egui::TextFormat {
                                color: if path.is_dir() { egui::Color32::from_rgb(52, 211, 153) } else { egui::Color32::from_rgb(96, 165, 250) },
                                ..Default::default()
                            },
                        );
                        job.append(" ", 0.0, egui::TextFormat::default());

                        for (idx, ch) in display.chars().enumerate() {
                            let color = if positions.contains(&idx) {
                                egui::Color32::from_rgb(245, 158, 11)
                            } else {
                                egui::Color32::from_rgb(229, 231, 235)
                            };
                            job.append(
                                &ch.to_string(),
                                0.0,
                                egui::TextFormat {
                                    color,
                                    ..Default::default()
                                },
                            );
                        }

                        let response = ui.add(egui::Label::new(job).sense(egui::Sense::click()));
                        if response.clicked() {
                            clicked_row = Some(i);
                        }
                        if response.double_clicked() {
                            execute_row = Some(i);
                        }
                    }
                    if let Some(i) = clicked_row {
                        self.current_row = Some(i);
                        self.set_preview_from_current();
                    }
                    if let Some(i) = execute_row {
                        self.current_row = Some(i);
                        self.execute_selected();
                    }
                });

                cols[1].heading("Preview");
                cols[1].add(
                    egui::TextEdit::multiline(&mut self.preview)
                        .desired_rows(30)
                        .interactive(false),
                );
            });
        });
    }
}
