use super::*;
fn setup(
    text: &str,
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
            ..Default::default()
        },
    };
    app.tab.open(Document {
        path: "Note.md".into(),
        text: text.into(),
        fingerprint: "test".into(),
    });
    settle(&mut app, &ctx, size, 4);
    (app, ctx)
}
fn edit(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2) {
    let button = ctx
        .read_response(egui::Id::new("journal-preview-edit"))
        .unwrap()
        .rect;
    click(app, ctx, button.center(), size);
    settle(app, ctx, size, 3);
}
fn select(
    app: &mut Harness,
    ctx: &egui::Context,
    size: egui::Vec2,
    text: &str,
    range: std::ops::Range<usize>,
) -> egui::Id {
    let output = frame(app, ctx, vec![], size);
    click(app, ctx, label(&output, text).center(), size);
    let id = ctx.memory(|m| m.focused()).unwrap();
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(range.start),
            egui::text::CCursor::new(range.end),
        )));
    state.store(ctx, id);
    settle(app, ctx, size, 3);
    id
}
fn bold(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2) {
    let rect = ctx
        .read_response(egui::Id::new(("journal-preview-format", "Bold")))
        .unwrap()
        .rect;
    click(app, ctx, rect.center(), size);
    settle(app, ctx, size, 3);
}
fn text(app: &Harness) -> &str {
    &app.tab.editor.as_ref().unwrap().document.text
}

#[test]
fn preview_edit_button_protects_reading_mode_and_formats_selection_in_every_layout() {
    for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
        for dark in [false, true] {
            for (mode, layout) in [
                (Mode::Preview, PreviewLayout::SideBySide),
                (Mode::Split, PreviewLayout::SideBySide),
                (Mode::Split, PreviewLayout::Stacked),
            ] {
                let (mut app, ctx) = setup("Hello 🌍 world.\n", mode, layout, dark, size);
                select(&mut app, &ctx, size, "Hello 🌍 world.", 6..7);
                frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::Text("Blocked".into())],
                    size,
                );
                assert_eq!(text(&app), "Hello 🌍 world.\n");
                edit(&mut app, &ctx, size);
                select(&mut app, &ctx, size, "Hello 🌍 world.", 6..7);
                bold(&mut app, &ctx, size);
                assert_eq!(text(&app), "Hello **🌍** world.\n", "{mode:?} {layout:?}");
                assert!(frame(&mut app, &ctx, vec![], size).shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "Hello 🌍 world.")));
                bold(&mut app, &ctx, size);
                assert_eq!(text(&app), "Hello 🌍 world.\n");
                preview_key(&mut app, &ctx, size, egui::Key::I, egui::Modifiers::COMMAND);
                settle(&mut app, &ctx, size, 3);
                assert_eq!(text(&app), "Hello *🌍* world.\n");
                preview_key(&mut app, &ctx, size, egui::Key::Z, egui::Modifiers::COMMAND);
                assert_eq!(text(&app), "Hello 🌍 world.\n");
                edit(&mut app, &ctx, size);
                select(&mut app, &ctx, size, "Hello 🌍 world.", 0..5);
                frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::Text("Blocked".into())],
                    size,
                );
                assert_eq!(text(&app), "Hello 🌍 world.\n");
            }
        }
    }
}

#[test]
fn new_text_can_be_formatted_without_switching_to_source() {
    let size = egui::vec2(980., 640.);
    let (mut app, ctx) = setup("", Mode::Preview, PreviewLayout::SideBySide, false, size);
    edit(&mut app, &ctx, size);
    bold(&mut app, &ctx, size);
    assert_eq!(text(&app), "**text**");
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("New 🌍 text".into())],
        size,
    );
    assert_eq!(text(&app), "**New 🌍 text**");
    select(&mut app, &ctx, size, "New 🌍 text", 0..3);
    bold(&mut app, &ctx, size);
    assert_eq!(text(&app), "New **🌍 text**");
    let headings = ctx
        .read_response(egui::Id::new(("preview", "journal-floating-headings")))
        .unwrap()
        .rect;
    click(&mut app, &ctx, headings.center(), size);
    let output = frame(&mut app, &ctx, vec![], size);
    click(&mut app, &ctx, label(&output, "Heading 2").center(), size);
    settle(&mut app, &ctx, size, 3);
    assert_eq!(text(&app), "## New **🌍 text**");
}

#[test]
fn preview_default_can_be_overridden_per_note_and_applies_to_new_notes() {
    let size = egui::vec2(980., 640.);
    let (mut app, ctx) = setup(
        "First",
        Mode::Preview,
        PreviewLayout::SideBySide,
        false,
        size,
    );
    app.prefs.preview_editable = true;
    settle(&mut app, &ctx, size, 3);
    let id = select(&mut app, &ctx, size, "First", 0..5);
    preview_replace(&mut app, &ctx, size, id, 0..5, "Changed");
    assert_eq!(text(&app), "Changed");
    edit(&mut app, &ctx, size);
    select(&mut app, &ctx, size, "Changed", 0..7);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("Blocked".into())],
        size,
    );
    assert_eq!(text(&app), "Changed");
    app.tab.open(Document {
        path: "Next.md".into(),
        text: "Next".into(),
        fingerprint: "test".into(),
    });
    settle(&mut app, &ctx, size, 3);
    select(&mut app, &ctx, size, "Next", 0..4);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("Editable".into())],
        size,
    );
    assert_eq!(text(&app), "Editable");
}

#[test]
fn preview_formats_recent_edits_entities_links_and_table_cells_without_changing_metadata() {
    let size = egui::vec2(980., 640.);
    let source = "---\r\nprivate: Keep\r\n---\r\nA &amp; **bold** and [[Target|Link]].\r\n\r\n| A | B |\r\n| --- | --- |\r\n| one | two |\r\n";
    let (mut app, ctx) = setup(
        source,
        Mode::Preview,
        PreviewLayout::SideBySide,
        false,
        size,
    );
    edit(&mut app, &ctx, size);
    let id = select(&mut app, &ctx, size, "A & bold and Link.", 4..8);
    preview_replace(&mut app, &ctx, size, id, 4..8, "brave🌍");
    select(&mut app, &ctx, size, "A & brave🌍 and Link.", 2..3);
    bold(&mut app, &ctx, size);
    assert!(
        text(&app).contains("A **&amp;** **brave🌍** and [[Target|Link]]."),
        "{}",
        text(&app)
    );
    select(&mut app, &ctx, size, "one", 0..3);
    bold(&mut app, &ctx, size);
    assert!(text(&app).contains("| **one** | two |"), "{}", text(&app));
    assert!(text(&app).starts_with("---\r\nprivate: Keep\r\n---\r\n"));
    assert!(text(&app).ends_with("\r\n"));
    bold(&mut app, &ctx, size);
    assert!(text(&app).contains("| one | two |"));
}

#[test]
fn preview_context_menu_and_link_dialog_preserve_selection_and_support_cancel() {
    let size = egui::vec2(980., 640.);
    let (mut app, ctx) = setup(
        "A clear thought.\n",
        Mode::Preview,
        PreviewLayout::SideBySide,
        false,
        size,
    );
    edit(&mut app, &ctx, size);
    select(&mut app, &ctx, size, "A clear thought.", 2..7);
    let output = frame(&mut app, &ctx, vec![], size);
    let pos = label(&output, "A clear thought.").left_center() + egui::vec2(10., 0.);
    for pressed in [true, false] {
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            size,
        );
    }
    let output = frame(&mut app, &ctx, vec![], size);
    click(&mut app, &ctx, label(&output, "Bold").center(), size);
    settle(&mut app, &ctx, size, 3);
    assert_eq!(text(&app), "A **clear** thought.\n");
    select(&mut app, &ctx, size, "A clear thought.", 2..7);
    for save in [false, true] {
        let link = ctx
            .read_response(egui::Id::new(("journal-preview-format", "Link")))
            .unwrap()
            .rect;
        click(&mut app, &ctx, link.center(), size);
        settle(&mut app, &ctx, size, 3);
        let id = egui::Id::new("journal-preview-link-url");
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(100),
            )));
        state.store(&ctx, id);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("Folder/Note.md".into())],
            size,
        );
        let output = frame(&mut app, &ctx, vec![], size);
        click(
            &mut app,
            &ctx,
            label(&output, if save { "Apply Link" } else { "Cancel" }).center(),
            size,
        );
        settle(&mut app, &ctx, size, 3);
        if !save {
            assert_eq!(text(&app), "A **clear** thought.\n");
        }
    }
    assert!(
        text(&app).contains("[clear](Folder/Note.md)"),
        "{}",
        text(&app)
    );
    assert!(!text(&app).contains("https://"));
}

#[test]
fn preview_code_language_menu_inserts_and_updates_a_single_fence_with_undo() {
    let size = egui::vec2(980., 640.);
    let (mut app, ctx) = setup(
        "print(\"🌍\")\n",
        Mode::Preview,
        PreviewLayout::SideBySide,
        true,
        size,
    );
    edit(&mut app, &ctx, size);
    select(&mut app, &ctx, size, "print(\"🌍\")", 0..10);
    for language in ["Python", "JavaScript"] {
        let code = ctx
            .read_response(egui::Id::new(("preview", "journal-floating-code")))
            .unwrap()
            .rect;
        click(&mut app, &ctx, code.center(), size);
        settle(&mut app, &ctx, size, 3);
        choose_language(&mut app, &ctx, size, language);
        settle(&mut app, &ctx, size, 3);
        assert_eq!(text(&app).matches("```").count(), 2, "{}", text(&app));
        assert!(text(&app).contains(if language == "Python" {
            "```python"
        } else {
            "```javascript"
        }));
    }
    preview_key(&mut app, &ctx, size, egui::Key::Z, egui::Modifiers::COMMAND);
    assert!(text(&app).contains("```python"));
}
