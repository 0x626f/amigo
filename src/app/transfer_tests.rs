use super::*;
use crate::app::tests::{harness, render_sized, text_rect, text_rect_optional};

fn settle(app: &mut Amigo, ctx: &egui::Context, size: egui::Vec2) -> egui::FullOutput {
    let mut output = render_sized(app, ctx, vec![], size);
    for _ in 0..10 {
        output = render_sized(app, ctx, vec![], size);
    }
    output
}
fn click_text(app: &mut Amigo, ctx: &egui::Context, size: egui::Vec2, label: &str) {
    let output = settle(app, ctx, size);
    let rect = text_rect(&output, label);
    assert!(
        rect.top() > 0.0 && rect.bottom() < size.y && rect.right() < size.x,
        "{label} is outside the dialog: {rect:?}"
    );
    for pressed in [true, false] {
        let _ = render_sized(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(rect.center()),
                egui::Event::PointerButton {
                    pos: rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            size,
        );
    }
}
fn wait(app: &mut Amigo, ctx: &egui::Context, size: egui::Vec2) {
    let started = Instant::now();
    while app.transfers.busy() {
        assert!(started.elapsed() < Duration::from_secs(15));
        let _ = render_sized(app, ctx, vec![], size);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn key(app: &mut Amigo, ctx: &egui::Context, size: egui::Vec2, key: egui::Key) {
    for pressed in [true, false] {
        let _ = render_sized(
            app,
            ctx,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            size,
        );
    }
}
fn add_task(app: &mut Amigo, title: &str) {
    let mut task = Task::new(app.workspace.tasks.len() as u32 + 1, "todo");
    task.title = title.into();
    app.workspace.tasks.push(task);
}

#[test]
fn board_export_saves_latest_edits_and_does_not_reload_the_workspace() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        add_task(&mut app, "Newest unsaved task");
        let board = app.library.settings.board.clone();
        let path = app.data_path.with_extension("amigo");
        app.handle_tasks_action(TasksAction::ExportBoard);
        app.transfers.dialog.as_mut().unwrap().path = path.to_string_lossy().into_owned();
        click_text(&mut app, &ctx, size, "Export Board");
        wait(&mut app, &ctx, size);
        assert!(
            app.transfers.dialog.is_none(),
            "{}",
            app.transfers
                .dialog
                .as_ref()
                .map(|d| d.error.as_str())
                .unwrap_or("")
        );
        assert_eq!(app.workspace.tasks[0].title, "Newest unsaved task");
        let archive = Archive::read(&path).unwrap();
        assert_eq!(archive.scope, Scope::Board(board));
        assert!(archive.review(&app.library).unwrap().contains("1 tasks"));
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
}

#[test]
fn setup_restore_requires_review_and_explicit_confirmation_and_cannot_autosave_old_data() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        add_task(&mut app, "Saved in backup");
        assert!(app.save_local());
        let path = app.data_path.with_extension("amigo");
        Archive::capture(&app.library, Scope::Setup)
            .unwrap()
            .write(&path, &app.data_path)
            .unwrap();
        add_task(&mut app, "Added after backup");
        assert!(app.save_local());
        app.open_data_dialog(Scope::Setup, DataAction::Import);
        app.transfers.dialog.as_mut().unwrap().path = path.to_string_lossy().into_owned();
        click_text(&mut app, &ctx, size, "Review Import");
        wait(&mut app, &ctx, size);
        assert!(app.transfers.dialog.as_ref().unwrap().reviewed.is_some());
        assert_eq!(app.workspace.tasks.len(), 2);
        let _ = settle(&mut app, &ctx, size);
        key(&mut app, &ctx, size, egui::Key::Enter);
        assert!(!app.transfers.busy());
        assert_eq!(app.workspace.tasks.len(), 2);
        key(&mut app, &ctx, size, egui::Key::Escape);
        assert!(app.transfers.dialog.is_none());
        assert_eq!(Amigo::load(app.data_path.clone()).workspace.tasks.len(), 2);
        app.open_data_dialog(Scope::Setup, DataAction::Import);
        let archive = Archive::read(&path).unwrap();
        let summary = archive.review(&app.library).unwrap();
        app.transfer_result(Ok(TransferResult::SetupReviewed(
            archive,
            summary,
            path.clone(),
        )));
        let reviewed = settle(&mut app, &ctx, size);
        assert!(text_rect_optional(&reviewed, "Browse…").is_none());
        click_text(&mut app, &ctx, size, "Replace All Data");
        wait(&mut app, &ctx, size);
        assert!(app.transfers.dialog.is_none());
        assert_eq!(app.workspace.tasks.len(), 1);
        assert_eq!(app.workspace.tasks[0].title, "Saved in backup");
        assert_eq!(app.workspace, app.last_saved);
        assert!(app.save_local());
        assert_eq!(Amigo::load(app.data_path.clone()).workspace.tasks.len(), 1);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
}

#[test]
fn board_and_vault_delete_cancel_or_switch_after_confirmation() {
    let size = egui::vec2(980.0, 640.0);
    let (mut app, ctx) = harness();
    add_task(&mut app, "Delete with board");
    assert!(app.save_local());
    let board = app.library.settings.board.clone();
    let remaining = app
        .library
        .create_board("Remaining", &Workspace::default())
        .unwrap();
    app.handle_tasks_action(TasksAction::DeleteBoard);
    let _ = settle(&mut app, &ctx, size);
    key(&mut app, &ctx, size, egui::Key::Enter);
    assert!(!app.transfers.busy());
    assert!(app.library.board_path(&board).unwrap().exists());
    click_text(&mut app, &ctx, size, "Cancel");
    assert!(app.transfers.dialog.is_none());
    assert_eq!(app.workspace.tasks.len(), 1);
    app.handle_tasks_action(TasksAction::DeleteBoard);
    click_text(&mut app, &ctx, size, "Delete Board");
    wait(&mut app, &ctx, size);
    assert_eq!(app.library.settings.board, remaining.id);
    assert!(!app.library.board_path(&board).unwrap().exists());
    assert!(app.workspace.tasks.is_empty());
    let vault = app.library.settings.vault.clone();
    app.handle_vault_action(VaultAction::DeleteVault);
    click_text(&mut app, &ctx, size, "Delete Vault");
    wait(&mut app, &ctx, size);
    assert_ne!(app.library.settings.vault, vault);
    assert!(!app.library.vault_path(&vault).unwrap().exists());
    assert!(app.save_local());
    assert!(!app.library.vault_path(&vault).unwrap().exists());
    std::fs::remove_dir_all(app.data_path).unwrap();
}

#[test]
fn invalid_import_and_wrong_file_type_show_errors_without_changing_data() {
    let size = egui::vec2(980.0, 640.0);
    let (mut app, ctx) = harness();
    add_task(&mut app, "Keep this task");
    assert!(app.save_local());
    let original = app.workspace.clone();
    let path = app.data_path.with_extension("amigo");
    std::fs::write(&path, "not an Amigo file").unwrap();
    app.handle_tasks_action(TasksAction::ImportBoard);
    app.transfers.dialog.as_mut().unwrap().path = path.to_string_lossy().into_owned();
    click_text(&mut app, &ctx, size, "Review Import");
    wait(&mut app, &ctx, size);
    assert!(
        app.transfers
            .dialog
            .as_ref()
            .unwrap()
            .error
            .contains("Invalid Amigo file")
    );
    assert_eq!(app.workspace, original);
    Archive::capture(&app.library, Scope::Setup)
        .unwrap()
        .write(&path, &app.data_path)
        .unwrap();
    click_text(&mut app, &ctx, size, "Review Import");
    wait(&mut app, &ctx, size);
    assert!(
        app.transfers
            .dialog
            .as_ref()
            .unwrap()
            .error
            .contains("Choose an Amigo Board file")
    );
    assert_eq!(app.workspace, original);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir_all(app.data_path).unwrap();
}

#[test]
fn toolbar_actions_route_to_the_correct_transfer_dialog_and_theme_stays_at_bottom() {
    for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
        for target in ["Board", "Vault", "Calendar", "Amigo Setup"] {
            let (mut app, ctx) = harness();
            app.section = match target {
                "Vault" => Section::Finance,
                "Calendar" => Section::Calendar,
                _ => Section::Tasks,
            };
            if target == "Amigo Setup" {
                app.open_settings(SettingsPage::Tasks);
            }
            settle(&mut app, &ctx, size);
            let prefix = match target {
                "Board" => "board",
                "Vault" => "vault",
                "Calendar" => "calendar",
                _ => "app-settings",
            };
            let names = if target == "Amigo Setup" {
                vec!["import", "export", "close"]
            } else {
                vec!["import", "export", "delete", "rename"]
            };
            let mut rects: Vec<_> = names
                .iter()
                .map(|name| {
                    ctx.read_response(egui::Id::new(format!("{prefix}-{name}")))
                        .unwrap()
                        .rect
                })
                .collect();
            if target == "Amigo Setup" {
                assert!(rects[0].right() < rects[1].left());
                assert!(rects[1].right() < rects[2].left());
            }
            rects.sort_by(|a, b| a.left().total_cmp(&b.left()));
            for rect in &rects {
                assert!(
                    rect.left() >= 0.
                        && rect.right() < size.x
                        && rect.top() > 0.
                        && rect.bottom() < size.y,
                    "{target} {rect:?}"
                );
            }
            for pair in rects.windows(2) {
                assert!(pair[0].right() < pair[1].left());
                assert!((pair[0].center().y - pair[1].center().y).abs() < 1.);
            }
            if target == "Amigo Setup" {
                let theme = ctx
                    .read_response(egui::Id::new("app-theme-switcher"))
                    .unwrap()
                    .rect;
                assert!(theme.top() > rects[0].bottom() + 100.);
                assert!(theme.bottom() <= size.y);
                std::fs::remove_dir_all(app.data_path).unwrap();
                continue;
            }
            let pos = ctx
                .read_response(egui::Id::new(format!("{prefix}-export")))
                .unwrap()
                .rect
                .center();
            for pressed in [true, false] {
                render_sized(
                    &mut app,
                    &ctx,
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
            let dialog = app.transfers.dialog.as_ref().unwrap();
            assert_eq!(dialog.scope.label(), target);
            assert_eq!(dialog.action, DataAction::Export);
            std::fs::remove_dir_all(app.data_path).unwrap();
        }
    }
}

#[test]
fn locked_vault_keeps_data_actions_and_rename_accessible_at_both_sizes() {
    for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
        let (mut app, ctx) = harness();
        app.section = Section::Finance;
        app.vault_unlocked = false;
        settle(&mut app, &ctx, size);
        for key in ["import", "export", "delete", "rename", "security"] {
            let rect = ctx
                .read_response(egui::Id::new(format!("vault-{key}")))
                .unwrap()
                .rect;
            assert!(rect.left() > 200. && rect.right() < size.x && rect.top() >= 0.);
        }
        let rect = ctx
            .read_response(egui::Id::new("vault-rename"))
            .unwrap()
            .rect;
        for pressed in [true, false] {
            render_sized(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                size,
            );
        }
        let output = settle(&mut app, &ctx, size);
        assert!(text_rect_optional(&output, "Rename Vault").is_some());
        assert!(!app.vault_unlocked);
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
}
