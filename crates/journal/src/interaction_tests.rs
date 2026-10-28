use super::*;

fn workspace_frame(
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
            egui::Panel::left("test-notebook-sidebar")
                .exact_size(224.)
                .resizable(false)
                .frame(egui::Frame::new().inner_margin(18))
                .show(ui, |ui| {
                    app.action = app.tab.sidebar(ui);
                });
            egui::CentralPanel::default().show(ui, |ui| {
                app.action = app.tab.show(ui, &mut app.prefs).or(app.action.take());
            });
            app.action = app.tab.dialogs(ctx).or(app.action.take());
        },
    );
    output.textures_delta.clear();
    output
}
fn settle_workspace(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2) -> egui::FullOutput {
    let mut output = workspace_frame(app, ctx, vec![], size);
    for _ in 0..4 {
        output = workspace_frame(app, ctx, vec![], size);
    }
    output
}
fn click_workspace(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2, pos: egui::Pos2) {
    for pressed in [true, false] {
        workspace_frame(
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
fn press(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2, key: egui::Key) {
    workspace_frame(
        app,
        ctx,
        vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        size,
    );
}
fn rect(ctx: &egui::Context, id: impl std::hash::Hash + std::fmt::Debug) -> egui::Rect {
    ctx.read_response(egui::Id::new(id)).unwrap().rect
}
fn tree_entries() -> Vec<Entry> {
    vec![
        Entry {
            path: "Root.md".into(),
            folder: false,
        },
        Entry {
            path: "Work".into(),
            folder: true,
        },
        Entry {
            path: "Work/Note.md".into(),
            folder: false,
        },
    ]
}

#[test]
fn notebook_rows_show_one_name_tooltip_without_the_markdown_extension() {
    fn text_count(shape: &egui::Shape, expected: &str) -> usize {
        match shape {
            egui::Shape::Text(text) => usize::from(text.galley.text() == expected),
            egui::Shape::Vec(shapes) => shapes.iter().map(|s| text_count(s, expected)).sum(),
            _ => 0,
        }
    }
    for dark in [false, true] {
        let ctx = egui::Context::default();
        design::install(&ctx);
        design::apply_theme(&ctx, dark);
        ctx.style_mut_of(
            if dark {
                egui::Theme::Dark
            } else {
                egui::Theme::Light
            },
            |style| {
                style.interaction.tooltip_delay = 0.;
                style.interaction.show_tooltips_only_when_still = false;
            },
        );
        let size = egui::vec2(980., 640.);
        let mut app = notebook(String::new());
        app.tab.entries = tree_entries();
        for (path, name) in [("Root.md", "Root"), ("Work", "Work")] {
            settle_workspace(&mut app, &ctx, size);
            let pos = rect(&ctx, ("journal-entry", path)).center();
            workspace_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)], size);
            let output = settle_workspace(&mut app, &ctx, size);
            let count = |expected| {
                output
                    .shapes
                    .iter()
                    .map(|s| text_count(&s.shape, expected))
                    .sum::<usize>()
            };
            assert_eq!(count(name), 2, "one row label and one tooltip for {path}");
            assert_eq!(count(&format!("{name}.md")), 0);
        }
    }
}

fn enter_creation_name(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2, name: &str) {
    click_workspace(app, ctx, size, rect(ctx, "journal-note-name").center());
    workspace_frame(
        app,
        ctx,
        vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            },
        }],
        size,
    );
    workspace_frame(
        app,
        ctx,
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text(name.into()),
        ],
        size,
    );
    assert_eq!(app.tab.dialog.as_ref().unwrap().value, name);
}

fn right_click(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2, pos: egui::Pos2) {
    for pressed in [true, false] {
        workspace_frame(
            app,
            ctx,
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
}
fn open_tree_menu(
    app: &mut Harness,
    ctx: &egui::Context,
    size: egui::Vec2,
    path: &str,
) -> egui::FullOutput {
    settle_workspace(app, ctx, size);
    if let Some(menu) = ctx.read_response(egui::Id::new(("journal-entry-menu", path))) {
        click_workspace(app, ctx, size, menu.rect.center());
    } else {
        right_click(app, ctx, size, rect(ctx, ("journal-entry", path)).center());
    }
    settle_workspace(app, ctx, size)
}
fn choose_tree_action(
    app: &mut Harness,
    ctx: &egui::Context,
    size: egui::Vec2,
    path: &str,
    action: &str,
) {
    open_tree_menu(app, ctx, size, path);
    click_workspace(
        app,
        ctx,
        size,
        rect(ctx, (format!("journal-entry-{action}"), path)).center(),
    );
}

#[test]
fn only_notes_have_direct_trash_shortcuts_with_confirmation_and_no_navigation() {
    for dark in [false, true] {
        for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            design::apply_theme(&ctx, dark);
            let mut app = notebook("Preserve current draft".into());
            app.tab.entries = tree_entries();
            app.tab.entries.extend([
                Entry {
                    path: TRASH_FOLDER.into(),
                    folder: true,
                },
                Entry {
                    path: "Trash/Folder".into(),
                    folder: true,
                },
                Entry {
                    path: "Trash/Old.md".into(),
                    folder: false,
                },
            ]);
            app.tab
                .expanded
                .extend(["Work".into(), TRASH_FOLDER.into()]);
            settle_workspace(&mut app, &ctx, size);
            for folder in ["Work", TRASH_FOLDER, "Trash/Folder"] {
                assert!(
                    ctx.read_response(egui::Id::new(("journal-note-trash", folder)))
                        .is_none()
                );
                assert_eq!(
                    rect(&ctx, ("journal-entry-menu", folder)).size(),
                    egui::vec2(22., 22.)
                );
            }
            for path in ["Root.md", "Work/Note.md", "Trash/Old.md"] {
                settle_workspace(&mut app, &ctx, size);
                let row = rect(&ctx, ("journal-entry", path));
                let trash = rect(&ctx, ("journal-note-trash", path));
                assert!(
                    ctx.read_response(egui::Id::new(("journal-entry-menu", path)))
                        .is_none()
                );
                assert_eq!(trash.size(), egui::vec2(22., 22.));
                assert!(row.right() < trash.left() && trash.right() <= 206.);
                app.tab.loading = true;
                settle_workspace(&mut app, &ctx, size);
                click_workspace(&mut app, &ctx, size, trash.center());
                assert!(app.tab.dialog.is_none() && app.action.is_none());
                app.tab.loading = false;
                settle_workspace(&mut app, &ctx, size);
                click_workspace(&mut app, &ctx, size, trash.center());
                assert!(
                    matches!(&app.tab.dialog.as_ref().unwrap().kind, NameKind::Delete(target) if target == path)
                );
                assert!(app.action.is_none());
                assert_eq!(
                    app.tab.editor.as_ref().unwrap().document.text,
                    "Preserve current draft"
                );
                press(&mut app, &ctx, size, egui::Key::Escape);
                assert!(app.tab.dialog.is_none());
                settle_workspace(&mut app, &ctx, size);
                click_workspace(&mut app, &ctx, size, trash.center());
                let output = settle_workspace(&mut app, &ctx, size);
                let permanent = is_in_trash(path);
                click_workspace(
                    &mut app,
                    &ctx,
                    size,
                    label(
                        &output,
                        if permanent {
                            "Delete Permanently"
                        } else {
                            "Move to Trash"
                        },
                    )
                    .center(),
                );
                assert!(match &app.action {
                    Some(Action::Trash(target)) if !permanent => target == path,
                    Some(Action::DeletePermanently(target)) if permanent => target == path,
                    _ => false,
                });
                assert!(
                    app.tab.expanded.contains("Work") && app.tab.expanded.contains(TRASH_FOLDER)
                );
            }
        }
    }
}

#[test]
fn tree_menus_create_in_their_folder_and_header_buttons_stay_at_root() {
    for dark in [false, true] {
        for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            design::apply_theme(&ctx, dark);
            let mut app = notebook("Preserve this draft".into());
            app.tab.entries = vec![
                Entry {
                    path: "Work".into(),
                    folder: true,
                },
                Entry {
                    path: "Work/Deep".into(),
                    folder: true,
                },
                Entry {
                    path: "Work/Deep/Note.md".into(),
                    folder: false,
                },
            ];
            app.tab.expanded.insert("Work".into());
            for parent in ["Work", "Work/Deep"] {
                settle_workspace(&mut app, &ctx, size);
                let button = rect(&ctx, ("journal-entry-menu", parent));
                assert_eq!(button.size(), egui::vec2(22., 22.));
                assert!(
                    rect(&ctx, ("journal-entry", parent)).right() < button.left()
                        && button.right() <= 206.
                );
                for kind in ["new-note", "new-folder"] {
                    let expanded = app.tab.expanded.clone();
                    open_tree_menu(&mut app, &ctx, size, parent);
                    let note = rect(&ctx, ("journal-entry-new-note", parent));
                    let folder = rect(&ctx, ("journal-entry-new-folder", parent));
                    let delete = rect(&ctx, ("journal-entry-delete", parent));
                    assert!(note.bottom() < folder.top() && folder.bottom() < delete.top());
                    assert!(note.left() >= 0. && delete.bottom() <= size.y);
                    click_workspace(
                        &mut app,
                        &ctx,
                        size,
                        rect(&ctx, (format!("journal-entry-{kind}"), parent)).center(),
                    );
                    let output = settle_workspace(&mut app, &ctx, size);
                    assert!(label_optional(&output, parent).is_some());
                    assert_eq!(app.tab.expanded, expanded);
                    enter_creation_name(&mut app, &ctx, size, "Created");
                    press(&mut app, &ctx, size, egui::Key::Enter);
                    let expected = format!("{parent}/Created");
                    assert!(
                        match &app.action {
                            Some(Action::NewNote(path)) if kind == "new-note" => path == &expected,
                            Some(Action::NewFolder(path)) if kind == "new-folder" =>
                                path == &expected,
                            _ => false,
                        },
                        "unexpected action: {:?}",
                        app.action
                    );
                    assert!(app.tab.dialog.as_ref().unwrap().pending);
                    app.tab.folder_edit_result(None);
                    assert!(app.tab.dialog.is_none());
                    assert_eq!(
                        app.tab.editor.as_ref().unwrap().document.text,
                        "Preserve this draft"
                    );
                }
            }
            app.tab.open(Document {
                path: "Work/Deep/Note.md".into(),
                text: "Nested note".into(),
                fingerprint: String::new(),
            });
            for (id, is_folder) in [("journal-new-note", false), ("journal-new-folder", true)] {
                settle_workspace(&mut app, &ctx, size);
                click_workspace(&mut app, &ctx, size, rect(&ctx, id).center());
                let output = settle_workspace(&mut app, &ctx, size);
                assert!(label_optional(&output, "Journal Root").is_some());
                let kind = if is_folder { "folder" } else { "note" };
                let area = ctx
                    .memory(|m| m.area_rect(egui::Id::new("journal-name-dialog")))
                    .unwrap();
                assert!(area.height() <= 80. && area.width() < size.x);
                assert!(ctx.memory(|m| m.has_focus(egui::Id::new("journal-note-name"))));
                let input = rect(&ctx, "journal-note-name");
                let confirm = rect(&ctx, format!("journal-{kind}-confirm"));
                let cancel = rect(&ctx, format!("journal-{kind}-cancel"));
                assert!(input.right() < confirm.left() && confirm.right() < cancel.left());
                assert!((input.center().y - confirm.center().y).abs() < 1.);
                enter_creation_name(&mut app, &ctx, size, "Root item");
                press(&mut app, &ctx, size, egui::Key::Enter);
                assert!(match &app.action {
                    Some(Action::NewNote(path)) if !is_folder => path == "Root item",
                    Some(Action::NewFolder(path)) if is_folder => path == "Root item",
                    _ => false,
                });
                assert!(app.tab.dialog.as_ref().unwrap().pending);
                app.tab
                    .folder_edit_result(Some("Could not create item; retry."));
                assert_eq!(app.tab.dialog.as_ref().unwrap().value, "Root item");
                settle_workspace(&mut app, &ctx, size);
                click_workspace(&mut app, &ctx, size, confirm.center());
                assert!(app.tab.dialog.as_ref().unwrap().pending);
                app.tab.folder_edit_result(None);
            }
        }
    }
}

#[test]
fn three_dot_and_context_menus_share_actions_without_navigating_and_spaces_target_their_parent() {
    for dark in [false, true] {
        for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            design::apply_theme(&ctx, dark);
            let mut app = notebook("Keep this note".into());
            app.tab.entries = tree_entries();
            app.tab.expanded.insert("Work".into());
            settle_workspace(&mut app, &ctx, size);
            assert!(rect(&ctx, "journal-search-open").right() <= 206.);
            for path in ["Root.md", "Work", "Work/Note.md"] {
                for context in [false, true] {
                    if context {
                        right_click(
                            &mut app,
                            &ctx,
                            size,
                            rect(&ctx, ("journal-entry", path)).center(),
                        );
                        settle_workspace(&mut app, &ctx, size);
                    } else {
                        open_tree_menu(&mut app, &ctx, size, path);
                    }
                    assert!(egui::Popup::is_any_open(&ctx));
                    assert!(rect(&ctx, ("journal-entry-delete", path)).height() <= 28.);
                    assert!(app.action.is_none());
                    assert!(app.tab.expanded.contains("Work"));
                    click_workspace(
                        &mut app,
                        &ctx,
                        size,
                        rect(&ctx, ("journal-entry-delete", path)).center(),
                    );
                    assert!(
                        matches!(&app.tab.dialog.as_ref().unwrap().kind, NameKind::Delete(target) if target == path)
                    );
                    assert_eq!(
                        app.tab.editor.as_ref().unwrap().document.text,
                        "Keep this note"
                    );
                    press(&mut app, &ctx, size, egui::Key::Escape);
                    assert!(app.tab.dialog.is_none() && app.action.is_none());
                    settle_workspace(&mut app, &ctx, size);
                }
            }
            for (space, parent) in [("Work/Note.md", "Work"), ("Root.md", "")] {
                right_click(
                    &mut app,
                    &ctx,
                    size,
                    rect(&ctx, ("journal-entry-space", space)).center(),
                );
                settle_workspace(&mut app, &ctx, size);
                click_workspace(
                    &mut app,
                    &ctx,
                    size,
                    rect(&ctx, ("journal-entry-new-note", parent)).center(),
                );
                assert!(
                    matches!(&app.tab.dialog.as_ref().unwrap().kind, NameKind::Note(folder) if folder == parent)
                );
                press(&mut app, &ctx, size, egui::Key::Escape);
                settle_workspace(&mut app, &ctx, size);
            }
            right_click(
                &mut app,
                &ctx,
                size,
                rect(&ctx, "journal-root-space").center(),
            );
            settle_workspace(&mut app, &ctx, size);
            assert!(
                ctx.read_response(egui::Id::new(("journal-entry-delete", "")))
                    .is_none()
            );
            click_workspace(
                &mut app,
                &ctx,
                size,
                rect(&ctx, ("journal-entry-new-folder", "")).center(),
            );
            let output = settle_workspace(&mut app, &ctx, size);
            assert!(label_optional(&output, "Journal Root").is_some());
            press(&mut app, &ctx, size, egui::Key::Escape);
            choose_tree_action(&mut app, &ctx, size, "Work", "delete");
            let output = settle_workspace(&mut app, &ctx, size);
            click_workspace(
                &mut app,
                &ctx,
                size,
                label(&output, "Move to Trash").center(),
            );
            assert!(matches!(&app.action, Some(Action::Trash(path)) if path == "Work"));
        }
    }
}

#[test]
fn notebook_search_owns_focus_ignores_stale_results_and_opens_with_keyboard() {
    for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut app = notebook("Original".into());
        app.tab.entries = tree_entries();
        settle_workspace(&mut app, &ctx, size);
        click_workspace(
            &mut app,
            &ctx,
            size,
            rect(&ctx, "journal-search-open").center(),
        );
        settle_workspace(&mut app, &ctx, size);
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(egui::Id::new("journal-search-query"))
        );
        workspace_frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("needle".into())],
            size,
        );
        app.tab.search.as_mut().unwrap().changed_at =
            Some(Instant::now() - std::time::Duration::from_secs(1));
        workspace_frame(&mut app, &ctx, vec![], size);
        assert!(matches!(&app.action, Some(Action::Search(query)) if query == "needle"));
        app.tab.apply_search(
            "stale",
            vec![Entry {
                path: "Wrong.md".into(),
                folder: false,
            }],
        );
        assert!(app.tab.search.as_ref().unwrap().results.is_none());
        app.tab.apply_search(
            "needle",
            vec![app.tab.entries[0].clone(), app.tab.entries[2].clone()],
        );
        settle_workspace(&mut app, &ctx, size);
        let modal = ctx
            .memory(|m| m.area_rect(egui::Id::new("journal-search-dialog")))
            .unwrap();
        assert!(modal.left() >= 0. && modal.right() <= size.x && modal.bottom() <= size.y);
        assert!(modal.contains_rect(rect(&ctx, ("journal-search-result", "Work/Note.md"))));
        let viewport = rect(&ctx, "journal-search-results-viewport");
        for path in ["Root.md", "Work/Note.md"] {
            let row = rect(&ctx, ("journal-search-result", path));
            assert!(
                viewport.expand(0.5).contains_rect(row),
                "{path}: {row:?} within {viewport:?}"
            );
        }
        press(&mut app, &ctx, size, egui::Key::ArrowDown);
        press(&mut app, &ctx, size, egui::Key::Enter);
        assert!(matches!(&app.action, Some(Action::Open(path)) if path == "Work/Note.md"));
        assert!(app.tab.search.is_none());
        assert_eq!(app.tab.entries, tree_entries());
        app.tab.open(Document {
            path: "Work/Note.md".into(),
            text: "Result".into(),
            fingerprint: String::new(),
        });
        settle_workspace(&mut app, &ctx, size);
        assert!(
            ctx.read_response(egui::Id::new(("journal-entry", "Work/Note.md")))
                .is_some()
        );
        click_workspace(
            &mut app,
            &ctx,
            size,
            rect(&ctx, "journal-search-open").center(),
        );
        settle_workspace(&mut app, &ctx, size);
        press(&mut app, &ctx, size, egui::Key::Escape);
        assert!(app.tab.search.is_none());
        assert_eq!(app.tab.editor.as_ref().unwrap().document.text, "Result");
    }
}

#[test]
fn external_file_destination_defaults_to_root_selects_folders_and_cancels() {
    let ctx = egui::Context::default();
    design::install(&ctx);
    let size = egui::vec2(980., 640.);
    let mut app = notebook("Keep this draft".into());
    app.tab.entries = tree_entries();
    app.tab.entries.push(Entry {
        path: TRASH_FOLDER.into(),
        folder: true,
    });
    let source = std::path::PathBuf::from("/tmp/External.md");
    app.tab
        .choose_external_destination(source.clone(), String::new(), String::new());
    let output = settle_workspace(&mut app, &ctx, size);
    assert!(label_optional(&output, "Journal Root").is_some());
    press(&mut app, &ctx, size, egui::Key::Enter);
    assert!(
        matches!(&app.action, Some(Action::StoreExternal { source: path, folder }) if path == &source && folder.is_empty())
    );
    app.tab
        .choose_external_destination(source.clone(), String::new(), String::new());
    settle_workspace(&mut app, &ctx, size);
    click_workspace(
        &mut app,
        &ctx,
        size,
        rect(&ctx, "journal-external-folder-open").center(),
    );
    settle_workspace(&mut app, &ctx, size);
    assert!(
        ctx.read_response(egui::Id::new(("journal-destination-folder", TRASH_FOLDER)))
            .is_none()
    );
    click_workspace(
        &mut app,
        &ctx,
        size,
        rect(&ctx, ("journal-destination-folder", "Work")).center(),
    );
    let output = settle_workspace(&mut app, &ctx, size);
    click_workspace(
        &mut app,
        &ctx,
        size,
        label(&output, "Save and Open").center(),
    );
    assert!(matches!(&app.action, Some(Action::StoreExternal { folder, .. }) if folder == "Work"));
    app.tab
        .choose_external_destination(source, String::new(), String::new());
    settle_workspace(&mut app, &ctx, size);
    press(&mut app, &ctx, size, egui::Key::Escape);
    assert!(app.tab.external_file.is_none() && app.action.is_none());
    assert_eq!(
        app.tab.editor.as_ref().unwrap().document.text,
        "Keep this draft"
    );
}

#[test]
fn deleting_a_folder_cleans_expansion_and_closing_a_note_discards_inline_rename() {
    let mut tab = JournalTab::default();
    tab.open(Document {
        path: "Work/Deep/Note.md".into(),
        text: String::new(),
        fingerprint: String::new(),
    });
    tab.update_entries(vec![Entry {
        path: "Work".into(),
        folder: true,
    }]);
    assert!(tab.expanded.contains("Work"));
    assert!(!tab.expanded.contains("Work/Deep"));
    tab.update_entries(vec![]);
    assert!(tab.expanded.is_empty());
    tab.title_edit = Some(TitleEdit {
        path: "Work/Deep/Note.md".into(),
        value: "Draft".into(),
        focus: false,
    });
    tab.close_note();
    assert!(tab.editor.is_none() && tab.title_edit.is_none());
}

#[test]
fn trash_folder_is_protected_and_expands_readable_notes_with_action_menus() {
    for dark in [false, true] {
        let ctx = egui::Context::default();
        design::install(&ctx);
        design::apply_theme(&ctx, dark);
        let size = egui::vec2(980., 640.);
        let mut app = notebook("Current note".into());
        let path = "Trash/123e4567-e89b-42d3-a456-426614174000-Deleted.md";
        app.tab.update_entries(vec![
            Entry {
                path: TRASH_FOLDER.into(),
                folder: true,
            },
            Entry {
                path: path.into(),
                folder: false,
            },
        ]);
        settle_workspace(&mut app, &ctx, size);
        let trash = rect(&ctx, ("journal-entry", TRASH_FOLDER));
        assert!(
            ctx.read_response(egui::Id::new(("journal-entry-delete", TRASH_FOLDER)))
                .is_none()
        );
        assert!(
            ctx.read_response(egui::Id::new(("journal-entry", path)))
                .is_none()
        );
        click_workspace(&mut app, &ctx, size, trash.center());
        let output = settle_workspace(&mut app, &ctx, size);
        assert!(label_optional(&output, "Deleted").is_some());
        assert_eq!(
            rect(&ctx, ("journal-note-trash", path)).size(),
            egui::vec2(22., 22.)
        );
        click_workspace(
            &mut app,
            &ctx,
            size,
            rect(&ctx, ("journal-entry", path)).center(),
        );
        assert!(matches!(&app.action, Some(Action::Open(opened)) if opened == path));
        app.tab.open(Document {
            path: path.into(),
            text: "Deleted content".into(),
            fingerprint: String::new(),
        });
        settle_workspace(&mut app, &ctx, size);
        click_workspace(
            &mut app,
            &ctx,
            size,
            rect(&ctx, "journal-search-open").center(),
        );
        settle_workspace(&mut app, &ctx, size);
        assert!(
            ctx.read_response(egui::Id::new(("journal-search-result", path)))
                .is_none()
        );
    }
}

#[test]
fn trash_items_require_explicit_permanent_deletion_and_cancel_preserves_the_editor() {
    for dark in [false, true] {
        for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            design::apply_theme(&ctx, dark);
            let mut app = notebook("Keep current draft".into());
            app.tab.update_entries(vec![
                Entry {
                    path: TRASH_FOLDER.into(),
                    folder: true,
                },
                Entry {
                    path: "Trash/Folder".into(),
                    folder: true,
                },
                Entry {
                    path: "Trash/Folder/Note.md".into(),
                    folder: false,
                },
            ]);
            app.tab
                .expanded
                .extend([TRASH_FOLDER.into(), "Trash/Folder".into()]);
            for path in ["Trash/Folder", "Trash/Folder/Note.md"] {
                settle_workspace(&mut app, &ctx, size);
                choose_tree_action(&mut app, &ctx, size, path, "delete");
                let output = settle_workspace(&mut app, &ctx, size);
                assert!(label_optional(&output, "Delete Permanently").is_some());
                assert!(label_optional(&output, "This item and any contents will be permanently deleted. This cannot be undone.").is_some());
                press(&mut app, &ctx, size, egui::Key::Enter);
                assert!(app.action.is_none() && app.tab.dialog.is_some());
                press(&mut app, &ctx, size, egui::Key::Escape);
                assert!(app.tab.dialog.is_none() && app.action.is_none());
                settle_workspace(&mut app, &ctx, size);
                choose_tree_action(&mut app, &ctx, size, path, "delete");
                let output = settle_workspace(&mut app, &ctx, size);
                click_workspace(&mut app, &ctx, size, label(&output, "Cancel").center());
                assert!(app.tab.dialog.is_none() && app.action.is_none());
                assert_eq!(
                    app.tab.editor.as_ref().unwrap().document.text,
                    "Keep current draft"
                );
                assert!(app.tab.expanded.contains("Trash/Folder"));
                settle_workspace(&mut app, &ctx, size);
                choose_tree_action(&mut app, &ctx, size, path, "delete");
                let output = settle_workspace(&mut app, &ctx, size);
                click_workspace(
                    &mut app,
                    &ctx,
                    size,
                    label(&output, "Delete Permanently").center(),
                );
                assert!(
                    matches!(&app.action, Some(Action::DeletePermanently(target)) if target == path)
                );
                assert!(app.tab.dialog.is_none());
            }
        }
    }
}

#[test]
fn trash_menus_restore_items_and_disable_actions_while_loading() {
    for dark in [false, true] {
        for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            design::apply_theme(&ctx, dark);
            let mut app = notebook("Keep current draft".into());
            app.tab.update_entries(vec![
                Entry {
                    path: TRASH_FOLDER.into(),
                    folder: true,
                },
                Entry {
                    path: "Trash/Folder".into(),
                    folder: true,
                },
                Entry {
                    path: "Trash/Folder/Note.md".into(),
                    folder: false,
                },
            ]);
            app.tab
                .expanded
                .extend([TRASH_FOLDER.into(), "Trash/Folder".into()]);
            open_tree_menu(&mut app, &ctx, size, TRASH_FOLDER);
            assert!(
                ctx.read_response(egui::Id::new(("journal-entry-restore", TRASH_FOLDER)))
                    .is_none()
            );
            assert!(
                ctx.read_response(egui::Id::new(("journal-entry-delete", TRASH_FOLDER)))
                    .is_none()
            );
            press(&mut app, &ctx, size, egui::Key::Escape);
            for path in ["Trash/Folder", "Trash/Folder/Note.md"] {
                app.tab.loading = true;
                settle_workspace(&mut app, &ctx, size);
                click_workspace(
                    &mut app,
                    &ctx,
                    size,
                    rect(
                        &ctx,
                        (
                            if path.ends_with(".md") {
                                "journal-note-trash"
                            } else {
                                "journal-entry-menu"
                            },
                            path,
                        ),
                    )
                    .center(),
                );
                assert!(!egui::Popup::is_any_open(&ctx));
                right_click(
                    &mut app,
                    &ctx,
                    size,
                    rect(&ctx, ("journal-entry", path)).center(),
                );
                settle_workspace(&mut app, &ctx, size);
                let restore = ctx
                    .read_response(egui::Id::new(("journal-entry-restore", path)))
                    .unwrap();
                assert!(!restore.enabled());
                click_workspace(&mut app, &ctx, size, restore.rect.center());
                assert!(app.action.is_none());
                press(&mut app, &ctx, size, egui::Key::Escape);
                app.tab.loading = false;
                choose_tree_action(&mut app, &ctx, size, path, "restore");
                assert!(matches!(&app.action, Some(Action::Restore(target)) if target == path));
                assert!(app.tab.dialog.is_none());
                assert_eq!(
                    app.tab.editor.as_ref().unwrap().document.text,
                    "Keep current draft"
                );
                assert!(app.tab.expanded.contains("Trash/Folder"));
            }
        }
    }
}

#[test]
fn compact_folder_editor_owns_focus_validates_and_recovers_failed_renames() {
    for dark in [false, true] {
        for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            design::apply_theme(&ctx, dark);
            let mut app = notebook("Keep the current note".into());
            app.tab.entries = tree_entries();
            app.tab.entries.push(Entry {
                path: TRASH_FOLDER.into(),
                folder: true,
            });
            settle_workspace(&mut app, &ctx, size);
            assert!(
                ctx.read_response(egui::Id::new(("journal-folder-rename", TRASH_FOLDER)))
                    .is_none()
            );
            assert!(
                ctx.read_response(egui::Id::new(("journal-folder-rename", "Root.md")))
                    .is_none()
            );
            click_workspace(
                &mut app,
                &ctx,
                size,
                rect(&ctx, ("journal-folder-rename", "Work")).center(),
            );
            settle_workspace(&mut app, &ctx, size);
            assert!(ctx.memory(|m| m.has_focus(egui::Id::new("journal-note-name"))));
            let area = ctx
                .memory(|m| m.area_rect(egui::Id::new("journal-name-dialog")))
                .unwrap();
            assert!(area.height() <= 80. && area.width() < size.x);
            let input = rect(&ctx, "journal-note-name");
            let confirm = rect(&ctx, "journal-folder-confirm");
            let cancel = rect(&ctx, "journal-folder-cancel");
            assert!(input.right() < confirm.left() && confirm.right() < cancel.left());
            assert!((input.center().y - confirm.center().y).abs() < 1.);
            // The selected folder name is ready to replace without an extra click.
            workspace_frame(
                &mut app,
                &ctx,
                vec![egui::Event::Text("Renamed 🌍".into())],
                size,
            );
            press(&mut app, &ctx, size, egui::Key::Enter);
            assert!(matches!(&app.action, Some(Action::Rename { old, name })
                if old == "Work" && name == "Renamed 🌍"));
            assert!(app.tab.dialog.as_ref().unwrap().pending);
            app.tab.folder_edit_result(Some("A disk error; try again."));
            settle_workspace(&mut app, &ctx, size);
            let draft = app.tab.dialog.as_ref().unwrap();
            assert_eq!(draft.value, "Renamed 🌍");
            assert!(!draft.pending && draft.error.contains("disk error"));
            click_workspace(
                &mut app,
                &ctx,
                size,
                rect(&ctx, "journal-folder-confirm").center(),
            );
            assert!(matches!(&app.action, Some(Action::Rename { old, name })
                if old == "Work" && name == "Renamed 🌍"));
            app.tab.folder_edit_result(None);
            settle_workspace(&mut app, &ctx, size);
            click_workspace(
                &mut app,
                &ctx,
                size,
                rect(&ctx, "journal-new-folder").center(),
            );
            settle_workspace(&mut app, &ctx, size);
            enter_creation_name(&mut app, &ctx, size, "Work");
            press(&mut app, &ctx, size, egui::Key::Enter);
            assert!(app.action.is_none());
            assert!(
                app.tab
                    .dialog
                    .as_ref()
                    .unwrap()
                    .error
                    .contains("already exists")
            );
            enter_creation_name(&mut app, &ctx, size, "../Elsewhere");
            press(&mut app, &ctx, size, egui::Key::Enter);
            assert!(app.action.is_none());
            assert!(
                app.tab
                    .dialog
                    .as_ref()
                    .unwrap()
                    .error
                    .contains("without a path")
            );
            press(&mut app, &ctx, size, egui::Key::Escape);
            assert!(app.tab.dialog.is_none());
            assert_eq!(
                app.tab.editor.as_ref().unwrap().document.text,
                "Keep the current note"
            );
        }
    }
}
