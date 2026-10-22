//! The Finance tab, including its inline record tables and background FX requests.
use amigo_core::{library::Vault, model::FinancePlan};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::mpsc,
};
pub mod rates;
mod views;
pub use views::{Editor, ExpenseOccurrence, FinanceView, Record};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ExpenseGroup {
    Regular,
    #[default]
    Monthly,
}

impl ExpenseGroup {
    pub fn is_regular(self) -> bool {
        self == Self::Regular
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryRange {
    LastMonths(u32),
    All,
    Custom,
}

pub struct FinanceTab {
    pub view: FinanceView,
    pub expense_group: ExpenseGroup,
    pub month: String,
    pub wallet_month: String,
    pub actual: bool,
    pub exchange_interval_seconds: u64,
    pub drafts: BTreeMap<String, Editor>,
    expanded_budgets: BTreeSet<String>,
    focus_row: Option<String>,
    active_row: Option<String>,
    pub search: String,
    search_control: amigo_ui::design::SearchField,
    selected_currencies: BTreeSet<String>,
    create_requested: bool,
    filters_open: bool,
    filter_snapshot: Option<FilterSnapshot>,
    settings: Option<VaultSettings>,
    embedded_settings: bool,
    settings_cancelled: bool,
    toolbar_action: Option<VaultAction>,
    currency_editor: Option<views::currencies::CurrencyEditor>,
    category_settings: Option<views::categories::CategorySettings>,
    exchange_base: String,
    exchange_quote: String,
    history_range: HistoryRange,
    history_from: String,
    history_to: String,
    pub storage_unit: usize,
    pub message: String,
    pub error: String,
    rate_receiver: Option<mpsc::Receiver<Result<rates::RateResult, String>>>,
    rate_automatic: bool,
    historical_cursor: usize,
}

struct FilterSnapshot {
    month: String,
    wallet_month: String,
    actual: bool,
    currencies: BTreeSet<String>,
    storage_unit: usize,
    search: String,
}

pub struct AppSettingsDraft {
    vault: Option<VaultSettings>,
    categories: Option<views::categories::CategorySettings>,
}
struct VaultSettings {
    exchange_interval_seconds: u64,
    name_edit: Option<amigo_ui::design::NameEdit>,
    error: String,
}
#[derive(Clone, Debug, PartialEq)]
pub enum VaultAction {
    RenameVaultRequested,
    Lock,
    Encrypt,
    ChangePassword,
    ImportVault,
    ExportVault,
    DeleteVault,
    SaveSettings {
        vault_name: Option<String>,
        exchange_interval_seconds: u64,
    },
}
impl Default for FinanceTab {
    fn default() -> Self {
        Self {
            view: FinanceView::Wallet,
            expense_group: ExpenseGroup::default(),
            month: String::new(),
            wallet_month: amigo_core::model::month(),
            actual: false,
            exchange_interval_seconds: 60,
            drafts: BTreeMap::new(),
            expanded_budgets: BTreeSet::new(),
            focus_row: None,
            active_row: None,
            search: String::new(),
            search_control: Default::default(),
            selected_currencies: BTreeSet::new(),
            create_requested: false,
            filters_open: false,
            filter_snapshot: None,
            settings: None,
            embedded_settings: false,
            settings_cancelled: false,
            toolbar_action: None,
            currency_editor: None,
            category_settings: None,
            exchange_base: String::new(),
            exchange_quote: String::new(),
            history_range: HistoryRange::LastMonths(12),
            history_from: String::new(),
            history_to: String::new(),
            storage_unit: 0,
            message: String::new(),
            error: String::new(),
            rate_receiver: None,
            rate_automatic: false,
            historical_cursor: 0,
        }
    }
}
impl FinanceTab {
    pub fn new(_plan: &FinancePlan) -> Self {
        Self::default()
    }
    pub fn reset(&mut self) {
        let interval = self.exchange_interval_seconds;
        *self = Self::default();
        self.exchange_interval_seconds = interval;
    }
    pub fn configure_vault(&mut self, _plan: &FinancePlan) {
        self.settings = Some(VaultSettings {
            exchange_interval_seconds: self.exchange_interval_seconds,
            name_edit: None,
            error: String::new(),
        });
    }
    pub fn configure_in_app(&mut self, plan: &FinancePlan) {
        self.configure_vault(plan);
        self.embedded_settings = true;
    }
    pub fn settings_open(&self) -> bool {
        self.settings.is_some()
    }
    pub fn take_settings_cancelled(&mut self) -> bool {
        std::mem::take(&mut self.settings_cancelled)
    }
    pub fn take_settings(&mut self) -> AppSettingsDraft {
        AppSettingsDraft {
            vault: self.settings.take(),
            categories: self.category_settings.take(),
        }
    }
    pub fn restore_settings(&mut self, draft: AppSettingsDraft) {
        self.settings = draft.vault;
        self.category_settings = draft.categories;
        self.embedded_settings = true;
    }

    pub fn categories_open(&self) -> bool {
        self.category_settings.is_some()
    }
    pub fn close_settings(&mut self) {
        self.settings = None;
        self.category_settings = None;
        self.embedded_settings = false;
    }
    pub fn settings_in(
        &mut self,
        plan: &mut FinancePlan,
        vault: Option<&Vault>,
        ui: &mut egui::Ui,
    ) -> Option<VaultAction> {
        views::show_settings_in(self, plan, vault, ui)
    }
    pub fn configure_categories_in_app(&mut self, view: FinanceView, plan: &FinancePlan) {
        self.settings = None;
        self.category_settings = Some(views::categories::CategorySettings::new(view, plan));
        self.embedded_settings = true;
    }
    pub fn categories_in(&mut self, plan: &mut FinancePlan, ui: &mut egui::Ui) {
        views::categories::show_in(self, plan, ui);
    }
    pub fn configure_categories(&mut self, plan: &FinancePlan) {
        if matches!(self.view, FinanceView::Expenses | FinanceView::Savings) {
            self.category_settings =
                Some(views::categories::CategorySettings::new(self.view, plan));
        }
    }
    pub fn open_filters(&mut self) {
        self.filters_open = true;
    }
    pub fn edit_record(&mut self, mut record: Record, is_new: bool) {
        if let Record::RegularExpense(occurrence) = record {
            record = Record::Expense((*occurrence.expense).clone());
        }
        record.ensure_id();
        self.view = record.view();
        if let Record::Expense(expense) = &record {
            self.expense_group = if expense.regular {
                ExpenseGroup::Regular
            } else {
                ExpenseGroup::Monthly
            };
        }
        if let Record::Stored(unit, _) = &record {
            self.storage_unit = *unit;
        } else if matches!(record, Record::Currency(_)) {
            self.storage_unit = 0;
        } else if matches!(record, Record::Chain(_)) {
            self.storage_unit = 1;
        } else if matches!(record, Record::Account(_)) {
            self.storage_unit = 4;
        }
        if let Record::Currency(currency) = record {
            self.currency_editor = Some(views::currencies::CurrencyEditor::new(currency, is_new));
            return;
        }
        let id = record.id().to_owned();
        self.focus_row = Some(id.clone());
        self.active_row = Some(id.clone());
        self.drafts.insert(id, Editor::new(record, is_new));
    }
    pub fn cancel_rates(&mut self) {
        self.rate_receiver = None;
    }
    pub fn show(&mut self, plan: &mut FinancePlan, vault: Option<&Vault>, ui: &mut egui::Ui) {
        views::show_tab(self, plan, vault, ui);
    }
    pub fn dialogs(
        &mut self,
        plan: &mut FinancePlan,
        vault: Option<&Vault>,
        ctx: &egui::Context,
    ) -> Option<VaultAction> {
        if self.embedded_settings {
            views::currencies::dialog(self, plan, ctx);
            return self.toolbar_action.take();
        }
        let toolbar_action = self.toolbar_action.take();
        let vault_settings_open = self.settings.is_some();
        let action = views::show_settings(self, plan, vault, ctx);
        views::currencies::dialog(self, plan, ctx);
        if !vault_settings_open {
            views::categories::dialog(self, plan, ctx);
        }
        toolbar_action.or(action)
    }
    pub fn sidebar(&mut self, ui: &mut egui::Ui) {
        amigo_ui::design::eyebrow(ui, "FINANCE");
        ui.add_space(3.0);
        for view in FinanceView::ALL {
            if amigo_ui::design::nav_row(ui, view.name(), view.icon(), self.view == view, None)
                .clicked()
            {
                self.view = view;
            }
        }
    }
    pub fn refresh_rates(&mut self, plan: &FinancePlan, ctx: &egui::Context) {
        self.request_rates(plan, ctx, false);
    }
    pub fn sync_rates(&mut self, plan: &FinancePlan, ctx: &egui::Context) {
        self.request_rates(plan, ctx, true);
    }
    fn request_rates(&mut self, plan: &FinancePlan, ctx: &egui::Context, automatic: bool) {
        if self.rate_receiver.is_some() {
            return;
        }
        let mut currencies = plan.tracked_currencies();
        currencies.extend(["USD".into(), "EUR".into()]);
        currencies.sort();
        currencies.dedup();
        if currencies.is_empty() {
            self.error = "Add currencies in Stored values first.".into();
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.rate_receiver = Some(receiver);
        self.rate_automatic = automatic;
        let missing: Vec<_> = plan
            .missing_snapshot_dates()
            .into_iter()
            .map(|(date, codes)| (date, codes.into_iter().collect::<Vec<_>>()))
            .collect();
        let dates: Vec<_> = missing
            .iter()
            .cycle()
            .skip(if missing.is_empty() {
                0
            } else {
                self.historical_cursor % missing.len()
            })
            .take(missing.len().min(4))
            .cloned()
            .collect();
        self.historical_cursor = self.historical_cursor.wrapping_add(dates.len());
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = rates::fetch_with_history(&currencies, &dates);
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
    pub fn rates_loading(&self) -> bool {
        self.rate_receiver.is_some()
    }
    pub fn poll_rates(&mut self, plan: &mut FinancePlan) {
        let Some(receiver) = &self.rate_receiver else {
            return;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.rate_receiver = None;
                match result {
                    Ok(result) => {
                        rates::merge(plan, &result);
                        if !self.rate_automatic {
                            self.message = format!(
                                "Updated {} rates for {}. {}",
                                result.rates.len(),
                                result.date,
                                if result.unavailable.is_empty() {
                                    String::new()
                                } else {
                                    format!(
                                        "Unavailable: {}. Existing rates preserved.",
                                        result.unavailable.join(", ")
                                    )
                                }
                            );
                        }
                        if !result.warnings.is_empty() {
                            self.error = result.warnings.join(" ");
                        }
                    }
                    Err(e) => self.error = e,
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.rate_receiver = None;
                self.error = "The rate request ended unexpectedly. Try again.".into();
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
}

#[cfg(test)]
mod sync_tests {
    use super::*;

    #[test]
    fn automatic_rates_do_not_overlap_and_merge_without_a_success_banner() {
        let ctx = egui::Context::default();
        let mut plan = FinancePlan::empty();
        plan.meta.reporting_currency = "USD".into();
        plan.meta.reporting_currency_id.clear();
        let mut tab = FinanceTab::new(&plan);
        let (sender, receiver) = mpsc::channel();
        tab.rate_receiver = Some(receiver);
        tab.rate_automatic = true;
        // The pending receiver keeps this test offline and must not be replaced.
        tab.sync_rates(&plan, &ctx);
        sender
            .send(Ok(rates::RateResult {
                date: amigo_core::model::today(),
                rates: vec![("USD".into(), 1.0)],
                historical: vec![],
                unavailable: vec![],
                warnings: vec![],
            }))
            .unwrap();
        tab.poll_rates(&mut plan);
        assert!(!tab.rates_loading());
        assert_eq!(plan.fx_rates.len(), 1);
        assert_eq!(plan.fx_rates[0].rate.value(), 1.0);
        assert!(tab.message.is_empty());
        assert!(tab.error.is_empty());
    }
    #[test]
    fn cancelled_rate_results_are_never_applied_to_another_vault() {
        let mut plan = FinancePlan::empty();
        let mut tab = FinanceTab::new(&plan);
        let (sender, receiver) = mpsc::channel();
        tab.rate_receiver = Some(receiver);
        tab.reset();
        assert!(
            sender
                .send(Ok(rates::RateResult {
                    date: amigo_core::model::today(),
                    rates: vec![("EUR".into(), 1.1)],
                    historical: vec![],
                    unavailable: vec![],
                    warnings: vec![],
                }))
                .is_err()
        );
        tab.poll_rates(&mut plan);
        assert!(plan.fx_rates.is_empty());
    }
}
