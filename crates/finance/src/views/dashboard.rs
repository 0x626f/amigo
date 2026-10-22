use super::*;

pub(super) fn wallet(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    app.state.wallet_month = canonical_month(&app.state.wallet_month).unwrap_or_else(month);
    let selected_month = app.state.wallet_month.clone();
    let period = month_label(&selected_month);
    let p = colors(ui);
    let (income, expense, missing) = app.plan.totals(&selected_month, app.state.actual);
    let currency = app.plan.meta.reporting_currency.clone();
    ui.columns(3, |columns| {
        for (column, label, value, color, icon) in [
            (0, "Net income", income, p.green, Icon::Income),
            (1, "Expenses", expense, p.red, Icon::Expenses),
            (2, "Remaining", income - expense, p.accent, Icon::Wallet),
        ] {
            panel(&columns[column])
                .fill(if column == 2 { p.blue_tint } else { p.card })
                .show(&mut columns[column], |ui| {
                    ui.spacing_mut().interact_size.y = 18.0;
                    ui.spacing_mut().item_spacing.y = 6.0;
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        hint(ui, label);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            design::icon(ui, icon, color, 20.0);
                        });
                    });
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(if reports(app) {
                            money(value, "")
                        } else {
                            "—".into()
                        })
                        .size(30.0)
                        .strong()
                        .color(if column == 2 { p.accent } else { p.ink }),
                    );
                    ui.horizontal(|ui| {
                        hint(ui, &currency);
                        ui.label(
                            RichText::new(format!(
                                "· {} · {period}",
                                if app.state.actual {
                                    "Actual"
                                } else {
                                    "Planned"
                                }
                            ))
                            .size(11.0)
                            .color(p.muted),
                        );
                    });
                });
        }
    });
    if !reports(app) {
        hint(ui, "Choose a reporting currency to compare your balances.");
    } else if missing > 0 {
        ui.colored_label(
            p.red,
            format!("{missing} records have missing rates and are excluded from these totals."),
        );
    }
    ui.add_space(20.0);
    super::history::show(app, ui);
    ui.add_space(20.0);
    ui.columns(2, |columns| {
        panel(&columns[0]).show(&mut columns[0], |ui| {
            ui.spacing_mut().interact_size.y = 18.0;
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.set_min_width(ui.available_width());
            ui.label(
                RichText::new("Monthly Income & Expenses")
                    .strong()
                    .size(16.0),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                balance_ring(
                    ui,
                    if reports(app) { income } else { 0.0 },
                    if reports(app) { expense } else { 0.0 },
                );
                ui.vertical(|ui| {
                    ui.add_space(28.0);
                    for (label, amount, color) in
                        [("Income", income, p.green), ("Spending", expense, p.accent)]
                    {
                        ui.horizontal(|ui| {
                            design::dot(ui, color);
                            hint(ui, label);
                        });
                        ui.label(
                            RichText::new(if reports(app) {
                                money(amount, &currency)
                            } else {
                                "—".into()
                            })
                            .strong()
                            .size(15.0),
                        );
                        ui.add_space(8.0);
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        design::Button::new(RichText::new("View income").color(p.accent))
                            .frame(false),
                    )
                    .clicked()
                {
                    app.state.view = FinanceView::Income;
                    app.state.month = selected_month.clone();
                }
                if ui
                    .add(
                        design::Button::new(RichText::new("View expenses").color(p.accent))
                            .frame(false),
                    )
                    .clicked()
                {
                    app.state.view = FinanceView::Expenses;
                    app.state.month = selected_month.clone();
                }
            });
        });
        panel(&columns[1])
            .fill(p.violet_tint)
            .show(&mut columns[1], |ui| {
                ui.spacing_mut().interact_size.y = 18.0;
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.set_min_width(ui.available_width());
                ui.set_min_height(248.0);
                let violet = p.violet;
                ui.horizontal(|ui| {
                    design::icon(ui, Icon::Savings, violet, 22.0);
                    ui.label(RichText::new("Your savings").strong().size(16.0));
                });
                ui.add_space(22.0);
                let mut missing = 0;
                let total: f64 = app
                    .plan
                    .savings
                    .iter()
                    .filter(|r| !r.deduct)
                    .map(|r| {
                        app.plan
                            .record_value(r.amount.value(), &r.currency, &r.audit, &today())
                            .unwrap_or_else(|| {
                                missing += 1;
                                0.0
                            })
                    })
                    .sum();
                ui.label(
                    RichText::new(if reports(app) {
                        money(total, &currency)
                    } else {
                        "—".into()
                    })
                    .size(32.0)
                    .strong()
                    .color(violet),
                );
                let accounts = app.plan.savings.iter().filter(|r| !r.deduct).count();
                hint(ui, &format!("Across {accounts} savings balances"));
                if missing > 0 && reports(app) {
                    ui.colored_label(
                        p.red,
                        format!("{missing} balances excluded: missing rates."),
                    );
                }
                ui.add_space(20.0);
                if ui
                    .add(
                        design::Button::new(RichText::new("Manage savings").color(violet))
                            .fill(p.card)
                            .stroke(egui::Stroke::NONE),
                    )
                    .clicked()
                {
                    app.state.view = FinanceView::Savings;
                }
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Balances marked Deduct are excluded.")
                        .size(10.0)
                        .color(p.muted),
                );
            });
    });
}

fn balance_ring(ui: &mut egui::Ui, income: f64, expense: f64) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(184.0, 176.0), egui::Sense::hover());
    let center = rect.center();
    let radius = 65.0;
    ui.painter()
        .circle_stroke(center, radius, egui::Stroke::new(15.0, colors(ui).column));
    let total = income.max(expense).max(0.0);
    if total > 0.0 {
        for (from, to, color) in [
            (0.0, (expense / total).clamp(0.0, 1.0), colors(ui).accent),
            ((expense / total).clamp(0.0, 1.0), 1.0, colors(ui).green),
        ] {
            if to - from > 0.005 {
                let points = (0..=80)
                    .map(|i| {
                        let angle = std::f32::consts::TAU
                            * (from + (to - from) * i as f64 / 80.0) as f32
                            - std::f32::consts::FRAC_PI_2;
                        center + egui::Vec2::angled(angle) * radius
                    })
                    .collect();
                ui.painter()
                    .add(egui::Shape::line(points, egui::Stroke::new(15.0, color)));
            }
        }
    }
    let ratio = if income > 0.0 {
        (expense / income * 100.0).clamp(0.0, 999.0)
    } else {
        0.0
    };
    ui.painter().text(
        center - egui::Vec2::new(0.0, 8.0),
        egui::Align2::CENTER_CENTER,
        format!("{ratio:.0}%"),
        egui::FontId::proportional(28.0),
        colors(ui).ink,
    );
    ui.painter().text(
        center + egui::Vec2::new(0.0, 20.0),
        egui::Align2::CENTER_CENTER,
        "of income spent",
        egui::FontId::proportional(10.0),
        colors(ui).muted,
    );
}
