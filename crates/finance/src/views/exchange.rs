use super::*;

pub(super) fn show(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    let mut options = app.plan.tracked_currencies();
    options.push("USD".into());
    options.sort();
    options.dedup();
    if !options.contains(&app.state.exchange_base) {
        app.state.exchange_base = if options.contains(&app.plan.meta.reporting_currency) {
            app.plan.meta.reporting_currency.clone()
        } else {
            options
                .iter()
                .find(|c| c.as_str() != "USD")
                .cloned()
                .unwrap_or_else(|| "USD".into())
        };
    }
    if !options.contains(&app.state.exchange_quote) {
        app.state.exchange_quote = "USD".into();
    }
    let width =
        ((ui.available_width() - design::CONTROL_HEIGHT - ui.spacing().item_spacing.x * 2.0) / 2.0)
            .max(80.0);
    ui.horizontal(|ui| {
        selector(
            ui,
            "exchange-base",
            "Base",
            &mut app.state.exchange_base,
            &options,
            width,
        );
        ui.vertical(|ui| {
            ui.add_space(30.0);
            if design::icon_button(
                ui,
                egui::Id::new("exchange-swap"),
                Icon::Rates,
                "Swap Base and Quote",
                design::CONTROL_HEIGHT,
            )
            .clicked()
            {
                std::mem::swap(&mut app.state.exchange_base, &mut app.state.exchange_quote);
            }
        });
        selector(
            ui,
            "exchange-quote",
            "Quote",
            &mut app.state.exchange_quote,
            &options,
            width,
        );
    });
    ui.add_space(24.0);
    match app.plan.exchange_rate(
        &app.state.exchange_base,
        &app.state.exchange_quote,
        &today(),
    ) {
        Some(rate) => {
            let value = if rate.rate.abs() < 0.00000001 || rate.rate.abs() >= 1e12 {
                format!("{:.8e}", rate.rate)
            } else {
                format!("{:.8}", rate.rate)
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string()
            };
            ui.label(
                RichText::new(format!(
                    "{}1 {} = {} {}",
                    if rate.fallback { "≈ " } else { "" },
                    app.state.exchange_base,
                    value,
                    app.state.exchange_quote
                ))
                .size(28.0)
                .strong(),
            );
            if rate.fallback {
                hint(
                    ui,
                    "Uses the earliest later reference rate where historical rates are unavailable.",
                );
            } else {
                hint(
                    ui,
                    "Calculated from the latest saved rates on or before today.",
                );
            }
        }
        None => {
            ui.label(RichText::new("Rate unavailable").size(24.0).strong());
            hint(ui, "Refresh rates to load the selected currency pair.");
        }
    }
}

fn selector(
    ui: &mut egui::Ui,
    id: &str,
    title: &str,
    value: &mut String,
    options: &[String],
    width: f32,
) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 76.0),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| {
            field_label(ui, title);
            egui::ComboBox::from_id_salt(id)
                .width(width)
                .selected_text(value.as_str())
                .show_ui(ui, |ui| {
                    for option in options {
                        ui.selectable_value(value, option.clone(), option);
                    }
                });
        },
    );
}
