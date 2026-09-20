use super::{normalize_path_for_display, FlistWalkerApp};
use eframe::egui;
use std::collections::BTreeSet;
use std::ops::Bound::{Excluded, Included, Unbounded};
use std::path::PathBuf;

const PAGE_SIZE: usize = 100;

/// Only the page cursor is retained. No full-selection clone or scan occurs on the UI thread.
pub(super) struct SelectionInspector {
    tab_id: Option<u64>,
    root: PathBuf,
    first: Option<PathBuf>,
    reset_scroll: bool,
    focus_close: bool,
    navigation_keys: Vec<egui::Event>,
}

fn selection_page(pins: &BTreeSet<PathBuf>, first: Option<&PathBuf>) -> Vec<PathBuf> {
    let start = first.map_or(Unbounded, Included);
    pins.range::<PathBuf, _>((start, Unbounded))
        .take(PAGE_SIZE)
        .cloned()
        .collect()
}

fn previous_page_start(pins: &BTreeSet<PathBuf>, first: &PathBuf) -> Option<PathBuf> {
    pins.range::<PathBuf, _>(..first)
        .rev()
        .take(PAGE_SIZE)
        .last()
        .cloned()
}

impl FlistWalkerApp {
    pub(super) fn open_selection_inspector(&mut self) {
        self.shell.ui.selection_inspector = Some(SelectionInspector {
            tab_id: self.current_tab_id(),
            root: self.shell.runtime.root.clone(),
            first: None,
            reset_scroll: true,
            focus_close: true,
            navigation_keys: Vec::new(),
        });
    }

    pub(super) fn handle_selection_inspector_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        if self.shell.ui.selection_inspector.is_none() {
            return false;
        }
        let close = self.consume_gui_cancel(ctx);
        if !close {
            let navigation_keys = ctx.input(|input| {
                input.events.iter().filter(|event| {
                    matches!(event,
                        egui::Event::Key { key: egui::Key::Tab, modifiers, .. }
                            if modifiers.is_none() || modifiers.shift_only()
                    ) || matches!(event,
                        egui::Event::Key { key: egui::Key::Enter | egui::Key::Space, modifiers, .. }
                            if modifiers.is_none()
                    )
                }).cloned().collect()
            });
            if let Some(inspector) = self.shell.ui.selection_inspector.as_mut() {
                inspector.navigation_keys = navigation_keys;
            }
        }
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::Copy
                        | egui::Event::Cut
                        | egui::Event::Paste(_)
                        | egui::Event::Text(_)
                        | egui::Event::Key { .. }
                        | egui::Event::Ime(_)
                )
            });
        });
        // The backing query's IME adapter also reads key-down edges independently of events.
        self.shell.ui.prev_space_down = ctx.input(|input| input.key_down(egui::Key::Space));
        self.shell.ui.ime_composition_active = false;
        if close {
            self.shell.ui.selection_inspector = None;
            self.request_focus_query();
        }
        // The caller must stop ordinary query/action shortcuts while the modal owns input.
        true
    }

    pub(super) fn remove_inspected_pin(&mut self, path: &PathBuf) {
        if self.shell.runtime.pinned_paths.remove(path) {
            self.shell.tabs.mark_active_tab_meaningfully_engaged();
            self.set_notice(format!(
                "Removed selection: {}",
                normalize_path_for_display(path)
            ));
        }
    }

    pub(super) fn render_selection_inspector(&mut self, ctx: &egui::Context) {
        let Some(mut inspector) = self.shell.ui.selection_inspector.take() else {
            return;
        };
        if inspector.tab_id != self.current_tab_id() || inspector.root != self.shell.runtime.root {
            return;
        }
        // Expose navigation/activation only to this modal, never to the backing query.
        ctx.input_mut(|input| input.events.append(&mut inspector.navigation_keys));
        let pins = &self.shell.runtime.pinned_paths;
        let mut page = selection_page(pins, inspector.first.as_ref());
        // A background refresh or removal can empty the last page; return to the first page.
        if page.is_empty() && !pins.is_empty() {
            inspector.first = None;
            inspector.reset_scroll = true;
            page = selection_page(pins, None);
        }
        let previous = page
            .first()
            .and_then(|first| previous_page_start(pins, first));
        let next = page.last().and_then(|last| {
            pins.range::<PathBuf, _>((Excluded(last), Unbounded))
                .next()
                .cloned()
        });
        let count = pins.len();
        let mut remove = None;
        let mut close = false;
        let available_width = ctx.input(|input| input.content_rect().width());
        egui::Modal::new(egui::Id::new("selected-paths-modal")).show(ctx, |ui| {
            ui.set_width((available_width - 48.0).clamp(160.0, 600.0));
            ui.heading(format!("Selected items ({count})"));
            ui.label("Open selected and Copy selected include items hidden by the current search.");
            if page.is_empty() {
                ui.label("No selected items.");
            }
            let row_height = ui.spacing().interact_size.y;
            let mut scroll = egui::ScrollArea::vertical()
                .id_salt("selected-paths-scroll")
                .max_height(320.0);
            if inspector.reset_scroll {
                scroll = scroll.vertical_scroll_offset(0.0);
                inspector.reset_scroll = false;
            }
            scroll.show_rows(ui, row_height, page.len(), |ui, rows| {
                for index in rows {
                    let path = &page[index];
                    ui.push_id(path, |ui| {
                        ui.horizontal(|ui| {
                            if ui.small_button("Remove").clicked() {
                                remove = Some(path.clone());
                            }
                            let display = normalize_path_for_display(path);
                            ui.add(egui::Label::new(&display).truncate())
                                .on_hover_text(display);
                        });
                    });
                }
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(previous.is_some(), egui::Button::new("Previous"))
                    .clicked()
                {
                    inspector.first = previous;
                    inspector.reset_scroll = true;
                }
                if ui
                    .add_enabled(next.is_some(), egui::Button::new("Next"))
                    .clicked()
                {
                    inspector.first = next;
                    inspector.reset_scroll = true;
                }
                ui.label(format!("{} shown · up to {PAGE_SIZE} per page", page.len()));
            });
            let close_button = ui.button("Close");
            if close_button.clicked() {
                close = true;
            }
            if inspector.focus_close {
                close_button.request_focus();
                inspector.focus_close = false;
            }
        });
        ctx.input_mut(|input| {
            input
                .events
                .retain(|event| !matches!(event, egui::Event::Key { .. }));
        });
        if let Some(path) = remove {
            self.remove_inspected_pin(&path);
        }
        if close {
            self.request_focus_query();
        } else {
            self.shell.ui.selection_inspector = Some(inspector);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ux_pin_pages_are_bounded_ordered_and_reversible() {
        let pins: BTreeSet<_> = (0..20_000)
            .map(|n| PathBuf::from(format!("item-{n:05}")))
            .collect();
        let first = selection_page(&pins, None);
        assert_eq!(first.len(), PAGE_SIZE);
        assert_eq!(first[0], PathBuf::from("item-00000"));
        let second_start = pins
            .range::<PathBuf, _>((Excluded(first.last().unwrap()), Unbounded))
            .next()
            .unwrap();
        let second = selection_page(&pins, Some(second_start));
        assert_eq!(second[0], PathBuf::from("item-00100"));
        assert_eq!(
            previous_page_start(&pins, &second[0]),
            Some(first[0].clone())
        );
        let far = selection_page(&pins, Some(&PathBuf::from("item-19995")));
        assert_eq!(far.len(), 5);
    }

    #[test]
    fn ux_pin_page_cursor_survives_removing_its_first_item() {
        let mut pins = BTreeSet::from([PathBuf::from("a"), PathBuf::from("b"), PathBuf::from("c")]);
        let cursor = PathBuf::from("b");
        pins.remove(&cursor);
        assert_eq!(
            selection_page(&pins, Some(&cursor)),
            vec![PathBuf::from("c")]
        );
        assert_eq!(
            previous_page_start(&pins, &cursor),
            Some(PathBuf::from("a"))
        );
    }
}
