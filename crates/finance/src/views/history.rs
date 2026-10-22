use super::*;
use crate::HistoryRange;
use chrono::{Datelike, Months, NaiveDate};

struct MonthTotals {
    month: NaiveDate,
    income: f64,
    expense: f64,
    missing: usize,
}

fn range_start(plan: &FinancePlan, count: Option<u32>, end: NaiveDate) -> NaiveDate {
    let end = end.with_day(1).unwrap();
    count
        .and_then(|count| end.checked_sub_months(Months::new(count.saturating_sub(1))))
        .unwrap_or_else(|| {
            plan.income
                .iter()
                .map(|r| r.date.as_str())
                .chain(plan.expenses.iter().map(|r| r.due_date.as_str()))
                .chain(plan.budgets.iter().flat_map(|b| {
                    b.expenses
                        .iter()
                        .filter(|expense| expense.paid)
                        .map(|e| e.audit.date())
                }))
                .filter_map(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
                .map(|date| date.with_day(1).unwrap())
                .filter(|date| *date <= end)
                .min()
                .unwrap_or_else(|| end.checked_sub_months(Months::new(11)).unwrap_or(end))
        })
}

fn month_end(date: NaiveDate) -> NaiveDate {
    date.with_day(1)
        .unwrap()
        .checked_add_months(Months::new(1))
        .and_then(|next| next.pred_opt())
        .unwrap_or(NaiveDate::MAX)
}

fn series(
    plan: &FinancePlan,
    count: Option<u32>,
    end: NaiveDate,
    actual: bool,
) -> Vec<MonthTotals> {
    series_between(plan, range_start(plan, count, end), month_end(end), actual)
}

fn series_between(
    plan: &FinancePlan,
    from: NaiveDate,
    through: NaiveDate,
    actual: bool,
) -> Vec<MonthTotals> {
    if from > through {
        return Vec::new();
    }
    let mut rows = Vec::new();
    let mut current = from.with_day(1).unwrap();
    while current <= through {
        let last_day = month_end(current);
        let (income, expense, missing) = if from <= current && last_day <= through {
            plan.totals(&current.format("%Y-%m").to_string(), actual)
        } else {
            plan.totals_between(from.max(current), through.min(last_day), actual)
        };
        rows.push(MonthTotals {
            month: current,
            income,
            expense,
            missing,
        });
        let Some(next) = current.checked_add_months(Months::new(1)) else {
            break;
        };
        current = next;
    }
    rows
}

fn custom_dates(from: &str, through: &str) -> Result<(NaiveDate, NaiveDate), &'static str> {
    let from = NaiveDate::parse_from_str(from.trim(), "%Y-%m-%d");
    let through = NaiveDate::parse_from_str(through.trim(), "%Y-%m-%d");
    let (Ok(from), Ok(through)) = (from, through) else {
        return Err("Enter valid From and To dates (YYYY-MM-DD).");
    };
    if from > through {
        return Err("From must be on or before To.");
    }
    Ok((from, through))
}

pub(super) fn show(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    panel(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let previous = app.state.history_range;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Income & Expenses Chart").size(18.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("wallet-history-range")
                    .height(300.0)
                    .selected_text(match app.state.history_range {
                        HistoryRange::LastMonths(count) => format!("Last {count} months"),
                        HistoryRange::All => "All history".into(),
                        HistoryRange::Custom => "Custom range".into(),
                    })
                    .show_ui(ui, |ui| {
                        for count in [12, 24, 36, 60] {
                            ui.selectable_value(&mut app.state.history_range, HistoryRange::LastMonths(count), format!("Last {count} months"));
                        }
                        ui.selectable_value(&mut app.state.history_range, HistoryRange::All, "All history");
                        ui.selectable_value(&mut app.state.history_range, HistoryRange::Custom, "Custom range");
                    });
            });
        });
        let today = chrono::Local::now().date_naive();
        let custom = if app.state.history_range == HistoryRange::Custom {
            if previous != HistoryRange::Custom && app.state.history_from.is_empty() && app.state.history_to.is_empty() {
                let count = match previous { HistoryRange::LastMonths(count) => Some(count), _ => None };
                app.state.history_from = range_start(app.plan, count, today).to_string();
                app.state.history_to = today.to_string();
            }
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                date_field(&mut columns[0], "From", "wallet-history-from", &mut app.state.history_from, false);
                date_field(&mut columns[1], "To", "wallet-history-to", &mut app.state.history_to, false);
            });
            match custom_dates(&app.state.history_from, &app.state.history_to) {
                Ok(dates) => Some(dates),
                Err(message) => {
                    ui.colored_label(colors(ui).red, message);
                    return;
                }
            }
        } else { None };
        if !reports(app) {
            ui.add_space(12.0);
            hint(ui, "Choose a reporting currency in Vault Settings to see this chart.");
            return;
        }
        let rows = if let Some((from, through)) = custom {
            series_between(app.plan, from, through, app.state.actual)
        } else {
            let count = match app.state.history_range { HistoryRange::LastMonths(count) => Some(count), _ => None };
            series(app.plan, count, today, app.state.actual)
        };
        let Some(first) = rows.first() else { return };
        let last = rows.last().unwrap();
        let period = if let Some((from, through)) = custom {
            format!("{} – {}", from.format("%d %b %Y"), through.format("%d %b %Y"))
        } else {
            format!("{} – {}", first.month.format("%b %Y"), last.month.format("%b %Y"))
        };
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (name, color) in [("Income", colors(ui).green), ("Expenses", colors(ui).red)] {
                design::dot(ui, color);
                ui.label(name);
                ui.add_space(10.0);
            }
            hint(ui, &format!("{period} · {} · {}", app.plan.meta.reporting_currency, if app.state.actual { "Actual" } else { "Planned" }));
        });
        ui.add_space(12.0);
        plot(ui, &rows, &app.plan.meta.reporting_currency);
        if rows.iter().any(|row| row.missing > 0) {
            hint(ui, "Some records have missing exchange rates and are excluded. Hover a month for details.");
        }
        if app.plan.income.iter().any(|r| r.date.is_empty()) || app.plan.expenses.iter().any(|r| r.due_date.is_empty()) {
            hint(ui, "Undated records are excluded from the chart.");
        }
    });
}

fn axis_value(value: f64) -> String {
    if value.abs() >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if value.abs() >= 1_000.0 {
        format!("{:.1}k", value / 1_000.0)
    } else if value.abs() < 10.0 {
        let text = money(value, "");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        let text = format!("{value:.0}");
        if text == "-0" { "0".into() } else { text }
    }
}

fn plot(ui: &mut egui::Ui, rows: &[MonthTotals], currency: &str) {
    let height = (ui.ctx().content_rect().height() * 0.28).clamp(180.0, 248.0);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    // Give tests and native integrations a stable rectangle for the entire chart.
    ui.interact(
        rect,
        egui::Id::new("wallet-history-plot"),
        egui::Sense::hover(),
    );
    let area = egui::Rect::from_min_max(
        rect.min + egui::vec2(54.0, 10.0),
        rect.max - egui::vec2(20.0, 34.0),
    );
    let low = rows
        .iter()
        .flat_map(|r| [r.income, r.expense])
        .fold(0.0_f64, f64::min);
    let high = rows
        .iter()
        .flat_map(|r| [r.income, r.expense])
        .fold(0.0_f64, f64::max);
    let span = (high - low).max(1.0);
    let low = if low < 0.0 { low - span * 0.08 } else { 0.0 };
    let high = if high > 0.0 { high + span * 0.08 } else { 1.0 };
    let x = |index: usize| {
        area.left()
            + if rows.len() > 1 {
                index as f32 / (rows.len() - 1) as f32 * area.width()
            } else {
                area.width() * 0.5
            }
    };
    let y = |value: f64| area.bottom() - ((value - low) / (high - low)) as f32 * area.height();
    let palette = colors(ui);
    let painter = ui.painter();
    for tick in 0..=4 {
        let value = low + (high - low) * tick as f64 / 4.0;
        let pos = y(value);
        painter.line_segment(
            [egui::pos2(area.left(), pos), egui::pos2(area.right(), pos)],
            egui::Stroke::new(1.0, palette.line.gamma_multiply(0.7)),
        );
        painter.text(
            egui::pos2(area.left() - 10.0, pos),
            egui::Align2::RIGHT_CENTER,
            axis_value(value),
            egui::FontId::proportional(11.0),
            palette.muted,
        );
    }
    let labels = (area.width() / 80.0).floor().max(2.0) as usize;
    let stride = rows
        .len()
        .saturating_sub(1)
        .div_ceil(labels.saturating_sub(1))
        .max(1);
    for (index, row) in rows.iter().enumerate() {
        // Always include the newest month without placing another label beside it.
        let last = rows.len() - 1;
        if index == last || (index.is_multiple_of(stride) && (last - index >= stride)) {
            painter.text(
                egui::pos2(x(index), area.bottom() + 16.0),
                egui::Align2::CENTER_CENTER,
                row.month.format("%b %y").to_string(),
                egui::FontId::proportional(11.0),
                palette.muted,
            );
        }
    }
    for (income, color) in [(true, palette.green), (false, palette.red)] {
        let points: Vec<_> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                egui::pos2(x(index), y(if income { row.income } else { row.expense }))
            })
            .collect();
        if points.len() > 1 {
            painter.add(egui::Shape::line(
                points.clone(),
                egui::Stroke::new(2.5, color),
            ));
        }
        if points.len() <= 60 {
            for point in points {
                painter.circle_filled(point, 2.5, color);
            }
        }
    }
    if let Some(pointer) = response.hover_pos().filter(|pos| area.contains(*pos)) {
        let index = if rows.len() == 1 {
            0
        } else {
            (((pointer.x - area.left()) / area.width()) * (rows.len() - 1) as f32).round() as usize
        }
        .min(rows.len() - 1);
        let row = &rows[index];
        painter.line_segment(
            [
                egui::pos2(x(index), area.top()),
                egui::pos2(x(index), area.bottom()),
            ],
            egui::Stroke::new(1.0, palette.muted.gamma_multiply(0.6)),
        );
        for (value, color) in [(row.income, palette.green), (row.expense, palette.red)] {
            painter.circle_filled(egui::pos2(x(index), y(value)), 4.5, color);
        }
        response.on_hover_ui(|ui| {
            ui.label(RichText::new(row.month.format("%B %Y").to_string()).strong());
            ui.label(format!("Income: {}", money(row.income, currency)));
            ui.label(format!("Expenses: {}", money(row.expense, currency)));
            ui.label(format!(
                "Net: {}",
                money(row.income - row.expense, currency)
            ));
            if row.missing > 0 {
                ui.label(format!(
                    "{} records excluded: missing exchange rates",
                    row.missing
                ));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_range_filters_days_keeps_empty_months_and_clamps_regular_dates() {
        let mut plan = FinancePlan::empty();
        plan.meta.reporting_currency = "EUR".into();
        plan.expenses.push(Expense {
            due_date: "2026-01-31".into(),
            currency: "EUR".into(),
            amount: Amount::Number(10.0),
            regular: true,
            ..Default::default()
        });
        let (from, through) = custom_dates("2026-01-31", "2026-04-01").unwrap();
        let rows = series_between(&plan, from, through, false);
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows.iter().map(|row| row.expense).collect::<Vec<_>>(),
            [10.0, 10.0, 10.0, 0.0]
        );
        assert_eq!(rows.first().unwrap().month.to_string(), "2026-01-01");
        assert_eq!(rows.last().unwrap().month.to_string(), "2026-04-01");
        let (from, through) = custom_dates("2026-02-28", "2026-02-28").unwrap();
        assert_eq!(series_between(&plan, from, through, true)[0].expense, 10.0);
        assert!(custom_dates("2026-02-29", "2026-03-01").is_err());
        assert!(custom_dates("invalid", "2026-03-01").is_err());
        assert!(custom_dates("", "2026-03-01").is_err());
        assert!(custom_dates("2026-03-02", "2026-03-01").is_err());
        let (from, through) = custom_dates("2027-01-01", "2027-03-31").unwrap();
        plan.expenses.clear();
        let rows = series_between(&plan, from, through, false);
        assert_eq!(rows.len(), 3);
        assert!(
            rows.iter()
                .all(|row| row.income == 0.0 && row.expense == 0.0)
        );
    }

    #[test]
    fn chart_history_ignores_unpaid_budget_expenses() {
        let mut plan = FinancePlan::empty();
        plan.meta.reporting_currency = "EUR".into();
        plan.budgets.push(Budget {
            currency: "EUR".into(),
            expenses: [("2000-01-01", false), ("2026-09-10", true)]
                .into_iter()
                .map(|(date, paid)| BudgetExpense {
                    currency: "EUR".into(),
                    amount: Amount::Number(40.0),
                    paid,
                    audit: RecordAudit {
                        created_at: format!("{date}T00:00:00Z"),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        });
        let end = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap();
        for actual in [false, true] {
            let rows = series(&plan, None, end, actual);
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].month.to_string(), "2026-09-01");
            assert_eq!(rows[0].expense, 40.0);
            assert_eq!(rows[1].expense, 0.0);
        }
        plan.budgets[0].expenses[1].paid = false;
        let rows = series(&plan, None, end, false);
        assert_eq!(rows.len(), 12);
        assert!(
            rows.iter()
                .all(|row| row.expense == 0.0 && row.missing == 0)
        );
    }

    #[test]
    fn monthly_history_handles_gaps_recurring_expenses_actual_and_fx() {
        let mut plan = FinancePlan::empty();
        plan.meta.reporting_currency = "EUR".into();
        plan.income = vec![
            Income {
                date: "2026-08-10".into(),
                currency: "USD".into(),
                gross_amount: Amount::Number(100.0),
                applied_tax: Amount::Number(10.0),
                received: true,
                ..Default::default()
            },
            Income {
                date: "2026-09-10".into(),
                currency: "EUR".into(),
                gross_amount: Amount::Number(200.0),
                ..Default::default()
            },
        ];
        plan.expenses = vec![
            Expense {
                due_date: "2026-08-01".into(),
                currency: "EUR".into(),
                amount: Amount::Number(10.0),
                regular: true,
                applicable: true,
                ..Default::default()
            },
            Expense {
                due_date: "2026-10-01".into(),
                currency: "XYZ".into(),
                amount: Amount::Number(500.0),
                ..Default::default()
            },
        ];
        plan.fx_rates = vec![ExchangeRate {
            date: "2026-01-01".into(),
            currency: "EUR".into(),
            rate: Amount::Number(1.2),
            ..Default::default()
        }];
        let end = NaiveDate::from_ymd_opt(2026, 10, 18).unwrap();
        let rows = series(&plan, Some(12), end, false);
        assert_eq!(rows.len(), 12);
        assert_eq!(rows[0].month.to_string(), "2025-11-01");
        for row in &rows {
            assert_eq!(
                (row.income, row.expense, row.missing),
                plan.totals(&row.month.format("%Y-%m").to_string(), false)
            );
        }
        assert_eq!(rows[8].income, 0.0);
        assert_eq!((rows[9].income, rows[9].expense), (75.0, 10.0));
        assert_eq!((rows[10].income, rows[10].expense), (200.0, 10.0));
        assert_eq!(
            (rows[11].income, rows[11].expense, rows[11].missing),
            (0.0, 10.0, 1)
        );
        let actual = series(&plan, Some(12), end, true);
        assert_eq!(actual[10].income, 0.0);
        assert_eq!(actual[11].missing, 0);
        assert_eq!(series(&plan, None, end, false).len(), 3);
        assert_eq!(series(&plan, Some(24), end, false).len(), 24);
    }
}
