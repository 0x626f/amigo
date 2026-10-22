use amigo_core::model::{Amount, ExchangeRate, FinancePlan, id, today};
use reqwest::blocking::Client;
use serde_json::Value;
use std::time::Duration;

pub struct RateResult {
    pub date: String,
    pub rates: Vec<(String, f64)>,
    pub historical: Vec<ExchangeRate>,
    pub unavailable: Vec<String>,
    pub warnings: Vec<String>,
}

fn get(client: &Client, url: &str) -> Result<Value, String> {
    client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| e.to_string())
}
pub fn fetch(currencies: &[String]) -> Result<RateResult, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("Amigo/0.1")
        .build()
        .map_err(|e| e.to_string())?;
    let mut data = None;
    for url in [
        "https://cdn.jsdelivr.net/npm/@fawazahmed0/currency-api@latest/v1/currencies/usd.json",
        "https://latest.currency-api.pages.dev/v1/currencies/usd.json",
    ] {
        if let Ok(value) = get(&client, url)
            && value["date"]
                .as_str()
                .is_some_and(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
            && value["usd"].is_object()
        {
            data = Some(value);
            break;
        }
    }
    let data = data.ok_or(
        "Exchange rates are unavailable. Existing rates and record snapshots are preserved.",
    )?;
    let date = data["date"].as_str().unwrap().to_string();
    let observed = parse_historical(&data, &date, currencies);
    let unavailable = currencies
        .iter()
        .filter(|c| !observed.iter().any(|r| r.currency == c.to_uppercase()))
        .cloned()
        .collect();
    Ok(RateResult {
        date,
        rates: observed
            .into_iter()
            .map(|r| (r.currency, r.rate.value()))
            .collect(),
        historical: vec![],
        unavailable,
        warnings: vec![],
    })
}
pub fn merge(plan: &mut FinancePlan, result: &RateResult) {
    let date = result.date.clone();
    for (currency, rate) in &result.rates {
        if let Some(existing) = plan
            .fx_rates
            .iter_mut()
            .find(|r| r.date == date && r.currency == *currency)
        {
            existing.rate = Amount::Number(*rate);
        } else {
            plan.fx_rates.push(ExchangeRate {
                id: id(),
                date: date.clone(),
                currency: currency.clone(),
                rate: Amount::Number(*rate),
                ..ExchangeRate::default()
            });
        }
    }
    for rate in &result.historical {
        // The first stored observation for a historical date is retained.
        if let Some(existing) = plan
            .fx_rates
            .iter_mut()
            .find(|r| r.currency == rate.currency && r.date == rate.date)
        {
            if existing.rate.value() <= 0.0 {
                existing.rate = rate.rate.clone();
            }
        } else {
            plan.fx_rates.push(rate.clone());
        }
    }
    plan.normalize();
}

/// Daily historical observations, with the provider's documented alternate host.
/// Never substitute today's price for a requested historical date.
pub fn fetch_with_history(
    currencies: &[String],
    dates: &[(String, Vec<String>)],
) -> Result<RateResult, String> {
    let mut result = fetch(currencies).unwrap_or_else(|error| RateResult {
        date: today(),
        rates: vec![],
        historical: vec![],
        unavailable: currencies.to_vec(),
        warnings: vec![error],
    });
    let client = Client::builder()
        .timeout(Duration::from_secs(4))
        .user_agent("Amigo/0.1")
        .build()
        .map_err(|e| e.to_string())?;
    for (date, codes) in dates {
        if chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
            continue;
        }
        let mut data = None;
        for url in [
            format!(
                "https://cdn.jsdelivr.net/npm/@fawazahmed0/currency-api@{date}/v1/currencies/usd.json"
            ),
            format!("https://{date}.currency-api.pages.dev/v1/currencies/usd.json"),
        ] {
            if let Ok(value) = get(&client, &url)
                && value["date"].as_str() == Some(date)
            {
                data = Some(value);
                break;
            }
        }
        if let Some(data) = data {
            result
                .historical
                .extend(parse_historical(&data, date, codes));
        }
    }
    Ok(result)
}

fn parse_historical(value: &Value, date: &str, currencies: &[String]) -> Vec<ExchangeRate> {
    if value["date"].as_str() != Some(date) {
        return vec![];
    }
    currencies
        .iter()
        .filter_map(|currency| {
            let code = currency.trim().to_uppercase();
            let units = if code == "USD" {
                1.0
            } else {
                value["usd"][code.to_lowercase()].as_f64()?
            };
            if units <= 0.0 || !units.is_finite() {
                return None;
            }
            Some(ExchangeRate {
                id: id(),
                date: date.into(),
                currency: code,
                rate: Amount::Number(1.0 / units),
                ..ExchangeRate::default()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn historical_payload_checks_date_and_inverts_usd_quotes_without_fabricating_missing_prices() {
        let value = serde_json::json!({"date":"2026-09-10", "usd":{"eur":0.8,"uah":40.0,"eth":0.0005,"usdt":1.001,"bad":0}});
        let codes = ["EUR", "UAH", "ETH", "USDT", "BAD", "XYZ"].map(String::from);
        let rows = parse_historical(&value, "2026-09-10", &codes);
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows.iter()
                .find(|r| r.currency == "EUR")
                .unwrap()
                .rate
                .value(),
            1.25
        );
        assert_eq!(
            rows.iter()
                .find(|r| r.currency == "ETH")
                .unwrap()
                .rate
                .value(),
            2000.0
        );
        assert!(rows.iter().all(|r| r.date == "2026-09-10"));
        assert!(parse_historical(&value, "2026-09-09", &codes).is_empty());
    }
    #[test]
    fn refresh_preserves_history_and_unavailable_manual_rates() {
        let mut plan = FinancePlan::empty();
        plan.fx_rates = vec![
            ExchangeRate {
                id: id(),
                date: "2000-01-01".into(),
                currency: "EUR".into(),
                rate: Amount::Number(1.0),
                ..ExchangeRate::default()
            },
            ExchangeRate {
                id: id(),
                date: today(),
                currency: "XYZ".into(),
                rate: Amount::Number(2.0),
                ..ExchangeRate::default()
            },
        ];
        let result = RateResult {
            date: today(),
            rates: vec![("EUR".into(), 1.2)],
            historical: vec![],
            unavailable: vec!["XYZ".into()],
            warnings: vec![],
        };
        merge(&mut plan, &result);
        assert_eq!(plan.fx_rates.len(), 3);
        assert_eq!(
            plan.fx_rates
                .iter()
                .find(|r| r.currency == "XYZ")
                .unwrap()
                .rate
                .value(),
            2.0
        );
        merge(&mut plan, &result);
        assert_eq!(plan.fx_rates.len(), 3);
        assert_eq!(
            plan.fx_rates
                .iter()
                .find(|r| r.date == "2000-01-01")
                .unwrap()
                .rate
                .value(),
            1.0
        );
    }
    #[test]
    #[ignore = "requires public exchange rate API network access"]
    fn live_fiat_rates() {
        let currencies = ["USD", "EUR", "UAH", "USDC", "USDT", "ETH"].map(String::from);
        let result = fetch(&currencies).unwrap();
        assert!(
            result.unavailable.is_empty(),
            "Unavailable: {:?}; warnings: {:?}",
            result.unavailable,
            result.warnings
        );
        assert_eq!(
            result.rates.iter().find(|(c, _)| c == "USD").unwrap().1,
            1.0
        );
        assert!(result.rates.iter().find(|(c, _)| c == "EUR").unwrap().1 > 0.0);
        let history =
            fetch_with_history(&currencies, &[("2026-09-10".into(), currencies.to_vec())]).unwrap();
        assert_eq!(history.historical.len(), currencies.len());
        assert!(
            history
                .historical
                .iter()
                .all(|r| r.date == "2026-09-10" && r.rate.value() > 0.0)
        );
    }
}
