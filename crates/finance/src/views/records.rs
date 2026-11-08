use super::*;

pub(super) fn income(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    if take_create(app) {
        edit(
            app,
            Record::Income(Income::new(&app.plan.meta.reporting_currency)),
            true,
        );
    }
    let rows: Vec<_> = app
        .plan
        .income
        .iter()
        .filter(|r| {
            currency_matches(app, &r.currency)
                && (app.state.month.is_empty() || month_key(&r.date) == app.state.month)
                && matches(
                    &app.state.search,
                    &format!(
                        "{} {} {} {}",
                        r.source, r.account, r.tax_country, r.currency
                    ),
                )
        })
        .cloned()
        .collect();
    if rows.is_empty()
        && !app
            .state
            .drafts
            .values()
            .any(|d| d.record.view() == app.state.view)
    {
        empty(
            ui,
            "No income here yet",
            "Use + Income to add a row to this table.",
        );
    }
    inline::table(
        app,
        ui,
        rows.into_iter().map(Record::Income).collect(),
        &[
            "Date",
            "Source",
            "Account",
            "Gross",
            "Currency",
            "Tax Country",
            "Tax %",
            "Fee %",
            "Net",
            "Reported Net",
            "Reported Gross",
            "Received",
        ],
    );
}
pub(super) fn expenses(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    let mut monthly = !app.state.expense_group.is_regular();
    ui.push_id("expense-groups", |ui| {
        design::segmented(ui, &mut monthly, "Regular", "Monthly");
    });
    app.state.expense_group = if monthly {
        crate::ExpenseGroup::Monthly
    } else {
        crate::ExpenseGroup::Regular
    };
    let regular = app.state.expense_group.is_regular();
    hint(
        ui,
        if regular {
            "Included automatically each month from the From date. Cancel excludes this and future months; removal stops inclusion from next month."
        } else {
            "Expenses grouped by month, including automatic regular expenses from their From month."
        },
    );
    ui.add_space(12.0);
    if take_create(app) {
        let mut expense = Expense::new(&app.plan.meta.reporting_currency);
        expense.regular = regular;
        edit(app, Record::Expense(expense), true);
    }
    let mut rows: Vec<_> = app
        .plan
        .expenses
        .iter()
        .filter(|r| {
            r.regular == regular
                && currency_matches(app, &r.currency)
                && r.scheduled_in_month(&app.state.month)
                && (!regular || !app.state.month.is_empty() || r.regular_until.is_empty())
                && matches(
                    &app.state.search,
                    &format!("{} {} {} {}", r.title, r.category, r.priority, r.currency),
                )
        })
        .cloned()
        .map(Record::Expense)
        .collect();
    if !regular {
        let months = if app.state.month.is_empty() {
            app.plan.expense_months(&month())
        } else {
            vec![app.state.month.clone()]
        };
        for expense in app.plan.expenses.iter().filter(|r| {
            r.regular
                && currency_matches(app, &r.currency)
                && matches(
                    &app.state.search,
                    &format!("{} {} {} {}", r.title, r.category, r.priority, r.currency),
                )
        }) {
            let expense = std::sync::Arc::new(expense.clone());
            for month in &months {
                if let Some(date) = expense.occurrence_date(month) {
                    rows.push(Record::RegularExpense(ExpenseOccurrence {
                        id: format!("regular-expense:{}:{month}", expense.id),
                        expense: expense.clone(),
                        date,
                    }));
                }
            }
        }
    }
    if !regular {
        for budget in &app.plan.budgets {
            for expense in &budget.expenses {
                let Some(month) = expense.planned_month() else {
                    continue;
                };
                if (app.state.month.is_empty() || month == app.state.month)
                    && currency_matches(app, &expense.currency)
                    && matches(
                        &app.state.search,
                        &format!(
                            "{} {} {} {}",
                            expense.label, budget.label, expense.comment, expense.currency
                        ),
                    )
                {
                    rows.push(Record::BudgetExpense {
                        budget_id: budget.id.clone(),
                        id: format!("budget-expense:{}:{}", budget.id, expense.id),
                        budget_label: budget.label.clone(),
                        expense: expense.clone(),
                    });
                }
            }
        }
    }
    if rows.is_empty()
        && !app
            .state
            .drafts
            .values()
            .any(|d| matches!(&d.record, Record::Expense(r) if r.regular == regular))
    {
        empty(
            ui,
            "No expenses here yet",
            if regular {
                "Use + Expense to add an automatic monthly expense."
            } else {
                "Use + Expense to add an expense for one month."
            },
        );
    }
    let mut columns = vec![
        "Title",
        if regular { "From" } else { "Date" },
        "Amount",
        "Currency",
        "Category",
        "Priority",
        "Reported",
    ];
    if !regular {
        columns.push("Paid");
    }
    inline::table(app, ui, rows, &columns);
}
pub(super) fn savings(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    if take_create(app) {
        edit(
            app,
            Record::Saving(Saving {
                id: id(),
                audit: RecordAudit::new(),
                currency: app.plan.meta.reporting_currency.clone(),
                ..Saving::default()
            }),
            true,
        );
    }
    let rows: Vec<_> = app
        .plan
        .savings
        .iter()
        .filter(|r| {
            currency_matches(app, &r.currency)
                && matches(
                    &app.state.search,
                    &format!("{} {} {} {}", r.source, r.comment, r.category, r.currency),
                )
        })
        .cloned()
        .collect();
    if rows.is_empty()
        && !app
            .state
            .drafts
            .values()
            .any(|d| d.record.view() == app.state.view)
    {
        empty(
            ui,
            "Your savings, together",
            "Add account balances in their original currencies.",
        );
    }
    inline::table(
        app,
        ui,
        rows.into_iter().map(Record::Saving).collect(),
        &[
            "Account", "Amount", "Currency", "Reported", "Category", "Comment", "Deduct", "Order",
        ],
    );
}
pub(super) fn budgets(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    if take_create(app) {
        edit(
            app,
            Record::Budget(Budget {
                id: id(),
                audit: RecordAudit::new(),
                currency: app.plan.meta.reporting_currency.clone(),
                month: if app.state.month.is_empty() {
                    month()
                } else {
                    app.state.month.clone()
                },
                ..Budget::default()
            }),
            true,
        );
    }
    let rows: Vec<_> = app
        .plan
        .budgets
        .iter()
        .filter(|r| currency_matches(app, &r.currency) && matches(&app.state.search, &r.label))
        .cloned()
        .collect();
    if rows.is_empty()
        && !app
            .state
            .drafts
            .values()
            .any(|d| d.record.view() == app.state.view)
    {
        empty(
            ui,
            "Plan your next trip or project",
            "Create a budget, then use its + button to add expenses.",
        );
    }
    inline::table(
        app,
        ui,
        rows.into_iter().map(Record::Budget).collect(),
        &[
            "Label",
            "Allowance",
            "Currency",
            "Spent",
            "Remaining",
            "Date",
        ],
    );
}
