use super::*;
fn start(
    source: &str,
    mode: Mode,
    layout: PreviewLayout,
    dark: bool,
    size: egui::Vec2,
) -> (Harness, egui::Context) {
    let ctx = egui::Context::default();
    design::install(&ctx);
    design::apply_theme(&ctx, dark);
    let mut app = Harness {
        action: None,
        tab: JournalTab::default(),
        prefs: Preferences {
            mode,
            preview_layout: layout,
            preview_editable: true,
            ..Default::default()
        },
    };
    app.tab.open(Document {
        path: "Note.md".into(),
        text: source.into(),
        fingerprint: "test".into(),
    });
    settle(&mut app, &ctx, size, 4);
    if mode == Mode::Source {
        let count = source.chars().count();
        select_source(&mut app, &ctx, size, count..count);
    } else {
        let response = app
            .tab
            .editor
            .as_ref()
            .unwrap()
            .preview
            .inputs
            .iter()
            .find(|i| i.accepts_enter && !i.text.is_empty())
            .or_else(|| app.tab.editor.as_ref().unwrap().preview.inputs.first())
            .unwrap()
            .output
            .response
            .clone();
        click(&mut app, &ctx, response.rect.center(), size);
        let id = ctx.memory(|m| m.focused()).unwrap();
        let value = app
            .tab
            .editor
            .as_ref()
            .unwrap()
            .preview
            .inputs
            .iter()
            .find(|i| i.output.response.id == id)
            .unwrap()
            .text
            .clone();
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(value.chars().count()),
            )));
        state.store(&ctx, id);
        settle(&mut app, &ctx, size, 3);
    }
    (app, ctx)
}
fn text(app: &Harness) -> &str {
    &app.tab.editor.as_ref().unwrap().document.text
}
fn focus_payload(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2, value: &str) {
    let input = app
        .tab
        .editor
        .as_ref()
        .unwrap()
        .preview
        .inputs
        .iter()
        .find(|i| i.text == value)
        .unwrap();
    let id = input.output.response.id;
    let rect = input.output.response.rect;
    click(app, ctx, rect.center(), size);
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(value.chars().count()),
        )));
    state.store(ctx, id);
    settle(app, ctx, size, 3);
}
fn enter(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2) {
    preview_key(app, ctx, size, egui::Key::Enter, egui::Modifiers::NONE);
    settle(app, ctx, size, 3);
}
#[test]
fn list_enter_continues_and_double_enter_exits_in_all_editors_sizes_and_themes() {
    for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
        for dark in [false, true] {
            for (mode, layout) in [
                (Mode::Source, PreviewLayout::SideBySide),
                (Mode::Preview, PreviewLayout::SideBySide),
                (Mode::Split, PreviewLayout::SideBySide),
                (Mode::Split, PreviewLayout::Stacked),
            ] {
                for (first, next) in [
                    ("- One", "- "),
                    ("9. Nine", "10. "),
                    ("- [x] Done", "- [ ] "),
                ] {
                    let (mut app, ctx) = start(first, mode, layout, dark, size);
                    enter(&mut app, &ctx, size);
                    assert_eq!(
                        text(&app),
                        format!("{first}\n{next}"),
                        "{mode:?} {layout:?}"
                    );
                    frame(
                        &mut app,
                        &ctx,
                        vec![egui::Event::Text("Second 🌍".into())],
                        size,
                    );
                    assert_eq!(text(&app), format!("{first}\n{next}Second 🌍"));
                    enter(&mut app, &ctx, size);
                    enter(&mut app, &ctx, size);
                    frame(
                        &mut app,
                        &ctx,
                        vec![egui::Event::Text("Plain paragraph".into())],
                        size,
                    );
                    assert_eq!(
                        text(&app),
                        format!("{first}\n{next}Second 🌍\n\nPlain paragraph")
                    );
                    preview_key(&mut app, &ctx, size, egui::Key::Z, egui::Modifiers::COMMAND);
                    assert!(!text(&app).contains("Plain paragraph"));
                }
            }
        }
    }
}
#[test]
fn empty_formatted_lists_keep_the_marker_input_and_receive_typed_text() {
    let size = egui::vec2(980., 640.);
    for (label_text, prefix) in [
        ("Bulleted List", "- "),
        ("Numbered List", "1. "),
        ("Task List", "- [ ] "),
    ] {
        let (mut app, ctx) = start("", Mode::Preview, PreviewLayout::SideBySide, false, size);
        let list = ctx
            .read_response(egui::Id::new(("preview", "journal-floating-lists")))
            .unwrap()
            .rect;
        click(&mut app, &ctx, list.center(), size);
        let output = frame(&mut app, &ctx, vec![], size);
        click(&mut app, &ctx, label(&output, label_text).center(), size);
        settle(&mut app, &ctx, size, 4);
        assert_eq!(text(&app), prefix);
        let preview = &app.tab.editor.as_ref().unwrap().preview;
        assert_eq!(preview.inputs.len(), 2, "empty list item plus append input");
        assert!(preview.inputs[0].output.response.has_focus());
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("First".into())],
            size,
        );
        assert_eq!(text(&app), format!("{prefix}First"));
        enter(&mut app, &ctx, size);
        enter(&mut app, &ctx, size);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("After".into())],
            size,
        );
        assert_eq!(text(&app), format!("{prefix}First\n\nAfter"));
    }
}

#[test]
fn typed_list_prefixes_become_visible_markers_and_tasks_without_waiting_for_blur() {
    let size = egui::vec2(980., 640.);
    for prefix in ["- ", "* ", "+ ", "1. ", "3) ", "- [ ] "] {
        let (mut app, ctx) = start("", Mode::Preview, PreviewLayout::SideBySide, false, size);
        frame(&mut app, &ctx, vec![egui::Event::Text(prefix.into())], size);
        settle(&mut app, &ctx, size, 3);
        assert_eq!(text(&app), prefix);
        assert_eq!(
            app.tab.editor.as_ref().unwrap().preview.inputs.len(),
            2,
            "{prefix}"
        );
        frame(&mut app, &ctx, vec![egui::Event::Text("Item".into())], size);
        assert_eq!(text(&app), format!("{prefix}Item"));
        enter(&mut app, &ctx, size);
        assert!(text(&app).ends_with(' '));
    }
    let (mut app, ctx) = start("", Mode::Preview, PreviewLayout::SideBySide, false, size);
    frame(&mut app, &ctx, vec![egui::Event::Text("- ".into())], size);
    settle(&mut app, &ctx, size, 3);
    frame(&mut app, &ctx, vec![egui::Event::Text("[ ] ".into())], size);
    settle(&mut app, &ctx, size, 3);
    assert_eq!(text(&app), "- [ ] ");
    frame(&mut app, &ctx, vec![egui::Event::Text("Task".into())], size);
    assert_eq!(text(&app), "- [ ] Task");
}

#[test]
fn nested_lists_outdent_then_exit_and_plain_paragraphs_can_be_inserted_between_existing_blocks() {
    let size = egui::vec2(980., 640.);
    for mode in [Mode::Source, Mode::Preview] {
        let (mut app, ctx) = start(
            "- Parent\n  - Child",
            mode,
            PreviewLayout::SideBySide,
            false,
            size,
        );
        if mode == Mode::Preview {
            focus_payload(&mut app, &ctx, size, "Child");
        }
        enter(&mut app, &ctx, size);
        assert_eq!(text(&app), "- Parent\n  - Child\n  - ");
        enter(&mut app, &ctx, size);
        assert_eq!(text(&app), "- Parent\n  - Child\n- ");
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("Sibling".into())],
            size,
        );
        assert_eq!(text(&app), "- Parent\n  - Child\n- Sibling");
    }
    let (mut app, ctx) = start(
        "- One\n\nAfter",
        Mode::Preview,
        PreviewLayout::SideBySide,
        false,
        size,
    );
    enter(&mut app, &ctx, size);
    enter(&mut app, &ctx, size);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("Between".into())],
        size,
    );
    assert_eq!(text(&app), "- One\n\nBetween\n\nAfter");
}

#[test]
fn shift_enter_keeps_a_single_item_and_fast_typing_before_enter_is_retained() {
    let size = egui::vec2(980., 640.);
    for mode in [Mode::Source, Mode::Preview] {
        let (mut app, ctx) = start("- One", mode, PreviewLayout::SideBySide, false, size);
        preview_key(
            &mut app,
            &ctx,
            size,
            egui::Key::Enter,
            egui::Modifiers::SHIFT,
        );
        settle(&mut app, &ctx, size, 3);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("Continuation".into())],
            size,
        );
        assert_eq!(text(&app), "- One  \n  Continuation");
        enter(&mut app, &ctx, size);
        assert_eq!(text(&app), "- One  \n  Continuation\n- ");
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::Text("Fast 🌍".into()),
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            size,
        );
        settle(&mut app, &ctx, size, 3);
        assert_eq!(text(&app), "- One  \n  Continuation\n- Fast 🌍\n- ");
    }
}

#[test]
fn continuing_formatted_payloads_keeps_closing_markers_and_link_targets_on_the_old_item() {
    let size = egui::vec2(980., 640.);
    for source in [
        "- **Bold 🌍**",
        "- *Italic*",
        "- [Link](Note.md)",
        "- `code`",
    ] {
        let (mut app, ctx) = start(
            source,
            Mode::Preview,
            PreviewLayout::SideBySide,
            false,
            size,
        );
        enter(&mut app, &ctx, size);
        assert_eq!(text(&app), format!("{source}\n- "));
        frame(&mut app, &ctx, vec![egui::Event::Text("Next".into())], size);
        assert_eq!(text(&app), format!("{source}\n- Next"));
    }
}

#[test]
fn saved_empty_markers_receive_a_space_before_their_first_payload_and_empty_tasks_can_toggle() {
    let size = egui::vec2(980., 640.);
    for source in ["-", "*", "+", "1.", "9)", "- [ ]"] {
        let (mut app, ctx) = start(
            source,
            Mode::Preview,
            PreviewLayout::SideBySide,
            false,
            size,
        );
        assert_eq!(
            app.tab.editor.as_ref().unwrap().preview.inputs.len(),
            2,
            "{source}"
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("Payload".into())],
            size,
        );
        assert_eq!(text(&app), format!("{source} Payload"));
    }
    let (mut app, ctx) = start(
        "- [ ] ",
        Mode::Preview,
        PreviewLayout::SideBySide,
        false,
        size,
    );
    let input = &app.tab.editor.as_ref().unwrap().preview.inputs[0];
    let checkbox = input.output.response.rect.left_center() - egui::vec2(28., 0.);
    click(&mut app, &ctx, checkbox, size);
    assert_eq!(text(&app), "- [x] ");
}

#[test]
fn undo_and_redo_restore_the_list_caret_when_the_empty_item_appears_or_disappears() {
    let size = egui::vec2(980., 640.);
    let (mut app, ctx) = start(
        "- One",
        Mode::Preview,
        PreviewLayout::SideBySide,
        false,
        size,
    );
    enter(&mut app, &ctx, size);
    preview_key(&mut app, &ctx, size, egui::Key::Z, egui::Modifiers::COMMAND);
    settle(&mut app, &ctx, size, 3);
    assert_eq!(text(&app), "- One");
    preview_key(
        &mut app,
        &ctx,
        size,
        egui::Key::Z,
        egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        },
    );
    settle(&mut app, &ctx, size, 3);
    assert_eq!(text(&app), "- One\n- ");
    frame(&mut app, &ctx, vec![egui::Event::Text("Two".into())], size);
    assert_eq!(text(&app), "- One\n- Two");
    enter(&mut app, &ctx, size);
    enter(&mut app, &ctx, size);
    preview_key(&mut app, &ctx, size, egui::Key::Z, egui::Modifiers::COMMAND);
    settle(&mut app, &ctx, size, 3);
    assert_eq!(text(&app), "- One\n- Two\n- ");
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("Three".into())],
        size,
    );
    assert_eq!(text(&app), "- One\n- Two\n- Three");
}
