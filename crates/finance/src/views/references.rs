use super::*;

pub(super) fn list(plan: &FinancePlan, unit: usize) -> &[StoredValue] {
    match unit {
        2 => &plan.categories,
        3 => &plan.clients,
        4 => &plan.income_accounts,
        5 => &plan.tax_countries,
        6 => &plan.expense_labels,
        7 => &plan.savings_categories,
        _ => &plan.labels,
    }
}
pub(super) fn list_mut(plan: &mut FinancePlan, unit: usize) -> &mut Vec<StoredValue> {
    match unit {
        2 => &mut plan.categories,
        3 => &mut plan.clients,
        4 => &mut plan.income_accounts,
        5 => &mut plan.tax_countries,
        6 => &mut plan.expense_labels,
        7 => &mut plan.savings_categories,
        _ => &mut plan.labels,
    }
}
pub(super) fn stored_values(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    ui.label(
        RichText::new(title_case(UNITS[app.state.storage_unit]))
            .size(20.0)
            .strong(),
    );
    ui.add_space(16.0);
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
    ui.add_space(16.0);
    let plan = &app.plan;
    match app.state.storage_unit {
        0 => super::currencies::table(app, ui),
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
                .filter(|r| matches(&app.state.search, &format!("{} {}", r.name, r.rpc_url)))
                .cloned()
                .collect();
            inline::table(
                app,
                ui,
                rows.into_iter().map(Record::Chain).collect(),
                &["Name", "RPC URL"],
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
                && !app
                    .state
                    .drafts
                    .values()
                    .any(|d| d.record.view() == app.state.view)
            {
                empty(
                    ui,
                    "No stored values yet",
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

pub(super) const UNITS: [&str; 9] = [
    "Currencies",
    "Chains",
    "Expense Categories",
    "Clients",
    "Accounts",
    "Countries",
    "Expense labels",
    "Savings Categories",
    "Legacy labels",
];
