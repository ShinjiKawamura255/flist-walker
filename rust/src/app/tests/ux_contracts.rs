//! GUI contract tests use real pointer/key events and capture worker requests at the channel.
//! They do not use render probes that short-circuit production callbacks or launch OS actions.
use super::*;
use crate::app::tab_state::TabResourceTransition;

#[derive(Default)]
struct Gui {
    ctx: egui::Context,
    time: f64,
}

impl Gui {
    fn frame(
        &mut self,
        app: &mut FlistWalkerApp,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> egui::FullOutput {
        self.time += 0.02;
        self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                time: Some(self.time),
                events,
                modifiers,
                ..Default::default()
            },
            |ui| app.run_ui_frame(ui),
        )
    }

    fn idle(&mut self, app: &mut FlistWalkerApp) -> egui::FullOutput {
        self.frame(app, vec![], egui::Modifiers::NONE)
    }

    fn click(
        &mut self,
        app: &mut FlistWalkerApp,
        pos: egui::Pos2,
        modifiers: egui::Modifiers,
    ) -> egui::FullOutput {
        self.frame(
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers,
                },
            ],
            modifiers,
        );
        self.frame(
            app,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
            modifiers,
        )
    }
}

fn maybe_text_rect(
    output: &egui::FullOutput,
    predicate: impl Fn(&str) -> bool,
) -> Option<egui::Rect> {
    fn find(shape: &egui::Shape, predicate: &impl Fn(&str) -> bool) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(text) if predicate(text.galley.text()) => {
                Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, predicate)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape, &predicate))
}

fn text_rect(output: &egui::FullOutput, predicate: impl Fn(&str) -> bool) -> egui::Rect {
    maybe_text_rect(output, predicate).expect("expected text must be painted")
}

fn prepare(app: &mut FlistWalkerApp, paths: &[PathBuf]) {
    reset_index_request_state_for_test(app);
    app.shell.indexing.pending_finish = None;
    app.shell
        .indexing
        .apply_resource_transition(TabResourceTransition::Success);
    app.shell.search.clear_active_request_state();
    app.shell.ui.show_preview = false;
    app.shell.runtime.use_filelist = false;
    let entries = Arc::new(paths.iter().cloned().map(file_entry).collect());
    let committed = app.shell.runtime.committed_for_test_mut();
    committed.entries = Arc::clone(&entries);
    committed.all_entries = entries;
    committed.results = paths.iter().cloned().map(|path| (path, 0.0)).collect();
    committed.base_results = committed.results.clone();
    committed.total_match_count = paths.len();
    committed.current_row = (!paths.is_empty()).then_some(0);
}

#[test]
fn ux_contract_search_failure_remains_visible_during_pending_input_and_after_clear() {
    let scope = test_settings_scope("ux-search-failure");
    let mut app = scope.app(test_root("ux-search-failure-root"), 50, "name:".into());
    prepare(&mut app, &[]);
    let (request_tx, _request_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    app.shell.search = SearchCoordinator::new(request_tx, response_rx);
    app.enqueue_search_request();
    drop(response_tx);
    app.poll_search_response();
    app.set_notice("Unrelated later notice");
    let mut gui = Gui::default();
    gui.idle(&mut app);
    let output = gui.idle(&mut app);
    text_rect(&output, |text| text.contains("Restart FlistWalker"));
    text_rect(&output, |text| text == "Enter a value after name:");
    assert!(maybe_text_rect(&output, |text| text == "Searching...").is_none());
    app.shell.runtime.query_state.query.clear();
    app.shell.runtime.query_state.search_error = None;
    app.update_results();
    let output = gui.idle(&mut app);
    text_rect(&output, |text| text.contains("Restart FlistWalker"));
    for history in [false, true] {
        app.shell.ui.ime_composition_active = !history;
        app.shell.runtime.query_state.history_search_active = history;
        let output = gui.ctx.run_ui(egui::RawInput::default(), |ui| {
            app.render_query_assistance(ui);
        });
        text_rect(&output, |text| text.contains("Restart FlistWalker"));
    }
}

#[test]
fn ux_contract_double_click_dispatches_clicked_path_and_enter_dispatches_pins() {
    for shift in [false, true] {
        let scope = test_settings_scope("ux-contract-pointer");
        let root = test_root("ux-contract-pointer-root");
        let clicked = root.join("clicked.txt");
        let hidden = root.join("hidden-pin.txt");
        let mut app = scope.app(root.clone(), 50, String::new());
        prepare(&mut app, std::slice::from_ref(&clicked));
        app.shell.runtime.pinned_paths.insert(hidden.clone());
        let (tx, rx) = bounded_request_channel::<ActionRequest>(8);
        app.shell.worker_bus.action.tx = tx;
        let mut gui = Gui::default();
        gui.idle(&mut app);
        let output = gui.idle(&mut app);
        let pos = text_rect(&output, |text| text.ends_with("clicked.txt")).center();
        let modifiers = if shift {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        };
        gui.click(&mut app, pos, modifiers);
        assert!(rx.try_recv().is_err(), "single click must not execute");
        gui.click(&mut app, pos, modifiers);
        let request = rx.try_recv().expect("double click must dispatch an action");
        assert_eq!(request.paths, vec![clicked]);
        assert_eq!(request.root, root);
        assert_eq!(request.open_parent_for_files, shift);
        assert!(rx.try_recv().is_err(), "one double click dispatches once");
        gui.frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            modifiers,
        );
        let request = rx.try_recv().expect("Enter must keep PIN-first activation");
        assert_eq!(request.paths, vec![hidden.clone()]);
        assert_eq!(request.open_parent_for_files, shift);
        assert!(app.shell.runtime.pinned_paths.contains(&hidden));
        assert!(rx.try_recv().is_err());
        let output = gui.idle(&mut app);
        let pos = text_rect(&output, |text| text == "Open selected (1)").center();
        gui.click(&mut app, pos, egui::Modifiers::NONE);
        let request = rx
            .try_recv()
            .expect("top action must also use PIN selection");
        assert_eq!(request.paths, vec![hidden]);
        assert!(!request.open_parent_for_files);
        assert!(rx.try_recv().is_err());
    }
}

#[test]
fn ux_contract_sort_dropdown_keeps_all_matches_through_the_selection_frame() {
    for mode in [
        ResultSortMode::ModifiedDesc,
        ResultSortMode::CreatedDesc,
        ResultSortMode::SizeDesc,
    ] {
        let scope = test_settings_scope("ux-contract-sort");
        let root = test_root("ux-contract-sort-root");
        let mut app = scope.app(root.clone(), 1, "item".into());
        prepare(&mut app, &[root.join("item.txt")]);
        let (tx, rx) = mpsc::channel();
        app.shell.search.tx = tx;
        let mut gui = Gui::default();
        gui.idle(&mut app);
        let output = gui.idle(&mut app);
        let pos = text_rect(&output, |text| text == "Score").center();
        gui.click(&mut app, pos, egui::Modifiers::NONE);
        let output = gui.idle(&mut app);
        let pos = text_rect(&output, |text| text == mode.label()).center();
        gui.click(&mut app, pos, egui::Modifiers::NONE);
        assert_eq!(app.shell.runtime.result_sort_mode, mode);
        assert_eq!(
            app.shell.runtime.result_sort_scope,
            ResultSortScope::AllMatches
        );
        let request = rx.try_recv().expect("GUI selection dispatches search");
        assert_eq!(request.sort_mode, mode);
        assert_eq!(request.sort_scope, ResultSortScope::AllMatches);
        assert!(
            rx.try_recv().is_err(),
            "scope must not be reverted by the same frame"
        );
        let output = gui.idle(&mut app);
        assert_eq!(
            app.shell.runtime.result_sort_scope,
            ResultSortScope::AllMatches
        );
        let pos = text_rect(&output, |text| text == "All matches").center();
        gui.click(&mut app, pos, egui::Modifiers::NONE);
        let output = gui.idle(&mut app);
        let pos = text_rect(&output, |text| text == "Shown results").center();
        gui.click(&mut app, pos, egui::Modifiers::NONE);
        assert_eq!(
            app.shell.runtime.result_sort_scope,
            ResultSortScope::ShownResults
        );
        let request = rx.try_recv().expect("explicit Shown search");
        assert_eq!(request.sort_scope, ResultSortScope::ShownResults);
        let output = gui.idle(&mut app);
        text_rect(&output, |text| text == "Sorting shown 1 results only");
    }
}

#[test]
fn ux_contract_typo_advice_and_query_error_are_both_painted() {
    let scope = test_settings_scope("ux-contract-error");
    let root = test_root("ux-contract-error-root");
    let mut app = scope.app(root.clone(), 50, "neme:report [".into());
    prepare(&mut app, &[root.join("report.txt")]);
    app.shell.runtime.use_regex = true;
    let (tx, rx) = mpsc::channel();
    app.shell.search.tx = tx;
    app.enqueue_search_request();
    let request = rx.try_recv().unwrap();
    let (results, error) = crate::search::rank_search_results(
        &request.entries,
        &request.query,
        &request.root,
        request.limit,
        request.use_regex,
        request.ignore_case,
        request.prefer_relative,
        &mut SearchPrefixCache::default(),
        request.sort_mode,
        request.sort_scope,
    );
    let error = error.expect("real query evaluator must reject the regex");
    assert!(crate::app::result_reducer::apply_active_search_response(
        &mut app,
        SearchResponse {
            request_id: request.request_id,
            results: results.results,
            total_match_count: results.total_match_count,
            sort_mode: request.sort_mode,
            sort_scope: request.sort_scope,
            error: Some(error.clone()),
        }
    ));
    let mut gui = Gui::default();
    gui.idle(&mut app);
    let output = gui.idle(&mut app);
    text_rect(&output, |text| text == "Did you mean name: ?");
    // Exact text excludes the separately prefixed footer notice.
    text_rect(&output, |text| text == error);
    assert_eq!(app.shell.runtime.query_state.query, "neme:report [");
}

#[test]
fn ux_contract_empty_field_keeps_non_error_input_prompt() {
    let scope = test_settings_scope("ux-contract-pending");
    let mut app = scope.app(test_root("ux-contract-pending-root"), 50, "name:".into());
    prepare(&mut app, &[]);
    let error = crate::query::CompiledQuery::compile(
        "name:",
        crate::query::QueryOptions {
            use_regex: false,
            ignore_case: true,
        },
    )
    .expect_err("empty field must be rejected by the shared parser");
    app.shell.runtime.query_state.search_error = Some(("name:".into(), error.clone()));
    let mut gui = Gui::default();
    gui.idle(&mut app);
    let output = gui.idle(&mut app);
    text_rect(&output, |text| text == "Enter a value after name:");
    assert!(maybe_text_rect(&output, |text| text == error).is_none());
}

#[test]
fn ux_contract_pin_count_and_clear_highlight_follow_real_clear_button() {
    fn has_fill(shape: &egui::Shape, pos: egui::Pos2, color: egui::Color32) -> bool {
        match shape {
            egui::Shape::Rect(rect) => rect.rect.contains(pos) && rect.fill == color,
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| has_fill(shape, pos, color)),
            _ => false,
        }
    }
    for dark in [false, true] {
        let scope = test_settings_scope("ux-contract-clear");
        let root = test_root("ux-contract-clear-root");
        let mut app = scope.app(root.clone(), 50, "keep query".into());
        prepare(&mut app, &[]);
        let mut gui = Gui::default();
        gui.ctx.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        let selected_color = crate::app::render_theme::selected_fill(dark);
        gui.idle(&mut app);
        let output = gui.idle(&mut app);
        let pos = text_rect(&output, |text| text == "Clear Selected").center();
        assert!(!output
            .shapes
            .iter()
            .any(|shape| has_fill(&shape.shape, pos, selected_color)));
        app.shell
            .runtime
            .pinned_paths
            .insert(root.join("hidden.txt"));
        let output = gui.idle(&mut app);
        text_rect(&output, |text| text == "Open selected (1)");
        text_rect(&output, |text| text == "Copy selected (1)");
        let pos = text_rect(&output, |text| text == "Clear Selected").center();
        assert!(output
            .shapes
            .iter()
            .any(|shape| has_fill(&shape.shape, pos, selected_color)));
        gui.click(&mut app, pos, egui::Modifiers::NONE);
        assert!(app.shell.runtime.pinned_paths.is_empty());
        assert_eq!(app.shell.runtime.query_state.query, "keep query");
        let output = gui.frame(
            &mut app,
            vec![egui::Event::PointerMoved(egui::pos2(-10.0, -10.0))],
            egui::Modifiers::NONE,
        );
        text_rect(&output, |text| text == "Open / Execute");
        text_rect(&output, |text| text == "Copy Path(s)");
        let pos = text_rect(&output, |text| text == "Clear Selected").center();
        assert!(!output
            .shapes
            .iter()
            .any(|shape| has_fill(&shape.shape, pos, selected_color)));
    }
}
