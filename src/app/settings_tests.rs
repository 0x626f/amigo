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
fn scroll_settings_page(
    app: &mut Amigo,
    ctx: &egui::Context,
    size: egui::Vec2,
    page: &SettingsPage,
) -> egui::Rect {
    for _ in 0..20 {
        for _ in 0..20 {
            render_sized(app, ctx, vec![], size);
        }
        let row = ctx
            .read_response(egui::Id::new(("settings-page", page)))
            .unwrap()
            .rect;
        let viewport = ctx
            .read_response(egui::Id::new("settings-navigation"))
            .unwrap()
            .rect;
        if row.top() >= viewport.top() && row.bottom() <= viewport.bottom() {
            return row;
        }
        render_sized(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(viewport.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    delta: egui::vec2(0., viewport.center().y - row.center().y),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            size,
        );
        settle(app, ctx, size);
    }
    panic!("Settings page never became visible: {page:?}");
}
#[test]
fn single_settings_entry_navigates_all_pages_and_fits_both_themes_and_sizes() {
    for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
        for dark in [false, true] {
            let (mut app, ctx) = harness();
            app.workspace.dark_mode = dark;
            app.section = Section::Journal;
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
                SettingsPage::FinanceReference(FinanceReference::Currencies),
                SettingsPage::FinanceReference(FinanceReference::Chains),
                SettingsPage::Reference(vault.clone(), FinanceReference::Clients),
                SettingsPage::Reference(vault.clone(), FinanceReference::Accounts),
                SettingsPage::FinanceReference(FinanceReference::Countries),
                SettingsPage::Expenses(vault.clone()),
                SettingsPage::Savings(vault),
                SettingsPage::Journal,
                SettingsPage::Updates,
                SettingsPage::Tasks,
            ] {
                settle(&mut app, &ctx, size);
                let nav = scroll_settings_page(&mut app, &ctx, size, &page);
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
    settle(&mut app, &ctx, size);
    let preview = ctx
        .read_response(egui::Id::new((
            "journal-default-mode",
            amigo_core::journal::Mode::Preview,
        )))
        .unwrap()
        .rect;
    click(&mut app, &ctx, size, preview.center());
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
fn preview_editing_default_is_a_saved_setting_and_cancel_discards_changes() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(1440., 900.);
    assert!(!app.library.settings.journal.preview_editable);
    for save in [false, true] {
        app.open_settings(SettingsPage::Journal);
        settle(&mut app, &ctx, size);
        let setting = ctx
            .read_response(egui::Id::new("journal-preview-editable-default"))
            .unwrap()
            .rect;
        click(&mut app, &ctx, size, setting.center());
        assert!(app.app_settings.as_ref().unwrap().journal.preview_editable);
        let output = settle(&mut app, &ctx, size);
        click(
            &mut app,
            &ctx,
            size,
            text_rect(&output, if save { "Save Settings" } else { "Cancel" }).center(),
        );
        assert_eq!(app.library.settings.journal.preview_editable, save);
    }
    let reopened = Amigo::load(app.data_path.clone());
    assert!(reopened.library.settings.journal.preview_editable);
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

#[test]
fn settings_rows_and_category_fields_share_edges_in_both_themes_and_sizes() {
    for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
        for dark in [false, true] {
            let (mut app, ctx) = harness();
            app.workspace.dark_mode = dark;
            app.workspace.finance.categories = [
                "Rental",
                "Subscription",
                "Insurance",
                "Support",
                "Health",
                "Transport",
                "Long category with Unicode 🌍",
                "X",
            ]
            .into_iter()
            .enumerate()
            .map(|(i, name)| StoredValue {
                id: format!("geometry-{i}"),
                name: name.into(),
            })
            .collect();
            let ids: Vec<_> = app
                .workspace
                .finance
                .categories
                .iter()
                .map(|c| c.id.clone())
                .collect();
            let vault = app.library.settings.vault.clone();
            app.open_settings(SettingsPage::Expenses(vault));
            let output = settle(&mut app, &ctx, size);
            assert!(crate::app::tests::text_rect_optional(&output, "General").is_none());
            let rows = app.settings_rows(&app.app_settings.as_ref().unwrap().expanded_vaults);
            let mut expected: Option<egui::Rect> = None;
            for (page, _, _, _) in rows {
                let row = ctx
                    .read_response(egui::Id::new(("settings-page", &page)))
                    .unwrap()
                    .rect;
                if let Some(expected) = expected {
                    assert!((row.left() - expected.left()).abs() < 0.1);
                    assert!((row.right() - expected.right()).abs() < 0.1);
                }
                expected = Some(row);
            }
            let mut expected: Option<egui::Rect> = None;
            for id in ids {
                let field = ctx
                    .read_response(egui::Id::new(("finance-category-name", id)))
                    .unwrap()
                    .rect;
                assert!(field.right() < size.x - 50.);
                if let Some(expected) = expected {
                    assert!((field.left() - expected.left()).abs() < 0.1);
                    assert!((field.right() - expected.right()).abs() < 0.1);
                }
                expected = Some(field);
            }
            app.close_settings();
            settle(&mut app, &ctx, size);
            let logo = ctx
                .read_response(egui::Id::new("app-brand-logo"))
                .unwrap()
                .rect;
            let title = ctx
                .read_response(egui::Id::new("app-brand-title"))
                .unwrap()
                .rect;
            assert!((logo.center().y - title.center().y).abs() < 1.);
            assert!(logo.right() < title.left());
            let settings = ctx
                .read_response(egui::Id::new("app-settings-open"))
                .unwrap()
                .rect;
            assert!(title.right() < settings.left());
            assert!((settings.center().y - logo.center().y).abs() < 1.);
            assert!((settings.right() - 206.).abs() < 1.);
            std::fs::remove_dir_all(app.data_path).unwrap();
        }
    }
}

fn settings_key(
    app: &mut Amigo,
    ctx: &egui::Context,
    size: egui::Vec2,
    key: egui::Key,
    modifiers: egui::Modifiers,
) {
    for pressed in [true, false] {
        render_sized(
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

#[test]
fn vault_reference_pages_keep_drafts_separate_and_save_with_the_vault() {
    for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
        let (mut app, ctx) = harness();
        app.section = Section::Finance;
        app.finance
            .edit_record(finance_ui::Record::Income(Income::new("EUR")), true);
        app.finance.search = "Keep transaction search".into();
        let main_draft = app.finance.drafts.keys().next().unwrap().clone();
        let output = settle(&mut app, &ctx, size);
        assert!(crate::app::tests::text_rect_optional(&output, "Stored Values").is_none());
        let vault = app.library.settings.vault.clone();
        app.open_settings(SettingsPage::Finance);
        settle(&mut app, &ctx, size);
        let manage = ctx
            .read_response(egui::Id::new(("vault-config-open", &vault)))
            .unwrap()
            .rect;
        click(&mut app, &ctx, size, manage.center());
        settle(&mut app, &ctx, size);
        assert_eq!(
            app.app_settings.as_ref().unwrap().page,
            SettingsPage::Vault(vault.clone())
        );
        let account_page = SettingsPage::Reference(vault.clone(), FinanceReference::Accounts);
        let account_nav = scroll_settings_page(&mut app, &ctx, size, &account_page);
        click(&mut app, &ctx, size, account_nav.center());
        let output = settle(&mut app, &ctx, size);
        click(
            &mut app,
            &ctx,
            size,
            text_rect(&output, "+ Account").center(),
        );
        settle(&mut app, &ctx, size);
        let key = app.app_settings.as_ref().unwrap().references[&account_page]
            .drafts
            .keys()
            .next()
            .unwrap()
            .clone();
        render_sized(
            &mut app,
            &ctx,
            vec![egui::Event::Text("Settings Account".into())],
            size,
        );
        settings_key(
            &mut app,
            &ctx,
            size,
            egui::Key::Enter,
            egui::Modifiers::NONE,
        );
        assert!(
            app.workspace
                .finance
                .accounts
                .iter()
                .any(|a| a.id == key && a.label == "Settings Account")
        );
        assert!(
            app.app_settings.is_some(),
            "row confirmation keeps settings open"
        );
        assert!(
            app.finance.drafts.contains_key(&main_draft),
            "settings cannot save a background transaction"
        );
        assert_eq!(app.finance.view, FinanceView::Income);
        assert_eq!(app.finance.search, "Keep transaction search");
        let field = ctx
            .read_response(egui::Id::new(("finance-cell", &key, "label")))
            .unwrap()
            .rect;
        click(&mut app, &ctx, size, field.center());
        settings_key(&mut app, &ctx, size, egui::Key::A, egui::Modifiers::COMMAND);
        render_sized(
            &mut app,
            &ctx,
            vec![egui::Event::Text("Unconfirmed account".into())],
            size,
        );
        app.open_settings(SettingsPage::Journal);
        settle(&mut app, &ctx, size);
        app.open_settings(account_page.clone());
        let output = settle(&mut app, &ctx, size);
        assert!(crate::app::tests::text_rect_optional(&output, "Unconfirmed account").is_some());
        click(&mut app, &ctx, size, text_rect(&output, "Done").center());
        assert!(app.app_settings.is_none());
        assert!(app.save_local());
        let reopened = Amigo::load(app.data_path.clone());
        assert!(
            reopened
                .workspace
                .finance
                .accounts
                .iter()
                .any(|a| a.id == key && a.label == "Settings Account")
        );
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
}

#[test]
fn currency_dialog_in_shared_finance_configuration_owns_escape_before_the_settings_window() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(980., 640.);
    app.open_settings(SettingsPage::FinanceReference(FinanceReference::Currencies));
    let output = settle(&mut app, &ctx, size);
    click(
        &mut app,
        &ctx,
        size,
        text_rect(&output, "+ Currency").center(),
    );
    let output = settle(&mut app, &ctx, size);
    assert!(crate::app::tests::text_rect_optional(&output, "Save Currency").is_some());
    settings_key(
        &mut app,
        &ctx,
        size,
        egui::Key::Escape,
        egui::Modifiers::NONE,
    );
    let output = settle(&mut app, &ctx, size);
    assert!(crate::app::tests::text_rect_optional(&output, "Save Currency").is_none());
    assert_eq!(
        app.app_settings.as_ref().unwrap().page,
        SettingsPage::FinanceReference(FinanceReference::Currencies)
    );
    settings_key(
        &mut app,
        &ctx,
        size,
        egui::Key::Escape,
        egui::Modifiers::NONE,
    );
    assert!(app.app_settings.is_none());
    std::fs::remove_dir_all(app.data_path).unwrap();
}

#[test]
fn vault_configuration_has_named_pages_and_one_expenses_and_savings_editor() {
    let (mut app, ctx) = harness();
    let vault = app.library.settings.vault.clone();
    app.open_settings(SettingsPage::Vault(vault.clone()));
    let rows = app.settings_rows(&app.app_settings.as_ref().unwrap().expanded_vaults);
    for label in [
        "Currencies",
        "Chains",
        "Clients",
        "Accounts",
        "Countries",
        "Expenses",
        "Savings",
    ] {
        assert_eq!(
            rows.iter()
                .filter(|(_, name, _, depth)| name == label
                    && *depth
                        == if ["Currencies", "Chains", "Countries"].contains(&label) {
                            1
                        } else {
                            2
                        })
                .count(),
            1,
            "{label}"
        );
    }
    assert!(!rows.iter().any(|(_, name, _, _)| {
        [
            "Stored Values",
            "Expense Categories",
            "Expense Labels",
            "Savings Categories",
        ]
        .contains(&name.as_str())
    }));
    let size = egui::vec2(980., 640.);
    for page in FinanceReference::ALL {
        app.open_settings(SettingsPage::Reference(vault.clone(), page));
        settle(&mut app, &ctx, size);
        assert!(
            ctx.read_response(egui::Id::new("stored-values-type-selector"))
                .is_none()
        );
        assert_eq!(
            app.app_settings.as_ref().unwrap().page,
            if page.is_global() {
                SettingsPage::FinanceReference(page)
            } else {
                SettingsPage::Reference(vault.clone(), page)
            }
        );
    }
    app.open_settings(SettingsPage::Expenses(vault));
    let output = settle(&mut app, &ctx, size);
    assert!(crate::app::tests::text_rect_optional(&output, "Categories").is_some());
    assert!(crate::app::tests::text_rect_optional(&output, "Labels").is_none());
    app.close_settings();
    std::fs::remove_dir_all(app.data_path).unwrap();
}

#[test]
fn shared_finance_references_save_across_vaults_and_remain_available_while_locked() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(980., 640.);
    let first = app.current_vault().unwrap().clone();
    let second = app
        .library
        .create_vault("Second", &FinancePlan::empty(), None)
        .unwrap();
    let currency_page = SettingsPage::FinanceReference(FinanceReference::Currencies);
    app.open_settings(currency_page.clone());
    let output = settle(&mut app, &ctx, size);
    click(
        &mut app,
        &ctx,
        size,
        text_rect(&output, "+ Currency").center(),
    );
    settle(&mut app, &ctx, size);
    render_sized(&mut app, &ctx, vec![egui::Event::Text("XTS".into())], size);
    settings_key(
        &mut app,
        &ctx,
        size,
        egui::Key::Enter,
        egui::Modifiers::NONE,
    );
    let currency = app
        .library
        .finance_settings()
        .unwrap()
        .currencies
        .into_iter()
        .find(|r| r.name == "XTS")
        .unwrap();
    assert!(
        app.workspace
            .finance
            .currencies
            .iter()
            .any(|r| r.id == currency.id)
    );
    app.close_settings();
    app.switch_vault(second.clone()).unwrap();
    assert!(
        app.workspace
            .finance
            .currencies
            .iter()
            .any(|r| r.id == currency.id)
    );
    let mut expense = Expense::new("XTS");
    expense.title = "Other vault expense".into();
    expense.currency_id = currency.id.clone();
    app.workspace.finance.expenses.push(expense);
    assert!(app.save_local());
    app.switch_vault(first).unwrap();
    app.open_settings(currency_page.clone());
    settle(&mut app, &ctx, size);
    app.app_settings
        .as_mut()
        .unwrap()
        .references
        .get_mut(&currency_page)
        .unwrap()
        .edit_record(finance_ui::Record::Currency(currency.clone()), false);
    let output = settle(&mut app, &ctx, size);
    click(&mut app, &ctx, size, text_rect(&output, "Delete…").center());
    settle(&mut app, &ctx, size);
    settings_key(
        &mut app,
        &ctx,
        size,
        egui::Key::Enter,
        egui::Modifiers::NONE,
    );
    let output = settle(&mut app, &ctx, size);
    assert!(
        crate::app::tests::text_rect_optional(
            &output,
            "This value is in use by a vault. Reassign its records before removing it."
        )
        .is_some()
    );
    assert!(
        app.library
            .finance_settings()
            .unwrap()
            .currencies
            .iter()
            .any(|r| r.id == currency.id)
    );
    app.close_settings();
    let key = VaultKey::new("shared-settings-password").unwrap();
    let encrypted = app
        .library
        .encrypt_vault(
            &second.entry.id,
            &app.library.load_plain_vault(&second).unwrap(),
            &key,
        )
        .unwrap();
    app.switch_vault(encrypted).unwrap();
    app.library_dialog = None;
    assert!(!app.vault_unlocked);
    let chain_page = SettingsPage::FinanceReference(FinanceReference::Chains);
    app.open_settings(chain_page.clone());
    let output = settle(&mut app, &ctx, size);
    assert!(
        app.library_dialog.is_none(),
        "Global settings must not request a vault password"
    );
    click(&mut app, &ctx, size, text_rect(&output, "+ Chain").center());
    settle(&mut app, &ctx, size);
    render_sized(
        &mut app,
        &ctx,
        vec![egui::Event::Text("Shared network".into())],
        size,
    );
    settings_key(
        &mut app,
        &ctx,
        size,
        egui::Key::Enter,
        egui::Modifiers::NONE,
    );
    assert!(
        app.library
            .finance_settings()
            .unwrap()
            .chains
            .iter()
            .any(|r| r.name == "Shared network")
    );
    assert!(!app.vault_unlocked);
    app.close_settings();
    let reopened = Amigo::load(app.data_path.clone());
    assert!(
        reopened
            .library
            .finance_settings()
            .unwrap()
            .currencies
            .iter()
            .any(|r| r.id == currency.id)
    );
    assert!(
        reopened
            .library
            .finance_settings()
            .unwrap()
            .chains
            .iter()
            .any(|r| r.name == "Shared network")
    );
    std::fs::remove_dir_all(app.data_path).unwrap();
}

#[test]
fn global_finance_pages_preserve_vault_sidebar_expansion() {
    for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
        for dark in [false, true] {
            let (mut app, ctx) = harness();
            app.workspace.dark_mode = dark;
            let vault = app.library.settings.vault.clone();
            let personal = SettingsPage::Vault(vault.clone());
            let children = [
                SettingsPage::Reference(vault.clone(), FinanceReference::Clients),
                SettingsPage::Reference(vault.clone(), FinanceReference::Accounts),
                SettingsPage::Expenses(vault.clone()),
                SettingsPage::Savings(vault.clone()),
            ];
            let global = SettingsPage::FinanceReference(FinanceReference::Currencies);
            app.open_settings(SettingsPage::Finance);
            let before = settle(&mut app, &ctx, size);
            assert!(crate::app::tests::text_rect_optional(&before, "Clients").is_none());
            let row = scroll_settings_page(&mut app, &ctx, size, &global);
            click(&mut app, &ctx, size, row.center());
            let output = settle(&mut app, &ctx, size);
            assert_eq!(app.app_settings.as_ref().unwrap().page, global);
            assert!(
                crate::app::tests::text_rect_optional(&output, "Clients").is_none(),
                "Opening global Currencies must not expand Personal"
            );
            let row = scroll_settings_page(&mut app, &ctx, size, &personal);
            click(&mut app, &ctx, size, row.center());
            settle(&mut app, &ctx, size);
            for child in &children {
                assert!(
                    ctx.read_response(egui::Id::new(("settings-page", child)))
                        .is_some()
                );
            }
            for page in [
                global,
                SettingsPage::FinanceReference(FinanceReference::Chains),
                SettingsPage::FinanceReference(FinanceReference::Countries),
                SettingsPage::Finance,
                SettingsPage::Journal,
            ] {
                let row = scroll_settings_page(&mut app, &ctx, size, &page);
                click(&mut app, &ctx, size, row.center());
                settle(&mut app, &ctx, size);
                assert_eq!(app.app_settings.as_ref().unwrap().page, page);
                for child in &children {
                    assert!(
                        ctx.read_response(egui::Id::new(("settings-page", child)))
                            .is_some(),
                        "Navigating to {page:?} must not collapse Personal"
                    );
                }
                assert_eq!(app.library.settings.vault, vault);
            }
            app.close_settings();
            std::fs::remove_dir_all(app.data_path).unwrap();
        }
    }
}
