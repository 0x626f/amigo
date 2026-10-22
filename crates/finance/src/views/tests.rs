use super::*;
use amigo_core::library::Entry;
struct Harness {
    tab: FinanceTab,
    plan: FinancePlan,
    vault: Vault,
}
fn harness() -> (Harness, egui::Context) {
    let ctx = egui::Context::default();
    design::install(&ctx);
    let mut plan = FinancePlan::empty();
    plan.meta.name = "Test".into();
    plan.meta.reporting_currency = "EUR".into();
    for (code, source) in [("EUR", "Euro Job"), ("USD", "Dollar Job")] {
        let mut row = Income::new(code);
        row.source = source.into();
        row.gross_amount = Amount::Number(100.0);
        plan.income.push(row);
    }
    plan.normalize();
    let mut tab = FinanceTab::new(&plan);
    tab.view = FinanceView::Income;
    (
        Harness {
            tab,
            plan,
            vault: Vault {
                entry: Entry {
                    id: "test".into(),
                    name: "Test".into(),
                },
                encrypted: false,
            },
        },
        ctx,
    )
}

#[test]
fn wallet_currency_picker_precedes_settings_and_persists_selection() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        app.tab.view = FinanceView::Wallet;
        for _ in 0..4 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let picker = ctx
            .read_response(egui::Id::new("wallet-reporting-currency"))
            .unwrap()
            .rect;
        let settings = ctx
            .read_response(egui::Id::new("vault-settings"))
            .unwrap()
            .rect;
        assert!(picker.right() < settings.left());
        assert!(picker.left() >= 252.0 && settings.right() <= size.x - 28.0);
        for currency in ["USD", "EUR"] {
            click(&mut app, &ctx, size, picker.center());
            click_text(&mut app, &ctx, size, currency);
            assert_eq!(app.plan.meta.reporting_currency, currency);
            assert_eq!(
                app.plan
                    .currencies
                    .iter()
                    .find(|item| item.id == app.plan.meta.reporting_currency_id)
                    .unwrap()
                    .name,
                currency
            );
            let saved = plan_from_value(serde_json::to_value(&app.plan).unwrap()).unwrap();
            assert_eq!(saved.meta.reporting_currency, currency);
            assert_eq!(
                saved.meta.reporting_currency_id,
                app.plan.meta.reporting_currency_id
            );
        }
        click(&mut app, &ctx, size, settings.center());
        for _ in 0..4 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Test Vault Settings").is_some());
        for removed in ["Finance Name", "Reporting Currency", "Rename Vault"] {
            assert!(text_rect(&output, removed).is_none());
        }
    }
}

fn frame(
    app: &mut Harness,
    ctx: &egui::Context,
    size: egui::Vec2,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| {
            egui::Panel::left("navigation")
                .exact_size(224.0)
                .show(ui, |_| {});
            egui::CentralPanel::default()
                .frame(egui::Frame::new().inner_margin(28))
                .show(ui, |ui| app.tab.show(&mut app.plan, Some(&app.vault), ui));
            let _ = app.tab.dialogs(&mut app.plan, Some(&app.vault), ui.ctx());
        },
    );
    output.textures_delta.clear();
    output
}
fn text_rect(output: &egui::FullOutput, text: &str) -> Option<egui::Rect> {
    fn find(shape: &egui::Shape, text: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == text => {
                Some(t.galley.rect.translate(t.pos.to_vec2()))
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, text)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .rev()
        .find_map(|s| find(&s.shape, text))
}
fn click(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2, pos: egui::Pos2) {
    for pressed in [true, false] {
        let _ = frame(
            app,
            ctx,
            size,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

fn reveal_table_cell(
    app: &mut Harness,
    ctx: &egui::Context,
    size: egui::Vec2,
    key: &str,
    column: usize,
) {
    let rect = ctx
        .read_response(egui::Id::new(("finance-table-cell", key, column)))
        .unwrap()
        .rect;
    let left = 275.0;
    let right = (0..32)
        .filter_map(|index| ctx.read_response(egui::Id::new(("finance-table-cell", key, index))))
        .next_back()
        .map_or(size.x - 136.0, |actions| actions.rect.left() - 4.0);
    let delta = if rect.left() < left {
        left - rect.left()
    } else if rect.right() > right {
        right - rect.right()
    } else {
        return;
    };
    let _ = frame(
        app,
        ctx,
        size,
        vec![
            egui::Event::PointerMoved(egui::pos2(500.0, rect.center().y)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                delta: egui::vec2(delta, 0.0),
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    for _ in 0..30 {
        let _ = frame(app, ctx, size, vec![]);
    }
}

fn select_month(
    app: &mut Harness,
    ctx: &egui::Context,
    size: egui::Vec2,
    selected: &str,
    label: &str,
) {
    let _ = frame(app, ctx, size, vec![]);
    let filters = ctx
        .read_response(egui::Id::new("finance-filters"))
        .unwrap()
        .rect;
    click(app, ctx, size, filters.center());
    let _ = frame(app, ctx, size, vec![]);
    let output = frame(app, ctx, size, vec![]);
    let current = month_label(if app.tab.view == FinanceView::Wallet {
        &app.tab.wallet_month
    } else {
        &app.tab.month
    });
    let picker = text_rect(&output, &current).expect("Date selector should be visible");
    click(app, ctx, size, picker.center());
    let _ = frame(app, ctx, size, vec![]);
    let output = frame(app, ctx, size, vec![]);
    let option = text_rect(&output, label).unwrap_or_else(|| {
        panic!(
            "Month option {selected} should be visible in {:?}",
            app.tab.view
        )
    });
    click(app, ctx, size, option.center());
    assert_eq!(
        if app.tab.view == FinanceView::Wallet {
            &app.tab.wallet_month
        } else {
            &app.tab.month
        },
        selected
    );
    click(app, ctx, size, egui::pos2(500.0, 48.0));
    let _ = frame(app, ctx, size, vec![]);
}

#[test]
fn month_filter_selects_records_through_the_nested_dropdown() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        for (view, january, february) in [
            (FinanceView::Income, "January Income", "February Income"),
            (FinanceView::Expenses, "January Expense", "February Expense"),
            (FinanceView::Wallet, "January Expense", "February Expense"),
        ] {
            let (mut app, ctx) = harness();
            app.plan.income.clear();
            for (date, prefix) in [("2026-01-12", "January"), ("2026-02-12", "February")] {
                let mut income = Income::new("EUR");
                income.date = date.into();
                income.source = format!("{prefix} Income");
                income.gross_amount =
                    Amount::Number(if prefix == "January" { 100.0 } else { 200.0 });
                app.plan.income.push(income);
                let mut expense = Expense::new("EUR");
                expense.due_date = date.into();
                expense.title = format!("{prefix} Expense");
                app.plan.expenses.push(expense);
                app.plan.budgets.push(Budget {
                    id: id(),
                    month: month_key(date),
                    label: format!("{prefix} Budget"),
                    ..Default::default()
                });
            }
            app.plan.normalize();
            app.tab.view = view;
            app.tab.month = "2026-01".into();
            select_month(&mut app, &ctx, size, "2026-02", "February 2026");
            let output = frame(&mut app, &ctx, size, vec![]);
            if view == FinanceView::Wallet {
                assert!(text_rect(&output, "200.00").is_some());
                assert!(text_rect(&output, "100.00").is_none());
                continue;
            }
            assert!(
                text_rect(&output, february).is_some(),
                "Missing {february} in {view:?}"
            );
            assert!(
                text_rect(&output, january).is_none(),
                "Unexpected {january} in {view:?}"
            );
            if view != FinanceView::Wallet {
                select_month(&mut app, &ctx, size, "", "All dates");
                let output = frame(&mut app, &ctx, size, vec![]);
                assert!(text_rect(&output, february).is_some());
                assert!(text_rect(&output, january).is_some());
            }
        }
    }
}

#[test]
fn stored_reference_filter_keeps_its_nested_selector_open() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(980.0, 640.0);
    app.tab.view = FinanceView::Storage;
    let _ = frame(&mut app, &ctx, size, vec![]);
    let filters = ctx
        .read_response(egui::Id::new("finance-filters"))
        .unwrap()
        .rect;
    click(&mut app, &ctx, size, filters.center());
    let _ = frame(&mut app, &ctx, size, vec![]);
    let output = frame(&mut app, &ctx, size, vec![]);
    click(
        &mut app,
        &ctx,
        size,
        text_rect(&output, "Currencies").unwrap().center(),
    );
    let _ = frame(&mut app, &ctx, size, vec![]);
    let output = frame(&mut app, &ctx, size, vec![]);
    click(
        &mut app,
        &ctx,
        size,
        text_rect(&output, "Chains").unwrap().center(),
    );
    assert_eq!(app.tab.storage_unit, 1);
}
#[test]
fn finance_toolbar_orders_controls_and_combines_search_currency_filters_and_create() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        let _ = frame(&mut app, &ctx, size, vec![]);
        let output = frame(&mut app, &ctx, size, vec![]);
        let search = ctx
            .read_response(egui::Id::new("finance-search"))
            .unwrap()
            .rect;
        let settings = ctx
            .read_response(egui::Id::new("vault-settings"))
            .unwrap()
            .rect;
        let filters = ctx
            .read_response(egui::Id::new("finance-filters"))
            .unwrap()
            .rect;
        let create = text_rect(&output, "+ Income").unwrap();
        assert!(
            search.right() < settings.left()
                && settings.right() < filters.left()
                && filters.right() < create.left()
        );
        assert!(create.right() < size.x - 20.0);
        click(&mut app, &ctx, size, search.center());
        for _ in 0..20 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        assert!(ctx.memory(|m| m.has_focus(egui::Id::new("finance-search-input"))));
        let output = frame(&mut app, &ctx, size, vec![egui::Event::Text("Euro".into())]);
        assert!(text_rect(&output, "Euro Job").is_some());
        assert!(text_rect(&output, "Dollar Job").is_none());
        click(&mut app, &ctx, size, filters.center());
        let _ = frame(&mut app, &ctx, size, vec![]);
        let output = frame(&mut app, &ctx, size, vec![]);
        click(
            &mut app,
            &ctx,
            size,
            text_rect(&output, "USD").unwrap().center(),
        );
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Euro Job").is_none());
        assert!(text_rect(&output, "Dollar Job").is_none());
        click(
            &mut app,
            &ctx,
            size,
            text_rect(&output, "Clear Filters").unwrap().center(),
        );
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Euro Job").is_some());
        assert!(text_rect(&output, "Dollar Job").is_some());
        click(
            &mut app,
            &ctx,
            size,
            text_rect(&output, "EUR").unwrap().center(),
        );
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Euro Job").is_some());
        assert!(text_rect(&output, "Dollar Job").is_none());
        click(&mut app, &ctx, size, create.center());
        assert!(matches!(
            app.tab.drafts.values().last().map(|e| &e.record),
            Some(Record::Income(_))
        ));
    }
}
#[test]
fn currency_dropdown_uses_available_currency_ids_even_when_codes_match() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(1440.0, 900.0);
    app.plan.chains = vec![
        Chain {
            id: "eth".into(),
            name: "Ethereum".into(),
            ..Default::default()
        },
        Chain {
            id: "sol".into(),
            name: "Solana".into(),
            ..Default::default()
        },
    ];
    app.plan.currencies.clear();
    app.plan.currencies.extend([
        Currency {
            id: "usdc-eth".into(),
            name: "USDC".into(),
            chain_id: "eth".into(),
            address: "0xabc".into(),
            ..Default::default()
        },
        Currency {
            id: "usdc-sol".into(),
            name: "USDC".into(),
            chain_id: "sol".into(),
            address: "abc".into(),
            ..Default::default()
        },
    ]);
    let mut row = Expense::new("USDC");
    row.title = "Test".into();
    row.currency_id = "usdc-eth".into();
    app.tab.view = FinanceView::Expenses;
    let key = row.id.clone();
    app.tab.edit_record(Record::Expense(row), true);
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    let output = frame(&mut app, &ctx, size, vec![]);
    click(
        &mut app,
        &ctx,
        size,
        text_rect(&output, "USDC (Ethereum)").unwrap().center(),
    );
    let _ = frame(&mut app, &ctx, size, vec![]);
    let output = frame(&mut app, &ctx, size, vec![]);
    click(
        &mut app,
        &ctx,
        size,
        text_rect(&output, "USDC · Solana · abc").unwrap().center(),
    );
    let Record::Expense(row) = &app.tab.drafts[&key].record else {
        panic!()
    };
    assert_eq!(row.currency, "USDC");
    assert_eq!(row.currency_id, "usdc-sol");
}

fn replace_cell(
    app: &mut Harness,
    ctx: &egui::Context,
    size: egui::Vec2,
    key: &str,
    field: &str,
    value: &str,
) {
    replace_input(
        app,
        ctx,
        size,
        egui::Id::new(("finance-cell", key, field)),
        value,
    );
}

fn replace_input(
    app: &mut Harness,
    ctx: &egui::Context,
    size: egui::Vec2,
    id: egui::Id,
    value: &str,
) {
    for _ in 0..3 {
        let _ = frame(app, ctx, size, vec![]);
    }
    let rect = ctx.read_response(id).unwrap().rect;
    click(app, ctx, size, rect.center());
    assert_eq!(ctx.memory(|m| m.focused()), Some(id));
    let _ = frame(
        app,
        ctx,
        size,
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
            },
            egui::Event::Text(value.into()),
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
            },
        ],
    );
    let _ = frame(app, ctx, size, vec![]);
}

fn row_action(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2, key: &str, action: &str) {
    let rect = ctx
        .read_response(egui::Id::new((action, key)))
        .unwrap()
        .rect;
    click(app, ctx, size, rect.center());
    let _ = frame(app, ctx, size, vec![]);
}

fn keyboard(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2, key: egui::Key) {
    for pressed in [true, false] {
        let _ = frame(
            app,
            ctx,
            size,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
}

#[test]
fn finance_keyboard_saves_all_pending_rows_and_escape_discards_only_active_row() {
    for (first_key, second_key) in [("a", "b"), ("b", "a")] {
        let (mut app, ctx) = harness();
        let size = egui::vec2(1440.0, 900.0);
        let mut first = Expense::new("EUR");
        first.id = first_key.into();
        let first_id = first.id.clone();
        let mut second = Expense::new("EUR");
        second.id = second_key.into();
        let second_id = second.id.clone();
        app.tab.edit_record(Record::Expense(first), true);
        app.tab.edit_record(Record::Expense(second), true);
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        replace_cell(&mut app, &ctx, size, &first_id, "title", "First");
        replace_cell(&mut app, &ctx, size, &second_id, "title", "Second");
        keyboard(&mut app, &ctx, size, egui::Key::Enter);
        assert_eq!(app.plan.expenses.len(), 2);
        assert!(app.tab.drafts.is_empty());
        assert_eq!(
            app.plan
                .expenses
                .iter()
                .find(|r| r.id == first_id)
                .unwrap()
                .title,
            "First"
        );
        assert_eq!(
            app.plan
                .expenses
                .iter()
                .find(|r| r.id == second_id)
                .unwrap()
                .title,
            "Second"
        );
        replace_cell(&mut app, &ctx, size, &first_id, "title", "Changed");
        replace_cell(&mut app, &ctx, size, &second_id, "title", "Discard");
        keyboard(&mut app, &ctx, size, egui::Key::Escape);
        assert_eq!(app.tab.drafts.len(), 1);
        assert!(app.tab.drafts.contains_key(&first_id));
        assert_eq!(
            app.plan
                .expenses
                .iter()
                .find(|r| r.id == second_id)
                .unwrap()
                .title,
            "Second"
        );
        // Focusing an unchanged row must not stop Enter saving another row's edits.
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let clean_cell = ctx
            .read_response(egui::Id::new(("finance-cell", &second_id, "title")))
            .unwrap()
            .rect;
        click(&mut app, &ctx, size, clean_cell.center());
        keyboard(&mut app, &ctx, size, egui::Key::Enter);
        assert_eq!(
            app.plan
                .expenses
                .iter()
                .find(|r| r.id == first_id)
                .unwrap()
                .title,
            "Changed"
        );
        assert!(app.tab.drafts.is_empty());
    }
}

#[test]
fn finance_keyboard_saves_hidden_drafts_and_retains_validation_errors() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(980.0, 640.0);
    let income = app.plan.income[0].clone();
    app.tab.edit_record(Record::Income(income.clone()), false);
    replace_cell(&mut app, &ctx, size, &income.id, "source", "Updated Income");
    let mut expense = Expense::new("EUR");
    expense.title = "Hidden Expense".into();
    app.tab.edit_record(Record::Expense(expense.clone()), true);
    let invalid = Expense::new("EUR");
    let invalid_id = invalid.id.clone();
    app.tab.edit_record(Record::Expense(invalid), true);
    app.tab.view = FinanceView::Wallet;
    app.tab.active_row = None;
    app.tab.focus_row = None;
    ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    keyboard(&mut app, &ctx, size, egui::Key::Enter);
    assert_eq!(
        app.plan
            .income
            .iter()
            .find(|r| r.id == income.id)
            .unwrap()
            .source,
        "Updated Income"
    );
    assert_eq!(app.plan.expenses.len(), 1);
    assert_eq!(app.plan.expenses[0].id, expense.id);
    assert_eq!(app.tab.drafts.len(), 1);
    assert!(app.tab.drafts[&invalid_id].error.contains("title"));
    let output = frame(&mut app, &ctx, size, vec![]);
    assert!(text_rect(&output, "1 pending row needs attention in Expenses.").is_some());
    app.tab.view = FinanceView::Expenses;
    replace_cell(&mut app, &ctx, size, &invalid_id, "title", "Corrected");
    keyboard(&mut app, &ctx, size, egui::Key::Enter);
    assert_eq!(app.plan.expenses.len(), 2);
    assert!(app.tab.drafts.is_empty());
}

#[test]
fn finance_save_checkmark_click_saves_one_row_but_enter_saves_all() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(1440.0, 900.0);
    let mut keys = Vec::new();
    for title in ["First", "Second", "Third"] {
        let mut expense = Expense::new("EUR");
        expense.title = title.into();
        keys.push(expense.id.clone());
        app.tab.edit_record(Record::Expense(expense), true);
    }
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    row_action(&mut app, &ctx, size, &keys[0], "finance-row-save");
    assert_eq!(app.plan.expenses.len(), 1);
    assert_eq!(app.tab.drafts.len(), 2);
    ctx.memory_mut(|m| m.request_focus(egui::Id::new(("finance-row-save", &keys[1]))));
    keyboard(&mut app, &ctx, size, egui::Key::Enter);
    assert_eq!(app.plan.expenses.len(), 3);
    assert!(app.tab.drafts.is_empty());
}

#[test]
fn finance_enter_includes_text_edits_from_the_same_frame() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(1440.0, 900.0);
    let mut first = Expense::new("EUR");
    first.title = "Other Row".into();
    app.tab.edit_record(Record::Expense(first), true);
    let second = Expense::new("EUR");
    let key = second.id.clone();
    app.tab.edit_record(Record::Expense(second), true);
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(egui::Id::new(("finance-cell", &key, "title")))
    );
    let _ = frame(
        &mut app,
        &ctx,
        size,
        vec![
            egui::Event::Text("Last Keystroke".into()),
            egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    assert_eq!(app.plan.expenses.len(), 2);
    assert_eq!(
        app.plan
            .expenses
            .iter()
            .find(|r| r.id == key)
            .unwrap()
            .title,
        "Last Keystroke"
    );
    assert!(app.tab.drafts.is_empty());
}

#[test]
fn finance_keyboard_respects_popups_modals_validation_and_delete_confirmation() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(1440.0, 900.0);
    let expense = Expense::new("EUR");
    let key = expense.id.clone();
    app.tab.edit_record(Record::Expense(expense), true);
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    keyboard(&mut app, &ctx, size, egui::Key::Enter);
    assert!(app.plan.expenses.is_empty());
    assert!(app.tab.drafts[&key].error.contains("title"));
    replace_cell(&mut app, &ctx, size, &key, "title", "Keep");
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    let pending_saving = Saving {
        id: id(),
        currency: "EUR".into(),
        ..Default::default()
    };
    let saving_id = pending_saving.id.clone();
    app.tab.drafts.insert(
        saving_id.clone(),
        Editor::new(Record::Saving(pending_saving), true),
    );
    let currency = ctx
        .read_response(egui::Id::new(("finance-table-cell", &key, 3usize)))
        .unwrap()
        .rect;
    click(&mut app, &ctx, size, currency.center());
    assert!(egui::Popup::is_any_open(&ctx));
    keyboard(&mut app, &ctx, size, egui::Key::Enter);
    assert!(app.plan.expenses.is_empty());
    assert!(app.plan.savings.is_empty());
    keyboard(&mut app, &ctx, size, egui::Key::Escape);
    assert!(!egui::Popup::is_any_open(&ctx));
    assert!(app.tab.drafts.contains_key(&key));
    app.tab.configure_vault(&app.plan);
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    keyboard(&mut app, &ctx, size, egui::Key::Escape);
    assert!(app.tab.settings.is_none());
    assert!(app.tab.drafts.contains_key(&key));
    assert!(app.plan.expenses.is_empty());
    app.tab.configure_vault(&app.plan);
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    keyboard(&mut app, &ctx, size, egui::Key::Enter);
    assert!(app.tab.settings.is_none());
    assert!(app.plan.expenses.is_empty());
    assert!(app.plan.savings.is_empty());
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    row_action(&mut app, &ctx, size, &key, "finance-row-save");
    assert!(app.tab.drafts.contains_key(&saving_id));
    row_action(&mut app, &ctx, size, &key, "finance-row-delete");
    keyboard(&mut app, &ctx, size, egui::Key::Escape);
    assert_eq!(app.plan.expenses.len(), 1);
    row_action(&mut app, &ctx, size, &key, "finance-row-delete");
    keyboard(&mut app, &ctx, size, egui::Key::Enter);
    assert!(app.plan.expenses.is_empty());
    assert!(app.plan.savings.is_empty());
    assert!(app.tab.drafts.contains_key(&saving_id));
}

#[test]
fn create_adds_inline_rows_for_every_finance_table_without_mutating_saved_records() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        for (view, unit, label) in [
            (FinanceView::Income, 0, "+ Income"),
            (FinanceView::Expenses, 0, "+ Expense"),
            (FinanceView::Savings, 0, "+ Saving"),
            (FinanceView::Budgets, 0, "+ Budget"),
            (FinanceView::Storage, 1, "+ Add Value"),
            (FinanceView::Storage, 2, "+ Add Value"),
        ] {
            let (mut app, ctx) = harness();
            app.tab.view = view;
            app.tab.storage_unit = unit;
            let original = app.plan.clone();
            let _ = frame(&mut app, &ctx, size, vec![]);
            let output = frame(&mut app, &ctx, size, vec![]);
            click(
                &mut app,
                &ctx,
                size,
                text_rect(&output, label).unwrap().center(),
            );
            let output = frame(&mut app, &ctx, size, vec![]);
            assert_eq!(app.plan, original);
            assert_eq!(app.tab.drafts.len(), 1);
            let key = app.tab.drafts.keys().next().unwrap().clone();
            assert!(
                ctx.read_response(egui::Id::new(("finance-row-save", &key)))
                    .is_some()
            );
            assert!(text_rect(&output, "Actions").is_none());
            assert!(text_rect(&output, "Cancel").is_none()); // No record modal footer.
            row_action(&mut app, &ctx, size, &key, "finance-row-cancel");
            assert!(
                app.tab.drafts.is_empty(),
                "Could not cancel {view:?} at {size:?}"
            );
            assert_eq!(app.plan, original);
        }
    }
}

#[test]
fn inline_expense_validates_saves_and_cancels_through_real_cells() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(1440.0, 900.0);
    app.tab.view = FinanceView::Expenses;
    let _ = frame(&mut app, &ctx, size, vec![]);
    let output = frame(&mut app, &ctx, size, vec![]);
    click(
        &mut app,
        &ctx,
        size,
        text_rect(&output, "+ Expense").unwrap().center(),
    );
    let _ = frame(&mut app, &ctx, size, vec![]);
    let key = app.tab.drafts.keys().next().unwrap().clone();
    replace_cell(&mut app, &ctx, size, &key, "title", "Rent");
    replace_cell(&mut app, &ctx, size, &key, "amount", "not a number");
    row_action(&mut app, &ctx, size, &key, "finance-row-save");
    assert!(app.plan.expenses.is_empty());
    assert!(
        app.tab.drafts[&key].error.contains("finite numbers"),
        "Error: {} / Title: {} / Amount: {} / Save: {:?}",
        app.tab.drafts[&key].error,
        match &app.tab.drafts[&key].record {
            Record::Expense(r) => &r.title,
            _ => unreachable!(),
        },
        match &app.tab.drafts[&key].record {
            Record::Expense(r) => r.amount.value(),
            _ => unreachable!(),
        },
        ctx.read_response(egui::Id::new(("finance-row-save", &key)))
            .unwrap()
            .rect
    );
    replace_cell(&mut app, &ctx, size, &key, "amount", "1250.50");
    row_action(&mut app, &ctx, size, &key, "finance-row-save");
    assert!(
        app.tab.drafts.is_empty(),
        "{}",
        app.tab
            .drafts
            .values()
            .map(|d| d.error.as_str())
            .collect::<Vec<_>>()
            .join(" / ")
    );
    assert_eq!(app.plan.expenses[0].title, "Rent");
    assert_eq!(app.plan.expenses[0].amount.value(), 1250.50);
    let saved = app.plan.clone();
    replace_cell(&mut app, &ctx, size, &key, "title", "Changed");
    assert_eq!(app.plan, saved);
    row_action(&mut app, &ctx, size, &key, "finance-row-cancel");
    assert_eq!(app.plan, saved);
    assert!(app.tab.drafts.is_empty());
}

#[test]
fn new_rows_remain_visible_under_filters_and_keep_independent_drafts() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(980.0, 640.0);
    app.tab.search = "Nothing matches".into();
    app.tab.month = "2031-02".into();
    app.tab.selected_currencies.insert("USD".into());
    let _ = frame(&mut app, &ctx, size, vec![]);
    for _ in 0..2 {
        let output = frame(&mut app, &ctx, size, vec![]);
        click(
            &mut app,
            &ctx,
            size,
            text_rect(&output, "+ Income").unwrap().center(),
        );
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    assert_eq!(app.tab.drafts.len(), 2);
    for (key, draft) in &app.tab.drafts {
        let Record::Income(row) = &draft.record else {
            panic!()
        };
        assert_eq!(row.date, "2031-02-01");
        assert_eq!(row.currency, "USD");
        assert!(
            ctx.read_response(egui::Id::new(("finance-row-save", key)))
                .is_some()
        );
    }
    assert_eq!(app.plan.income.len(), 2);
}

#[test]
fn inline_label_rename_updates_references_and_delete_stays_guarded() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(1440.0, 900.0);
    let mut expense = Expense::new("EUR");
    expense.title = "Rent".into();
    expense.labels = vec!["Essential".into()];
    app.plan.expenses.push(expense);
    app.plan.normalize();
    let key = app.plan.expense_labels[0].id.clone();
    app.tab.view = FinanceView::Storage;
    app.tab.storage_unit = 6;
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    replace_cell(&mut app, &ctx, size, &key, "name", "Necessities");
    row_action(&mut app, &ctx, size, &key, "finance-row-save");
    assert_eq!(app.plan.expense_labels[0].name, "Necessities");
    assert_eq!(app.plan.expenses[0].labels, ["Necessities"]);
    assert_eq!(
        app.plan.expenses[0].label_ids.as_slice(),
        std::slice::from_ref(&key)
    );
    row_action(&mut app, &ctx, size, &key, "finance-row-delete");
    row_action(&mut app, &ctx, size, &key, "finance-row-confirm-delete");
    assert_eq!(app.plan.expense_labels.len(), 1);
    assert!(app.tab.drafts[&key].error.contains("in use"));
}

#[test]
fn expense_columns_and_month_groups_show_each_regular_expense_once() {
    for size in [egui::vec2(980.0, 900.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        let mut regular = Expense::new("EUR");
        regular.title = "Automatic rent".into();
        regular.due_date = "2026-02-20".into();
        regular.regular = true;
        regular.amount = Amount::Number(10.0);
        regular.labels = vec!["Legacy label".into()];
        let regular_id = regular.id.clone();
        app.plan.expenses.push(regular);
        for (date, amount) in [
            ("2026-01-05", 20.0),
            ("2026-02-01", 30.0),
            ("2026-02-15", 40.0),
            ("2026-03-10", 50.0),
        ] {
            let mut expense = Expense::new("EUR");
            expense.title = format!("Manual {date}");
            expense.due_date = date.into();
            expense.amount = Amount::Number(amount);
            app.plan.expenses.push(expense);
        }
        app.plan.normalize();
        let saved = app.plan.expenses.clone();
        app.tab.view = FinanceView::Expenses;
        app.tab.month = "2026-02".into();
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Date").is_some());
        assert!(text_rect(&output, "February 2026").is_some());
        for removed in ["Labels", "Use", "Applicable", "Through", "Due", "Start"] {
            assert!(
                text_rect(&output, removed).is_none(),
                "Unexpected {removed} column"
            );
        }
        let occurrence = format!("regular-expense:{regular_id}:2026-02");
        assert!(
            ctx.read_response(egui::Id::new(("finance-table-cell", &occurrence, 0usize)))
                .is_some()
        );
        assert!(
            ctx.read_response(egui::Id::new(("finance-row-delete", &occurrence)))
                .is_none()
        );
        assert!(
            ctx.read_response(egui::Id::new(("finance-row-save", &occurrence)))
                .is_none()
        );
        assert_eq!(app.plan.totals("2026-02", false).1, 80.0);
        assert_eq!(app.plan.totals("2026-02", true).1, 10.0);
        let count = |shape: &egui::Shape| -> usize {
            fn count(shape: &egui::Shape) -> usize {
                match shape {
                    egui::Shape::Text(text) => usize::from(text.galley.text() == "Automatic rent"),
                    egui::Shape::Vec(shapes) => shapes.iter().map(count).sum(),
                    _ => 0,
                }
            }
            count(shape)
        };
        assert_eq!(
            output
                .shapes
                .iter()
                .map(|shape| count(&shape.shape))
                .sum::<usize>(),
            1
        );
        app.tab.month = "2026-01".into();
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "January 2026").is_some());
        assert!(text_rect(&output, "Automatic rent").is_none());
        assert_eq!(app.plan.totals("2026-01", false).1, 20.0);
        app.tab.month = "2026-03".into();
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "March 2026").is_some());
        assert!(text_rect(&output, "Automatic rent").is_some());
        assert_eq!(app.plan.totals("2026-03", false).1, 60.0);
        click_text(&mut app, &ctx, size, "Regular");
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "From").is_some());
        for removed in ["Labels", "Use", "Through", "Start"] {
            assert!(text_rect(&output, removed).is_none());
        }
        assert_eq!(app.plan.expenses, saved);
        assert!(app.tab.drafts.is_empty());
    }
}

#[test]
fn expense_group_buttons_create_regular_and_manual_monthly_records() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        app.tab.view = FinanceView::Expenses;
        app.tab.month = "2020-02".into();
        click_text(&mut app, &ctx, size, "Regular");
        click_text(&mut app, &ctx, size, "+ Expense");
        let regular_key = app.tab.drafts.keys().next().unwrap().clone();
        let Record::Expense(expense) = &app.tab.drafts[&regular_key].record else {
            panic!()
        };
        assert!(expense.regular);
        assert_eq!(expense.due_date, "2020-02-01");
        replace_cell(
            &mut app,
            &ctx,
            size,
            &regular_key,
            "title",
            "Automatic rent",
        );
        replace_cell(&mut app, &ctx, size, &regular_key, "amount", "100");
        // Switching groups preserves the pending regular row but does not show it
        // in Monthly or change the type of a newly created monthly expense.
        click_text(&mut app, &ctx, size, "Monthly");
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Automatic rent").is_none());
        click_text(&mut app, &ctx, size, "+ Expense");
        let monthly_key = app
            .tab
            .drafts
            .keys()
            .find(|key| **key != regular_key)
            .unwrap()
            .clone();
        let Record::Expense(expense) = &app.tab.drafts[&monthly_key].record else {
            panic!()
        };
        assert!(!expense.regular);
        assert_eq!(expense.due_date, "2020-02-01");
        replace_cell(
            &mut app,
            &ctx,
            size,
            &monthly_key,
            "title",
            "Manual purchase",
        );
        replace_cell(&mut app, &ctx, size, &monthly_key, "amount", "50");
        row_action(&mut app, &ctx, size, &monthly_key, "finance-row-save");
        assert_eq!(app.plan.expenses.len(), 1);
        assert!(!app.plan.expenses[0].regular);
        click_text(&mut app, &ctx, size, "Regular");
        row_action(&mut app, &ctx, size, &regular_key, "finance-row-save");
        assert_eq!(app.plan.expenses.len(), 2);
        assert!(app.tab.drafts.is_empty());
        assert_eq!(app.plan.totals("2020-02", false).1, 150.0);
        assert_eq!(app.plan.totals("2020-03", false).1, 100.0);
    }
}

#[test]
fn monthly_expenses_created_from_wallet_use_its_selected_month() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(1440.0, 900.0);
    app.tab.view = FinanceView::Wallet;
    app.tab.wallet_month = "2020-02".into();
    app.tab.month = "2021-05".into();
    click_text(&mut app, &ctx, size, "+ Create");
    click_text(&mut app, &ctx, size, "Expense");
    let Record::Expense(expense) = &app.tab.drafts.values().next().unwrap().record else {
        panic!()
    };
    assert!(!expense.regular);
    assert_eq!(expense.due_date, "2020-02-01");
    assert_eq!(app.tab.expense_group, crate::ExpenseGroup::Monthly);
}

#[test]
fn validation_errors_in_the_other_expense_group_remain_visible() {
    let (mut app, ctx) = harness();
    let mut regular = Expense::new("EUR");
    regular.regular = true;
    app.tab.edit_record(Record::Expense(regular), true);
    inline::confirm_pending(&mut app.tab, &mut app.plan);
    assert_eq!(app.tab.drafts.len(), 1);
    app.tab.expense_group = crate::ExpenseGroup::Monthly;
    let output = frame(&mut app, &ctx, egui::vec2(1440.0, 900.0), vec![]);
    assert!(
        text_rect(
            &output,
            "1 pending row needs attention in Regular Expenses."
        )
        .is_some()
    );
}

#[test]
fn expense_cancellation_button_precedes_remove_and_stays_disabled_with_its_date() {
    for regular in [false, true] {
        for width in [980.0, 1440.0] {
            let (mut app, ctx) = harness();
            ctx.global_style_mut(|style| style.interaction.tooltip_delay = 0.0);
            let size = egui::vec2(width, 900.0);
            let mut expense = Expense::new("EUR");
            expense.title = "Cancellable expense".into();
            expense.due_date = if regular {
                "2020-01-31".into()
            } else {
                today()
            };
            expense.regular = regular;
            expense.applicable = true;
            expense.amount = Amount::Number(100.0);
            let key = expense.id.clone();
            app.plan.expenses.push(expense);
            app.plan.normalize();
            app.tab.view = FinanceView::Expenses;
            app.tab.expense_group = if regular {
                crate::ExpenseGroup::Regular
            } else {
                crate::ExpenseGroup::Monthly
            };
            // Cancellation always uses today, even while reviewing a historical month.
            app.tab.month = if regular { "2020-02".into() } else { month() };
            for _ in 0..3 {
                let _ = frame(&mut app, &ctx, size, vec![]);
            }
            let cancellation_id = egui::Id::new(("finance-expense-cancel", &key));
            let cancel = ctx.read_response(cancellation_id).unwrap();
            let remove = ctx
                .read_response(egui::Id::new(("finance-row-delete", &key)))
                .unwrap();
            assert!(cancel.enabled());
            assert!(cancel.rect.right() <= remove.rect.left());
            assert!(cancel.rect.left() >= 224.0);
            assert!(remove.rect.right() <= width - 28.0);
            // Discard unsaved edits when cancelling, preserving recorded history.
            replace_cell(&mut app, &ctx, size, &key, "amount", "999");
            row_action(&mut app, &ctx, size, &key, "finance-expense-cancel");
            assert_eq!(app.plan.expenses[0].cancelled_on, today());
            assert_eq!(app.plan.expenses[0].amount.value(), 100.0);
            assert!(app.tab.drafts.is_empty());
            assert_eq!(app.plan.totals(&month(), false).1, 0.0);
            assert_eq!(app.plan.totals(&month(), true).1, 0.0);
            assert_eq!(app.plan.totals("2099-01", false).1, 0.0);
            if regular {
                assert_eq!(app.plan.totals("2020-02", false).1, 100.0);
            }
            app.tab.month = month();
            let _ = frame(&mut app, &ctx, size, vec![]);
            let response = ctx.read_response(cancellation_id).unwrap();
            assert!(!response.enabled());
            let pos = response.rect.center();
            // A tooltip opens on a later hover, not immediately after clicking
            // and leaving the pointer resting on the same icon.
            let _ = frame(
                &mut app,
                &ctx,
                size,
                vec![egui::Event::PointerMoved(egui::pos2(400.0, 100.0))],
            );
            for _ in 0..10 {
                let _ = frame(&mut app, &ctx, size, vec![]);
            }
            let mut tooltip_shown = false;
            for _ in 0..30 {
                let output = frame(&mut app, &ctx, size, vec![egui::Event::PointerMoved(pos)]);
                tooltip_shown |= text_rect(&output, &format!("Cancelled on {}", today())).is_some();
            }
            assert!(
                tooltip_shown,
                "The disabled icon must display its cancellation date"
            );
            let saved = app.plan.clone();
            click(&mut app, &ctx, size, pos);
            assert_eq!(app.plan, saved);
            app.tab.month.clear();
            let output = frame(&mut app, &ctx, size, vec![]);
            assert!(text_rect(&output, "Cancellable expense").is_some());
            if regular {
                app.tab.view = FinanceView::Expenses;
                app.tab.expense_group = crate::ExpenseGroup::Monthly;
                app.tab.month = month();
                let output = frame(&mut app, &ctx, size, vec![]);
                assert!(text_rect(&output, "Cancellable expense").is_none());
                app.tab.month = "2020-02".into();
                let output = frame(&mut app, &ctx, size, vec![]);
                assert!(text_rect(&output, "Cancellable expense").is_some());
            }
        }
    }
}

#[test]
fn regular_expense_deletion_uses_the_current_month_and_preserves_recorded_amounts() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(1440.0, 900.0);
    let mut expense = Expense::new("EUR");
    expense.title = "Regular rent".into();
    expense.due_date = "2020-01-31".into();
    expense.amount = Amount::Number(100.0);
    expense.regular = true;
    expense.applicable = true;
    let key = expense.id.clone();
    app.plan.expenses.push(expense);
    app.plan.normalize();
    app.tab.view = FinanceView::Expenses;
    app.tab.month = "2020-02".into();
    click_text(&mut app, &ctx, size, "Regular");
    replace_cell(&mut app, &ctx, size, &key, "amount", "999");
    row_action(&mut app, &ctx, size, &key, "finance-row-delete");
    row_action(&mut app, &ctx, size, &key, "finance-row-confirm-delete");
    assert_eq!(app.plan.expenses.len(), 1);
    assert_eq!(app.plan.expenses[0].regular_until, month());
    assert_eq!(app.plan.expenses[0].amount.value(), 100.0);
    assert_eq!(app.plan.totals("2020-02", false).1, 100.0);
    assert_eq!(app.plan.totals(&month(), false).1, 100.0);
    let next_month = chrono::Local::now()
        .date_naive()
        .checked_add_months(chrono::Months::new(1))
        .unwrap()
        .format("%Y-%m")
        .to_string();
    assert_eq!(app.plan.totals(&next_month, false).1, 0.0);
    app.tab.month.clear();
    let output = frame(&mut app, &ctx, size, vec![]);
    assert!(text_rect(&output, "Regular rent").is_none());
    app.tab.month = "2020-02".into();
    let output = frame(&mut app, &ctx, size, vec![]);
    assert!(text_rect(&output, "Regular rent").is_some());
    app.tab.month = next_month.clone();
    let output = frame(&mut app, &ctx, size, vec![]);
    assert!(text_rect(&output, "Regular rent").is_none());
}

#[test]
fn every_table_keeps_columns_aligned_and_actions_visible_during_window_resizing() {
    for (record, count) in [
        (Record::Account(Account::default()), 2),
        (Record::Income(Income::new("EUR")), 13),
        (Record::Expense(Expense::new("EUR")), 8),
        (Record::Saving(Saving::default()), 9),
        (Record::Budget(Budget::default()), 6),
        (Record::Chain(Chain::default()), 3),
        (Record::Stored(2, StoredValue::default()), 2),
    ] {
        let (mut app, ctx) = harness();
        app.tab.edit_record(record, true);
        let key = app.tab.drafts.keys().next().unwrap().clone();
        for width in [1440.0, 980.0, 1920.0, 1100.0, 980.0] {
            let size = egui::vec2(width, 900.0);
            let _ = frame(&mut app, &ctx, size, vec![]);
            let _ = frame(&mut app, &ctx, size, vec![]);
            reveal_table_cell(&mut app, &ctx, size, &key, 0);
            let mut previous = f32::NEG_INFINITY;
            for index in 0..count {
                let body = ctx
                    .read_response(egui::Id::new(("finance-table-cell", &key, index)))
                    .unwrap()
                    .rect;
                let header = ctx
                    .read_response(egui::Id::new(("finance-table-cell", "headers", index)))
                    .unwrap()
                    .rect;
                assert!(
                    index == count - 1 || body.left() >= previous - 0.5,
                    "Columns overlap at {width}: {body:?}"
                );
                assert!(
                    (width < 1200.0 && index != count - 1) || body.right() <= width - 28.0 + 0.5,
                    "A column is outside the window: {body:?}"
                );
                assert!((body.left() - header.left()).abs() < 0.5);
                assert!((body.width() - header.width()).abs() < 0.5);
                previous = body.right();
            }
            let actions = ctx
                .read_response(egui::Id::new(("finance-table-cell", &key, count - 1)))
                .unwrap()
                .rect;
            for action in ["finance-row-save", "finance-row-cancel"] {
                let button = ctx
                    .read_response(egui::Id::new((action, &key)))
                    .unwrap()
                    .rect;
                assert!(
                    actions.expand(0.5).contains_rect(button),
                    "Clipped {action}: {actions:?}, {button:?}"
                );
            }
            let date = match &app.tab.drafts[&key].record {
                Record::Income(_) | Record::Rate(_) => Some(("date", 0usize)),
                Record::Expense(_) => Some(("due", 1usize)),
                _ => None,
            };
            if let Some((field, column)) = date {
                let cell = ctx
                    .read_response(egui::Id::new(("finance-table-cell", &key, column)))
                    .unwrap()
                    .rect;
                let picker = format!("finance-{key}-{field}");
                for control in ["input", "calendar"] {
                    let rect = ctx
                        .read_response(egui::Id::new((&picker, control)))
                        .unwrap()
                        .rect;
                    assert!(
                        cell.expand(0.5).contains_rect(rect),
                        "Clipped date {control} at {width}: {cell:?}, {rect:?}"
                    );
                }
            }
            // Narrow layouts scroll data columns together while actions stay pinned.
            let actions_before = actions;
            reveal_table_cell(&mut app, &ctx, size, &key, count - 2);
            let data = ctx
                .read_response(egui::Id::new(("finance-table-cell", &key, count - 2)))
                .unwrap()
                .rect;
            let heading = ctx
                .read_response(egui::Id::new(("finance-table-cell", "headers", count - 2)))
                .unwrap()
                .rect;
            assert!((data.left() - heading.left()).abs() < 0.5);
            let actions_after = ctx
                .read_response(egui::Id::new(("finance-table-cell", &key, count - 1)))
                .unwrap()
                .rect;
            assert!((actions_before.left() - actions_after.left()).abs() < 0.5);
            assert!(
                data.right() <= actions_after.left() + 0.5,
                "{width} {:?}: data {data:?}, actions {actions_after:?}",
                app.tab.view
            );
        }
    }
}

#[test]
fn income_month_headers_separate_years_and_follow_filters() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        let original = app.plan.income[0].clone();
        app.plan.income = ["2025-04-15", "2026-06-10", "2026-04-04"]
            .into_iter()
            .enumerate()
            .map(|(index, date)| {
                let mut row = original.clone();
                row.id = format!("month-{index}");
                row.date = date.into();
                row
            })
            .collect();
        let saved = app.plan.clone();
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Income by month").is_none());
        let june = text_rect(&output, "June 2026").unwrap();
        let april = text_rect(&output, "April 2026").unwrap();
        let old_april = text_rect(&output, "April 2025").unwrap();
        let row = |index| {
            ctx.read_response(egui::Id::new((
                "finance-table-cell",
                format!("month-{index}"),
                0usize,
            )))
            .unwrap()
            .rect
        };
        assert!(june.bottom() < row(1).top());
        assert!(row(1).bottom() < april.top());
        assert!(april.bottom() < row(2).top());
        assert!(row(2).bottom() < old_april.top());
        assert!(old_april.bottom() < row(0).top());
        app.tab.month = "2025-04".into();
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "April 2025").is_some());
        assert!(text_rect(&output, "June 2026").is_none());
        assert!(text_rect(&output, "April 2026").is_none());
        assert_eq!(app.plan, saved);
    }
}

#[test]
fn large_tables_virtualize_rows_keep_headers_fixed_and_preserve_offscreen_edits() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        let template = app.plan.income[0].clone();
        app.plan.income = (0..5000)
            .map(|index| {
                let mut row = template.clone();
                row.id = format!("large-{index:05}");
                row.source = format!("Source {index:05}");
                row.date = (chrono::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()
                    - chrono::Duration::days(index / 4))
                .to_string();
                row
            })
            .collect();
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Source 00000").is_some());
        assert!(text_rect(&output, "Source 04999").is_none());
        assert!(
            output.shapes.len() < 4000,
            "Offscreen rows should not produce widgets"
        );
        let header = ctx
            .read_response(egui::Id::new(("finance-table-cell", "headers", 0usize)))
            .unwrap()
            .rect;
        let wheel = |delta| {
            vec![
                egui::Event::PointerMoved(egui::pos2(500.0, header.bottom() + 60.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    delta: egui::vec2(0.0, delta),
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let _ = frame(&mut app, &ctx, size, wheel(-4600.0));
        for _ in 0..30 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let output = frame(&mut app, &ctx, size, vec![]);
        assert_eq!(
            ctx.read_response(egui::Id::new(("finance-table-cell", "headers", 0usize)))
                .unwrap()
                .rect,
            header
        );
        assert!(text_rect(&output, "Source 00000").is_none());
        assert!(output.shapes.len() < 4000);

        // Editing the last saved row must scroll directly to it, even though its
        // controls have never been instantiated. Scrolling away retains key ownership.
        let last = app.plan.income.last().unwrap().clone();
        app.tab.edit_record(Record::Income(last.clone()), false);
        replace_cell(
            &mut app,
            &ctx,
            size,
            &last.id,
            "source",
            "Changed offscreen",
        );
        let rect = ctx
            .read_response(egui::Id::new(("finance-cell", &last.id, "source")))
            .unwrap()
            .rect;
        assert!(rect.top() >= header.bottom() && rect.bottom() < size.y);
        let _ = frame(&mut app, &ctx, size, wheel(4600.0));
        for _ in 0..30 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let rect = ctx
            .read_response(egui::Id::new(("finance-cell", &last.id, "source")))
            .unwrap()
            .rect;
        assert!(rect.top() > size.y, "Active row should now be offscreen");
        keyboard(&mut app, &ctx, size, egui::Key::Escape);
        assert!(app.tab.drafts.is_empty());
        assert_eq!(app.plan.income.last().unwrap().source, last.source);

        app.tab.edit_record(Record::Income(last.clone()), false);
        replace_cell(&mut app, &ctx, size, &last.id, "source", "Saved offscreen");
        let first = app.plan.income[0].clone();
        app.tab.edit_record(Record::Income(first.clone()), false);
        replace_cell(&mut app, &ctx, size, &first.id, "source", "Saved first");
        keyboard(&mut app, &ctx, size, egui::Key::Enter);
        assert!(app.tab.drafts.is_empty());
        assert_eq!(app.plan.income[0].source, "Saved first");
        assert_eq!(app.plan.income.last().unwrap().source, "Saved offscreen");
    }
}

#[test]
fn stored_accounts_use_only_labels_and_preserve_linked_income_and_savings() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        app.tab.view = FinanceView::Storage;
        app.tab.storage_unit = 4;
        click_text(&mut app, &ctx, size, "+ Add Value");
        let key = app.tab.drafts.keys().next().unwrap().clone();
        replace_cell(&mut app, &ctx, size, &key, "label", "Daily Account");
        keyboard(&mut app, &ctx, size, egui::Key::Enter);
        assert!(app.tab.drafts.is_empty());
        assert_eq!(app.plan.accounts[0].label, "Daily Account");
        assert_eq!(app.plan.income_accounts[0].id, key);
        app.plan.income[0].account_id = key.clone();
        app.plan.income[0].account = "Daily Account".into();
        app.plan.savings.push(Saving {
            id: id(),
            source_id: key.clone(),
            source: "Daily Account".into(),
            currency: "EUR".into(),
            ..Default::default()
        });
        app.plan.normalize();
        let account = Record::Account(app.plan.accounts[0].clone());
        assert!(inline::commit(&mut app.plan, &account, true).is_err());
        let output = frame(&mut app, &ctx, size, vec![]);
        for removed in ["Country", "Principal", "Comment"] {
            assert!(text_rect(&output, removed).is_none());
        }
        assert!(text_rect(&output, "Label").is_some());
        replace_cell(&mut app, &ctx, size, &key, "label", "Renamed Account");
        keyboard(&mut app, &ctx, size, egui::Key::Enter);
        assert_eq!(app.plan.income[0].account, "Renamed Account");
        assert_eq!(app.plan.savings[0].source, "Renamed Account");
        replace_cell(&mut app, &ctx, size, &key, "label", "Discarded");
        keyboard(&mut app, &ctx, size, egui::Key::Escape);
        assert_eq!(app.plan.accounts[0].label, "Renamed Account");
        app.tab.search = "Renamed".into();
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Renamed Account").is_some());
        assert_eq!(
            plan_from_value(serde_json::to_value(&app.plan).unwrap()).unwrap(),
            app.plan
        );
        app.tab.view = FinanceView::Wallet;
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Account List").is_none());
        click_text(&mut app, &ctx, size, "+ Create");
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Account").is_none());
    }
}

#[test]
fn finance_filter_keyboard_cancels_or_accepts_and_nested_dropdown_closes_first() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(1440.0, 900.0);
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    for key in [egui::Key::Escape, egui::Key::Enter] {
        app.tab.open_filters();
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let output = frame(&mut app, &ctx, size, vec![]);
        click(
            &mut app,
            &ctx,
            size,
            text_rect(&output, "EUR").unwrap().center(),
        );
        assert!(app.tab.selected_currencies.contains("EUR"));
        let output = frame(&mut app, &ctx, size, vec![]);
        let current = month_label(&app.tab.month);
        click(
            &mut app,
            &ctx,
            size,
            text_rect(&output, &current).unwrap().center(),
        );
        assert!(egui::Popup::is_any_open(&ctx));
        keyboard(&mut app, &ctx, size, egui::Key::Escape);
        assert!(app.tab.filters_open);
        assert!(!egui::Popup::is_any_open(&ctx));
        assert!(app.tab.selected_currencies.contains("EUR"));
        keyboard(&mut app, &ctx, size, key);
        assert!(!app.tab.filters_open);
        assert_eq!(
            app.tab.selected_currencies.contains("EUR"),
            key == egui::Key::Enter
        );
    }
}

fn click_text(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2, text: &str) {
    for _ in 0..3 {
        let _ = frame(app, ctx, size, vec![]);
    }
    let output = frame(app, ctx, size, vec![]);
    let rect = text_rect(&output, text).unwrap_or_else(|| panic!("Missing {text}"));
    click(app, ctx, size, rect.center());
}

#[test]
fn legacy_months_display_full_names_in_chronological_order() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(980.0, 640.0);
    app.plan.income = ["2026-9-2", "2026-11-3", "2026-10-15"]
        .map(|date| Income {
            date: date.into(),
            currency: "EUR".into(),
            ..Default::default()
        })
        .into();
    app.plan.normalize();
    app.tab.month = "2026-9-".into();
    app.tab.open_filters();
    click_text(&mut app, &ctx, size, "September 2026");
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    let output = frame(&mut app, &ctx, size, vec![]);
    let november = text_rect(&output, "November 2026").unwrap();
    let october = text_rect(&output, "October 2026").unwrap();
    let september = text_rect(&output, "September 2026").unwrap();
    assert!(november.top() < october.top() && october.top() < september.top());
    assert!(text_rect(&output, "2026-9-").is_none());
    assert_eq!(app.tab.month, "2026-09");
    click(&mut app, &ctx, size, september.center());
    keyboard(&mut app, &ctx, size, egui::Key::Enter);
    assert_eq!(app.plan.totals(&app.tab.month, false).2, 0);
}

fn saved_sort_rate(currency: &str) -> RecordAudit {
    RecordAudit {
        created_at: "2026-01-01T00:00:00Z".into(),
        rates: vec![RateSnapshot {
            base: currency.into(),
            quote: "EUR".into(),
            date: "2026-01-01".into(),
            rate: Some(2.0),
        }],
        ..Default::default()
    }
}

#[test]
fn income_and_expenses_sort_by_reported_amount_with_saved_rates_and_missing_values_last() {
    for size in [egui::vec2(980.0, 900.0), egui::vec2(1440.0, 900.0)] {
        for (view, regular) in [
            (FinanceView::Income, false),
            (FinanceView::Expenses, false),
            (FinanceView::Expenses, true),
        ] {
            let (mut app, ctx) = harness();
            app.plan.income.clear();
            for (index, (currency, amount)) in [
                ("EUR", 100.0),
                ("USD", 200.0),
                ("EUR", 300.0),
                ("XYZ", 99999.0),
                ("EUR", -10.0),
                ("EUR", 0.0),
            ]
            .into_iter()
            .enumerate()
            {
                let date = format!("2026-10-{:02}", 30 - index);
                app.plan.income.push(Income {
                    id: format!("rank-{index}"),
                    source: format!("Rank {index}"),
                    date: date.clone(),
                    currency: currency.into(),
                    gross_amount: Amount::Number(if index == 0 { 1000.0 } else { amount }),
                    applied_tax: Amount::Number(if index == 0 { 90.0 } else { 0.0 }),
                    audit: if currency == "USD" {
                        saved_sort_rate(currency)
                    } else {
                        RecordAudit::new()
                    },
                    ..Default::default()
                });
                app.plan.expenses.push(Expense {
                    id: format!("rank-{index}"),
                    title: format!("Rank {index}"),
                    due_date: date,
                    currency: currency.into(),
                    amount: Amount::Number(amount),
                    regular,
                    audit: if currency == "USD" {
                        saved_sort_rate(currency)
                    } else {
                        RecordAudit::new()
                    },
                    ..Default::default()
                });
            }
            if view == FinanceView::Expenses && !regular {
                app.plan.expenses.push(Expense {
                    id: "projected".into(),
                    title: "Projected regular expense".into(),
                    due_date: "2026-09-01".into(),
                    currency: "USD".into(),
                    amount: Amount::Number(175.0),
                    regular: true,
                    audit: saved_sort_rate("USD"),
                    ..Default::default()
                });
            }
            app.plan.normalize();
            let saved = app.plan.clone();
            app.tab.view = view;
            app.tab.month = "2026-10".into();
            app.tab.expense_group = if regular {
                crate::ExpenseGroup::Regular
            } else {
                crate::ExpenseGroup::Monthly
            };
            for _ in 0..3 {
                let _ = frame(&mut app, &ctx, size, vec![]);
            }
            let mut expected: Vec<String> = [1, 2, 0, 5, 4, 3]
                .into_iter()
                .map(|index| format!("rank-{index}"))
                .collect();
            if view == FinanceView::Expenses && !regular {
                expected.insert(1, "regular-expense:projected:2026-10".into());
            }
            let positions: Vec<_> = expected
                .iter()
                .map(|key| {
                    ctx.read_response(egui::Id::new(("finance-table-cell", key, 0usize)))
                        .unwrap()
                        .rect
                        .top()
                })
                .collect();
            assert!(
                positions.windows(2).all(|pair| pair[0] < pair[1]),
                "Wrong {view:?} order: {positions:?}"
            );
            assert_eq!(app.plan, saved);
            // A saved edit immediately takes its new position, using net income.
            replace_cell(
                &mut app,
                &ctx,
                size,
                "rank-0",
                if view == FinanceView::Income {
                    "gross"
                } else {
                    "amount"
                },
                "6000",
            );
            row_action(&mut app, &ctx, size, "rank-0", "finance-row-save");
            for _ in 0..2 {
                let _ = frame(&mut app, &ctx, size, vec![]);
            }
            let y = |key| {
                ctx.read_response(egui::Id::new(("finance-table-cell", key, 0usize)))
                    .unwrap()
                    .rect
                    .top()
            };
            assert!(
                y("rank-0") < y("rank-1"),
                "Wrong order after saving {view:?} at {}",
                size.x
            );
        }
    }
}

#[test]
fn equal_reported_amounts_use_dates_as_tiebreakers_and_budgets_keep_date_order() {
    for view in [
        FinanceView::Income,
        FinanceView::Expenses,
        FinanceView::Budgets,
    ] {
        let (mut app, ctx) = harness();
        let size = egui::vec2(1440.0, 1100.0);
        app.plan.income.clear();
        for (index, date) in ["2026-10-02", "2026-10-20", "2026-10-09", ""]
            .into_iter()
            .enumerate()
        {
            let mut row = Income::new("EUR");
            row.id = format!("row-{index}");
            row.source = format!("Income {index}");
            row.date = date.into();
            app.plan.income.push(row);
            let mut row = Expense::new("EUR");
            row.id = format!("row-{index}");
            row.title = format!("Expense {index}");
            row.due_date = date.into();
            app.plan.expenses.push(row);
            app.plan.fx_rates.push(ExchangeRate {
                id: format!("row-{index}"),
                date: date.into(),
                currency: "EUR".into(),
                rate: Amount::Number(1.1),
                ..Default::default()
            });
            app.plan.budgets.push(Budget {
                id: format!("row-{index}"),
                month: ["2026-08", "2026-11", "2026-09", ""][index].into(),
                label: format!("Budget {index}"),
                ..Default::default()
            });
        }
        app.plan.normalize();
        app.tab.view = view;
        app.tab.month.clear();
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let y = |index| {
            ctx.read_response(egui::Id::new((
                "finance-table-cell",
                format!("row-{index}"),
                0usize,
            )))
            .unwrap()
            .rect
            .top()
        };
        assert!(
            y(1) < y(2) && y(2) < y(0) && y(0) < y(3),
            "Wrong order for {view:?}"
        );
    }
}

#[test]
fn currency_symbol_list_opens_details_validates_tokens_and_preserves_links() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        let eth = app
            .plan
            .currencies
            .iter()
            .find(|c| c.name == "ETH")
            .unwrap()
            .clone();
        app.tab.edit_record(Record::Currency(eth.clone()), false);
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Network").is_some());
        assert!(text_rect(&output, "Token Address *").is_none());
        keyboard(&mut app, &ctx, size, egui::Key::Enter);
        assert!(app.tab.currency_editor.is_none());
        assert_eq!(
            app.plan.currencies.iter().find(|c| c.id == eth.id).unwrap(),
            &eth
        );
        app.plan.chains.push(Chain {
            id: "ethereum".into(),
            name: "Ethereum".into(),
            rpc_url: "https://example.test".into(),
        });
        app.tab.view = FinanceView::Storage;
        click_text(&mut app, &ctx, size, "+ Currency");
        assert!(app.tab.currency_editor.is_some());
        assert!(app.tab.drafts.is_empty());
        let _ = frame(&mut app, &ctx, size, vec![egui::Event::Text("tok".into())]);
        click_text(&mut app, &ctx, size, "Crypto Token");
        keyboard(&mut app, &ctx, size, egui::Key::Enter);
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Select a chain for this token.").is_some());
        click_text(&mut app, &ctx, size, "Select Chain");
        click_text(&mut app, &ctx, size, "Ethereum");
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("currency-symbol-input")));
        keyboard(&mut app, &ctx, size, egui::Key::Enter);
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Enter the token address.").is_some());
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("currency-address-input")));
        let _ = frame(
            &mut app,
            &ctx,
            size,
            vec![egui::Event::Text("0xtoken".into())],
        );
        keyboard(&mut app, &ctx, size, egui::Key::Enter);
        assert!(app.tab.currency_editor.is_none());
        let token = app
            .plan
            .currencies
            .iter()
            .find(|c| c.name == "TOK")
            .unwrap()
            .clone();
        assert_eq!(token.chain_id, "ethereum");
        assert_eq!(token.address, "0xtoken");
        let mut income = Income::new("TOK");
        income.currency_id = token.id.clone();
        app.plan.income.push(income);
        app.plan.normalize();
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Symbol").is_some());
        assert!(text_rect(&output, "Token Address").is_none());
        assert!(text_rect(&output, "Chain").is_none());
        click_text(&mut app, &ctx, size, "TOK (Ethereum)");
        click_text(&mut app, &ctx, size, "Crypto Token");
        keyboard(&mut app, &ctx, size, egui::Key::Escape);
        assert_eq!(
            app.plan
                .currencies
                .iter()
                .find(|c| c.id == token.id)
                .unwrap(),
            &token
        );
        click_text(&mut app, &ctx, size, "TOK (Ethereum)");
        click_text(&mut app, &ctx, size, "Crypto Token");
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        keyboard(&mut app, &ctx, size, egui::Key::Enter);
        let saved = app
            .plan
            .currencies
            .iter()
            .find(|c| c.id == token.id)
            .unwrap();
        assert!(saved.address.is_empty() && saved.chain_id.is_empty());
        assert_eq!(app.plan.income.last().unwrap().currency_id, token.id);
        click_text(&mut app, &ctx, size, "TOK");
        click_text(&mut app, &ctx, size, "Delete…");
        keyboard(&mut app, &ctx, size, egui::Key::Enter);
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(
            text_rect(
                &output,
                "This value is in use. Update the linked records before deleting it."
            )
            .is_some()
        );
        assert!(app.plan.currencies.iter().any(|c| c.id == token.id));
        keyboard(&mut app, &ctx, size, egui::Key::Escape);
        assert!(app.tab.currency_editor.is_some());
        keyboard(&mut app, &ctx, size, egui::Key::Escape);
        assert!(app.tab.currency_editor.is_none());
    }
}

#[test]
fn exchange_selectors_show_cross_rates_and_swap_without_changing_reporting() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        app.plan.fx_rates = [("EUR", 1.2), ("GBP", 1.5)]
            .map(|(currency, rate)| ExchangeRate {
                id: id(),
                currency: currency.into(),
                date: "2026-01-01".into(),
                rate: Amount::Number(rate),
                ..Default::default()
            })
            .into();
        app.plan.currencies.push(Currency {
            id: id(),
            name: "GBP".into(),
            ..Default::default()
        });
        app.plan.normalize();
        app.tab.view = FinanceView::Rates;
        let original = app.plan.clone();
        click_text(&mut app, &ctx, size, "USD");
        click_text(&mut app, &ctx, size, "GBP");
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "1 EUR = 0.8 GBP").is_some());
        assert_eq!(FinanceView::Rates.name(), "Exchange");
        let swap = ctx
            .read_response(egui::Id::new("exchange-swap"))
            .unwrap()
            .rect;
        click(&mut app, &ctx, size, swap.center());
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "1 GBP = 1.25 EUR").is_some());
        assert_eq!(app.plan, original);
        assert!(text_rect(&output, "USD Reference Rates").is_none());
        assert!(text_rect(&output, "USD per Unit").is_none());
        assert!(text_rect(&output, "+ Manual Rate").is_none());
        assert!(text_rect(&output, "Refresh Rates").is_some());
        assert!(app.tab.drafts.is_empty());
    }
}

#[test]
fn finance_defaults_keep_transactions_unfiltered_and_wallet_on_current_month() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(1440.0, 900.0);
    app.plan.income[0].date = "2026-01-10".into();
    app.plan.income[1].date = "2026-02-10".into();
    let output = frame(&mut app, &ctx, size, vec![]);
    assert!(text_rect(&output, "Euro Job").is_some() && text_rect(&output, "Dollar Job").is_some());
    assert!(
        app.tab.month.is_empty()
            && app.tab.search.is_empty()
            && app.tab.selected_currencies.is_empty()
            && !app.tab.actual
    );
    app.tab.view = FinanceView::Wallet;
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    assert!(app.tab.month.is_empty());
    assert_eq!(app.tab.wallet_month, month());
    app.tab.open_filters();
    let _ = frame(&mut app, &ctx, size, vec![]);
    let _ = frame(&mut app, &ctx, size, vec![]);
    let output = frame(&mut app, &ctx, size, vec![]);
    assert!(text_rect(&output, &month_label(&month())).is_some());
    assert!(text_rect(&output, "All dates").is_none());
    keyboard(&mut app, &ctx, size, egui::Key::Escape);
    app.tab.month = "2026-01".into();
    app.tab.wallet_month = "2020-01".into();
    app.tab.actual = true;
    app.tab.search = "Euro".into();
    app.tab.selected_currencies.insert("EUR".into());
    app.tab.open_filters();
    click_text(&mut app, &ctx, size, "Clear Filters");
    assert!(
        app.tab.month.is_empty()
            && app.tab.search.is_empty()
            && app.tab.selected_currencies.is_empty()
            && !app.tab.actual
    );
    assert_eq!(app.tab.wallet_month, month());
    app.tab.month = "2026-01".into();
    app.tab.actual = true;
    app.tab.search = "Euro".into();
    app.tab.selected_currencies.insert("EUR".into());
    app.tab.reset();
    assert_eq!(app.tab.wallet_month, month());
    assert!(
        app.tab.month.is_empty()
            && app.tab.search.is_empty()
            && app.tab.selected_currencies.is_empty()
            && !app.tab.actual
    );
}

#[test]
fn wallet_month_changes_transaction_totals_but_not_savings() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(1440.0, 1200.0);
    app.plan.income.clear();
    for (date, amount, received) in [
        (today(), 120.0, true),
        (today(), 80.0, false),
        ("2020-01-10".into(), 900.0, true),
    ] {
        let mut row = Income::new("EUR");
        row.date = date;
        row.gross_amount = Amount::Number(amount);
        row.received = received;
        app.plan.income.push(row);
    }
    for (date, amount) in [(today(), 30.0), ("2020-01-10".into(), 200.0)] {
        let mut row = Expense::new("EUR");
        row.due_date = date;
        row.amount = Amount::Number(amount);
        row.applicable = true;
        app.plan.expenses.push(row);
    }
    app.plan.savings = vec![
        Saving {
            id: id(),
            amount: Amount::Number(1200.0),
            currency: "EUR".into(),
            ..Default::default()
        },
        Saving {
            id: id(),
            amount: Amount::Number(500.0),
            currency: "EUR".into(),
            deduct: true,
            ..Default::default()
        },
    ];
    app.plan.normalize();
    app.tab.month = "2025-06".into();
    app.tab.view = FinanceView::Wallet;
    for _ in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![]);
    }
    let output = frame(&mut app, &ctx, size, vec![]);
    for amount in ["200.00", "30.00", "170.00", "1,200.00 EUR"] {
        assert!(
            text_rect(&output, amount).is_some(),
            "Missing monthly value {amount}"
        );
    }
    assert_eq!(app.tab.wallet_month, month());
    app.tab.actual = true;
    let output = frame(&mut app, &ctx, size, vec![]);
    assert!(text_rect(&output, "120.00").is_some());
    assert!(text_rect(&output, "90.00").is_some());
    app.tab.actual = false;
    select_month(&mut app, &ctx, size, "2020-01", "January 2020");
    let output = frame(&mut app, &ctx, size, vec![]);
    for amount in ["900.00", "200.00", "700.00", "1,200.00 EUR"] {
        assert!(
            text_rect(&output, amount).is_some(),
            "Missing selected-month value {amount}"
        );
    }
    assert_eq!(app.tab.month, "2025-06");
    app.tab.open_filters();
    click_text(&mut app, &ctx, size, "Clear Filters");
    assert_eq!(app.tab.wallet_month, month());
    keyboard(&mut app, &ctx, size, egui::Key::Escape);
    assert_eq!(
        app.tab.wallet_month, "2020-01",
        "Escape must restore the previous Wallet period"
    );
}

#[test]
fn income_country_dropdown_searches_catalogue_and_rejects_arbitrary_values() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        app.plan.income.truncate(1);
        let key = app.plan.income[0].id.clone();
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let cell = ctx
            .read_response(egui::Id::new(("finance-table-cell", &key, 5usize)))
            .unwrap()
            .rect;
        click(&mut app, &ctx, size, cell.center());
        click_text(&mut app, &ctx, size, "Search Countries…");
        assert!(egui::Popup::is_any_open(&ctx));
        let _ = frame(
            &mut app,
            &ctx,
            size,
            vec![egui::Event::Text("Ukraine".into())],
        );
        click_text(&mut app, &ctx, size, "Ukraine");
        assert!(!egui::Popup::is_any_open(&ctx));
        let Record::Income(row) = &app.tab.drafts[&key].record else {
            panic!()
        };
        assert_eq!(row.tax_country, "Ukraine");
        row_action(&mut app, &ctx, size, &key, "finance-row-save");
        assert_eq!(app.plan.income[0].tax_country, "Ukraine");
        assert_eq!(app.plan.income[0].tax_country_id, "country-UA");
        let mut row = app.plan.income[0].clone();
        row.tax_country = "Atlantis".into();
        row.tax_country_id.clear();
        assert!(
            inline::validate(&Record::Income(row), &app.plan)
                .unwrap_err()
                .contains("country")
        );
        app.tab.view = FinanceView::Storage;
        app.tab.storage_unit = 5;
        app.tab.search = "Ukraine".into();
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Ukraine").is_some() && text_rect(&output, "UA").is_some());
        assert!(app.tab.drafts.is_empty());
    }
}

#[test]
fn computed_values_and_dates_are_centered_beneath_their_headers() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        let mut income = Income::new("USD");
        income.date = "2026-10-18".into();
        income.gross_amount = Amount::Number(120.0);
        income.applied_tax = Amount::Number(10.0);
        income.tax_country = "Ukraine".into();
        let key = income.id.clone();
        app.plan.income = vec![income];
        app.plan.fx_rates = vec![ExchangeRate {
            date: app.plan.income[0].audit.date().into(),
            currency: "EUR".into(),
            rate: Amount::Number(1.2),
            ..Default::default()
        }];
        app.plan.normalize();
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let _ = frame(&mut app, &ctx, size, vec![]);
        let country_width = ctx
            .read_response(egui::Id::new(("finance-table-cell", &key, 5usize)))
            .unwrap()
            .rect
            .width();
        let country_label = if country_width < 86.0 {
            "UA"
        } else {
            "Ukraine"
        };
        for (column, text) in [
            (0usize, "2026-10-18"),
            (4, "USD"),
            (5, country_label),
            (8, "108.00 USD"),
            (9, "90.00 EUR"),
            (10, "100.00 EUR"),
        ] {
            reveal_table_cell(&mut app, &ctx, size, &key, column);
            let output = frame(&mut app, &ctx, size, vec![]);
            let cell = ctx
                .read_response(egui::Id::new(("finance-table-cell", &key, column)))
                .unwrap()
                .rect;
            fn in_cell(shape: &egui::Shape, text: &str, cell: egui::Rect) -> Option<egui::Rect> {
                match shape {
                    egui::Shape::Text(t) if t.galley.text() == text => {
                        let rect = t.galley.rect.translate(t.pos.to_vec2());
                        cell.expand(0.5).contains_rect(rect).then_some(rect)
                    }
                    egui::Shape::Vec(shapes) => {
                        shapes.iter().find_map(|shape| in_cell(shape, text, cell))
                    }
                    _ => None,
                }
            }
            let value = output
                .shapes
                .iter()
                .find_map(|shape| in_cell(&shape.shape, text, cell))
                .unwrap_or_else(|| panic!("Missing {text} inside {cell:?}"));
            assert!(
                (value.center().x - cell.center().x).abs() < 2.0,
                "Misaligned {text}: {value:?}, {cell:?}"
            );
        }
    }
}

#[test]
fn money_formatting_suppresses_rounded_negative_zero_and_keeps_real_negatives() {
    for value in [-0.0, -0.00000000001, -0.0049, 0.0, 0.004] {
        assert_eq!(money(value, "EUR"), "0.00 EUR");
    }
    assert_eq!(money(-0.006, "EUR"), "−0.01 EUR");
    assert_eq!(money(-1234.56, "USD"), "−1,234.56 USD");
    assert_eq!(money(1234.56, ""), "1,234.56");
}

#[test]
fn expense_and_savings_categories_are_separate_settings_owned_dropdowns() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        for (view, name, column) in [
            (FinanceView::Expenses, "Travel", 4usize),
            (FinanceView::Savings, "Reserve", 4usize),
        ] {
            app.tab.view = view;
            for _ in 0..3 {
                let _ = frame(&mut app, &ctx, size, vec![]);
            }
            let settings = ctx
                .read_response(egui::Id::new("vault-settings"))
                .unwrap()
                .rect;
            click(&mut app, &ctx, size, settings.center());
            assert!(app.tab.category_settings.is_some());
            click_text(&mut app, &ctx, size, "+ Category");
            app.tab
                .category_settings
                .as_mut()
                .unwrap()
                .categories
                .last_mut()
                .unwrap()
                .name = name.into();
            click_text(&mut app, &ctx, size, "Save Settings");
            assert!(app.tab.category_settings.is_none());
            let record = if view == FinanceView::Expenses {
                Record::Expense(Expense {
                    title: "Trip".into(),
                    ..Expense::new("EUR")
                })
            } else {
                Record::Saving(Saving {
                    id: id(),
                    currency: "EUR".into(),
                    ..Default::default()
                })
            };
            let key = record.id().to_string();
            app.tab.edit_record(record, true);
            for _ in 0..3 {
                let _ = frame(&mut app, &ctx, size, vec![]);
            }
            let cell = ctx
                .read_response(egui::Id::new(("finance-table-cell", &key, column)))
                .unwrap()
                .rect;
            click(&mut app, &ctx, size, cell.center());
            click_text(&mut app, &ctx, size, name);
            row_action(&mut app, &ctx, size, &key, "finance-row-save");
            assert!(app.tab.drafts.is_empty());
            app.tab.configure_categories(&app.plan);
            app.tab.category_settings.as_mut().unwrap().categories[0].name =
                format!("{name} Renamed");
            click_text(&mut app, &ctx, size, "Vault Settings…");
            assert!(app.tab.settings.is_some() && app.tab.category_settings.is_some());
            for _ in 0..3 {
                let _ = frame(&mut app, &ctx, size, vec![]);
            }
            keyboard(&mut app, &ctx, size, egui::Key::Escape);
            assert!(app.tab.settings.is_none() && app.tab.category_settings.is_some());
            assert_eq!(
                app.tab.category_settings.as_ref().unwrap().categories[0].name,
                format!("{name} Renamed")
            );
            app.tab
                .category_settings
                .as_mut()
                .unwrap()
                .categories
                .clear();
            click_text(&mut app, &ctx, size, "Save Settings");
            assert!(app.tab.category_settings.is_some());
            click_text(&mut app, &ctx, size, "Cancel");
        }
        assert_eq!(app.plan.categories[0].name, "Travel");
        assert_eq!(app.plan.savings_categories[0].name, "Reserve");
        assert_eq!(app.plan.savings[0].category, "Reserve");
        let mut bad = app.plan.expenses[0].clone();
        bad.category = "Unlisted".into();
        bad.category_id.clear();
        assert!(
            inline::validate(&Record::Expense(bad), &app.plan)
                .unwrap_err()
                .contains("category")
        );
        let mut bad = app.plan.savings[0].clone();
        bad.category = "Travel".into();
        bad.category_id.clear();
        assert!(inline::validate(&Record::Saving(bad), &app.plan).is_err());
    }
}

#[test]
fn budget_row_controls_expand_independently_and_keep_hidden_expense_edits() {
    for width in [980.0, 1440.0] {
        let (mut app, ctx) = harness();
        let size = egui::vec2(width, 900.0);
        for index in 0..2 {
            app.plan.budgets.push(Budget {
                id: format!("budget-{index}"),
                label: format!("Budget {index}"),
                currency: "EUR".into(),
                amount: Amount::Number(1000.0),
                audit: RecordAudit::new(),
                expenses: vec![BudgetExpense {
                    id: format!("expense-{index}"),
                    label: format!("Expense {index}"),
                    currency: "EUR".into(),
                    amount: Amount::Number(50.0),
                    audit: RecordAudit::new(),
                    ..Default::default()
                }],
                ..Default::default()
            });
        }
        app.plan.normalize();
        let saved = app.plan.clone();
        app.tab.view = FinanceView::Budgets;
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        for key in ["budget-0", "budget-1"] {
            let actions = ctx
                .read_response(egui::Id::new(("finance-table-cell", key, 5usize)))
                .unwrap()
                .rect;
            for action in ["budget-expenses-toggle", "budget-expense-add"] {
                let button = ctx
                    .read_response(egui::Id::new((action, key)))
                    .unwrap()
                    .rect;
                assert!(
                    actions.expand(0.5).contains_rect(button),
                    "{width} {action}: actions={actions:?}, button={button:?}"
                );
                assert!(button.left() >= 252.0 && button.right() <= width - 28.0);
            }
        }
        assert!(app.tab.expanded_budgets.is_empty());
        assert!(
            ctx.read_response(egui::Id::new(("finance-cell", "expense-0", "label")))
                .is_none()
        );
        row_action(&mut app, &ctx, size, "budget-0", "budget-expenses-toggle");
        for _ in 0..2 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        assert!(app.tab.expanded_budgets.contains("budget-0"));
        assert!(!app.tab.expanded_budgets.contains("budget-1"));
        assert!(
            ctx.read_response(egui::Id::new(("finance-cell", "expense-0", "label")))
                .is_some()
        );
        assert!(
            ctx.read_response(egui::Id::new(("finance-cell", "expense-1", "label")))
                .is_none()
        );
        assert!(app.tab.drafts.is_empty());
        assert_eq!(app.plan, saved);
        // The plus button opens its own budget without changing another list.
        row_action(&mut app, &ctx, size, "budget-1", "budget-expense-add");
        let Record::Budget(draft) = &app.tab.drafts["budget-1"].record else {
            panic!()
        };
        let added = draft.expenses[1].id.clone();
        assert_eq!(draft.expenses[1].currency, "EUR");
        assert!(app.tab.expanded_budgets.contains("budget-1"));
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(egui::Id::new(("finance-cell", &added, "label")))
        );
        assert_eq!(app.plan, saved);
        replace_cell(&mut app, &ctx, size, &added, "label", "New expense");
        replace_cell(&mut app, &ctx, size, &added, "amount", "75");
        row_action(&mut app, &ctx, size, "budget-1", "budget-expenses-toggle");
        assert!(!app.tab.expanded_budgets.contains("budget-1"));
        row_action(&mut app, &ctx, size, "budget-1", "finance-row-save");
        assert_eq!(
            app.plan
                .budgets
                .iter()
                .find(|row| row.id == "budget-1")
                .unwrap()
                .expenses
                .len(),
            2
        );
        assert!(app.tab.drafts.is_empty());
        let saved = app.plan.clone();
        // Keyboard activation adds one unsaved item, without also saving the row.
        ctx.memory_mut(|memory| {
            memory.request_focus(egui::Id::new(("budget-expense-add", "budget-1")))
        });
        keyboard(&mut app, &ctx, size, egui::Key::Enter);
        let Record::Budget(draft) = &app.tab.drafts["budget-1"].record else {
            panic!()
        };
        assert_eq!(draft.expenses.len(), 3);
        assert_eq!(app.plan, saved);
        row_action(&mut app, &ctx, size, "budget-1", "finance-row-cancel");
        assert!(app.tab.drafts.is_empty());
        assert_eq!(app.plan, saved);
    }
}

#[test]
fn budgets_edit_nested_expenses_and_recalculate_with_saved_currency_rates() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        app.plan.fx_rates.push(ExchangeRate {
            id: id(),
            date: RecordAudit::new().date().into(),
            currency: "EUR".into(),
            rate: Amount::Number(1.1),
            ..Default::default()
        });
        let budget = Budget {
            id: id(),
            label: "Trip to Lisbon".into(),
            currency: "EUR".into(),
            amount: Amount::Number(1000.0),
            ..Default::default()
        };
        let key = budget.id.clone();
        app.tab.edit_record(Record::Budget(budget), true);
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        row_action(&mut app, &ctx, size, &key, "budget-expense-add");
        let Record::Budget(budget) = &app.tab.drafts[&key].record else {
            panic!()
        };
        let first = budget.expenses[0].id.clone();
        replace_cell(&mut app, &ctx, size, &first, "label", "Hotel");
        replace_cell(&mut app, &ctx, size, &first, "amount", "100");
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        row_action(&mut app, &ctx, size, &key, "budget-expense-add");
        let Record::Budget(budget) = &app.tab.drafts[&key].record else {
            panic!()
        };
        let second = budget.expenses[1].id.clone();
        replace_cell(&mut app, &ctx, size, &second, "label", "Flight");
        replace_cell(&mut app, &ctx, size, &second, "amount", "110");
        let currency = ctx
            .read_response(egui::Id::new(("finance-table-cell", &second, 2usize)))
            .unwrap()
            .rect;
        click(&mut app, &ctx, size, currency.center());
        click_text(&mut app, &ctx, size, "USD");
        row_action(&mut app, &ctx, size, &key, "finance-row-save");
        assert!(app.tab.drafts.is_empty());
        assert_eq!(app.plan.budgets[0].expenses.len(), 2);
        assert!(
            app.plan.budgets[0]
                .expenses
                .iter()
                .all(|expense| !expense.paid)
        );
        assert_eq!(app.plan.budget_spent(&app.plan.budgets[0]), Some(0.0));
        assert_eq!(app.plan.totals(&month(), false).1, 0.0);
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Paid").is_some());
        for (expense_id, expected) in [(&first, 100.0), (&second, 200.0)] {
            let paid = ctx
                .read_response(egui::Id::new(("finance-table-cell", expense_id, 4usize)))
                .unwrap()
                .rect;
            click(&mut app, &ctx, size, paid.center());
            let Record::Budget(draft) = &app.tab.drafts[&key].record else {
                panic!()
            };
            assert!(
                draft
                    .expenses
                    .iter()
                    .find(|expense| expense.id == *expense_id)
                    .unwrap()
                    .paid
            );
            let _ = frame(&mut app, &ctx, size, vec![]);
            row_action(&mut app, &ctx, size, &key, "finance-row-save");
            assert!((app.plan.budget_spent(&app.plan.budgets[0]).unwrap() - expected).abs() < 1e-9);
            assert!((app.plan.totals(&month(), true).1 - expected).abs() < 1e-9);
        }
        assert!((app.plan.budget_spent(&app.plan.budgets[0]).unwrap() - 200.0).abs() < 1e-9);
        assert!(app.plan.expense_labels.is_empty());
        app.plan.fx_rates[0].rate = Amount::Number(9.9);
        app.plan.normalize();
        assert!((app.plan.budget_spent(&app.plan.budgets[0]).unwrap() - 200.0).abs() < 1e-9);
        replace_cell(&mut app, &ctx, size, &first, "amount", "200");
        keyboard(&mut app, &ctx, size, egui::Key::Enter);
        assert!((app.plan.budget_spent(&app.plan.budgets[0]).unwrap() - 300.0).abs() < 1e-9);
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "700.00 EUR").is_some());
        let paid = ctx
            .read_response(egui::Id::new(("finance-table-cell", &second, 4usize)))
            .unwrap()
            .rect;
        click(&mut app, &ctx, size, paid.center());
        let _ = frame(&mut app, &ctx, size, vec![]);
        row_action(&mut app, &ctx, size, &key, "finance-row-cancel");
        assert!(app.plan.budgets[0].expenses[1].paid);
        for _ in 0..2 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let paid = ctx
            .read_response(egui::Id::new(("finance-table-cell", &second, 4usize)))
            .unwrap()
            .rect;
        click(&mut app, &ctx, size, paid.center());
        let _ = frame(&mut app, &ctx, size, vec![]);
        row_action(&mut app, &ctx, size, &key, "finance-row-save");
        assert!(!app.plan.budgets[0].expenses[1].paid);
        assert_eq!(app.plan.budget_spent(&app.plan.budgets[0]), Some(200.0));
        assert!((app.plan.totals(&month(), false).1 - 200.0).abs() < 1e-9);
    }
}

#[test]
fn comments_open_a_large_multiline_popup_and_cancel_or_apply_without_saving_other_rows() {
    let (mut app, ctx) = harness();
    let size = egui::vec2(980.0, 640.0);
    let comment = "A long comment with details that do not fit in a table cell.\nSecond paragraph.";
    let saving = Saving {
        id: id(),
        source: "Bank".into(),
        currency: "EUR".into(),
        comment: comment.into(),
        ..Default::default()
    };
    let key = saving.id.clone();
    app.plan.savings.push(saving);
    app.plan.normalize();
    app.tab.view = FinanceView::Savings;
    for apply in [false, true] {
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let cell = ctx
            .read_response(egui::Id::new(("finance-table-cell", &key, 5usize)))
            .unwrap()
            .rect;
        click(&mut app, &ctx, size, cell.center());
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let input_id = egui::Id::new(("finance-comment-editor", &key));
        let editor = ctx.read_response(input_id).unwrap().rect;
        assert!(editor.width() > 300.0 && editor.height() > 150.0);
        ctx.memory_mut(|m| m.request_focus(input_id));
        let _ = frame(
            &mut app,
            &ctx,
            size,
            vec![egui::Event::Text("More text".into())],
        );
        keyboard(&mut app, &ctx, size, egui::Key::Enter);
        assert!(egui::Popup::is_any_open(&ctx));
        assert_eq!(app.plan.savings[0].comment, comment);
        if apply {
            click_text(&mut app, &ctx, size, "Apply Comment");
            assert!(!egui::Popup::is_any_open(&ctx));
            assert!(app.tab.drafts.contains_key(&key));
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
            keyboard(&mut app, &ctx, size, egui::Key::Enter);
            assert!(app.plan.savings[0].comment.contains("More text"));
            assert!(app.tab.drafts.is_empty());
        } else {
            keyboard(&mut app, &ctx, size, egui::Key::Escape);
            assert_eq!(app.plan.savings[0].comment, comment);
            assert!(app.tab.drafts.is_empty());
        }
    }
}

#[test]
fn wallet_history_range_extends_through_real_dropdown_and_preserves_filters() {
    for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        app.tab.view = FinanceView::Wallet;
        app.plan.income.truncate(1);
        app.plan.income[0].date = "2020-01-01".into();
        for _ in 0..3 {
            let _ = frame(&mut app, &ctx, size, vec![]);
        }
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Income & Expenses Chart").is_some());
        assert_eq!(app.tab.history_range, crate::HistoryRange::LastMonths(12));
        let plot = ctx
            .read_response(egui::Id::new("wallet-history-plot"))
            .unwrap()
            .rect;
        assert!(plot.left() >= 252.0 && plot.right() <= size.x - 28.0);
        let _ = frame(
            &mut app,
            &ctx,
            size,
            vec![egui::Event::PointerMoved(plot.center())],
        );
        let mut hover = frame(&mut app, &ctx, size, vec![]);
        for _ in 0..45 {
            hover = frame(&mut app, &ctx, size, vec![]);
        }
        assert!(text_rect(&hover, "Income: 0.00 EUR").is_some());
        assert!(text_rect(&hover, "Expenses: 0.00 EUR").is_some());
        click_text(&mut app, &ctx, size, "Last 12 months");
        click_text(&mut app, &ctx, size, "Last 24 months");
        assert_eq!(app.tab.history_range, crate::HistoryRange::LastMonths(24));
        click_text(&mut app, &ctx, size, "Last 24 months");
        click_text(&mut app, &ctx, size, "All history");
        assert_eq!(app.tab.history_range, crate::HistoryRange::All);
        let saved = app.plan.clone();
        let wallet_month = app.tab.wallet_month.clone();
        click_text(&mut app, &ctx, size, "All history");
        click_text(&mut app, &ctx, size, "Custom range");
        assert_eq!(app.tab.history_range, crate::HistoryRange::Custom);
        assert_eq!(app.tab.history_from, "2020-01-01");
        assert_eq!(app.tab.history_to, today());
        for name in ["wallet-history-from", "wallet-history-to"] {
            let rect = ctx
                .read_response(egui::Id::new((name, "input")))
                .unwrap()
                .rect;
            assert!(rect.left() >= 252.0 && rect.right() <= size.x - 28.0);
            assert!(rect.bottom() < size.y);
        }
        replace_input(
            &mut app,
            &ctx,
            size,
            egui::Id::new(("wallet-history-from", "input")),
            "2026-01-10",
        );
        replace_input(
            &mut app,
            &ctx,
            size,
            egui::Id::new(("wallet-history-to", "input")),
            "2026-02-05",
        );
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "10 Jan 2026 – 05 Feb 2026 · EUR · Planned").is_some());
        let calendar = ctx
            .read_response(egui::Id::new(("wallet-history-from", "calendar")))
            .unwrap()
            .rect;
        click(&mut app, &ctx, size, calendar.center());
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "January 2026").is_some());
        keyboard(&mut app, &ctx, size, egui::Key::Escape);
        replace_input(
            &mut app,
            &ctx,
            size,
            egui::Id::new(("wallet-history-from", "input")),
            "2026-03-01",
        );
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "From must be on or before To.").is_some());
        replace_input(
            &mut app,
            &ctx,
            size,
            egui::Id::new(("wallet-history-from", "input")),
            "invalid",
        );
        let output = frame(&mut app, &ctx, size, vec![]);
        assert!(text_rect(&output, "Enter valid From and To dates (YYYY-MM-DD).").is_some());
        replace_input(
            &mut app,
            &ctx,
            size,
            egui::Id::new(("wallet-history-from", "input")),
            "2026-01-10",
        );
        click_text(&mut app, &ctx, size, "Custom range");
        click_text(&mut app, &ctx, size, "Last 12 months");
        assert_eq!(app.tab.history_range, crate::HistoryRange::LastMonths(12));
        click_text(&mut app, &ctx, size, "Last 12 months");
        click_text(&mut app, &ctx, size, "Custom range");
        assert_eq!(app.tab.history_from, "2026-01-10");
        assert_eq!(app.tab.history_to, "2026-02-05");
        assert_eq!(app.tab.wallet_month, wallet_month);
        assert_eq!(app.plan, saved);
        assert!(app.tab.month.is_empty() && app.tab.search.is_empty());
        assert!(app.tab.selected_currencies.is_empty());
        app.plan.meta.reporting_currency.clear();
        app.plan.meta.reporting_currency_id.clear();
        app.plan.normalize();
        let output = frame(&mut app, &ctx, size, vec![]);
        assert_eq!(app.plan.meta.reporting_currency, "EUR");
        assert!(text_rect(&output, "Income & Expenses Chart").is_some());
    }
}
