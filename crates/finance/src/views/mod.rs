use crate::{FinanceTab, VaultAction};
use amigo_core::{library::Vault, model::*};
use amigo_ui::design::{hint, panel, primary};
use amigo_ui::{
    design::{self, Icon, colors},
    forms::*,
};
use egui::{self, RichText};
use std::collections::BTreeMap;
pub(crate) mod categories;
pub(crate) mod currencies;
mod dashboard;
mod exchange;
mod history;
mod inline;
mod records;
mod references;
mod settings;
#[cfg(test)]
mod tests;
mod toolbar;
use dashboard::wallet;
use records::{budgets, expenses, income, savings};
use references::{exchange_rates, list, list_mut, stored_values};
pub(crate) use settings::{show_settings, show_settings_in};

struct FinanceUi<'a> {
    state: &'a mut FinanceTab,
    plan: &'a mut FinancePlan,
    vault: Option<&'a Vault>,
    keys: design::ActionKeys,
    confirm_all: bool,
}

#[derive(Clone, Copy, PartialEq, Debug, Hash)]
pub enum FinanceView {
    Wallet,
    Income,
    Expenses,
    Savings,
    Budgets,
    Storage,
    Rates,
}
impl FinanceView {
    pub const ALL: [Self; 7] = [
        Self::Wallet,
        Self::Income,
        Self::Expenses,
        Self::Savings,
        Self::Budgets,
        Self::Storage,
        Self::Rates,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Wallet => "Wallet",
            Self::Income => "Income",
            Self::Expenses => "Expenses",
            Self::Savings => "Savings",
            Self::Budgets => "Budgets",
            Self::Storage => "Stored Values",
            Self::Rates => "Exchange",
        }
    }
    pub fn icon(self) -> Icon {
        match self {
            Self::Wallet => Icon::Wallet,
            Self::Income => Icon::Income,
            Self::Expenses => Icon::Expenses,
            Self::Savings => Icon::Savings,
            Self::Budgets => Icon::Budgets,
            Self::Storage => Icon::Storage,
            Self::Rates => Icon::Rates,
        }
    }
}
#[derive(Clone, PartialEq)]
pub struct ExpenseOccurrence {
    pub expense: std::sync::Arc<Expense>,
    pub date: String,
    pub id: String,
}

#[derive(Clone, PartialEq)]
pub enum Record {
    Account(Account),
    Income(Income),
    Expense(Expense),
    RegularExpense(ExpenseOccurrence),
    Saving(Saving),
    Budget(Budget),
    Rate(ExchangeRate),
    Currency(Currency),
    Chain(Chain),
    Stored(usize, StoredValue),
}
impl Record {
    pub fn id(&self) -> &str {
        match self {
            Self::Account(r) => &r.id,
            Self::Income(r) => &r.id,
            Self::Expense(r) => &r.id,
            Self::RegularExpense(r) => &r.id,
            Self::Saving(r) => &r.id,
            Self::Budget(r) => &r.id,
            Self::Rate(r) => &r.id,
            Self::Currency(r) => &r.id,
            Self::Chain(r) => &r.id,
            Self::Stored(_, r) => &r.id,
        }
    }
    pub fn view(&self) -> FinanceView {
        match self {
            Self::Account(_) => FinanceView::Storage,
            Self::Income(_) => FinanceView::Income,
            Self::Expense(_) | Self::RegularExpense(_) => FinanceView::Expenses,
            Self::Saving(_) => FinanceView::Savings,
            Self::Budget(_) => FinanceView::Budgets,
            Self::Rate(_) => FinanceView::Rates,
            Self::Currency(_) | Self::Chain(_) | Self::Stored(_, _) => FinanceView::Storage,
        }
    }
    fn reported_amount(&self, plan: &FinancePlan) -> Option<f64> {
        match self {
            Self::Income(row) => plan.record_value(row.net(), &row.currency, &row.audit, &row.date),
            Self::Expense(row) => expense_reported_amount(plan, row),
            Self::RegularExpense(row) => expense_reported_amount(plan, &row.expense),
            _ => None,
        }
    }
    fn sort_date(&self) -> Option<chrono::NaiveDate> {
        let value = match self {
            Self::Income(r) => r.date.clone(),
            Self::Expense(r) => r.due_date.clone(),
            Self::RegularExpense(r) => r.date.clone(),
            Self::Rate(r) => r.date.clone(),
            Self::Budget(r) => {
                if r.audit.inferred || r.audit.date().is_empty() {
                    format!("{}-01", canonical_month(&r.month)?)
                } else {
                    r.audit.date().into()
                }
            }
            _ => return None,
        };
        chrono::NaiveDate::parse_from_str(&value, "%Y-%m-%d").ok()
    }
    pub(crate) fn ensure_id(&mut self) {
        if !self.id().is_empty() {
            return;
        }
        let key = id();
        match self {
            Self::Account(r) => r.id = key,
            Self::Income(r) => r.id = key,
            Self::Expense(r) => r.id = key,
            Self::RegularExpense(r) => r.id = key,
            Self::Saving(r) => r.id = key,
            Self::Budget(r) => r.id = key,
            Self::Rate(r) => r.id = key,
            Self::Currency(r) => r.id = key,
            Self::Chain(r) => r.id = key,
            Self::Stored(_, r) => r.id = key,
        }
    }
}
pub struct Editor {
    pub record: Record,
    pub is_new: bool,
    delete_confirm: bool,
    error: String,
    dirty: bool,
}
impl Editor {
    pub fn new(mut record: Record, is_new: bool) -> Self {
        if is_new {
            let audit = match &mut record {
                Record::Account(r) => Some(&mut r.audit),
                Record::Income(r) => Some(&mut r.audit),
                Record::Expense(r) => Some(&mut r.audit),
                Record::Saving(r) => Some(&mut r.audit),
                Record::Budget(r) => Some(&mut r.audit),
                _ => None,
            };
            if let Some(audit) = audit
                && audit.created_at.is_empty()
            {
                *audit = RecordAudit::new();
            }
        }
        Self {
            record,
            is_new,
            delete_confirm: false,
            error: String::new(),
            dirty: false,
        }
    }
}
fn edit(app: &mut FinanceUi<'_>, mut record: Record, is_new: bool) {
    if is_new {
        let preferred = if app.state.selected_currencies.len() == 1 {
            app.state
                .selected_currencies
                .first()
                .cloned()
                .unwrap_or_default()
        } else if !app.plan.meta.reporting_currency.is_empty() {
            app.plan.meta.reporting_currency.clone()
        } else {
            app.plan
                .currencies
                .first()
                .map(|c| c.name.clone())
                .unwrap_or_default()
        };
        let selected_month = if app.state.view == FinanceView::Wallet {
            &app.state.wallet_month
        } else {
            &app.state.month
        };
        let month_date = if !selected_month.is_empty() && *selected_month != month() {
            format!("{selected_month}-01")
        } else {
            today()
        };
        match &mut record {
            Record::Income(r) => {
                r.currency = preferred;
                r.date = month_date;
            }
            Record::Expense(r) => {
                r.currency = preferred;
                r.due_date = month_date;
            }
            Record::Saving(r) => r.currency = preferred,
            Record::Budget(r) => r.currency = preferred,
            Record::Rate(r) => r.currency = preferred,
            _ => {}
        }
    }
    app.state.edit_record(record, is_new);
}
fn money(amount: f64, currency: &str) -> String {
    let value = format!("{:.2}", amount.abs());
    let Some((integer, fraction)) = value.split_once('.') else {
        return format!("{amount} {currency}").trim().into();
    };
    let mut grouped = String::new();
    for (index, digit) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!(
        "{}{grouped}.{fraction} {currency}",
        if amount.is_sign_negative() && value != "0.00" {
            "−"
        } else {
            ""
        }
    )
    .trim()
    .into()
}

fn show(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    app.toolbar(ui);
    let invalid: Vec<_> = app
        .state
        .drafts
        .values()
        .filter(|draft| !draft.error.is_empty() && (
            draft.record.view() != app.state.view
                || matches!(&draft.record, Record::Expense(r) if r.regular != app.state.expense_group.is_regular())
        ))
        .collect();
    if !invalid.is_empty() {
        let views: std::collections::BTreeSet<_> = invalid
            .iter()
            .map(|draft| match &draft.record {
                Record::Expense(r) if app.state.view == FinanceView::Expenses => {
                    if r.regular {
                        "Regular Expenses"
                    } else {
                        "Monthly Expenses"
                    }
                }
                _ => draft.record.view().name(),
            })
            .collect();
        ui.colored_label(
            colors(ui).red,
            format!(
                "{} pending {} attention in {}.",
                invalid.len(),
                if invalid.len() == 1 {
                    "row needs"
                } else {
                    "rows need"
                },
                views.into_iter().collect::<Vec<_>>().join(", ")
            ),
        );
    }
    ui.add_space(22.0);
    egui::ScrollArea::vertical()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .id_salt(("finance-content", app.state.view))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if app.state.view == FinanceView::Wallet {
                wallet(app, ui);
            } else {
                panel(ui).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    match app.state.view {
                        FinanceView::Income => income(app, ui),
                        FinanceView::Expenses => expenses(app, ui),
                        FinanceView::Savings => savings(app, ui),
                        FinanceView::Budgets => budgets(app, ui),
                        FinanceView::Storage => stored_values(app, ui),
                        FinanceView::Rates => exchange_rates(app, ui),
                        FinanceView::Wallet => {}
                    }
                });
            }
        });
}
fn expense_reported_amount(plan: &FinancePlan, row: &Expense) -> Option<f64> {
    plan.record_value(row.amount.value(), &row.currency, &row.audit, &row.due_date)
}

fn compare_reported_desc(left: Option<f64>, right: Option<f64>) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => right.total_cmp(&left),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn reports(app: &FinanceUi<'_>) -> bool {
    !app.plan.meta.reporting_currency.is_empty()
}
fn reported(
    ui: &mut egui::Ui,
    app: &FinanceUi<'_>,
    amount: f64,
    currency: &str,
    date: &str,
    audit: &RecordAudit,
) {
    if !reports(app) {
        ui.add_sized(
            [ui.available_width(), design::CONTROL_HEIGHT],
            egui::Label::new("—").halign(egui::Align::Center),
        );
        return;
    }
    let plan = &app.plan;
    match plan.record_value(amount, currency, audit, date) {
        Some(converted) => {
            let display = money(converted, &plan.meta.reporting_currency);
            let response = ui
                .add_sized(
                    [ui.available_width(), design::CONTROL_HEIGHT],
                    egui::Label::new(&display)
                        .truncate()
                        .halign(egui::Align::Center),
                )
                .on_hover_text(&display);
            response.on_hover_text(format!(
                "Creation date: {}. Uses the record’s saved rate.",
                audit.date()
            ));
        }
        None => {
            ui.add_sized(
                [ui.available_width(), design::CONTROL_HEIGHT],
                egui::Label::new(RichText::new("Missing FX rate").color(colors(ui).red))
                    .truncate()
                    .halign(egui::Align::Center),
            )
            .on_hover_text(format!(
                "Waiting for the {} exchange rate for {currency} to {}.",
                audit.date(),
                plan.meta.reporting_currency
            ));
        }
    }
}
fn month_label(value: &str) -> String {
    canonical_month(value)
        .and_then(|month| {
            chrono::NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").ok()
        })
        .map(|date| date.format("%B %Y").to_string())
        .unwrap_or_else(|| "All dates".into())
}
fn month_selector(app: &mut FinanceUi<'_>, ui: &mut egui::Ui, all: bool) {
    let selected = if app.state.view == FinanceView::Wallet {
        &mut app.state.wallet_month
    } else {
        &mut app.state.month
    };
    *selected =
        canonical_month(selected).unwrap_or_else(|| if all { String::new() } else { month() });
    let mut months: Vec<String> = app
        .plan
        .income
        .iter()
        .map(|r| month_key(&r.date))
        .chain(app.plan.expenses.iter().map(|r| month_key(&r.due_date)))
        .chain(app.plan.expense_months(&month()))
        .chain(
            app.plan
                .budgets
                .iter()
                .filter_map(|r| canonical_month(&r.month)),
        )
        .chain([month(), selected.clone()])
        .filter(|m| !m.is_empty())
        .collect();
    months.sort_by(|a, b| b.cmp(a));
    months.dedup();
    egui::ComboBox::from_id_salt("finance-month")
        .width(ui.available_width())
        .selected_text(month_label(selected))
        .show_ui(ui, |ui| {
            if all {
                ui.selectable_value(selected, String::new(), "All dates");
            }
            for month in months {
                let label = month_label(&month);
                ui.selectable_value(selected, month, label);
            }
        });
}
fn take_create(app: &mut FinanceUi<'_>) -> bool {
    std::mem::take(&mut app.state.create_requested)
}
fn currency_matches(app: &FinanceUi<'_>, currency: &str) -> bool {
    app.state.selected_currencies.is_empty() || app.state.selected_currencies.contains(currency)
}
fn matches(query: &str, text: &str) -> bool {
    query.trim().is_empty() || text.to_lowercase().contains(&query.trim().to_lowercase())
}
fn empty(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    panel(ui).show(ui, |ui| {
        ui.label(RichText::new(title).size(18.0).strong());
        hint(ui, subtitle);
    });
    ui.add_space(14.0);
}

pub(crate) fn show_tab(
    state: &mut FinanceTab,
    plan: &mut FinancePlan,
    vault: Option<&Vault>,
    ui: &mut egui::Ui,
) {
    let keys = design::ActionKeys::capture(ui.ctx()).blocked(
        state.filters_open
            || state.settings.is_some()
            || state.currency_editor.is_some()
            || state.category_settings.is_some(),
    );
    let mut view = FinanceUi {
        state,
        plan,
        vault,
        keys,
        confirm_all: false,
    };
    show(&mut view, ui);
    let deleting = view
        .state
        .active_row
        .as_ref()
        .and_then(|key| view.state.drafts.get(key))
        .is_some_and(|draft| draft.delete_confirm);
    if !view.confirm_all && !view.state.drafts.is_empty() && !deleting {
        view.confirm_all = keys.in_ui(ui, true).confirm;
    }
    if view.confirm_all {
        inline::confirm_pending(view.state, view.plan);
        ui.ctx().request_repaint();
    }
}
impl FinanceUi<'_> {
    fn vault_controls(&mut self, ui: &mut egui::Ui) {
        let Some(vault) = self.vault else {
            return;
        };
        for (key, icon, label, action) in [
            (
                "lock",
                if vault.encrypted {
                    Icon::Lock
                } else {
                    Icon::Unlock
                },
                if vault.encrypted {
                    "Lock Vault"
                } else {
                    "Set Vault Password"
                },
                if vault.encrypted {
                    VaultAction::Lock
                } else {
                    VaultAction::Encrypt
                },
            ),
            (
                "rename",
                Icon::Edit,
                "Rename Vault",
                VaultAction::RenameVaultRequested,
            ),
            (
                "delete",
                Icon::Trash,
                "Delete Vault",
                VaultAction::DeleteVault,
            ),
            (
                "export",
                Icon::Export,
                "Export Vault",
                VaultAction::ExportVault,
            ),
            (
                "import",
                Icon::Import,
                "Import Vault",
                VaultAction::ImportVault,
            ),
        ] {
            if design::icon_button(ui, egui::Id::new(format!("vault-{key}")), icon, label, 30.)
                .clicked()
            {
                self.state.toolbar_action = Some(action);
            }
        }
    }
}
