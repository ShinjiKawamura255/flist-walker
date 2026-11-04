use super::*;

const SELECTION_FILL: egui::Color32 = egui::Color32::from_rgb(237, 19, 173);

fn paints_selection(shape: &egui::Shape) -> bool {
    match shape {
        egui::Shape::Rect(rect) => rect.fill == SELECTION_FILL,
        egui::Shape::Text(text) => text.galley.rows.iter().any(|row| {
            row.visuals
                .mesh
                .vertices
                .iter()
                .any(|vertex| vertex.color == SELECTION_FILL)
        }),
        egui::Shape::Vec(shapes) => shapes.iter().any(paints_selection),
        _ => false,
    }
}

fn ctrl_a(os: egui::os::OperatingSystem, repeat: bool) -> egui::Event {
    egui::Event::Key {
        key: egui::Key::A,
        physical_key: Some(egui::Key::A),
        pressed: true,
        repeat,
        // Native integrations set command alongside Ctrl on Windows/Linux.
        modifiers: egui::Modifiers {
            ctrl: true,
            command: os != egui::os::OperatingSystem::Mac,
            ..Default::default()
        },
    }
}

fn query_ctrl_a_frame(
    os: egui::os::OperatingSystem,
    enabled: bool,
    history: bool,
    insert_after: bool,
) {
    let root = test_root(&format!("ctrl-a-{os:?}-{enabled}-{history}-{insert_after}"));
    fs::create_dir_all(&root).expect("fixture root");
    let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
    app.shell.runtime.emacs_keybindings_enabled = enabled;
    let original = "alpha 日本語 beta".repeat(20);
    if history {
        app.start_history_search();
        app.shell.runtime.query_state.history_search_query = original.clone();
    } else {
        app.shell.runtime.query_state.query = original.clone();
    }
    let ctx = egui::Context::default();
    ctx.set_os(os);
    ctx.all_styles_mut(|style| style.visuals.selection.bg_fill = SELECTION_FILL);
    let id = app.shell.ui.query_input_id;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| app.run_ui_frame(ui));
    app.clear_focus_query_request();
    ctx.memory_mut(|memory| memory.request_focus(id));
    let mut state = egui::text_edit::TextEditState::load(&ctx, id).expect("query state");
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(original.chars().count()),
        )));
    state.store(&ctx, id);

    for repeat in [false, true] {
        let mut events = vec![ctrl_a(os, repeat)];
        if insert_after {
            events.push(egui::Event::Text("界".to_string()));
        }
        let output = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| app.run_ui_frame(ui),
        );
        let state = egui::text_edit::TextEditState::load(&ctx, id).expect("query state");
        let range = state.cursor.char_range().expect("query cursor");
        let text = if history {
            &app.shell.runtime.query_state.history_search_query
        } else {
            &app.shell.runtime.query_state.query
        };
        if insert_after {
            let prefix = if repeat { "界界" } else { "界" };
            assert_eq!(
                text,
                &format!("{prefix}{original}"),
                "{os:?}, history={history}"
            );
            assert_eq!(range.primary.index.0, 1);
            assert_eq!(range.secondary.index.0, 1);
        } else if enabled {
            assert_eq!(text, &original);
            assert_eq!(range.primary.index.0, 0);
            assert_eq!(range.secondary.index.0, 0);
        } else {
            assert_eq!(text, &original);
            assert_eq!(range.sorted_cursors()[0].index.0, 0);
            assert_eq!(range.sorted_cursors()[1].index.0, original.chars().count());
        }
        assert_eq!(
            output.shapes.iter().any(|shape| paints_selection(&shape.shape)),
            !enabled,
            "selection paint must match the setting in the SAME frame: {os:?}, history={history}, repeat={repeat}"
        );
    }
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn regression_ctrl_a_moves_query_and_history_without_painting_select_all() {
    for os in [
        egui::os::OperatingSystem::Windows,
        egui::os::OperatingSystem::Nix,
        egui::os::OperatingSystem::Mac,
    ] {
        for history in [false, true] {
            query_ctrl_a_frame(os, true, history, false);
        }
    }
}

#[test]
fn regression_ctrl_a_selects_all_when_emacs_is_disabled() {
    for os in [
        egui::os::OperatingSystem::Windows,
        egui::os::OperatingSystem::Nix,
        egui::os::OperatingSystem::Mac,
    ] {
        for history in [false, true] {
            query_ctrl_a_frame(os, false, history, false);
        }
    }
}

#[test]
fn regression_ctrl_a_preserves_order_with_text_in_the_same_frame() {
    for os in [
        egui::os::OperatingSystem::Windows,
        egui::os::OperatingSystem::Mac,
    ] {
        for history in [false, true] {
            query_ctrl_a_frame(os, true, history, true);
        }
    }
}

#[test]
fn regression_ctrl_a_shared_fields_and_macos_command_a_keep_selection_contract() {
    for os in [
        egui::os::OperatingSystem::Windows,
        egui::os::OperatingSystem::Mac,
    ] {
        for enabled in [false, true] {
            for command_a in [false, true] {
                if command_a && os != egui::os::OperatingSystem::Mac {
                    continue;
                }
                let ctx = egui::Context::default();
                ctx.set_os(os);
                ctx.all_styles_mut(|style| style.visuals.selection.bg_fill = SELECTION_FILL);
                let id = egui::Id::new("ctrl-a-shared-field");
                let mut text = "日本語 alpha".to_string();
                let mut kill_buffer = "retained kill buffer".to_string();
                let mut render = |ui: &mut egui::Ui| {
                    FlistWalkerApp::emacs_singleline_text_edit(
                        ui,
                        &mut text,
                        &mut kill_buffer,
                        enabled,
                        false,
                        crate::app::render::EmacsSinglelineOptions::new(Some(id), 400.0, None),
                    );
                };
                ctx.memory_mut(|memory| memory.request_focus(id));
                let _ = ctx.run_ui(egui::RawInput::default(), &mut render);
                let event = if command_a {
                    egui::Event::Key {
                        key: egui::Key::A,
                        physical_key: Some(egui::Key::A),
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers {
                            mac_cmd: true,
                            command: true,
                            ..Default::default()
                        },
                    }
                } else {
                    ctrl_a(os, false)
                };
                let output = ctx.run_ui(
                    egui::RawInput {
                        events: vec![event],
                        ..Default::default()
                    },
                    &mut render,
                );
                let state = egui::text_edit::TextEditState::load(&ctx, id).expect("field state");
                let range = state.cursor.char_range().expect("field cursor");
                let selects_all = !enabled || command_a;
                assert_eq!(
                    range.is_empty(),
                    !selects_all,
                    "{os:?}, enabled={enabled}, command_a={command_a}"
                );
                assert_eq!(range.sorted_cursors()[0].index.0, 0);
                assert_eq!(
                    range.sorted_cursors()[1].index.0,
                    if selects_all { 9 } else { 0 }
                );
                assert_eq!(
                    output
                        .shapes
                        .iter()
                        .any(|shape| paints_selection(&shape.shape)),
                    selects_all
                );
                assert_eq!(text, "日本語 alpha");
                assert_eq!(kill_buffer, "retained kill buffer");
            }
        }
    }
}

#[test]
fn regression_ctrl_a_preparation_leaves_unfocused_and_ime_events_untouched() {
    for (focused, ime_active) in [(false, false), (true, true)] {
        let ctx = egui::Context::default();
        let events = vec![ctrl_a(egui::os::OperatingSystem::Windows, false)];
        ctx.begin_pass(egui::RawInput {
            events: events.clone(),
            ..Default::default()
        });
        FlistWalkerApp::prepare_emacs_text_edit_shortcuts(&ctx, focused, true, ime_active);
        assert_eq!(ctx.input(|input| input.events.clone()), events);
        let _ = ctx.end_pass();
    }
}
