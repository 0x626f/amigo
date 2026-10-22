use crate::model::*;
use serde_json::json;

#[test]
fn task_priority_migration_preserves_existing_tasks() {
    let mut workspace = Workspace::default();
    for (index, priority) in ["Normal", "normal", "", "High", "Low"].iter().enumerate() {
        let mut task = Task::new(index as u32 + 1, "todo");
        task.title = format!("Existing task {index}");
        task.priority = (*priority).into();
        workspace.tasks.push(task);
    }
    let original = workspace.tasks.clone();
    workspace.normalize();
    for (index, task) in workspace.tasks.iter().enumerate() {
        let mut expected = original[index].clone();
        if index < 3 {
            expected.priority = "Medium".into();
        }
        assert_eq!(*task, expected);
    }
    assert_eq!(Task::new(6, "todo").priority, "Medium");
}

fn plan() -> FinancePlan {
    let mut p = FinancePlan::empty();
    p.meta.reporting_currency = "EUR".into();
    for (date, currency, rate) in [
        ("2026-01-01", "EUR", 1.1),
        ("2026-03-01", "EUR", 1.2),
        ("2026-01-01", "BTC", 50_000.0),
    ] {
        p.fx_rates.push(ExchangeRate {
            date: date.into(),
            currency: currency.into(),
            rate: Amount::Number(rate),
            ..ExchangeRate::default()
        });
    }
    p.normalize();
    p
}
#[test]
fn reporting_currency_defaults_to_eur_for_new_and_previously_disabled_vaults() {
    assert_eq!(FinancePlan::empty().meta.reporting_currency, "EUR");
    let plan = plan_from_value(
        json!({"meta":{"reportingCurrency":"", "reportingCurrencyId":"old-currency"}}),
    )
    .unwrap();
    assert_eq!(plan.meta.reporting_currency, "EUR");
    assert_eq!(
        plan.currencies
            .iter()
            .find(|currency| currency.id == plan.meta.reporting_currency_id)
            .unwrap()
            .name,
        "EUR"
    );
    let plan = plan_from_value(json!({"meta":{"reportingCurrency":"USD"}})).unwrap();
    assert_eq!(plan.meta.reporting_currency, "USD");
}

#[test]
fn income_tax_and_fee_are_percentages_of_gross() {
    let income = Income {
        gross_amount: Amount::Text("1000".into()),
        applied_tax: Amount::Number(20.0),
        withdraw_fee: Amount::Text("2.5".into()),
        ..Income::default()
    };
    assert_eq!(income.net(), 775.0);
}

#[test]
fn accounts_migrate_to_labels_and_keep_shared_reference_ids() {
    let mut plan = plan_from_value(json!({
        "meta":{"name":"Old vault"}, "incomeAccounts":[{"id":"bank","name":"Main Bank"}],
        "accounts":[{"id":"bank","label":"Main Bank","country":"UA","principal":"Old holder","comment":"Old details"}],
        "income":[{"account":"Main Bank","accountId":"bank","currency":"EUR"}],
        "savings":[{"source":"Main Bank","sourceId":"bank","currency":"EUR"}]
    }))
    .unwrap();
    assert_eq!(plan.accounts.len(), 1);
    assert_eq!(plan.accounts[0].id, "bank");
    plan.income_accounts[0].name = "Renamed Bank".into();
    plan.normalize();
    assert_eq!(plan.accounts.len(), 1);
    assert_eq!(plan.accounts[0].label, "Renamed Bank");
    let value = serde_json::to_value(&plan.accounts[0]).unwrap();
    for removed in ["country", "principal", "comment"] {
        assert!(value.get(removed).is_none());
    }
    assert_eq!(plan.income[0].account, "Renamed Bank");
    assert_eq!(plan.savings[0].source, "Renamed Bank");
    assert_eq!(
        plan_from_value(serde_json::to_value(&plan).unwrap()).unwrap(),
        plan
    );
}
#[test]
fn conversion_uses_usd_reference_historical_and_future_fallback() {
    let p = plan();
    assert_eq!(p.convert(100.0, "EUR", "2026-02-01"), Some(100.0));
    assert!((p.convert(110.0, "USD", "2026-02-01").unwrap() - 100.0).abs() < 1e-8);
    assert!((p.convert(120.0, "USDC", "2026-04-01").unwrap() - 100.0).abs() < 1e-8);
    assert_eq!(p.convert(100.0, "EURC", "2026-04-01"), Some(100.0));
    assert!((p.convert(1.0, "BTC", "2026-02-01").unwrap() - 50_000.0 / 1.1).abs() < 1e-8);
    assert!(p.conversion("USD", "2025-01-01").unwrap().fallback);
    assert!(!p.conversion("USD", "2026-04-01").unwrap().fallback);
    assert!(p.convert(100.0, "XYZ", "2026-04-01").is_none());
}
#[test]
fn wallet_actual_planned_and_recurring_start_month() {
    let mut p = plan();
    p.income.push(Income {
        date: "2026-02-01".into(),
        gross_amount: Amount::Number(1000.0),
        currency: "EUR".into(),
        received: true,
        ..Income::default()
    });
    p.income.push(Income {
        date: "2026-02-15".into(),
        gross_amount: Amount::Number(200.0),
        currency: "EUR".into(),
        ..Income::default()
    });
    for (date, regular, applicable, amount) in [
        ("2026-01-10", true, true, 100.0),
        ("2026-02-10", false, false, 50.0),
        ("2026-03-10", true, true, 20.0),
    ] {
        p.expenses.push(Expense {
            due_date: date.into(),
            regular,
            applicable,
            amount: Amount::Number(amount),
            currency: "EUR".into(),
            ..Expense::default()
        });
    }
    assert_eq!(p.totals("2026-02", false), (1200.0, 150.0, 0));
    assert_eq!(p.totals("2026-02", true), (1000.0, 100.0, 0));
    assert_eq!(p.totals("2025-12", false), (0.0, 0.0, 0));
    p.expenses.push(Expense {
        due_date: "2026-02-10".into(),
        currency: "UNKNOWN".into(),
        amount: Amount::Number(1.0),
        ..Expense::default()
    });
    assert_eq!(p.totals("2026-02", false).2, 1);
}
#[test]
fn budget_spending_and_wallet_totals_include_only_paid_expenses() {
    let mut p = plan_from_value(json!({
        "meta": {"reportingCurrency":"EUR"},
        "budgets": [{"id":"trip", "label":"Trip", "currency":"EUR", "amount":500,
            "expenses": [
                {"id":"paid", "label":"Hotel", "currency":"EUR", "amount":100, "paid":true,
                    "audit":{"createdAt":"2026-09-10T12:00:00Z"}},
                {"id":"legacy", "label":"Taxi", "currency":"EUR", "amount":200,
                    "audit":{"createdAt":"2026-10-01T12:00:00Z"}},
                {"id":"missing-fx", "label":"Flight", "currency":"XYZ", "amount":50, "paid":false,
                    "audit":{"createdAt":"2026-09-10T12:00:00Z"}}
            ]}]
    }))
    .unwrap();
    assert!(!p.budgets[0].expenses[1].paid);
    assert_eq!(p.budget_spent(&p.budgets[0]), Some(100.0));
    for actual in [false, true] {
        assert_eq!(p.totals("2026-09", actual), (0.0, 100.0, 0));
        assert_eq!(p.totals("2026-10", actual), (0.0, 0.0, 0));
        assert_eq!(p.totals("", actual), (0.0, 100.0, 0));
        let date = chrono::NaiveDate::from_ymd_opt(2026, 9, 10).unwrap();
        assert_eq!(p.totals_between(date, date, actual), (0.0, 100.0, 0));
    }
    p.budgets[0].expenses[1].paid = true;
    assert_eq!(p.budget_spent(&p.budgets[0]), Some(300.0));
    assert_eq!(p.totals("2026-10", true), (0.0, 200.0, 0));
    // An unpaid item with missing FX must not block Spent or count as missing.
    p.budgets[0].expenses[2].paid = true;
    assert_eq!(p.budget_spent(&p.budgets[0]), None);
    assert_eq!(p.totals("2026-09", false), (0.0, 100.0, 1));
    p.budgets[0].expenses[0].paid = false;
    p.budgets[0].expenses[2].paid = false;
    assert_eq!(p.budget_spent(&p.budgets[0]), Some(200.0));
    assert_eq!(p.totals("2026-09", false), (0.0, 0.0, 0));
    assert_eq!(p.totals("", false), (0.0, 200.0, 0));
    let reopened = plan_from_value(serde_json::to_value(&p).unwrap()).unwrap();
    assert_eq!(reopened, p);
    assert_eq!(reopened.budget_spent(&reopened.budgets[0]), Some(200.0));
}

#[test]
fn inclusive_date_totals_cover_boundaries_recurrence_cancellation_budgets_and_actual() {
    let mut p = plan();
    let date = |value: &str| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap();
    p.income = [
        ("2026-01-09", 999.0, true),
        ("2026-01-10", 100.0, true),
        ("2026-02-05", 200.0, false),
        ("2026-02-06", 999.0, true),
        ("", 999.0, true),
    ]
    .into_iter()
    .map(|(date, amount, received)| Income {
        date: date.into(),
        currency: "EUR".into(),
        gross_amount: Amount::Number(amount),
        received,
        ..Default::default()
    })
    .collect();
    p.expenses = vec![Expense {
        due_date: "2025-12-31".into(),
        currency: "EUR".into(),
        amount: Amount::Number(10.0),
        regular: true,
        cancelled_on: "2026-02-20".into(),
        ..Default::default()
    }];
    p.expenses.extend(
        [
            ("2026-01-10", "EUR", 20.0, true),
            ("2026-02-05", "EUR", 30.0, false),
            ("2026-02-05", "XYZ", 5.0, false),
            ("2026-02-06", "XYZ", 999.0, true),
        ]
        .into_iter()
        .map(|(date, currency, amount, applicable)| Expense {
            due_date: date.into(),
            currency: currency.into(),
            amount: Amount::Number(amount),
            applicable,
            ..Default::default()
        }),
    );
    p.budgets = vec![Budget {
        expenses: [
            ("2026-01-09", 999.0),
            ("2026-01-10", 7.0),
            ("2026-02-05", 8.0),
            ("2026-02-06", 999.0),
        ]
        .into_iter()
        .map(|(date, amount)| BudgetExpense {
            paid: true,
            currency: "EUR".into(),
            amount: Amount::Number(amount),
            audit: RecordAudit {
                created_at: format!("{date}T00:00:00Z"),
                ..Default::default()
            },
            ..Default::default()
        })
        .collect(),
        ..Default::default()
    }];
    assert_eq!(
        p.totals_between(date("2026-01-10"), date("2026-02-05"), false),
        (300.0, 75.0, 1)
    );
    assert_eq!(
        p.totals_between(date("2026-01-10"), date("2026-02-05"), true),
        (100.0, 45.0, 0)
    );
    assert_eq!(
        p.totals_between(date("2026-01-10"), date("2026-01-10"), false),
        (100.0, 27.0, 0)
    );
    assert_eq!(
        p.totals_between(date("2026-02-05"), date("2026-02-05"), false),
        (200.0, 38.0, 1)
    );
    assert_eq!(
        p.totals_between(date("2026-02-05"), date("2026-02-05"), true),
        (0.0, 8.0, 0)
    );
    assert_eq!(
        p.totals_between(date("2026-02-05"), date("2026-01-10"), false),
        (0.0, 0.0, 0)
    );
    for actual in [true, false] {
        assert_eq!(
            p.totals_between(date("2026-01-01"), date("2026-01-31"), actual),
            p.totals("2026-01", actual)
        );
    }
}

#[test]
fn regular_expenses_apply_automatically_from_their_from_month() {
    let mut p = plan();
    let mut regular = Expense::new("EUR");
    regular.regular = true;
    regular.applicable = false;
    regular.due_date = "2026-10-31".into();
    regular.amount = Amount::Number(100.0);
    let key = regular.id.clone();
    assert_eq!(
        regular.occurrence_date("2026-10").as_deref(),
        Some("2026-10-31")
    );
    assert_eq!(
        regular.occurrence_date("2026-11").as_deref(),
        Some("2026-11-30")
    );
    assert_eq!(
        regular.occurrence_date("2027-02").as_deref(),
        Some("2027-02-28")
    );
    assert_eq!(
        regular.occurrence_date("2028-02").as_deref(),
        Some("2028-02-29")
    );
    assert!(regular.occurrence_date("2026-09").is_none());
    p.expenses.push(regular);
    for actual in [false, true] {
        assert_eq!(p.totals("2026-09", actual).1, 0.0);
        assert_eq!(p.totals("2026-10", actual).1, 100.0);
        assert_eq!(p.totals("2026-11", actual).1, 100.0);
    }
    assert_eq!(
        p.expense_months("2026-12"),
        ["2026-10", "2026-11", "2026-12"]
    );
    p.remove_expense(&key, "2026-10").unwrap();
    assert_eq!(p.expense_months("2026-12"), ["2026-10"]);
    assert!(p.expenses[0].occurrence_date("2026-11").is_none());
    assert_eq!(p.totals("2026-10", true).1, 100.0);
    assert_eq!(p.totals("2026-11", true).1, 0.0);
}

#[test]
fn regular_expense_removal_preserves_history_and_monthly_expenses_do_not_repeat() {
    let mut p = plan();
    let mut regular = Expense::new("EUR");
    regular.title = "Rent".into();
    regular.due_date = "2026-09-30".into();
    regular.amount = Amount::Number(100.0);
    regular.regular = true;
    regular.applicable = true;
    let mut monthly = Expense::new("EUR");
    monthly.title = "October purchase".into();
    monthly.due_date = "2026-10-15".into();
    monthly.amount = Amount::Number(50.0);
    monthly.applicable = true;
    p.expenses = vec![regular.clone(), monthly.clone()];
    for actual in [false, true] {
        assert_eq!(p.totals("2026-08", actual).1, 0.0);
        assert_eq!(p.totals("2026-09", actual).1, 100.0);
        assert_eq!(p.totals("2026-10", actual).1, 150.0);
        assert_eq!(p.totals("2026-11", actual).1, 100.0);
        assert_eq!(p.totals("2027-01", actual).1, 100.0);
    }
    p.remove_expense(&regular.id, "2026-10").unwrap();
    for actual in [false, true] {
        assert_eq!(p.totals("2026-09", actual).1, 100.0);
        assert_eq!(p.totals("2026-10", actual).1, 150.0);
        assert_eq!(p.totals("2026-11", actual).1, 0.0);
        assert_eq!(p.totals("2027-01", actual).1, 0.0);
    }
    let retained = &p.expenses[0];
    assert_eq!(retained.regular_until, "2026-10");
    assert_eq!(retained.id, regular.id);
    assert_eq!(retained.audit, regular.audit);
    assert_eq!(retained.amount, regular.amount);
    // Removing an ended series again must never extend its recurrence.
    p.remove_expense(&regular.id, "2026-11").unwrap();
    assert_eq!(p.expenses[0].regular_until, "2026-10");
    p.remove_expense(&monthly.id, "2026-10").unwrap();
    assert_eq!(p.expenses.len(), 1);
    assert_eq!(p.totals("2026-10", false).1, 100.0);
}

#[test]
fn cancelling_expenses_excludes_current_and_future_months_but_preserves_history() {
    let mut p = plan();
    let mut regular = Expense::new("EUR");
    regular.regular = true;
    regular.due_date = "2026-12-31".into();
    regular.amount = Amount::Number(100.0);
    let mut monthly = Expense::new("EUR");
    monthly.due_date = "2027-01-05".into();
    monthly.applicable = true;
    monthly.amount = Amount::Number(50.0);
    let mut past = monthly.clone();
    past.id = id();
    past.due_date = "2026-12-05".into();
    let mut future = monthly.clone();
    future.id = id();
    future.due_date = "2027-02-05".into();
    let keys = [
        regular.id.clone(),
        monthly.id.clone(),
        past.id.clone(),
        future.id.clone(),
    ];
    p.expenses = vec![regular.clone(), monthly, past, future];
    p.normalize();
    let original = p.clone();
    assert!(p.cancel_expense(&keys[0], "2027-02-30").is_err());
    assert!(p.cancel_expense("missing", "2027-01-20").is_err());
    assert_eq!(p, original);
    for key in &keys {
        p.cancel_expense(key, "2027-01-20").unwrap();
        // Repeating cancellation cannot move the cutoff or overwrite its date.
        p.cancel_expense(key, "2027-02-20").unwrap();
        p.cancel_expense(key, "2026-12-20").unwrap();
    }
    for (saved, original) in p.expenses.iter().zip(&original.expenses) {
        let mut expected = original.clone();
        expected.cancelled_on = "2027-01-20".into();
        assert_eq!(*saved, expected);
    }
    for actual in [false, true] {
        assert_eq!(p.totals("2026-11", actual).1, 0.0);
        assert_eq!(p.totals("2026-12", actual).1, 150.0);
        assert_eq!(p.totals("2027-01", actual).1, 0.0);
        assert_eq!(p.totals("2027-02", actual).1, 0.0);
        assert_eq!(p.totals("2028-01", actual).1, 0.0);
        assert_eq!(p.totals("", actual).1, 150.0);
    }
    assert!(p.expenses[0].occurrence_date("2026-12").is_some());
    assert!(p.expenses[0].occurrence_date("2027-01").is_none());
    assert!(p.expenses[0].scheduled_in_month("2027-02"));
    assert!(p.expenses[1].scheduled_in_month("2027-01"));
    assert_eq!(
        p.expense_months("2028-01"),
        ["2026-12", "2027-01", "2027-02"]
    );
    // A future-starting series cancelled before its first occurrence contributes nothing.
    regular.id = id();
    regular.due_date = "2027-03-01".into();
    let key = regular.id.clone();
    p.expenses = vec![regular];
    p.cancel_expense(&key, "2027-01-20").unwrap();
    assert!(p.expense_months("2028-01").is_empty());
    assert_eq!(p.totals("", false).1, 0.0);
    assert_eq!(p.totals("2027-03", false).1, 0.0);
}

#[test]
fn regular_expense_cutoff_handles_year_rollover_and_legacy_records() {
    let mut p = plan_from_value(json!({
        "meta": {"reportingCurrency":"EUR"},
        "expenses": [{"id":"rent", "title":"Rent", "currency":"EUR", "amount":100,
            "dueDate":"2026-11-30", "regular":true, "applicable":true}]
    }))
    .unwrap();
    assert!(p.expenses[0].regular_until.is_empty());
    assert!(p.expenses[0].cancelled_on.is_empty());
    assert_eq!(p.totals("2027-02", false).1, 100.0);
    let original = p.clone();
    assert!(p.remove_expense("rent", "invalid").is_err());
    assert_eq!(p, original);
    p.remove_expense("rent", "2026-12").unwrap();
    assert_eq!(p.totals("2026-11", false).1, 100.0);
    assert_eq!(p.totals("2026-12", false).1, 100.0);
    assert_eq!(p.totals("2027-01", false).1, 0.0);
    let reopened = plan_from_value(serde_json::to_value(&p).unwrap()).unwrap();
    assert_eq!(reopened, p);
    assert_eq!(reopened.totals("2027-01", false).1, 0.0);
    // Cancelling a future-starting regular expense must not create occurrences.
    let mut future = Expense::new("EUR");
    future.due_date = "2027-02-01".into();
    future.regular = true;
    let key = future.id.clone();
    p.expenses.push(future);
    p.remove_expense(&key, "2026-12").unwrap();
    assert!(!p.expenses[1].includes_month("2026-12"));
    assert!(!p.expenses[1].includes_month("2027-02"));
}

#[test]
fn legacy_import_keeps_all_finance_fields_and_resolves_ids() {
    let p = plan_from_value(json!({
        "version":1, "meta":{"name":"Legacy", "reportingCurrency":"eur"},
        "currencies":["EUR", "USD"], "chains":[{"id":"eth", "name":"Ethereum", "rpcUrl":"https://rpc.example"}],
        "currencyTokens":[{"id":"token", "name":"USDC", "chain":"Ethereum", "address":"0x123"}],
        "categories":["Home"], "labels":["Essential"], "incomeAccounts":["Bank"],
        "income":[{"grossAmount":"1000", "appliedTax":200, "withdrawFee":125, "currency":"usdc", "currencyTokenId":"token", "source":"Client", "account":"Bank"}],
        "expenses":[{"title":"Rent", "amount":"500", "currency":"eur", "category":"Home", "labels":["Essential"], "regular":true}],
        "budgets":[{"month":"2026-01", "label":"Essential", "amount":"600"}],
        "savings":[{"source":"Bank", "origin":"Emergency fund", "deduct":true, "currency":"EUR", "amount":100, "labels":["Reserve"]}]
    })).unwrap();
    assert_eq!(p.version, 2);
    assert_eq!(p.meta.reporting_currency, "EUR");
    assert_eq!(p.income[0].currency_id, "token");
    assert_eq!(p.income[0].applied_tax.value(), 20.0);
    assert_eq!(p.income[0].withdraw_fee.value(), 12.5);
    assert_eq!(
        p.currencies
            .iter()
            .find(|r| r.id == "token")
            .unwrap()
            .chain_id,
        "eth"
    );
    assert_eq!(p.expenses[0].category_id, p.categories[0].id);
    assert_eq!(
        p.expenses[0].label_ids,
        vec![p.expense_labels[0].id.clone()]
    );
    assert_eq!(p.budgets[0].label, "Essential");
    assert!(p.budgets[0].label_id.is_empty());
    assert_eq!(p.budgets[0].currency, "EUR");
    assert_eq!(p.savings[0].category, "Reserve");
    assert_eq!(p.savings[0].category_id, p.savings_categories[0].id);
    assert_eq!(p.savings[0].comment, "Emergency fund");
    assert!(p.savings[0].deduct);
    let roundtrip = plan_from_value(serde_json::to_value(&p).unwrap()).unwrap();
    assert_eq!(p, roundtrip);
}
#[test]
fn renaming_reference_preserves_links_without_recreating_old_currency() {
    let mut p = plan();
    p.income.push(Income::new("EUR"));
    p.normalize();
    let id = p.income[0].currency_id.clone();
    p.currencies.iter_mut().find(|r| r.id == id).unwrap().name = "GBP".into();
    p.normalize();
    assert_eq!(p.income[0].currency, "GBP");
    assert_eq!(p.meta.reporting_currency, "GBP");
    assert!(!p.currencies.iter().any(|r| r.name == "EUR"));
}
#[test]
fn moving_tasks_reorders_and_preserves_content() {
    let mut w = Workspace {
        tasks: vec![
            Task::new(1, "backlog"),
            Task::new(2, "todo"),
            Task::new(3, "todo"),
        ],
        ..Default::default()
    };
    let a = w.tasks[0].id.clone();
    let c = w.tasks[2].id.clone();
    w.move_task(&a, "todo", Some(&c));
    assert_eq!(
        w.tasks.iter().map(|t| t.number).collect::<Vec<_>>(),
        vec![2, 1, 3]
    );
    assert_eq!(w.tasks[1].status, "todo");
    w.move_task(&a, "done", None);
    assert_eq!(w.tasks[2].status, "done");
    w.move_task(&a, "unknown", None);
    assert_eq!(w.tasks[2].status, "done");
    w.normalize();
    assert_eq!(w.next_task, 4);
}
#[test]
fn rejects_wrong_files_and_bad_field_types() {
    assert!(plan_from_value(json!({"unrelated":true})).is_err());
    assert!(plan_from_value(json!({"version":3,"meta":{}})).is_err());
    assert!(plan_from_value(json!({"income":"bad"})).is_err());
}

#[test]
fn custom_status_order_rename_and_completion_keep_task_references() {
    let mut w = Workspace::default();
    w.tasks.push(Task::new(1, "review"));
    w.task_categories.push(StoredValue {
        id: "work".into(),
        name: "Work".into(),
    });
    w.tasks[0].category_id = "work".into();
    let mut statuses = w.task_statuses.clone();
    statuses.insert(
        2,
        TaskStatus {
            id: "review".into(),
            name: "In review".into(),
            color: StatusColor::Violet,
            completed: false,
        },
    );
    w.update_task_board(statuses, w.task_categories.clone(), &Default::default())
        .unwrap();
    assert!(!w.is_task_complete(&w.tasks[0]));
    let mut statuses = w.task_statuses.clone();
    statuses[2].name = "Approved".into();
    statuses[2].completed = true;
    statuses.swap(0, 2);
    let mut categories = w.task_categories.clone();
    categories[0].name = "Projects".into();
    w.update_task_board(statuses, categories, &Default::default())
        .unwrap();
    assert_eq!(w.task_statuses[0].id, "review");
    assert_eq!(w.tasks[0].status, "review");
    assert_eq!(w.tasks[0].category_id, "work");
    assert!(w.is_task_complete(&w.tasks[0]));
    let task = w.tasks[0].id.clone();
    w.move_task(&task, "todo", None);
    assert_eq!(w.tasks[0].status, "todo");
    assert_eq!(w.tasks[0].category_id, "work");
    assert!(!w.is_task_complete(&w.tasks[0]));
    w.normalize();
    let restored = crate::storage::workspace_from_value(serde_json::to_value(&w).unwrap()).unwrap();
    assert_eq!(restored, w);
}
#[test]
fn deleting_used_status_requires_reassignment_and_keeps_tasks() {
    use std::collections::BTreeMap;
    let mut w = Workspace::default();
    let mut task = Task::new(1, "todo");
    task.title = "Keep this task".into();
    task.category_id = "work".into();
    w.tasks.push(task);
    w.task_categories.push(StoredValue {
        id: "work".into(),
        name: "Work".into(),
    });
    let statuses: Vec<_> = w
        .task_statuses
        .iter()
        .filter(|s| s.id != "todo" && s.id != "in-progress")
        .cloned()
        .collect();
    let original = w.clone();
    assert!(
        w.update_task_board(statuses.clone(), vec![], &BTreeMap::new())
            .is_err()
    );
    assert_eq!(w, original);
    let cyclic = BTreeMap::from([
        ("todo".into(), "in-progress".into()),
        ("in-progress".into(), "todo".into()),
    ]);
    assert!(
        w.update_task_board(statuses.clone(), vec![], &cyclic)
            .is_err()
    );
    assert_eq!(w, original);
    let reassignments = BTreeMap::from([
        ("todo".into(), "in-progress".into()),
        ("in-progress".into(), "done".into()),
    ]);
    w.update_task_board(statuses, vec![], &reassignments)
        .unwrap();
    assert_eq!(w.tasks.len(), 1);
    assert_eq!(w.tasks[0].id, original.tasks[0].id);
    assert_eq!(w.tasks[0].title, "Keep this task");
    assert_eq!(w.tasks[0].status, "done");
    assert!(w.tasks[0].category_id.is_empty());
    assert!(
        w.update_task_board(vec![], vec![], &BTreeMap::new())
            .is_err()
    );
}
#[test]
fn invalid_board_names_do_not_mutate_workspace() {
    let mut w = Workspace::default();
    let original = w.clone();
    let mut statuses = w.task_statuses.clone();
    statuses[0].name = "  ".into();
    assert!(
        w.update_task_board(statuses, vec![], &Default::default())
            .is_err()
    );
    assert_eq!(w, original);
    let mut statuses = w.task_statuses.clone();
    statuses[0].name = " to DO ".into();
    assert!(
        w.update_task_board(statuses, vec![], &Default::default())
            .is_err()
    );
    assert_eq!(w, original);
    let categories = vec![
        StoredValue {
            id: "a".into(),
            name: "Work".into(),
        },
        StoredValue {
            id: "b".into(),
            name: " work ".into(),
        },
    ];
    assert!(
        w.update_task_board(w.task_statuses.clone(), categories, &Default::default())
            .is_err()
    );
    assert_eq!(w, original);
}
#[test]
fn legacy_task_statuses_migrate_without_losing_metadata() {
    let legacy = json!({"version":1,"darkMode":true,"nextTask":9,"tasks":[
        {"id":"a","number":1,"title":"Backlog task","status":0,"labels":["home"],"description":"Notes"},
        {"id":"b","number":2,"title":"Todo task","status":1,"priority":"High"},
        {"id":"c","number":3,"title":"Active task","status":2,"dueDate":"2026-10-15"},
        {"id":"d","number":4,"title":"Completed task","status":3}
    ],"finance":{"version":2,"meta":{"name":"Preserved finance"}}});
    let w = crate::storage::workspace_from_value(legacy).unwrap();
    assert_eq!(w.version, 2);
    assert!(w.dark_mode);
    assert_eq!(w.next_task, 9);
    assert_eq!(w.task_statuses.len(), 4);
    assert!(w.task_categories.is_empty());
    for (task, status) in w.tasks.iter().zip(DEFAULT_STATUS_IDS) {
        assert_eq!(task.status, status);
        assert!(task.category_id.is_empty());
    }
    assert_eq!(w.tasks[0].description, "Notes");
    assert_eq!(w.tasks[0].labels, vec!["home"]);
    assert_eq!(w.tasks[1].priority, "High");
    assert_eq!(w.tasks[2].due_date, "2026-10-15");
    assert_eq!(w.finance.meta.name, "Preserved finance");
    assert_eq!(
        crate::storage::workspace_from_value(serde_json::to_value(&w).unwrap()).unwrap(),
        w
    );
}

#[test]
fn finance_dates_normalize_without_losing_invalid_values() {
    let mut plan = FinancePlan::empty();
    plan.income = vec![Income {
        date: "2026-9-2".into(),
        ..Default::default()
    }];
    plan.expenses = vec![
        Expense {
            due_date: "2026-10-3".into(),
            ..Default::default()
        },
        Expense {
            due_date: "bad date".into(),
            ..Default::default()
        },
    ];
    plan.budgets = vec![Budget {
        month: "2026-9".into(),
        ..Default::default()
    }];
    plan.fx_rates = vec![ExchangeRate {
        date: "2026-9-1".into(),
        ..Default::default()
    }];
    plan.normalize();
    assert_eq!(plan.income[0].date, "2026-09-02");
    assert_eq!(plan.expenses[0].due_date, "2026-10-03");
    assert_eq!(plan.expenses[1].due_date, "bad date");
    assert_eq!(plan.fx_rates[0].date, "2026-09-01");
    assert_eq!(plan.budgets[0].month, "2026-09");
    assert_eq!(month_key("2026-9-2"), "2026-09");
    assert_eq!(month_key("invalid"), "");
}

#[test]
fn exchange_pairs_are_independent_of_reporting_and_use_dated_reference_rates() {
    let mut plan = FinancePlan::empty();
    plan.fx_rates = vec![
        ExchangeRate {
            currency: "EUR".into(),
            date: "2026-09-01".into(),
            rate: Amount::Number(1.2),
            ..Default::default()
        },
        ExchangeRate {
            currency: "GBP".into(),
            date: "2026-09-01".into(),
            rate: Amount::Number(1.5),
            ..Default::default()
        },
        ExchangeRate {
            currency: "EUR".into(),
            date: "2026-10-01".into(),
            rate: Amount::Number(0.0),
            ..Default::default()
        },
        ExchangeRate {
            currency: "EUR".into(),
            date: "2026-11-01".into(),
            rate: Amount::Number(1.8),
            ..Default::default()
        },
    ];
    let pair = plan.exchange_rate("EUR", "GBP", "2026-10-18").unwrap();
    assert!((pair.rate - 0.8).abs() < 1e-10);
    assert!(!pair.fallback);
    assert!((plan.exchange_rate("GBP", "EUR", "2026-10-18").unwrap().rate - 1.25).abs() < 1e-10);
    assert_eq!(
        plan.exchange_rate("USDC", "USD", "2026-10-18")
            .unwrap()
            .rate,
        1.0
    );
    assert_eq!(
        plan.exchange_rate("XYZ", "XYZ", "2026-10-18").unwrap().rate,
        1.0
    );
    assert!(plan.exchange_rate("XYZ", "EUR", "2026-10-18").is_none());
    assert!(plan.exchange_rate("", "EUR", "2026-10-18").is_none());
    assert!(
        plan.exchange_rate("EUR", "GBP", "2026-08-01")
            .unwrap()
            .fallback
    );
    plan.meta.reporting_currency = "JPY".into();
    plan.meta.reporting_currency_id.clear();
    assert!((plan.exchange_rate("EUR", "GBP", "2026-10-18").unwrap().rate - 0.8).abs() < 1e-10);
}

#[test]
fn default_reference_catalogue_is_complete_stable_and_preserves_custom_entries() {
    let mut plan = FinancePlan::empty();
    assert_eq!(plan, FinancePlan::empty());
    assert_eq!(plan.currencies.len(), 7);
    assert_eq!(plan.tax_countries.len(), 249);
    assert_eq!(
        plan.currencies.iter().filter(|c| c.name == "USDT").count(),
        2
    );
    let ethereum = plan
        .chains
        .iter()
        .find(|c| c.name == "Ethereum Mainnet")
        .unwrap();
    let eth = plan.currencies.iter().find(|c| c.name == "ETH").unwrap();
    assert!(eth.native_asset && eth.address.is_empty());
    assert_eq!(eth.chain_id, ethereum.id);
    assert!(
        plan.currencies
            .iter()
            .filter(|c| c.name == "USDT" || c.name == "USDC")
            .all(|c| !c.chain_id.is_empty() && !c.address.is_empty() && !c.native_asset)
    );
    let original = plan.clone();
    plan.normalize();
    assert_eq!(plan, original);
    plan.currencies.retain(|c| c.name != "ETH");
    plan.normalize();
    assert!(!plan.currencies.iter().any(|c| c.name == "ETH"));
    let restored = plan_from_value(serde_json::to_value(&plan).unwrap()).unwrap();
    assert_eq!(restored, plan);
    let legacy = plan_from_value(json!({"meta":{}, "chains":[{"id":"custom-eth", "name":"Ethereum", "rpcUrl":"https://custom.test"}], "currencies":[{"id":"legacy-eur", "name":"EUR"}], "taxCountries":[{"id":"legacy-country", "name":"UK"}], "income":[{"currency":"EUR", "taxCountry":"UK", "taxCountryId":"legacy-country"}]})).unwrap();
    assert_eq!(
        legacy
            .currencies
            .iter()
            .find(|c| c.name == "EUR")
            .unwrap()
            .id,
        "legacy-eur"
    );
    assert_eq!(
        legacy
            .chains
            .iter()
            .find(|c| c.id == "custom-eth")
            .unwrap()
            .rpc_url,
        "https://custom.test"
    );
    assert_eq!(legacy.income[0].tax_country, "United Kingdom");
    assert_eq!(legacy.income[0].tax_country_id, "legacy-country");
    assert_eq!(
        legacy
            .tax_countries
            .iter()
            .filter(|c| c.name == "United Kingdom")
            .count(),
        1
    );
}

#[test]
fn country_catalogue_has_unique_codes_and_recognizes_names_and_aliases() {
    use crate::reference_data::{countries, country};
    assert_eq!(countries().len(), 249);
    let codes: std::collections::BTreeSet<_> = countries().iter().map(|c| c.code).collect();
    assert_eq!(codes.len(), 249);
    for (query, code) in [
        ("Ukraine", "UA"),
        ("ua", "UA"),
        ("USA", "US"),
        ("United Kingdom", "GB"),
        ("Turkey", "TR"),
        ("Türkiye", "TR"),
        ("TW", "TW"),
        ("South Sudan", "SS"),
    ] {
        assert_eq!(country(query).unwrap().code, code);
    }
    assert!(country("Atlantis").is_none());
    assert!(country("").is_none());
    assert!(!codes.contains("AN"));
}

#[test]
fn all_date_totals_include_every_record_once_and_keep_actual_selection_explicit() {
    let mut plan = FinancePlan::empty();
    plan.meta.reporting_currency = "EUR".into();
    plan.income = vec![
        Income {
            date: "2026-01-01".into(),
            currency: "EUR".into(),
            gross_amount: Amount::Number(100.0),
            received: true,
            ..Default::default()
        },
        Income {
            date: "2026-02-01".into(),
            currency: "EUR".into(),
            gross_amount: Amount::Number(200.0),
            ..Default::default()
        },
    ];
    plan.expenses = [
        ("2026-01-01", 20.0, true),
        ("2026-02-01", 40.0, false),
        ("", 10.0, false),
    ]
    .map(|(date, amount, applicable)| Expense {
        due_date: date.into(),
        amount: Amount::Number(amount),
        currency: "EUR".into(),
        applicable,
        regular: true,
        ..Default::default()
    })
    .into();
    assert_eq!(plan.totals("", false), (300.0, 70.0, 0));
    assert_eq!(plan.totals("", true), (100.0, 70.0, 0));
    assert_eq!(plan.totals("2026-02", false), (200.0, 60.0, 0));
}
