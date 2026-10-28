use super::*;

pub(super) fn list(plan: &FinancePlan, unit: usize) -> &[StoredValue] {
    match unit {
        2 => &plan.categories,
        3 => &plan.clients,
        4 => &plan.income_accounts,
        5 => &plan.tax_countries,
        7 => &plan.savings_categories,
        _ => unreachable!("Unsupported reference catalogue: {unit}"),
    }
}
pub(super) fn list_mut(plan: &mut FinancePlan, unit: usize) -> &mut Vec<StoredValue> {
    match unit {
        2 => &mut plan.categories,
        3 => &mut plan.clients,
        4 => &mut plan.income_accounts,
        5 => &mut plan.tax_countries,
        7 => &mut plan.savings_categories,
        _ => unreachable!("Unsupported reference catalogue: {unit}"),
    }
}
pub(super) fn stored_values(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    ui.label(
        RichText::new(title_case(unit_name(app.state.storage_unit)))
            .size(20.0)
            .strong(),
    );
    ui.add_space(16.0);
    contents(app, ui);
}
pub(super) fn settings(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    if !UNITS
        .iter()
        .any(|(unit, _)| *unit == app.state.storage_unit)
    {
        app.state.storage_unit = 0;
    }
    let expanded = (ui.available_width() * 0.3).clamp(140., 220.);
    let search_width = app
        .state
        .search_control
        .width(ui, "stored-values-search", expanded);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.state.storage_unit != 5 {
                let label = match app.state.storage_unit {
                    0 => "+ Currency",
                    1 => "+ Chain",
                    3 => "+ Client",
                    4 => "+ Account",
                    _ => "+ Add Value",
                };
                let create = primary(ui, label);
                ui.interact(
                    create.rect,
                    egui::Id::new("stored-values-create"),
                    egui::Sense::hover(),
                );
                if create.clicked() {
                    app.state.create_requested = true;
                }
            }
            app.state.search_control.show(
                ui,
                "stored-values-search",
                &mut app.state.search,
                search_width,
                "Search…",
            );
        });
    });
    ui.add_space(12.);
    let list = egui::ScrollArea::vertical()
        .id_salt(("stored-values-settings", app.state.storage_unit))
        .max_height((design::dialog_body_height(ui.ctx()) - 145.).max(140.))
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            design::settings_group(ui, "", |ui| contents(app, ui));
        });
    ui.interact(
        list.inner_rect,
        egui::Id::new("stored-values-list"),
        egui::Sense::hover(),
    );
}
fn contents(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    if take_create(app) && app.state.storage_unit != 5 {
        let record = match app.state.storage_unit {
            0 => Record::Currency(Currency {
                id: id(),
                ..Currency::default()
            }),
            1 => Record::Chain(Chain {
                id: id(),
                ..Chain::default()
            }),
            4 => Record::Account(Account {
                id: id(),
                ..Account::default()
            }),
            unit => Record::Stored(
                unit,
                StoredValue {
                    id: id(),
                    name: String::new(),
                },
            ),
        };
        edit(app, record, true);
    }
    let plan = &app.plan;
    match app.state.storage_unit {
        0 => super::currencies::list(app, ui),
        4 => {
            let rows = plan
                .accounts
                .iter()
                .filter(|a| matches(&app.state.search, &a.label))
                .cloned()
                .map(Record::Account)
                .collect();
            inline::table(app, ui, rows, &["Label"]);
        }
        1 => {
            let rows: Vec<_> = plan
                .chains
                .iter()
                .filter(|r| matches(&app.state.search, &r.name))
                .cloned()
                .collect();
            inline::table(
                app,
                ui,
                rows.into_iter().map(Record::Chain).collect(),
                &["Name"],
            );
        }
        5 => country_list(app, ui),
        unit => {
            let rows: Vec<_> = list(plan, unit)
                .iter()
                .filter(|r| matches(&app.state.search, &r.name))
                .cloned()
                .collect();
            if rows.is_empty()
                && !app.reference_settings
                && !app
                    .state
                    .drafts
                    .values()
                    .any(|d| d.record.view() == app.state.view)
            {
                empty(
                    ui,
                    &format!("No {} yet", unit_name(unit).to_lowercase()),
                    "Values are also remembered when you save finance records.",
                );
            }
            inline::table(
                app,
                ui,
                rows.into_iter().map(|r| Record::Stored(unit, r)).collect(),
                &["Name"],
            );
        }
    }
}
fn country_list(app: &FinanceUi<'_>, ui: &mut egui::Ui) {
    let width = ui.available_width();
    egui::Grid::new("countries-catalogue")
        .num_columns(2)
        .min_row_height(design::CONTROL_HEIGHT)
        .show(ui, |ui| {
            for (name, fraction) in [("Country", 0.8), ("Code", 0.15)] {
                ui.add_sized(
                    [width * fraction, design::CONTROL_HEIGHT],
                    egui::Label::new(RichText::new(name).size(15.0).strong())
                        .halign(egui::Align::Center),
                );
            }
            ui.end_row();
            for country in amigo_core::reference_data::countries().iter().filter(|c| {
                matches(
                    &app.state.search,
                    &format!("{} {} {}", c.name, c.code, c.alpha3),
                )
            }) {
                ui.add_sized(
                    [width * 0.8, design::CONTROL_HEIGHT],
                    egui::Label::new(country.name).halign(egui::Align::Center),
                );
                ui.add_sized(
                    [width * 0.15, design::CONTROL_HEIGHT],
                    egui::Label::new(country.code).halign(egui::Align::Center),
                );
                ui.end_row();
            }
        });
}
pub(super) fn exchange_rates(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    super::exchange::show(app, ui);
}

pub(super) const UNITS: [(usize, &str); 7] = [
    (0, "Currencies"),
    (1, "Chains"),
    (2, "Expense Categories"),
    (3, "Clients"),
    (4, "Accounts"),
    (5, "Countries"),
    (7, "Savings Categories"),
];

pub(super) fn unit_name(unit: usize) -> &'static str {
    UNITS
        .iter()
        .find(|(id, _)| *id == unit)
        .map_or("Currencies", |(_, name)| name)
}
