use super::*;
use crate::app::tests::{harness, render_sized, text_rect};
fn settle(app: &mut Amigo, ctx: &egui::Context, size: egui::Vec2) -> egui::FullOutput {
    let mut output = render_sized(app, ctx, vec![], size);
    for _ in 0..4 {
        output = render_sized(app, ctx, vec![], size);
    }
    output
}
fn click(app: &mut Amigo, ctx: &egui::Context, size: egui::Vec2, pos: egui::Pos2) {
    for pressed in [true, false] {
        render_sized(
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
fn single_settings_entry_navigates_all_pages_and_fits_both_themes_and_sizes() {
    for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
        for dark in [false, true] {
            let (mut app, ctx) = harness();
            app.workspace.dark_mode = dark;
            settle(&mut app, &ctx, size);
            let open = ctx
                .read_response(egui::Id::new("app-settings-open"))
                .unwrap()
                .rect;
            let theme = ctx
                .read_response(egui::Id::new("app-theme-switcher"))
                .unwrap()
                .rect;
            assert!(theme.top() > size.y - 90.);
            assert!(open.bottom() < 180.);
            click(&mut app, &ctx, size, open.center());
            settle(&mut app, &ctx, size);
            assert!(app.app_settings.is_some());
            let board = app.library.settings.board.clone();
            let vault = app.library.settings.vault.clone();
            for page in [
                SettingsPage::Board(board),
                SettingsPage::Calendar,
                SettingsPage::Finance,
                SettingsPage::Vault(vault.clone()),
                SettingsPage::Expenses(vault.clone()),
                SettingsPage::Savings(vault),
                SettingsPage::Journal,
                SettingsPage::Updates,
                SettingsPage::General,
            ] {
                settle(&mut app, &ctx, size);
                let nav = ctx
                    .read_response(egui::Id::new(("settings-page", &page)))
                    .unwrap()
                    .rect;
                assert!(nav.bottom() < size.y && nav.left() > 0.);
                click(&mut app, &ctx, size, nav.center());
                settle(&mut app, &ctx, size);
                assert_eq!(app.app_settings.as_ref().unwrap().page, page);
                let rect = ctx
                    .memory(|m| m.area_rect(egui::Id::new("app-settings")))
                    .unwrap();
                assert!(
                    rect.top() >= 0.
                        && rect.bottom() <= size.y
                        && rect.left() >= 0.
                        && rect.right() <= size.x,
                    "{page:?}: {rect:?} within {size:?}"
                );
            }
            app.close_settings();
            std::fs::remove_dir_all(app.data_path).unwrap();
        }
    }
}
#[test]
fn journal_preferences_save_through_enter_and_survive_reload() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(980., 640.);
    app.open_settings(SettingsPage::Journal);
    let output = settle(&mut app, &ctx, size);
    click(&mut app, &ctx, size, text_rect(&output, "Preview").center());
    for pressed in [true, false] {
        render_sized(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            size,
        );
    }
    assert_eq!(
        app.library.settings.journal.mode,
        amigo_core::journal::Mode::Preview
    );
    app.close_settings();
    let reopened = Amigo::load(app.data_path.clone());
    assert_eq!(
        reopened.library.settings.journal.mode,
        amigo_core::journal::Mode::Preview
    );
    std::fs::remove_dir_all(app.data_path).unwrap();
}
#[test]
fn navigating_settings_retains_board_drafts_and_cancel_preserves_saved_data() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(980., 640.);
    let board = app.library.settings.board.clone();
    let status = app.workspace.task_statuses[0].clone();
    app.open_settings(SettingsPage::Board(board.clone()));
    settle(&mut app, &ctx, size);
    let field = ctx
        .read_response(egui::Id::new(("status-name", &status.id)))
        .unwrap()
        .rect;
    click(&mut app, &ctx, size, field.center());
    for pressed in [true, false] {
        render_sized(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
            size,
        );
    }
    render_sized(
        &mut app,
        &ctx,
        vec![egui::Event::Text("Unconfirmed draft".into())],
        size,
    );
    app.open_settings(SettingsPage::Calendar);
    settle(&mut app, &ctx, size);
    app.open_settings(SettingsPage::Board(board.clone()));
    let output = settle(&mut app, &ctx, size);
    assert!(crate::app::tests::text_rect_optional(&output, "Unconfirmed draft").is_some());
    assert_eq!(app.workspace.task_statuses[0], status);
    click(&mut app, &ctx, size, text_rect(&output, "Cancel").center());
    assert!(app.app_settings.is_none());
    app.open_settings(SettingsPage::Board(board));
    settle(&mut app, &ctx, size);
    assert_eq!(app.workspace.task_statuses[0], status);
    app.close_settings();
    std::fs::remove_dir_all(app.data_path).unwrap();
}
#[test]
fn stashed_vault_save_does_not_revert_the_shared_finance_refresh_interval() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(980., 640.);
    let vault = app.library.settings.vault.clone();
    app.open_settings(SettingsPage::Vault(vault.clone()));
    settle(&mut app, &ctx, size);
    app.open_settings(SettingsPage::Finance);
    app.app_settings.as_mut().unwrap().exchange_interval = 317;
    let output = settle(&mut app, &ctx, size);
    click(
        &mut app,
        &ctx,
        size,
        text_rect(&output, "Save Settings").center(),
    );
    assert_eq!(
        app.library.settings.sync.exchange_rate_interval_seconds,
        317
    );
    app.open_settings(SettingsPage::Vault(vault));
    let output = settle(&mut app, &ctx, size);
    click(
        &mut app,
        &ctx,
        size,
        text_rect(&output, "Save Settings").center(),
    );
    assert_eq!(
        app.library.settings.sync.exchange_rate_interval_seconds,
        317
    );
    assert_eq!(app.finance.exchange_interval_seconds, 317);
    app.close_settings();
    std::fs::remove_dir_all(app.data_path).unwrap();
}
