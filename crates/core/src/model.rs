pub use crate::audit::{RateSnapshot, RecordAudit};
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}
pub fn month() -> String {
    Local::now().format("%Y-%m").to_string()
}
/// Persist schema cleanup once when an older vault is opened.
pub const RECORD_METADATA_VERSION: u32 = 4;
pub fn month_key(date: &str) -> String {
    chrono::NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map(|date| date.format("%Y-%m").to_string())
        .unwrap_or_default()
}
pub fn canonical_month(value: &str) -> Option<String> {
    chrono::NaiveDate::parse_from_str(
        &format!("{}-01", value.trim().trim_end_matches('-')),
        "%Y-%m-%d",
    )
    .ok()
    .map(|date| date.format("%Y-%m").to_string())
}
fn normalize_date(value: &mut String) {
    if let Ok(date) = chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d") {
        *value = date.format("%Y-%m-%d").to_string();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Amount {
    Number(f64),
    Text(String),
}
impl Default for Amount {
    fn default() -> Self {
        Self::Number(0.0)
    }
}
impl Amount {
    pub fn value(&self) -> f64 {
        let n = match self {
            Self::Number(n) => *n,
            Self::Text(s) => s.trim().parse().unwrap_or(0.0),
        };
        if n.is_finite() { n } else { 0.0 }
    }
}

macro_rules! record {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
        #[serde(default, rename_all = "camelCase")]
        pub struct $name { $(pub $field: $ty),* }
    };
}
record!(StoredValue {
    id: String,
    name: String
});
record!(Meta {
    name: String,
    reporting_currency: String,
    reporting_currency_id: String,
    reference_defaults_version: u32,
    savings_categories_version: u32,
    record_metadata_version: u32
});
record!(Chain {
    id: String,
    name: String
});
record!(Currency {
    id: String,
    name: String,
    chain_id: String,
    address: String,
    native_asset: bool
});
record!(ExchangeRate {
    id: String,
    date: String,
    currency: String,
    currency_id: String,
    rate: Amount
});
record!(Income {
    id: String,
    audit: RecordAudit,
    date: String,
    gross_amount: Amount,
    currency: String,
    currency_id: String,
    account: String,
    account_id: String,
    source: String,
    client_id: String,
    tax_country: String,
    tax_country_id: String,
    applied_tax: Amount,
    withdraw_fee: Amount,
    received: bool
});
record!(Expense {
    audit: RecordAudit,
    id: String,
    title: String,
    due_date: String,
    amount: Amount,
    currency: String,
    currency_id: String,
    category: String,
    category_id: String,
    priority: String,
    regular: bool,
    regular_until: String,
    cancelled_on: String
});
record!(Budget {
    id: String,
    audit: RecordAudit,
    month: String,
    label: String,
    amount: Amount,
    currency: String,
    currency_id: String,
    expenses: Vec<BudgetExpense>
});
record!(BudgetExpense {
    id: String,
    label: String,
    amount: Amount,
    currency: String,
    currency_id: String,
    comment: String,
    paid: bool,
    audit: RecordAudit
});
record!(Saving {
    audit: RecordAudit,
    category: String,
    category_id: String,
    id: String,
    source: String,
    source_id: String,
    deduct: bool,
    comment: String,
    amount: Amount,
    currency: String,
    currency_id: String
});
record!(Account {
    id: String,
    label: String,
    audit: RecordAudit
});
record!(FinancePlan {
    version: u32, meta: Meta, chains: Vec<Chain>, currencies: Vec<Currency>, categories: Vec<StoredValue>,
    clients: Vec<StoredValue>, income_accounts: Vec<StoredValue>, tax_countries: Vec<StoredValue>,
    savings_categories: Vec<StoredValue>, accounts: Vec<Account>,
    fx_rates: Vec<ExchangeRate>, income: Vec<Income>, expenses: Vec<Expense>, budgets: Vec<Budget>, savings: Vec<Saving>
});

impl Income {
    pub fn new(currency: &str) -> Self {
        Self {
            id: id(),
            audit: RecordAudit::new(),
            date: today(),
            currency: currency.into(),
            ..Self::default()
        }
    }
    pub fn net(&self) -> f64 {
        self.gross_amount.value()
            * (1.0 - self.applied_tax.value() / 100.0 - self.withdraw_fee.value() / 100.0)
    }
}
impl Expense {
    pub fn new(currency: &str) -> Self {
        Self {
            id: id(),
            audit: RecordAudit::new(),
            due_date: today(),
            currency: currency.into(),
            priority: "Normal".into(),
            ..Self::default()
        }
    }
    /// Cancellation excludes its entire month and all later months while
    /// retaining earlier history. With no month filter, retain any past expense.
    pub fn includes_month(&self, selected_month: &str) -> bool {
        let cancelled_month = month_key(&self.cancelled_on);
        let reporting_month = if selected_month.is_empty() {
            month_key(&self.due_date)
        } else {
            selected_month.to_owned()
        };
        self.scheduled_in_month(selected_month)
            && (cancelled_month.is_empty() || reporting_month < cancelled_month)
    }

    /// The saved schedule, including cancelled records so their cancellation
    /// date remains available in the expense management tables.
    pub fn scheduled_in_month(&self, selected_month: &str) -> bool {
        if selected_month.is_empty() {
            return true;
        }
        let start = month_key(&self.due_date);
        if start.is_empty() {
            return false;
        }
        if self.regular {
            start.as_str() <= selected_month
                && (self.regular_until.is_empty() || selected_month <= self.regular_until.as_str())
        } else {
            start == selected_month
        }
    }

    pub fn occurrence_date(&self, selected_month: &str) -> Option<String> {
        use chrono::Datelike;
        if !self.regular || !self.includes_month(selected_month) {
            return None;
        }
        let from = chrono::NaiveDate::parse_from_str(&self.due_date, "%Y-%m-%d").ok()?;
        let month =
            chrono::NaiveDate::parse_from_str(&format!("{selected_month}-01"), "%Y-%m-%d").ok()?;
        // A From date on the 31st still has one occurrence in a shorter month.
        (1..=from.day())
            .rev()
            .find_map(|day| month.with_day(day))
            .map(|date| date.to_string())
    }
}

pub fn peg(currency: &str) -> String {
    let c = currency.trim().to_uppercase();
    match c.as_str() {
        "BUSD" | "DAI" | "FDUSD" | "GUSD" | "PYUSD" | "TUSD" | "USDC" | "USDD" | "USDP"
        | "USDS" | "USDT" | "USDG" => "USD".into(),
        "AGEUR" | "EUR24" | "EURI" | "EURC" | "EUROC" | "EURS" | "EURT" | "PAR" | "SEUR"
        | "VEUR" => "EUR".into(),
        _ => c,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Conversion {
    pub rate: f64,
    pub fallback: bool,
}
impl FinancePlan {
    /// Finite months represented by manual expenses and regular occurrences,
    /// including regular-only months through the current reporting period.
    pub fn expense_months(&self, through: &str) -> Vec<String> {
        let mut months: std::collections::BTreeSet<_> = self
            .expenses
            .iter()
            .filter(|expense| !expense.regular)
            .map(|expense| month_key(&expense.due_date))
            .filter(|month| !month.is_empty())
            .collect();
        let last = self
            .expenses
            .iter()
            .map(|expense| month_key(&expense.due_date))
            .chain(canonical_month(through))
            .filter(|month| !month.is_empty())
            .max();
        if let Some(last) = last {
            for expense in self.expenses.iter().filter(|expense| expense.regular) {
                let start = month_key(&expense.due_date);
                let Ok(mut date) =
                    chrono::NaiveDate::parse_from_str(&format!("{start}-01"), "%Y-%m-%d")
                else {
                    continue;
                };
                let end = if expense.regular_until.is_empty() {
                    &last
                } else {
                    std::cmp::min(&last, &expense.regular_until)
                };
                loop {
                    let month = date.format("%Y-%m").to_string();
                    if month.as_str() > end.as_str() {
                        break;
                    }
                    if !expense.includes_month(&month) {
                        break;
                    }
                    months.insert(month);
                    let Some(next) = date.checked_add_months(chrono::Months::new(1)) else {
                        break;
                    };
                    date = next;
                }
            }
        }
        months.into_iter().collect()
    }
    /// Cancel from this calendar month onward, preserving the original date
    /// on repeated requests and leaving the saved record available for review.
    pub fn cancel_expense(&mut self, expense_id: &str, cancelled_on: &str) -> Result<(), String> {
        let date = chrono::NaiveDate::parse_from_str(cancelled_on.trim(), "%Y-%m-%d")
            .map_err(|_| "Choose a valid cancellation date.")?;
        let expense = self
            .expenses
            .iter_mut()
            .find(|e| e.id == expense_id)
            .ok_or("Expense no longer exists.")?;
        if expense.cancelled_on.is_empty() {
            expense.cancelled_on = date.to_string();
        }
        Ok(())
    }

    /// Removing a regular expense ends future recurrence without rewriting past
    /// months. Monthly expenses are independent records and can be deleted.
    pub fn remove_expense(&mut self, expense_id: &str, removed_in: &str) -> Result<(), String> {
        let cutoff = canonical_month(removed_in).ok_or("Choose a valid removal month.")?;
        if let Some(expense) = self.expenses.iter_mut().find(|e| e.id == expense_id) {
            if expense.regular {
                if expense.regular_until.is_empty() || cutoff < expense.regular_until {
                    expense.regular_until = cutoff;
                }
            } else {
                self.expenses.retain(|e| e.id != expense_id);
            }
        }
        Ok(())
    }
    pub fn empty() -> Self {
        let mut plan = Self {
            version: 2,
            ..Self::default()
        };
        plan.meta.record_metadata_version = RECORD_METADATA_VERSION;
        plan.normalize();
        plan
    }
    fn reference_rate(&self, currency: &str, date: &str) -> Option<Conversion> {
        let exact = currency.trim().to_uppercase();
        let c = if self
            .fx_rates
            .iter()
            .any(|r| r.currency == exact && r.rate.value() > 0.0)
        {
            exact
        } else {
            peg(currency)
        };
        if c == "USD" {
            return Some(Conversion {
                rate: 1.0,
                fallback: false,
            });
        }
        let before = self
            .fx_rates
            .iter()
            .filter(|r| r.currency == c && r.date.as_str() <= date && r.rate.value() > 0.0)
            .max_by_key(|r| &r.date);
        if let Some(r) = before.filter(|r| r.rate.value() > 0.0) {
            return Some(Conversion {
                rate: r.rate.value(),
                fallback: false,
            });
        }
        self.fx_rates
            .iter()
            .filter(|r| r.currency == c && r.date.as_str() > date && r.rate.value() > 0.0)
            .min_by_key(|r| &r.date)
            .filter(|r| r.rate.value() > 0.0)
            .map(|r| Conversion {
                rate: r.rate.value(),
                fallback: true,
            })
    }
    pub fn conversion(&self, currency: &str, date: &str) -> Option<Conversion> {
        let fallback_date = today();
        let date = if date.is_empty() {
            &fallback_date
        } else {
            date
        };
        if self.meta.reporting_currency.is_empty() || currency == self.meta.reporting_currency {
            return Some(Conversion {
                rate: 1.0,
                fallback: false,
            });
        }
        self.exchange_rate(currency, &self.meta.reporting_currency, date)
    }
    /// Quote currency units for one base unit, independent of reporting settings.
    pub fn exchange_rate(&self, base: &str, quote: &str, date: &str) -> Option<Conversion> {
        if base.trim().is_empty() || quote.trim().is_empty() {
            return None;
        }
        if base.trim().eq_ignore_ascii_case(quote.trim()) {
            return Some(Conversion {
                rate: 1.0,
                fallback: false,
            });
        }
        let current_date = today();
        let date = if date.is_empty() { &current_date } else { date };
        let from = self.reference_rate(base, date)?;
        let to = self.reference_rate(quote, date)?;
        Some(Conversion {
            rate: from.rate / to.rate,
            fallback: from.fallback || to.fallback,
        })
    }
    pub fn convert(&self, amount: f64, currency: &str, date: &str) -> Option<f64> {
        self.conversion(currency, date).map(|c| amount * c.rate)
    }
    pub fn totals(&self, selected_month: &str, actual: bool) -> (f64, f64, usize) {
        self.month_totals(selected_month, actual, None)
    }
    /// Inclusive date totals with regular expenses dated by their occurrence in
    /// each month. Uses the same Actual rules and saved FX rates as Wallet totals.
    pub fn totals_between(
        &self,
        from: chrono::NaiveDate,
        through: chrono::NaiveDate,
        actual: bool,
    ) -> (f64, f64, usize) {
        use chrono::Datelike;
        let mut totals = (0.0, 0.0, 0);
        if from > through {
            return totals;
        }
        let mut month = from.with_day(1).unwrap();
        while month <= through {
            let row = self.month_totals(
                &month.format("%Y-%m").to_string(),
                actual,
                Some((from, through)),
            );
            totals.0 += row.0;
            totals.1 += row.1;
            totals.2 += row.2;
            let Some(next) = month.checked_add_months(chrono::Months::new(1)) else {
                break;
            };
            month = next;
        }
        totals
    }
    fn month_totals(
        &self,
        selected_month: &str,
        actual: bool,
        dates: Option<(chrono::NaiveDate, chrono::NaiveDate)>,
    ) -> (f64, f64, usize) {
        let in_range = |value: &str| {
            dates.is_none_or(|(from, through)| {
                chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                    .is_ok_and(|date| from <= date && date <= through)
            })
        };
        let mut missing = 0;
        let income = self
            .income
            .iter()
            .filter(|r| {
                (selected_month.is_empty() || month_key(&r.date) == selected_month)
                    && (!actual || r.received)
                    && in_range(&r.date)
            })
            .map(|r| {
                self.record_value(r.net(), &r.currency, &r.audit, &r.date)
                    .unwrap_or_else(|| {
                        missing += 1;
                        0.0
                    })
            })
            .sum();
        let expense = self
            .expenses
            .iter()
            .filter(|r| r.includes_month(selected_month))
            .filter(|r| {
                dates.is_none()
                    || if r.regular {
                        r.occurrence_date(selected_month)
                            .is_some_and(|date| in_range(&date))
                    } else {
                        in_range(&r.due_date)
                    }
            })
            .map(|r| {
                self.record_value(r.amount.value(), &r.currency, &r.audit, &r.due_date)
                    .unwrap_or_else(|| {
                        missing += 1;
                        0.0
                    })
            })
            .sum::<f64>();
        let budget_expenses: f64 = self
            .budgets
            .iter()
            .flat_map(|b| &b.expenses)
            .filter(|expense| expense.paid)
            .filter(|e| selected_month.is_empty() || month_key(e.audit.date()) == selected_month)
            .filter(|e| in_range(e.audit.date()))
            .map(|e| {
                self.record_value(e.amount.value(), &e.currency, &e.audit, e.audit.date())
                    .unwrap_or_else(|| {
                        missing += 1;
                        0.0
                    })
            })
            .sum();
        (income, expense + budget_expenses, missing)
    }
    pub fn tracked_currencies(&self) -> Vec<String> {
        let mut rows: Vec<_> = self
            .currencies
            .iter()
            .map(|c| c.name.clone())
            .chain(std::iter::once(self.meta.reporting_currency.clone()))
            .chain(self.income.iter().map(|r| r.currency.clone()))
            .chain(self.expenses.iter().map(|r| r.currency.clone()))
            .chain(self.savings.iter().map(|r| r.currency.clone()))
            .chain(self.budgets.iter().map(|r| r.currency.clone()))
            .chain(
                self.budgets
                    .iter()
                    .flat_map(|r| r.expenses.iter().map(|e| e.currency.clone())),
            )
            .filter(|c| !c.is_empty())
            .collect();
        rows.sort();
        rows.dedup();
        rows
    }
    pub fn normalize(&mut self) {
        self.version = 2;
        for chain in &mut self.chains {
            ensure_id(&mut chain.id);
        }
        crate::reference_data::seed(self);
        // Keep the existing account reference IDs, including externally edited catalogues.
        for account in &mut self.accounts {
            if account.id.is_empty() {
                account.id = self
                    .income_accounts
                    .iter()
                    .find(|r| r.name == account.label)
                    .map(|r| r.id.clone())
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(id);
            }
            account.label = account.label.trim().into();
            if let Some(reference) = self.income_accounts.iter().find(|r| r.id == account.id) {
                account.label = reference.name.clone();
            } else {
                self.income_accounts.push(StoredValue {
                    id: account.id.clone(),
                    name: account.label.clone(),
                });
            }
        }
        self.meta.savings_categories_version = self.meta.savings_categories_version.max(1);
        for row in &mut self.tax_countries {
            if let Some(country) = crate::reference_data::country(&row.name) {
                row.name = country.name.into();
            }
        }
        for row in &mut self.income {
            normalize_date(&mut row.date);
        }
        for row in &mut self.expenses {
            normalize_date(&mut row.due_date);
            normalize_date(&mut row.cancelled_on);
            if let Some(month) = canonical_month(&row.regular_until) {
                row.regular_until = month;
            }
        }
        for row in &mut self.fx_rates {
            normalize_date(&mut row.date);
        }
        for row in &mut self.budgets {
            if let Some(month) = canonical_month(&row.month) {
                row.month = month;
            }
        }
        self.meta.reporting_currency = self.meta.reporting_currency.trim().to_uppercase();
        if self.meta.reporting_currency.is_empty() {
            self.meta.reporting_currency = "EUR".into();
            self.meta.reporting_currency_id.clear();
        }
        for c in &mut self.currencies {
            ensure_id(&mut c.id);
            c.name = c.name.trim().to_uppercase();
        }
        for list in [
            &mut self.categories,
            &mut self.clients,
            &mut self.income_accounts,
            &mut self.tax_countries,
            &mut self.savings_categories,
        ] {
            for r in list.iter_mut() {
                ensure_id(&mut r.id);
                r.name = r.name.trim().into();
            }
        }
        resolve_currency(
            &mut self.meta.reporting_currency,
            &mut self.meta.reporting_currency_id,
            &self.currencies,
        );
        for r in &mut self.income {
            resolve_currency(&mut r.currency, &mut r.currency_id, &self.currencies);
        }
        for r in &mut self.expenses {
            resolve_currency(&mut r.currency, &mut r.currency_id, &self.currencies);
        }
        for r in &mut self.savings {
            resolve_currency(&mut r.currency, &mut r.currency_id, &self.currencies);
        }
        for r in &mut self.fx_rates {
            resolve_currency(&mut r.currency, &mut r.currency_id, &self.currencies);
        }
        let names = self.tracked_currencies();
        for name in names {
            if !self.currencies.iter().any(|c| c.name == name) {
                self.currencies.push(Currency {
                    id: id(),
                    name,
                    ..Currency::default()
                });
            }
        }
        for c in &mut self.chains {
            ensure_id(&mut c.id);
        }
        let currencies = self.currencies.clone();
        resolve_currency(
            &mut self.meta.reporting_currency,
            &mut self.meta.reporting_currency_id,
            &currencies,
        );
        for r in &mut self.income {
            ensure_id(&mut r.id);
            resolve_currency(&mut r.currency, &mut r.currency_id, &currencies);
            resolve(&mut r.account, &mut r.account_id, &mut self.income_accounts);
            resolve(&mut r.source, &mut r.client_id, &mut self.clients);
            if let Some(country) = crate::reference_data::country(&r.tax_country) {
                r.tax_country = country.name.into();
            }
            resolve(
                &mut r.tax_country,
                &mut r.tax_country_id,
                &mut self.tax_countries,
            );
        }
        for r in &mut self.expenses {
            ensure_id(&mut r.id);
            resolve_currency(&mut r.currency, &mut r.currency_id, &currencies);
            resolve(&mut r.category, &mut r.category_id, &mut self.categories);
            if r.priority.is_empty() {
                r.priority = "Normal".into();
            }
        }
        for r in &mut self.savings {
            ensure_id(&mut r.id);
            resolve_currency(&mut r.currency, &mut r.currency_id, &currencies);
            resolve(&mut r.source, &mut r.source_id, &mut self.income_accounts);
            resolve(
                &mut r.category,
                &mut r.category_id,
                &mut self.savings_categories,
            );
        }
        for r in &mut self.fx_rates {
            ensure_id(&mut r.id);
            resolve_currency(&mut r.currency, &mut r.currency_id, &currencies);
        }
        for r in &mut self.budgets {
            ensure_id(&mut r.id);
            if r.currency.is_empty() {
                r.currency = if self.meta.reporting_currency.is_empty() {
                    "USD".into()
                } else {
                    self.meta.reporting_currency.clone()
                };
            }
            resolve_currency(&mut r.currency, &mut r.currency_id, &currencies);
            for expense in &mut r.expenses {
                ensure_id(&mut expense.id);
                resolve_currency(&mut expense.currency, &mut expense.currency_id, &currencies);
            }
        }
        for list in [
            &mut self.categories,
            &mut self.clients,
            &mut self.income_accounts,
            &mut self.tax_countries,
            &mut self.savings_categories,
        ] {
            for r in list.iter_mut() {
                ensure_id(&mut r.id);
                r.name = r.name.trim().into();
            }
        }
        for reference in &self.income_accounts {
            if let Some(account) = self.accounts.iter_mut().find(|r| r.id == reference.id) {
                account.label = reference.name.clone();
            } else {
                self.accounts.push(Account {
                    id: reference.id.clone(),
                    label: reference.name.clone(),
                    ..Account::default()
                });
            }
        }
        self.capture_record_rates();
    }
}

fn ensure_id(s: &mut String) {
    if s.is_empty() {
        *s = id();
    }
}
fn resolve_currency(name: &mut String, reference: &mut String, records: &[Currency]) {
    *name = name.trim().to_uppercase();
    if let Some(r) = records
        .iter()
        .find(|r| !reference.is_empty() && r.id == *reference)
        .or_else(|| records.iter().find(|r| r.name == *name))
    {
        *name = r.name.clone();
        *reference = r.id.clone();
    }
}
fn resolve(name: &mut String, reference: &mut String, records: &mut Vec<StoredValue>) {
    *name = name.trim().into();
    if let Some(r) = records
        .iter()
        .find(|r| !reference.is_empty() && r.id == *reference)
        .or_else(|| records.iter().find(|r| r.name == *name))
    {
        *name = r.name.clone();
        *reference = r.id.clone();
    } else if !name.is_empty() {
        *reference = id();
        records.push(StoredValue {
            id: reference.clone(),
            name: name.clone(),
        });
    }
}
/// Read the original browser format, including its older string lists and token references.
pub fn plan_from_value(mut value: Value) -> Result<FinancePlan, String> {
    if !value.is_object()
        || !["meta", "income", "expenses", "savings", "currencies"]
            .iter()
            .any(|k| value.get(k).is_some())
    {
        return Err("This JSON does not contain a Finscreen finance plan.".into());
    }
    if value.get("version").and_then(Value::as_u64).unwrap_or(2) > 2 {
        return Err("This finance plan uses a newer, unsupported version.".into());
    }
    for key in [
        "categories",
        "clients",
        "incomeAccounts",
        "savingsLabels",
        "taxCountries",
        "currencies",
    ] {
        if let Some(rows) = value.get_mut(key).and_then(Value::as_array_mut) {
            for row in rows {
                if let Some(name) = row.as_str() {
                    *row = serde_json::json!({"id": id(), "name": name});
                }
            }
        }
    }
    migrate_savings_categories(&mut value);
    let object = value.as_object_mut().unwrap();
    if let Some(tokens) = object
        .remove("currencyTokens")
        .and_then(|v| v.as_array().cloned())
    {
        let chains = object
            .get("chains")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let currencies = object
            .entry("currencies")
            .or_insert_with(|| Value::Array(vec![]));
        if let Some(rows) = currencies.as_array_mut() {
            for mut token in tokens {
                if let Some(chain) = token.get("chain").and_then(Value::as_str).map(String::from) {
                    token["chainId"] = chains
                        .iter()
                        .find(|r| r["name"] == chain || r["id"] == chain)
                        .map(|r| r["id"].clone())
                        .unwrap_or(Value::String(String::new()));
                }
                rows.push(token);
            }
        }
    }
    for key in ["income", "expenses", "savings"] {
        if let Some(rows) = value.get_mut(key).and_then(Value::as_array_mut) {
            for row in rows {
                if row
                    .get("currencyId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .is_empty()
                    && let Some(v) = row.get("currencyTokenId").cloned()
                {
                    row["currencyId"] = v;
                }
                if key == "income" {
                    if row.get("clientId").is_none()
                        && let Some(v) = row.get("sourceId").cloned()
                    {
                        row["clientId"] = v;
                    }
                    let gross = json_number(&row["grossAmount"]);
                    for field in ["appliedTax", "withdrawFee"] {
                        let percent = json_number(&row[field]);
                        if gross > 0.0 && percent > 100.0 {
                            row[field] = serde_json::json!(
                                (percent / gross * 1_000_000.0).round() / 10_000.0
                            );
                        }
                    }
                }
                if key == "savings"
                    && row.get("comment").is_none()
                    && let Some(v) = row.get("origin").cloned()
                {
                    row["comment"] = v;
                }
            }
        }
    }
    let mut plan: FinancePlan =
        serde_json::from_value(value).map_err(|e| format!("Invalid finance plan: {e}"))?;
    plan.normalize();
    Ok(plan)
}
// Convert the former savings labels before serde discards the obsolete properties.
// This runs only for files predating the separate savings category catalogue.
fn migrate_savings_categories(value: &mut Value) {
    if value["meta"]["savingsCategoriesVersion"]
        .as_u64()
        .unwrap_or(0)
        != 0
    {
        return;
    }
    let labels = value["savingsLabels"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut categories = value["savingsCategories"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for label in &labels {
        if !categories
            .iter()
            .any(|category| category["name"] == label["name"])
        {
            categories.push(label.clone());
        }
    }
    if let Some(rows) = value.get_mut("savings").and_then(Value::as_array_mut) {
        for row in rows {
            if row["category"].as_str().unwrap_or("").is_empty()
                && row["categoryId"].as_str().unwrap_or("").is_empty()
            {
                let label = row["labels"]
                    .as_array()
                    .and_then(|names| names.first())
                    .cloned();
                let reference = row["labelIds"].as_array().and_then(|ids| ids.first());
                let linked =
                    reference.and_then(|id| labels.iter().find(|label| label["id"] == *id));
                if let Some(name) = linked.map(|label| label["name"].clone()).or(label) {
                    row["category"] = name;
                }
            }
        }
    }
    value["savingsCategories"] = Value::Array(categories);
}

fn json_number(v: &Value) -> f64 {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0.0)
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StatusColor {
    Gray,
    #[default]
    Blue,
    Amber,
    Green,
    Violet,
    Rose,
}
impl StatusColor {
    pub const ALL: [Self; 6] = [
        Self::Gray,
        Self::Blue,
        Self::Amber,
        Self::Green,
        Self::Violet,
        Self::Rose,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Gray => "Gray",
            Self::Blue => "Blue",
            Self::Amber => "Amber",
            Self::Green => "Green",
            Self::Violet => "Violet",
            Self::Rose => "Rose",
        }
    }
}
record!(TaskStatus {
    id: String,
    name: String,
    color: StatusColor,
    completed: bool
});
pub const DEFAULT_STATUS_IDS: [&str; 4] = ["backlog", "todo", "in-progress", "done"];
pub fn default_task_statuses() -> Vec<TaskStatus> {
    [
        ("Backlog", StatusColor::Gray, false),
        ("To do", StatusColor::Blue, false),
        ("In progress", StatusColor::Amber, false),
        ("Done", StatusColor::Green, true),
    ]
    .into_iter()
    .zip(DEFAULT_STATUS_IDS)
    .map(|((name, color, completed), id)| TaskStatus {
        id: id.into(),
        name: name.into(),
        color,
        completed,
    })
    .collect()
}
record!(Task {
    id: String, number: u32, title: String, description: String, status: String, category_id: String, priority: String,
    due_date: String, created: String, attachments: Vec<TaskAttachment>
});
record!(TaskAttachment {
    id: String,
    name: String,
    size: u64
});
impl Task {
    pub fn priority_rank(&self) -> u8 {
        match self.priority.as_str() {
            "High" => 0,
            "Low" => 2,
            _ => 1,
        }
    }
    pub fn new(number: u32, status: &str) -> Self {
        Self {
            id: id(),
            number,
            status: status.into(),
            priority: "Medium".into(),
            created: today(),
            ..Self::default()
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Workspace {
    pub version: u32,
    pub dark_mode: bool,
    pub tasks: Vec<Task>,
    pub task_statuses: Vec<TaskStatus>,
    pub task_categories: Vec<StoredValue>,
    pub next_task: u32,
    pub finance: FinancePlan,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            version: 2,
            dark_mode: false,
            tasks: vec![],
            task_statuses: default_task_statuses(),
            task_categories: vec![],
            next_task: 1,
            finance: FinancePlan::empty(),
        }
    }
}
impl Workspace {
    pub fn normalize(&mut self) {
        self.version = 2;
        self.finance.normalize();
        if self.task_statuses.is_empty() {
            self.task_statuses = default_task_statuses();
        }
        let mut ids = std::collections::HashSet::new();
        for status in &mut self.task_statuses {
            ensure_id(&mut status.id);
            if !ids.insert(status.id.clone()) {
                status.id = id();
                ids.insert(status.id.clone());
            }
            status.name = status.name.trim().into();
            if status.name.is_empty() {
                status.name = "Untitled status".into();
            }
        }
        ids.clear();
        for category in &mut self.task_categories {
            ensure_id(&mut category.id);
            if !ids.insert(category.id.clone()) {
                category.id = id();
                ids.insert(category.id.clone());
            }
            category.name = category.name.trim().into();
            if category.name.is_empty() {
                category.name = "Untitled category".into();
            }
        }
        for task in &mut self.tasks {
            ensure_id(&mut task.id);
            if task.priority.trim().is_empty() || task.priority.eq_ignore_ascii_case("Normal") {
                task.priority = "Medium".into();
            }
            if task.status.is_empty() {
                task.status = self.task_statuses[0].id.clone();
            }
            if !self.task_statuses.iter().any(|s| s.id == task.status) {
                self.task_statuses.push(TaskStatus {
                    id: task.status.clone(),
                    name: format!("Recovered status {}", self.task_statuses.len() + 1),
                    ..TaskStatus::default()
                });
            }
            if !task.category_id.is_empty()
                && !self
                    .task_categories
                    .iter()
                    .any(|c| c.id == task.category_id)
            {
                self.task_categories.push(StoredValue {
                    id: task.category_id.clone(),
                    name: format!("Recovered category {}", self.task_categories.len() + 1),
                });
            }
        }
        self.next_task = self.next_task.max(
            self.tasks
                .iter()
                .map(|t| t.number)
                .max()
                .unwrap_or(0)
                .saturating_add(1),
        );
    }
    pub fn is_task_complete(&self, task: &Task) -> bool {
        self.task_statuses
            .iter()
            .any(|s| s.id == task.status && s.completed)
    }
    pub fn default_task_status(&self) -> &str {
        self.task_statuses
            .iter()
            .find(|s| s.id == "todo")
            .or_else(|| self.task_statuses.iter().find(|s| !s.completed))
            .or_else(|| self.task_statuses.first())
            .map(|s| s.id.as_str())
            .unwrap_or("todo")
    }
    pub fn move_task(&mut self, task_id: &str, status: &str, before: Option<&str>) {
        if !self.task_statuses.iter().any(|s| s.id == status) || before == Some(task_id) {
            return;
        }
        if let Some(index) = self.tasks.iter().position(|t| t.id == task_id) {
            let mut task = self.tasks.remove(index);
            task.status = status.into();
            let index = before
                .and_then(|id| {
                    self.tasks
                        .iter()
                        .position(|t| t.id == id && t.status == status)
                })
                .unwrap_or(self.tasks.len());
            self.tasks.insert(index, task);
        }
    }
    /// Apply the whole board configuration atomically, keeping task references stable.
    pub fn update_task_board(
        &mut self,
        mut statuses: Vec<TaskStatus>,
        mut categories: Vec<StoredValue>,
        reassignments: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), String> {
        if statuses.is_empty() {
            return Err("Keep at least one status on the board.".into());
        }
        let validate = |records: Vec<(&str, &str)>, kind: &str| -> Result<(), String> {
            let mut ids = std::collections::HashSet::new();
            let mut names = std::collections::HashSet::new();
            for (id, name) in records {
                if id.is_empty() || name.trim().is_empty() {
                    return Err(format!("Every {kind} needs a name."));
                }
                if !ids.insert(id) || !names.insert(name.trim().to_lowercase()) {
                    return Err(format!("{kind} names must be unique."));
                }
            }
            Ok(())
        };
        validate(
            statuses
                .iter()
                .map(|s| (s.id.as_str(), s.name.as_str()))
                .collect(),
            "Status",
        )?;
        validate(
            categories
                .iter()
                .map(|c| (c.id.as_str(), c.name.as_str()))
                .collect(),
            "Category",
        )?;
        let mut tasks = self.tasks.clone();
        for task in &mut tasks {
            let mut visited = std::collections::HashSet::new();
            while !statuses.iter().any(|s| s.id == task.status) {
                if !visited.insert(task.status.clone()) {
                    return Err("Choose a remaining status for reassigned tasks.".into());
                }
                task.status = reassignments
                    .get(&task.status)
                    .cloned()
                    .ok_or("Choose a replacement for every removed status that contains tasks.")?;
            }
            if !categories.iter().any(|c| c.id == task.category_id) {
                task.category_id.clear();
            }
        }
        for s in &mut statuses {
            s.name = s.name.trim().into();
        }
        for c in &mut categories {
            c.name = c.name.trim().into();
        }
        self.tasks = tasks;
        self.task_statuses = statuses;
        self.task_categories = categories;
        Ok(())
    }
}
