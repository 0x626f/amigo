use super::*;
use chrono::{Datelike, Months, NaiveDate};

struct Item {
    id: String,
    title: String,
    source: String,
    date: String,
    amount: f64,
    income: bool,
    settled: bool,
    month_only: bool,
    day: u32,
}

fn items(plan: &FinancePlan, month: &str) -> (Vec<Item>, usize) {
    let mut items = Vec::new();
    let mut missing = 0;
    let mut add = |id: String,
                   title: String,
                   source: String,
                   date: String,
                   amount: f64,
                   currency: &str,
                   audit: &RecordAudit,
                   income,
                   settled| {
        let month_only = canonical_month(&date).is_some();
        let parsed = if month_only {
            NaiveDate::parse_from_str(&format!("{date}-01"), "%Y-%m-%d")
        } else {
            NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        };
        let Ok(parsed) = parsed else {
            return;
        };
        if parsed.format("%Y-%m").to_string() != month {
            return;
        }
        let Some(amount) = plan.record_value(amount, currency, audit, &date) else {
            missing += 1;
            return;
        };
        items.push(Item {
            id,
            title,
            source,
            date,
            amount,
            income,
            settled,
            month_only,
            day: parsed.day(),
        });
    };
    for income in &plan.income {
        add(
            format!("income:{}", income.id),
            income.source.clone(),
            "Income".into(),
            income.date.clone(),
            income.net(),
            &income.currency,
            &income.audit,
            true,
            income.received,
        );
    }
    for expense in &plan.expenses {
        if !expense.includes_month(month) {
            continue;
        }
        let date = if expense.regular {
            let Some(date) = expense.occurrence_date(month) else {
                continue;
            };
            date
        } else {
            expense.due_date.clone()
        };
        add(
            format!("expense:{}", expense.id),
            expense.title.clone(),
            if expense.regular {
                "Regular expense"
            } else {
                "Expense"
            }
            .into(),
            date,
            expense.amount.value(),
            &expense.currency,
            &expense.audit,
            false,
            expense.is_paid_in(month),
        );
    }
    for budget in &plan.budgets {
        for expense in &budget.expenses {
            add(
                format!("budget:{}:{}", budget.id, expense.id),
                expense.label.clone(),
                format!("Budget: {}", budget.label),
                expense.date.clone(),
                expense.amount.value(),
                &expense.currency,
                &expense.audit,
                false,
                expense.paid,
            );
        }
    }
    items.sort_by(|a, b| {
        a.day
            .cmp(&b.day)
            .then(b.income.cmp(&a.income))
            .then(a.title.cmp(&b.title))
            .then(a.id.cmp(&b.id))
    });
    (items, missing)
}

fn line(
    painter: &egui::Painter,
    from: egui::Pos2,
    to: egui::Pos2,
    color: egui::Color32,
    solid: bool,
) {
    let stroke = egui::Stroke::new(3.0, color);
    if solid {
        painter.line_segment([from, to], stroke);
    } else {
        let length = to.x - from.x;
        let mut offset = 0.0;
        while offset < length {
            painter.line_segment(
                [
                    from + egui::vec2(offset, 0.0),
                    from + egui::vec2((offset + 5.0).min(length), 0.0),
                ],
                stroke,
            );
            offset += 9.0;
        }
    }
}

pub(super) fn show(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    panel(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            RichText::new("Income & Expenses Timeline")
                .size(18.0)
                .strong(),
        );
        ui.add_space(6.0);
        hint(
            ui,
            &format!(
                "{} · {}",
                month_label(&app.state.wallet_month),
                app.plan.meta.reporting_currency
            ),
        );
        let p = colors(ui);
        ui.horizontal_wrapped(|ui| {
            for (label, color) in [("Income", p.green), ("Expenses", p.red)] {
                design::dot(ui, color);
                ui.label(label);
                ui.add_space(8.0);
            }
            for (label, solid) in [("Paid / Received", true), ("Pending", false)] {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(30.0, 16.0), egui::Sense::hover());
                line(
                    ui.painter(),
                    rect.left_center(),
                    rect.right_center(),
                    p.muted,
                    solid,
                );
                ui.label(label);
                ui.add_space(8.0);
            }
        });
        if !reports(app) {
            hint(ui, "Choose a reporting currency to see this timeline.");
            return;
        }
        let (items, missing) = items(app.plan, &app.state.wallet_month);
        if missing > 0 {
            ui.colored_label(
                p.red,
                format!("{missing} items excluded: missing exchange rates."),
            );
        }
        if items.is_empty() {
            ui.add_space(12.0);
            hint(ui, "No dated income or expenses for this month.");
            return;
        }
        let Ok(start) =
            NaiveDate::parse_from_str(&format!("{}-01", app.state.wallet_month), "%Y-%m-%d")
        else {
            return;
        };
        let Some(end) = start
            .checked_add_months(Months::new(1))
            .and_then(|date| date.pred_opt())
        else {
            return;
        };
        let days = end.day();
        let label_width = (ui.available_width() * 0.25).clamp(110.0, 220.0);
        let amount_width = 110.0;
        let (axis, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), egui::Sense::hover());
        let chart_left = axis.left() + label_width;
        let chart_right = (axis.right() - amount_width).max(chart_left + 30.0);
        let x = |day: f32| chart_left + day / days as f32 * (chart_right - chart_left);
        let ticks: Vec<_> = (1..=days)
            .filter(|day| *day == 1 || *day == days || (day % 7 == 0 && days - day > 3))
            .collect();
        for day in &ticks {
            ui.painter().text(
                egui::pos2(x(*day as f32 - 0.5), axis.center().y),
                egui::Align2::CENTER_CENTER,
                day.to_string(),
                egui::FontId::proportional(11.0),
                p.muted,
            );
        }
        for item in &items {
            let (rect, response) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 38.0), egui::Sense::hover());
            ui.interact(
                rect,
                egui::Id::new(("wallet-timeline-item", &item.id)),
                egui::Sense::hover(),
            );
            for day in &ticks {
                let x = x(*day as f32 - 0.5);
                ui.painter()
                    .vline(x, rect.y_range(), egui::Stroke::new(0.5, p.line));
            }
            let label_rect =
                egui::Rect::from_min_max(rect.min, egui::pos2(chart_left - 12.0, rect.bottom()));
            let mut label_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(label_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            let color = if item.income { p.green } else { p.red };
            design::dot(&mut label_ui, color);
            label_ui.add(egui::Label::new(&item.title).truncate());
            let (from, through) = if item.month_only {
                (x(0.0) + 3.0, x(days as f32) - 3.0)
            } else {
                (x(item.day as f32 - 1.0) + 1.0, x(item.day as f32) - 1.0)
            };
            let from = egui::pos2(from, rect.center().y);
            let through = egui::pos2(through.max(from.x + 4.0), rect.center().y);
            line(ui.painter(), from, through, color, item.settled);
            ui.painter().text(
                rect.right_center() - egui::vec2(4.0, 0.0),
                egui::Align2::RIGHT_CENTER,
                money(item.amount, ""),
                egui::FontId::proportional(13.0),
                color,
            );
            response.on_hover_ui(|ui| {
                ui.label(RichText::new(&item.title).strong());
                ui.label(&item.source);
                ui.label(format_planning_date(&item.date));
                ui.label(money(item.amount, &app.plan.meta.reporting_currency));
                ui.label(if item.settled {
                    if item.income { "Received" } else { "Paid" }
                } else if item.income {
                    "To be received"
                } else {
                    "To be paid"
                });
                if item.month_only {
                    hint(ui, "Planned for this month; no specific day selected.");
                }
            });
        }
        hint(
            ui,
            "Month-only plans span the month. Hover an item for details.",
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_uses_status_dates_recurring_months_and_budget_precision() {
        let mut plan = FinancePlan::empty();
        plan.meta.reporting_currency = "EUR".into();
        for (title, received) in [("Received", true), ("Expected", false)] {
            let mut income = Income::new("EUR");
            income.source = title.into();
            income.date = "2026-02-10".into();
            income.gross_amount = Amount::Number(100.0);
            income.received = received;
            plan.income.push(income);
        }
        let mut expense = Expense::new("EUR");
        expense.title = "Rent".into();
        expense.regular = true;
        expense.due_date = "2026-01-31".into();
        expense.amount = Amount::Number(20.0);
        expense.paid_months.push("2026-02".into());
        plan.expenses.push(expense);
        plan.budgets.push(Budget {
            label: "Trip".into(),
            expenses: vec![
                BudgetExpense {
                    label: "Hotel".into(),
                    date: "2026-02".into(),
                    currency: "EUR".into(),
                    amount: Amount::Number(40.0),
                    ..Default::default()
                },
                BudgetExpense {
                    label: "Train".into(),
                    date: "2026-02-15".into(),
                    currency: "EUR".into(),
                    amount: Amount::Number(10.0),
                    paid: true,
                    ..Default::default()
                },
                BudgetExpense {
                    label: "Undated".into(),
                    currency: "EUR".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        let (rows, missing) = items(&plan, "2026-02");
        assert_eq!(missing, 0);
        assert_eq!(rows.len(), 5);
        assert!(
            rows.iter()
                .find(|row| row.title == "Received")
                .unwrap()
                .settled
        );
        assert!(
            !rows
                .iter()
                .find(|row| row.title == "Expected")
                .unwrap()
                .settled
        );
        let rent = rows.iter().find(|row| row.title == "Rent").unwrap();
        assert!(rent.settled);
        assert_eq!(rent.day, 28);
        let hotel = rows.iter().find(|row| row.title == "Hotel").unwrap();
        assert!(hotel.month_only);
        assert!(!hotel.settled);
        assert!(
            rows.iter()
                .find(|row| row.title == "Train")
                .unwrap()
                .settled
        );
        let (next, _) = items(&plan, "2026-03");
        assert_eq!(next.len(), 1);
        assert!(!next[0].settled);
        plan.expenses[0].cancelled_on = "2026-03-01".into();
        assert!(items(&plan, "2026-03").0.is_empty());
    }
}
