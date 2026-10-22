//! Creation dates and immutable, dated exchange-rate snapshots on financial records.
use crate::model::*;
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct RateSnapshot {
    pub base: String,
    pub quote: String,
    pub date: String,
    /// Quote units per base unit. None means the historical rate is still unavailable.
    pub rate: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct RecordAudit {
    pub created_at: String,
    /// Old files did not record creation dates; their transaction dates are the best evidence.
    pub inferred: bool,
    pub rates: Vec<RateSnapshot>,
}

impl RecordAudit {
    pub fn new() -> Self {
        Self {
            created_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            ..Self::default()
        }
    }
    pub fn date(&self) -> &str {
        self.created_at.get(..10).unwrap_or("")
    }
    fn initialize(&mut self, legacy_date: &str) {
        if self.created_at.is_empty() {
            let date = NaiveDate::parse_from_str(legacy_date, "%Y-%m-%d")
                .map(|d| d.to_string())
                .unwrap_or_else(|_| today());
            self.created_at = format!("{date}T00:00:00Z");
            self.inferred = true;
        }
    }
    pub fn capture(
        &mut self,
        plan: &FinancePlan,
        currency: &str,
        related: Option<&str>,
        legacy_date: &str,
    ) {
        self.initialize(legacy_date);
        if currency.is_empty() {
            return;
        }
        for quote in [
            "USD",
            "EUR",
            plan.meta.reporting_currency.as_str(),
            related.unwrap_or(""),
        ] {
            if quote.is_empty() {
                continue;
            }
            let date = self.date().to_string();
            let index = self
                .rates
                .iter()
                .position(|r| r.base == currency && r.quote == quote && r.date == date);
            if index.is_some_and(|i| self.rates[i].rate.is_some()) {
                continue;
            }
            let rate = plan.exact_exchange_rate(currency, quote, &date);
            if let Some(index) = index {
                self.rates[index].rate = rate;
            } else {
                self.rates.push(RateSnapshot {
                    base: currency.into(),
                    quote: quote.into(),
                    date,
                    rate,
                });
            }
        }
    }
    pub fn conversion(&self, plan: &FinancePlan, base: &str, quote: &str) -> Option<f64> {
        if base.trim().eq_ignore_ascii_case(quote.trim()) && !base.is_empty() {
            return Some(1.0);
        }
        self.rates
            .iter()
            .find(|r| r.base == base && r.quote == quote && r.date == self.date())
            .and_then(|r| r.rate)
            .or_else(|| plan.exact_exchange_rate(base, quote, self.date()))
            // Keep legacy totals usable while their dated snapshots are being retrieved.
            .or_else(|| {
                self.inferred
                    .then(|| plan.exchange_rate(base, quote, self.date()).map(|r| r.rate))
                    .flatten()
            })
    }
}

impl FinancePlan {
    pub fn exact_exchange_rate(&self, base: &str, quote: &str, date: &str) -> Option<f64> {
        if base.is_empty() || quote.is_empty() {
            return None;
        }
        if base.trim().eq_ignore_ascii_case(quote.trim()) {
            return Some(1.0);
        }
        let reference = |code: &str| {
            let code = code.trim().to_uppercase();
            if code == "USD" {
                Some(1.0)
            } else {
                self.fx_rates
                    .iter()
                    .find(|r| r.currency == code && r.date == date)
                    .map(|r| r.rate.value())
                    .filter(|n| n.is_finite() && *n > 0.0)
            }
        };
        Some(reference(base)? / reference(quote)?)
    }
    pub fn record_value(
        &self,
        amount: f64,
        currency: &str,
        audit: &RecordAudit,
        legacy_date: &str,
    ) -> Option<f64> {
        if self.meta.reporting_currency.is_empty() {
            return Some(amount);
        }
        if audit.created_at.is_empty() {
            return self.convert(amount, currency, legacy_date);
        }
        audit
            .conversion(self, currency, &self.meta.reporting_currency)
            .map(|r| r * amount)
    }
    pub fn budget_spent(&self, budget: &Budget) -> Option<f64> {
        budget
            .expenses
            .iter()
            .filter(|expense| expense.paid)
            .try_fold(0.0, |total, e| {
                e.audit
                    .conversion(self, &e.currency, &budget.currency)
                    .map(|rate| total + e.amount.value() * rate)
            })
    }
    pub fn capture_record_rates(&mut self) {
        let mut accounts = std::mem::take(&mut self.accounts);
        for account in &mut accounts {
            account.audit.capture(self, "", None, "");
        }
        self.accounts = accounts;
        let mut records = std::mem::take(&mut self.income);
        for r in &mut records {
            r.audit.capture(self, &r.currency, None, &r.date);
        }
        self.income = records;
        let mut records = std::mem::take(&mut self.expenses);
        for r in &mut records {
            r.audit.capture(self, &r.currency, None, &r.due_date);
        }
        self.expenses = records;
        let mut records = std::mem::take(&mut self.savings);
        for r in &mut records {
            r.audit.capture(self, &r.currency, None, "");
        }
        self.savings = records;
        let mut records = std::mem::take(&mut self.budgets);
        for r in &mut records {
            r.audit
                .capture(self, &r.currency, None, &format!("{}-01", r.month));
            for e in &mut r.expenses {
                e.audit.capture(self, &e.currency, Some(&r.currency), "");
            }
        }
        self.budgets = records;
    }
    pub fn missing_snapshot_dates(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut dates: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let audits = self
            .income
            .iter()
            .map(|r| &r.audit)
            .chain(self.expenses.iter().map(|r| &r.audit))
            .chain(self.savings.iter().map(|r| &r.audit))
            .chain(self.budgets.iter().map(|r| &r.audit))
            .chain(
                self.budgets
                    .iter()
                    .flat_map(|r| r.expenses.iter().map(|e| &e.audit)),
            );
        for audit in audits {
            for rate in audit.rates.iter().filter(|r| r.rate.is_none()) {
                let codes = dates.entry(rate.date.clone()).or_default();
                codes.insert(rate.base.clone());
                codes.insert(rate.quote.clone());
            }
        }
        dates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rate(date: &str, currency: &str, value: f64) -> ExchangeRate {
        ExchangeRate {
            id: id(),
            date: date.into(),
            currency: currency.into(),
            rate: Amount::Number(value),
            ..Default::default()
        }
    }
    fn audit(date: &str) -> RecordAudit {
        RecordAudit {
            created_at: format!("{date}T12:34:56Z"),
            ..Default::default()
        }
    }

    #[test]
    fn record_snapshots_freeze_usd_eur_reporting_and_budget_conversions_on_creation_date() {
        let mut plan = FinancePlan::empty();
        plan.meta.reporting_currency = "GBP".into();
        plan.meta.reporting_currency_id.clear();
        plan.fx_rates = vec![
            rate("2026-09-10", "EUR", 1.25),
            rate("2026-09-10", "GBP", 2.0),
            rate("2026-09-11", "EUR", 2.0),
        ];
        plan.budgets.push(Budget {
            id: id(),
            label: "Trip".into(),
            currency: "EUR".into(),
            amount: Amount::Number(500.0),
            audit: audit("2026-09-10"),
            expenses: vec![
                BudgetExpense {
                    id: id(),
                    label: "Flight".into(),
                    currency: "USD".into(),
                    amount: Amount::Number(100.0),
                    paid: true,
                    audit: audit("2026-09-10"),
                    ..Default::default()
                },
                BudgetExpense {
                    id: id(),
                    label: "Hotel".into(),
                    currency: "USD".into(),
                    amount: Amount::Number(100.0),
                    paid: true,
                    audit: audit("2026-09-11"),
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        plan.income.push(Income {
            id: id(),
            currency: "USD".into(),
            date: "2027-01-01".into(),
            gross_amount: Amount::Number(200.0),
            audit: audit("2026-09-10"),
            ..Default::default()
        });
        plan.savings.push(Saving {
            id: id(),
            currency: "USD".into(),
            audit: audit("2026-09-10"),
            ..Default::default()
        });
        plan.expenses.push(Expense {
            id: id(),
            title: "Other".into(),
            currency: "USD".into(),
            audit: audit("2026-09-10"),
            ..Default::default()
        });
        plan.normalize();
        assert!((plan.budget_spent(&plan.budgets[0]).unwrap() - 130.0).abs() < 1e-9);
        assert_eq!(plan.income[0].audit.rates.len(), 3);
        assert_eq!(
            plan.record_value(200.0, "USD", &plan.income[0].audit, "2027-01-01"),
            Some(100.0)
        );
        for record in [&plan.expenses[0].audit, &plan.savings[0].audit] {
            assert_eq!(
                record.rates.iter().find(|r| r.quote == "EUR").unwrap().rate,
                Some(0.8)
            );
            assert_eq!(
                record.rates.iter().find(|r| r.quote == "USD").unwrap().rate,
                Some(1.0)
            );
        }
        let snapshots = plan.income[0].audit.clone();
        for rate in &mut plan.fx_rates {
            rate.rate = Amount::Number(99.0);
        }
        plan.normalize();
        assert_eq!(plan.income[0].audit, snapshots);
        assert!((plan.budget_spent(&plan.budgets[0]).unwrap() - 130.0).abs() < 1e-9);
        assert_eq!(
            plan_from_value(serde_json::to_value(&plan).unwrap()).unwrap(),
            plan
        );
    }

    #[test]
    fn unavailable_snapshot_stays_pending_until_exact_dated_rate_arrives() {
        let mut plan = FinancePlan::empty();
        plan.meta.reporting_currency = "EUR".into();
        plan.income.push(Income {
            id: id(),
            currency: "UAH".into(),
            audit: audit("2026-09-10"),
            ..Default::default()
        });
        plan.fx_rates = vec![
            rate("2026-09-09", "UAH", 0.02),
            rate("2026-09-11", "EUR", 1.2),
        ];
        plan.normalize();
        assert_eq!(
            plan.record_value(100.0, "UAH", &plan.income[0].audit, ""),
            None
        );
        assert!(plan.missing_snapshot_dates()["2026-09-10"].contains("UAH"));
        plan.fx_rates.extend([
            rate("2026-09-10", "UAH", 0.025),
            rate("2026-09-10", "EUR", 1.25),
        ]);
        plan.normalize();
        assert_eq!(
            plan.record_value(100.0, "UAH", &plan.income[0].audit, ""),
            Some(2.0)
        );
        assert!(plan.missing_snapshot_dates().is_empty());
    }
    #[test]
    fn stablecoin_snapshots_use_observed_prices_instead_of_assuming_a_dollar_peg() {
        let mut plan = FinancePlan::empty();
        plan.fx_rates = vec![
            rate("2026-09-10", "USDT", 0.999),
            rate("2026-09-10", "EUR", 1.25),
        ];
        let mut record = audit("2026-09-10");
        record.capture(&plan, "USDT", None, "");
        assert_eq!(record.conversion(&plan, "USDT", "USD"), Some(0.999));
        assert_eq!(
            plan.exchange_rate("USDT", "USD", "2026-09-10")
                .unwrap()
                .rate,
            0.999
        );
        plan.fx_rates[0].rate = Amount::Number(1.001);
        record.capture(&plan, "USDT", None, "");
        assert_eq!(record.conversion(&plan, "USDT", "USD"), Some(0.999));
    }
}
