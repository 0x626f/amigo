use super::*;
#[path = "interaction_tests.rs"]
mod interactions;
#[path = "list_editing_tests.rs"]
mod list_editing;
#[path = "preview_format_tests.rs"]
mod preview_format;
struct Harness {
    action: Option<Action>,
    tab: JournalTab,
    prefs: Preferences,
}
fn frame(
    app: &mut Harness,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    size: egui::Vec2,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                app.action = app.tab.show(ui, &mut app.prefs);
            });
        },
    );
    output.textures_delta.clear();
    output
}
fn label_optional(output: &egui::FullOutput, text: &str) -> Option<egui::Rect> {
    fn find(shape: &egui::Shape, text: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == text => {
                Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
            }
            egui::Shape::Vec(items) => items.iter().find_map(|s| find(s, text)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .rev()
        .find_map(|s| find(&s.shape, text).filter(|rect| s.clip_rect.contains_rect(*rect)))
}
fn label(output: &egui::FullOutput, text: &str) -> egui::Rect {
    label_optional(output, text).unwrap_or_else(|| panic!("Missing {text}"))
}
#[test]
fn context_formatting_and_undo_keep_unicode_selection_and_markdown() {
    let ctx = egui::Context::default();
    design::install(&ctx);
    let mut app = Harness {
        action: None,
        tab: JournalTab::default(),
        prefs: Preferences {
            mode: Mode::Source,
            ..Default::default()
        },
    };
    app.tab.open(Document {
        path: "Note.md".into(),
        text: "Hello 🌍".into(),
        fingerprint: "test".into(),
    });
    let size = egui::vec2(980., 640.);
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![], size);
    }
    let id = egui::Id::new(("journal-editor", "Note.md"));
    let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(5),
        )));
    state.store(&ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
    frame(&mut app, &ctx, vec![], size);
    let pos = ctx.read_response(id).unwrap().rect.min + egui::vec2(12., 10.);
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
    let bold = label(&output, "Bold").center();
    for pressed in [true, false] {
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(bold),
                egui::Event::PointerButton {
                    pos: bold,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            size,
        );
    }
    assert_eq!(
        app.tab.editor.as_ref().unwrap().document.text,
        "**Hello** 🌍"
    );
    for pressed in [true, false] {
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Z,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            size,
        );
    }
    assert_eq!(app.tab.editor.as_ref().unwrap().document.text, "Hello 🌍");
}
#[test]
fn exclusive_load_blocks_edits_but_autosave_does_not() {
    let ctx = egui::Context::default();
    design::install(&ctx);
    let mut app = Harness {
        action: None,
        tab: JournalTab::default(),
        prefs: Preferences {
            mode: Mode::Source,
            ..Default::default()
        },
    };
    app.tab.open(Document {
        path: "Note.md".into(),
        text: "Original".into(),
        fingerprint: "test".into(),
    });
    let size = egui::vec2(980., 640.);
    let id = egui::Id::new(("journal-editor", "Note.md"));
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], size);
    }
    ctx.memory_mut(|m| m.request_focus(id));
    app.tab.busy = true;
    app.tab.loading = true;
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("Blocked".into())],
        size,
    );
    assert_eq!(app.tab.editor.as_ref().unwrap().document.text, "Original");
    app.tab.loading = false;
    frame(&mut app, &ctx, vec![], size);
    ctx.memory_mut(|m| m.request_focus(id));
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("Allowed".into())],
        size,
    );
    assert!(
        app.tab
            .editor
            .as_ref()
            .unwrap()
            .document
            .text
            .contains("Allowed")
    );
}
fn click(app: &mut Harness, ctx: &egui::Context, pos: egui::Pos2, size: egui::Vec2) {
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            size,
        );
    }
}
#[test]
fn notebook_keeps_title_and_icon_controls_and_double_click_renames() {
    for size in [egui::vec2(680., 500.), egui::vec2(1160., 760.)] {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut app = Harness {
            action: None,
            tab: JournalTab::default(),
            prefs: Preferences::default(),
        };
        app.tab.open(Document {
            path: "Folder/Original.md".into(),
            text: "# Original\n\nBody".into(),
            fingerprint: "test".into(),
        });
        for _ in 0..4 {
            frame(&mut app, &ctx, vec![], size);
        }
        let title = ctx
            .read_response(egui::Id::new("journal-note-title"))
            .unwrap()
            .rect;
        let notebook = ctx
            .read_response(egui::Id::new("journal-notebook"))
            .unwrap()
            .rect;
        assert!(
            ctx.read_response(egui::Id::new("journal-page-title"))
                .is_none()
        );
        let open_external = ctx
            .read_response(egui::Id::new("journal-open-external"))
            .unwrap()
            .rect;
        assert!(notebook.contains_rect(open_external));
        assert!((open_external.left() - notebook.left() - 12.).abs() < 2.);
        assert!((open_external.center().y - title.center().y).abs() < 1.);
        assert!(open_external.right() < title.left());
        assert!((title.center().x - notebook.center().x).abs() < 1.);
        assert!(notebook.right() <= size.x && notebook.bottom() <= size.y);
        assert!(
            ctx.read_response(egui::Id::new(("journal", "rename")))
                .is_none()
        );
        let controls = [
            egui::Id::new(("journal-mode", Mode::Source)),
            egui::Id::new(("journal-mode", Mode::Split)),
            egui::Id::new(("journal-mode", Mode::Preview)),
        ]
        .map(|id| ctx.read_response(id).unwrap().rect);
        for control in controls {
            assert!(notebook.contains_rect(control));
            assert!((control.center().y - title.center().y).abs() < 1.);
        }
        for pair in controls.windows(2) {
            assert!(pair[0].right() < pair[1].left());
        }
        assert!(
            ctx.read_response(egui::Id::new(("journal", "export")))
                .is_none()
        );
        click(&mut app, &ctx, title.center(), size);
        click(&mut app, &ctx, title.center(), size);
        frame(&mut app, &ctx, vec![], size);
        assert!(app.tab.title_edit.is_some());
        let name_field = ctx
            .read_response(egui::Id::new("journal-title-edit"))
            .unwrap()
            .rect;
        let save_name = ctx
            .read_response(egui::Id::new("journal-title-save"))
            .unwrap()
            .rect;
        let cancel_name = ctx
            .read_response(egui::Id::new("journal-title-cancel"))
            .unwrap()
            .rect;
        assert!(name_field.right() < save_name.left());
        assert!(cancel_name.right() < controls[0].left());
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(egui::Id::new("journal-title-edit"))
        );
        let cursor = egui::TextEdit::load_state(&ctx, egui::Id::new("journal-title-edit"))
            .unwrap()
            .cursor
            .char_range()
            .unwrap();
        assert!(cursor.is_empty());
        assert_eq!(cursor.primary.index.0, "Original".chars().count());
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text(" revised 🌍".into())],
            size,
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            size,
        );
        assert!(
            matches!(&app.action, Some(Action::Rename {old, name}) if old == "Folder/Original.md" && name == "Folder/Original revised 🌍")
        );
        assert_eq!(
            app.tab.editor.as_ref().unwrap().document.text,
            "# Original\n\nBody"
        );
        app.tab.title_edit = Some(TitleEdit {
            path: "Folder/Original.md".into(),
            value: "Discard me".into(),
            focus: true,
        });
        frame(&mut app, &ctx, vec![], size);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            size,
        );
        assert!(app.tab.title_edit.is_none());
        assert!(app.action.is_none());
        for mode in [Mode::Source, Mode::Split, Mode::Preview] {
            let button = ctx
                .read_response(egui::Id::new(("journal-mode", mode)))
                .unwrap()
                .rect;
            click(&mut app, &ctx, button.center(), size);
            assert_eq!(app.prefs.mode, mode);
        }
        for name in ["import", "delete", "rename"] {
            assert!(
                ctx.read_response(egui::Id::new(("journal", name)))
                    .is_none()
            );
        }
        click(&mut app, &ctx, open_external.center(), size);
        assert!(matches!(&app.action, Some(Action::OpenExternal)));
        app.tab.editor = None;
        settle(&mut app, &ctx, size, 4);
        click(&mut app, &ctx, open_external.center(), size);
        assert!(matches!(&app.action, Some(Action::OpenExternal)));
    }
}
#[test]
fn folder_disclosure_opens_and_closes_nested_notes() {
    let ctx = egui::Context::default();
    design::install(&ctx);
    let mut tab = JournalTab {
        loaded: true,
        entries: vec![
            Entry {
                path: "Folder".into(),
                folder: true,
            },
            Entry {
                path: "Folder/Note.md".into(),
                folder: false,
            },
        ],
        ..Default::default()
    };
    fn render(tab: &mut JournalTab, ctx: &egui::Context, events: Vec<egui::Event>) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(224., 600.),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    tab.sidebar(ui);
                });
            },
        );
        output.textures_delta.clear();
    }
    for _ in 0..3 {
        render(&mut tab, &ctx, vec![]);
    }
    for open in [true, false] {
        let folder = ctx
            .read_response(egui::Id::new(("journal-entry", "Folder")))
            .unwrap()
            .rect;
        for pressed in [true, false] {
            render(
                &mut tab,
                &ctx,
                vec![
                    egui::Event::PointerMoved(folder.center()),
                    egui::Event::PointerButton {
                        pos: folder.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        render(&mut tab, &ctx, vec![]);
        render(&mut tab, &ctx, vec![]);
        assert_eq!(tab.expanded.contains("Folder"), open);
        assert_eq!(
            ctx.read_response(egui::Id::new(("journal-entry", "Folder/Note.md")))
                .is_some(),
            open
        );
    }
    assert!(!tab.expanded.contains("Folder"));
}
fn notebook(text: String) -> Harness {
    let mut tab = JournalTab::default();
    tab.open(Document {
        path: "Note.md".into(),
        text,
        fingerprint: "test".into(),
    });
    Harness {
        action: None,
        tab,
        prefs: Preferences::default(),
    }
}
fn settle(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2, frames: usize) {
    for _ in 0..frames {
        frame(app, ctx, vec![], size);
    }
}
fn select_source(
    app: &mut Harness,
    ctx: &egui::Context,
    size: egui::Vec2,
    range: std::ops::Range<usize>,
) {
    let id = egui::Id::new(("journal-editor", "Note.md"));
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(range.start),
            egui::text::CCursor::new(range.end),
        )));
    state.store(ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
    settle(app, ctx, size, 4);
}
fn choose_language(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2, language: &str) {
    let mut output = frame(app, ctx, vec![], size);
    if let Some(item) = label_optional(&output, language) {
        click(app, ctx, item.center(), size);
        return;
    }
    let scroll_pos = label(&output, "Plain Text").center() + egui::vec2(0., 100.);
    for _ in 0..6 {
        if let Some(item) = label_optional(&output, language) {
            assert!(item.top() >= 0. && item.bottom() <= size.y);
            click(app, ctx, item.center(), size);
            return;
        }
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(scroll_pos),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    delta: egui::vec2(0., -90.),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            size,
        );
        settle(app, ctx, size, 12);
        output = frame(app, ctx, vec![], size);
    }
    panic!("Missing language {language}");
}
#[test]
fn code_language_picker_inserts_one_fenced_block_changes_language_and_supports_undo() {
    for size in [egui::vec2(680., 500.), egui::vec2(1160., 760.)] {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let body = "print(\"🌍\")";
        let mut app = notebook(body.into());
        app.prefs.mode = Mode::Source;
        settle(&mut app, &ctx, size, 4);
        select_source(&mut app, &ctx, size, 0..body.chars().count());
        for (label_text, language) in [("Python", "python"), ("JavaScript", "javascript")] {
            let code = ctx
                .read_response(egui::Id::new("journal-floating-code"))
                .unwrap()
                .rect;
            click(&mut app, &ctx, code.center(), size);
            settle(&mut app, &ctx, size, 3);
            choose_language(&mut app, &ctx, size, label_text);
            settle(&mut app, &ctx, size, 3);
            let editor = app.tab.editor.as_ref().unwrap();
            assert_eq!(editor.document.text, format!("```{language}\n{body}\n```"));
            assert!(!egui::Popup::is_any_open(&ctx));
            let blocks = pulldown_cmark::Parser::new(&editor.document.text)
                .filter(|event| {
                    matches!(
                        event,
                        pulldown_cmark::Event::Start(pulldown_cmark::Tag::CodeBlock(_))
                    )
                })
                .count();
            assert_eq!(blocks, 1);
        }
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Z,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            size,
        );
        assert_eq!(
            app.tab.editor.as_ref().unwrap().document.text,
            format!("```python\n{body}\n```")
        );
    }
}
#[test]
fn grouped_context_submenus_keep_selection_while_choosing_code_language() {
    let ctx = egui::Context::default();
    design::install(&ctx);
    let size = egui::vec2(980., 640.);
    let mut app = notebook("let n = 1; 🌍".into());
    app.prefs.mode = Mode::Source;
    settle(&mut app, &ctx, size, 4);
    select_source(&mut app, &ctx, size, 0..12);
    let editor = ctx
        .read_response(egui::Id::new(("journal-editor", "Note.md")))
        .unwrap()
        .rect;
    let pos = editor.min + egui::vec2(20., 10.);
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
    for item in ["Blocks", "Code Block"] {
        settle(&mut app, &ctx, size, 4);
        let output = frame(&mut app, &ctx, vec![], size);
        click(&mut app, &ctx, label(&output, item).center(), size);
    }
    settle(&mut app, &ctx, size, 4);
    choose_language(&mut app, &ctx, size, "Rust");
    assert_eq!(
        app.tab.editor.as_ref().unwrap().document.text,
        "```rust\nlet n = 1; 🌍\n```"
    );
    settle(&mut app, &ctx, size, 3);
    assert!(!egui::Popup::is_any_open(&ctx));
}
#[test]
fn floating_toolbar_formats_the_selected_text_and_undo_restores_it() {
    let ctx = egui::Context::default();
    design::install(&ctx);
    let size = egui::vec2(980., 640.);
    let mut app = notebook("Hello 🌍 world".into());
    settle(&mut app, &ctx, size, 4);
    let id = egui::Id::new(("journal-editor", "Note.md"));
    let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(7),
        )));
    state.store(&ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
    settle(&mut app, &ctx, size, 4);
    let bold = ctx
        .read_response(egui::Id::new(("journal-floating-format", "Bold")))
        .expect("selection toolbar")
        .rect;
    assert!(bold.left() >= 0. && bold.right() <= size.x);
    click(&mut app, &ctx, bold.center(), size);
    assert_eq!(
        app.tab.editor.as_ref().unwrap().document.text,
        "**Hello 🌍** world"
    );
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }],
        size,
    );
    assert_eq!(
        app.tab.editor.as_ref().unwrap().document.text,
        "Hello 🌍 world"
    );
    settle(&mut app, &ctx, size, 3);
    let style = ctx
        .read_response(egui::Id::new("journal-floating-headings"))
        .unwrap()
        .rect;
    click(&mut app, &ctx, style.center(), size);
    settle(&mut app, &ctx, size, 3);
    let output = frame(&mut app, &ctx, vec![], size);
    click(&mut app, &ctx, label(&output, "Heading 2").center(), size);
    assert_eq!(
        app.tab.editor.as_ref().unwrap().document.text,
        "## Hello 🌍 world"
    );
}
#[test]
fn formatting_controls_detect_existing_styles_toggle_and_preserve_code_with_undo() {
    for dark in [false, true] {
        let ctx = egui::Context::default();
        design::install(&ctx);
        design::apply_theme(&ctx, dark);
        let size = egui::vec2(980., 640.);
        let original = "## **🌍 testing**\n\n```javascript\nconst x = 1;\n```";
        let mut app = notebook(original.into());
        settle(&mut app, &ctx, size, 4);
        select_source(&mut app, &ctx, size, 3..16);
        assert!(
            app.tab
                .editor
                .as_mut()
                .unwrap()
                .formatting_state()
                .is_active(Format::Bold)
        );
        let id = egui::Id::new(("journal-floating-format", "Bold"));
        click(
            &mut app,
            &ctx,
            ctx.read_response(id).unwrap().rect.center(),
            size,
        );
        assert_eq!(
            app.tab.editor.as_ref().unwrap().document.text,
            "## 🌍 testing\n\n```javascript\nconst x = 1;\n```"
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Z,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            size,
        );
        assert_eq!(app.tab.editor.as_ref().unwrap().document.text, original);
        select_source(&mut app, &ctx, size, 0..original.chars().count());
        let headings = ctx
            .read_response(egui::Id::new("journal-floating-headings"))
            .unwrap()
            .rect;
        click(&mut app, &ctx, headings.center(), size);
        settle(&mut app, &ctx, size, 3);
        let output = frame(&mut app, &ctx, vec![], size);
        click(&mut app, &ctx, label(&output, "Heading 3").center(), size);
        assert_eq!(
            app.tab.editor.as_ref().unwrap().document.text,
            "### **🌍 testing**\n\n```javascript\nconst x = 1;\n```"
        );
        // The toolbar and keyboard both refuse to insert emphasis into code.
        let text = &app.tab.editor.as_ref().unwrap().document.text;
        let body = text[..text.find("const").unwrap()].chars().count();
        select_source(&mut app, &ctx, size, body..body + 5);
        assert!(!ctx.read_response(id).unwrap().enabled());
        let revision = app.tab.editor.as_ref().unwrap().revision;
        let before = app.tab.editor.as_ref().unwrap().document.text.clone();
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::B,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            size,
        );
        let editor = app.tab.editor.as_ref().unwrap();
        assert_eq!(editor.document.text, before);
        assert_eq!(
            editor.revision, revision,
            "ignored styles must not create edits"
        );
    }
}
#[test]
fn split_layout_menu_and_drag_resize_the_actual_panes() {
    for size in [egui::vec2(680., 500.), egui::vec2(1160., 760.)] {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut app = notebook("# Title\n\nA paragraph.".into());
        settle(&mut app, &ctx, size, 4);
        let source = ctx
            .read_response(egui::Id::new("journal-source-pane"))
            .unwrap()
            .rect;
        let preview = ctx
            .read_response(egui::Id::new("journal-preview-pane"))
            .unwrap()
            .rect;
        assert!(source.right() <= preview.left());
        let handle = ctx
            .read_response(egui::Id::new("journal-split-divider"))
            .unwrap()
            .rect
            .center();
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(handle),
                egui::Event::PointerButton {
                    pos: handle,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            size,
        );
        let moved = handle + egui::vec2(65., 0.);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(moved)], size);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: moved,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            size,
        );
        settle(&mut app, &ctx, size, 3);
        assert!(app.tab.editor.as_ref().unwrap().scroll.split_ratio > 0.5);
        let menu = ctx
            .read_response(egui::Id::new("journal-view-options"))
            .unwrap()
            .rect;
        click(&mut app, &ctx, menu.center(), size);
        settle(&mut app, &ctx, size, 3);
        let stacked = ctx
            .read_response(egui::Id::new("journal-layout-stacked"))
            .unwrap()
            .rect;
        click(&mut app, &ctx, stacked.center(), size);
        settle(&mut app, &ctx, size, 3);
        assert_eq!(app.prefs.preview_layout, PreviewLayout::Stacked);
        let source = ctx
            .read_response(egui::Id::new("journal-source-pane"))
            .unwrap()
            .rect;
        let preview = ctx
            .read_response(egui::Id::new("journal-preview-pane"))
            .unwrap()
            .rect;
        assert!(source.bottom() <= preview.top());
        assert!(preview.bottom() <= size.y);
    }
}
#[test]
fn scrolling_tracks_source_locations_in_both_directions_and_can_be_disabled() {
    let ctx = egui::Context::default();
    design::install(&ctx);
    let size = egui::vec2(1160., 760.);
    let text = (0..90).map(|n| format!("## Section {n}\n\nA paragraph with **bold** and enough text to wrap on a narrow display. Words stay in the same source paragraph.\n\n")).collect();
    let mut app = notebook(text);
    settle(&mut app, &ctx, size, 5);
    let source = ctx
        .read_response(egui::Id::new("journal-source-pane"))
        .unwrap()
        .rect;
    let wheel = |delta| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        phase: egui::TouchPhase::Move,
        delta: egui::vec2(0., delta),
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(source.center()), wheel(-400.)],
        size,
    );
    settle(&mut app, &ctx, size, 30);
    let editor = app.tab.editor.as_ref().unwrap();
    assert!(editor.scroll.source_offset > 100.);
    assert!(editor.scroll.preview_offset > 10.);
    let source_line = editor
        .scroll
        .line_for_y((editor.scroll.source_offset - 10.).max(0.));
    let preview_line = editor
        .preview
        .line_for_y((editor.scroll.preview_offset - 20.).max(0.));
    assert!(
        (source_line - preview_line).abs() < 2.,
        "{source_line} vs {preview_line}"
    );
    let preview = ctx
        .read_response(egui::Id::new("journal-preview-pane"))
        .unwrap()
        .rect;
    let old = editor.scroll.source_offset;
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(preview.center()), wheel(-300.)],
        size,
    );
    settle(&mut app, &ctx, size, 30);
    assert!(app.tab.editor.as_ref().unwrap().scroll.source_offset > old);
    app.prefs.sync_scroll = false;
    let old = app.tab.editor.as_ref().unwrap().scroll.preview_offset;
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(source.center()), wheel(-300.)],
        size,
    );
    settle(&mut app, &ctx, size, 30);
    assert!((app.tab.editor.as_ref().unwrap().scroll.preview_offset - old).abs() < 1.);
}
#[test]
fn every_mode_fits_both_window_sizes_and_cache_tracks_edits() {
    for size in [egui::vec2(680., 500.), egui::vec2(1160., 760.)] {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut app = Harness {
            action: None,
            tab: JournalTab::default(),
            prefs: Preferences::default(),
        };
        app.tab.open(Document{path:"Notebook.md".into(),text:"# Notebook\n\n**Bold** and _italic_.\n\n| A | B |\n| --- | --- |\n| one | two |\n\n- [ ] Task\n\n```rust\nlet n=1;\n```\n".into(),fingerprint:"test".into()});
        for mode in [Mode::Source, Mode::Split, Mode::Preview] {
            app.prefs.mode = mode;
            for _ in 0..4 {
                frame(&mut app, &ctx, vec![], size);
            }
            let editor = app.tab.editor.as_ref().unwrap();
            if mode != Mode::Source {
                assert_eq!(editor.preview_revision, editor.revision);
                assert!(editor.preview_source.contains("Notebook"));
            }
        }
    }
}
#[test]
fn wikilinks_preserve_relative_links_and_do_not_touch_code_or_front_matter() {
    let source = markdown_source(
        "---\ntitle: Private\n---\n[[Folder/Note|My note]] [Other](Other%20Note.md)\n\n```md\n[[code]]\n```\n",
    );
    let inline = markdown_source("`[[literal]]` and [[Real]]\n\n    [[indented]]\n");
    assert!(inline.contains("`[[literal]]`"));
    assert!(inline.contains("    [[indented]]"));
    assert!(inline.contains("[Real](amigo-journal://Real)"));
    let windows = markdown_source("\u{feff}---\r\ntitle: Private\r\n---\r\n# Windows\r\n");
    assert_eq!(windows, "# Windows\r\n");
    assert!(!source.contains("title: Private"));
    assert!(source.contains("[My note](amigo-journal://Folder/Note)"));
    assert!(source.contains("[[code]]"));
    assert!(source.contains("[Other](Other%20Note.md)"));
}

fn preview_key(
    app: &mut Harness,
    ctx: &egui::Context,
    size: egui::Vec2,
    key: egui::Key,
    modifiers: egui::Modifiers,
) {
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            }],
            size,
        );
    }
}
fn preview_replace(
    app: &mut Harness,
    ctx: &egui::Context,
    size: egui::Vec2,
    id: egui::Id,
    range: std::ops::Range<usize>,
    value: &str,
) {
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(range.start),
            egui::text::CCursor::new(range.end),
        )));
    state.store(ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
    frame(app, ctx, vec![egui::Event::Text(value.into())], size);
}
#[test]
fn preview_text_edits_keep_markdown_sync_and_undo_in_every_layout() {
    for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
        for dark in [false, true] {
            for (mode, layout) in [
                (Mode::Preview, PreviewLayout::SideBySide),
                (Mode::Split, PreviewLayout::SideBySide),
                (Mode::Split, PreviewLayout::Stacked),
            ] {
                let ctx = egui::Context::default();
                design::install(&ctx);
                design::apply_theme(&ctx, dark);
                let original = "---\nprivate: Keep me\n---\n# Heading\n\nHello **bold** and [link](Note.md).\n";
                let mut app = Harness {
                    action: None,
                    tab: JournalTab::default(),
                    prefs: Preferences {
                        mode,
                        preview_editable: true,
                        preview_layout: layout,
                        ..Default::default()
                    },
                };
                app.tab.open(Document {
                    path: "Note.md".into(),
                    text: original.into(),
                    fingerprint: "test".into(),
                });
                for _ in 0..4 {
                    frame(&mut app, &ctx, vec![], size);
                }
                let output = frame(&mut app, &ctx, vec![], size);
                click(
                    &mut app,
                    &ctx,
                    label(&output, "Hello bold and link.").center(),
                    size,
                );
                let id = ctx.memory(|m| m.focused()).unwrap();
                preview_replace(&mut app, &ctx, size, id, 6..10, "brave🌍");
                let edited = "---\nprivate: Keep me\n---\n# Heading\n\nHello **brave🌍** and [link](Note.md).\n";
                assert_eq!(app.tab.editor.as_ref().unwrap().document.text, edited);
                assert!(app.tab.dirty());
                preview_key(&mut app, &ctx, size, egui::Key::Z, egui::Modifiers::COMMAND);
                assert_eq!(
                    app.tab.editor.as_ref().unwrap().document.text,
                    original,
                    "{mode:?} {layout:?}"
                );
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
                assert_eq!(app.tab.editor.as_ref().unwrap().document.text, edited);
                app.prefs.mode = Mode::Source;
                for _ in 0..3 {
                    frame(&mut app, &ctx, vec![], size);
                }
                assert_eq!(app.tab.editor.as_ref().unwrap().document.text, edited);
            }
        }
    }
}
#[test]
fn preview_code_edits_preserve_fences_and_loading_blocks_input() {
    let ctx = egui::Context::default();
    design::install(&ctx);
    let size = egui::vec2(980., 640.);
    let original = "```rust\nlet n = 1;\n```\n";
    let mut app = Harness {
        action: None,
        tab: JournalTab::default(),
        prefs: Preferences {
            mode: Mode::Preview,
            preview_editable: true,
            ..Default::default()
        },
    };
    app.tab.open(Document {
        path: "Code.md".into(),
        text: original.into(),
        fingerprint: "test".into(),
    });
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![], size);
    }
    let output = frame(&mut app, &ctx, vec![], size);
    click(&mut app, &ctx, label(&output, "let n = 1;").center(), size);
    let id = ctx.memory(|m| m.focused()).unwrap();
    preview_replace(&mut app, &ctx, size, id, 8..9, "2");
    assert_eq!(
        app.tab.editor.as_ref().unwrap().document.text,
        "```rust\nlet n = 2;\n```\n"
    );
    app.tab.loading = true;
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("Blocked".into())],
        size,
    );
    assert!(
        !app.tab
            .editor
            .as_ref()
            .unwrap()
            .document
            .text
            .contains("Blocked")
    );
    app.tab.loading = false;
    ctx.memory_mut(|m| m.request_focus(id));
    frame(&mut app, &ctx, vec![egui::Event::Text("3".into())], size);
    assert!(
        app.tab
            .editor
            .as_ref()
            .unwrap()
            .document
            .text
            .contains("23")
    );
}

#[test]
fn preview_table_cells_tasks_and_separate_paragraphs_edit_independently() {
    let ctx = egui::Context::default();
    design::install(&ctx);
    let size = egui::vec2(980., 640.);
    let mut app = Harness {
        action: None,
        tab: JournalTab::default(),
        prefs: Preferences {
            mode: Mode::Preview,
            preview_editable: true,
            ..Default::default()
        },
    };
    app.tab.open(Document { path: "Note.md".into(), text: "First **paragraph**.\n\nSecond paragraph.\n\n- [ ] Task\n\n| A | B |\n| --- | --- |\n| one | two |\n".into(), fingerprint: "test".into() });
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![], size);
    }
    let output = frame(&mut app, &ctx, vec![], size);
    click(
        &mut app,
        &ctx,
        label(&output, "First paragraph.").center(),
        size,
    );
    let first_id = ctx.memory(|m| m.focused()).unwrap();
    preview_replace(&mut app, &ctx, size, first_id, 6..15, "edited");
    preview_key(
        &mut app,
        &ctx,
        size,
        egui::Key::Enter,
        egui::Modifiers::NONE,
    );
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("New line".into())],
        size,
    );
    assert!(
        app.tab
            .editor
            .as_ref()
            .unwrap()
            .document
            .text
            .starts_with("First **edited\nNew line**."),
        "{:?}",
        app.tab.editor.as_ref().unwrap().document.text
    );
    let output = frame(&mut app, &ctx, vec![], size);
    click(
        &mut app,
        &ctx,
        label(&output, "Second paragraph.").center(),
        size,
    );
    let second_id = ctx.memory(|m| m.focused()).unwrap();
    assert_ne!(first_id, second_id);
    preview_replace(&mut app, &ctx, size, second_id, 0..6, "Another");
    let output = frame(&mut app, &ctx, vec![], size);
    let task = label(&output, "Task");
    click(
        &mut app,
        &ctx,
        task.left_center() - egui::vec2(28., 0.),
        size,
    );
    assert!(
        app.tab
            .editor
            .as_ref()
            .unwrap()
            .document
            .text
            .contains("- [x] Task")
    );
    preview_key(&mut app, &ctx, size, egui::Key::Z, egui::Modifiers::COMMAND);
    assert!(
        app.tab
            .editor
            .as_ref()
            .unwrap()
            .document
            .text
            .contains("- [ ] Task")
    );
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
    assert!(
        app.tab
            .editor
            .as_ref()
            .unwrap()
            .document
            .text
            .contains("- [x] Task")
    );
    let output = frame(&mut app, &ctx, vec![], size);
    click(&mut app, &ctx, label(&output, "one").center(), size);
    let one_id = ctx.memory(|m| m.focused()).unwrap();
    preview_replace(&mut app, &ctx, size, one_id, 0..3, "1");
    let output = frame(&mut app, &ctx, vec![], size);
    click(&mut app, &ctx, label(&output, "two").center(), size);
    let two_id = ctx.memory(|m| m.focused()).unwrap();
    assert_ne!(one_id, two_id);
    preview_replace(&mut app, &ctx, size, two_id, 0..3, "2");
    let text = &app.tab.editor.as_ref().unwrap().document.text;
    assert!(text.contains("Another paragraph."), "{text}");
    assert!(text.contains("| 1 | 2 |"), "{text}");
    assert!(text.starts_with("First **edited\nNew line**."), "{text}");
}
