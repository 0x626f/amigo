//! Bundled ISO 3166-1 country data and the initial finance reference catalogue.
//! Sources and snapshot provenance: docs/reference-data.md.
use crate::model::{Chain, Currency, FinancePlan, StoredValue};
use std::sync::OnceLock;

pub struct Country {
    pub code: &'static str,
    pub alpha3: &'static str,
    pub name: &'static str,
    aliases: &'static str,
}
pub fn countries() -> &'static [Country] {
    static COUNTRIES: OnceLock<Vec<Country>> = OnceLock::new();
    COUNTRIES.get_or_init(|| {
        include_str!("countries.tsv")
            .lines()
            .map(|line| {
                let mut fields = line.split('\t');
                Country {
                    code: fields.next().unwrap(),
                    alpha3: fields.next().unwrap(),
                    name: fields.next().unwrap(),
                    aliases: fields.next().unwrap_or_default(),
                }
            })
            .collect()
    })
}
pub fn country(value: &str) -> Option<&'static Country> {
    let value = value.trim();
    countries().iter().find(|c| {
        c.name.eq_ignore_ascii_case(value)
            || c.code.eq_ignore_ascii_case(value)
            || c.alpha3.eq_ignore_ascii_case(value)
            || c.aliases
                .split('|')
                .any(|s| !s.is_empty() && s.eq_ignore_ascii_case(value))
            || (c.code == "GB" && value.eq_ignore_ascii_case("UK"))
    })
}

pub(crate) fn seed(plan: &mut FinancePlan) {
    if plan.meta.reference_defaults_version >= 1 {
        return;
    }
    let ethereum = chain(
        plan,
        "ethereum-mainnet",
        "Ethereum Mainnet",
        &["Ethereum", "Ethereum Mainnet", "ETH Mainnet"],
    );
    let tron = chain(plan, "tron-mainnet", "Tron", &["Tron", "Tron Mainnet"]);
    for (symbol, network, address, native_asset) in [
        ("EUR", "", "", false),
        ("USD", "", "", false),
        ("UAH", "", "", false),
        (
            "USDC",
            ethereum.as_str(),
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
            false,
        ),
        (
            "USDT",
            ethereum.as_str(),
            "0xdAC17F958D2ee523a2206206994597C13D831ec7",
            false,
        ),
        (
            "USDT",
            tron.as_str(),
            "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t",
            false,
        ),
        ("ETH", ethereum.as_str(), "", true),
    ] {
        if let Some(existing) = plan.currencies.iter_mut().find(|c| {
            c.name.trim().eq_ignore_ascii_case(symbol)
                && c.chain_id == network
                && if network == ethereum {
                    c.address.eq_ignore_ascii_case(address)
                } else {
                    c.address == address
                }
        }) {
            if native_asset {
                existing.native_asset = true;
            }
        } else {
            plan.currencies.push(Currency {
                id: available_id(
                    &format!(
                        "default-currency-{}-{}",
                        symbol.to_lowercase(),
                        if network.is_empty() { "fiat" } else { network }
                    ),
                    plan.currencies.iter().map(|c| c.id.as_str()),
                ),
                name: symbol.into(),
                chain_id: network.into(),
                address: address.into(),
                native_asset,
            });
        }
    }
    let existing_countries: std::collections::BTreeSet<_> = plan
        .tax_countries
        .iter()
        .filter_map(|r| country(&r.name).map(|c| c.code))
        .collect();
    for c in countries() {
        if !existing_countries.contains(c.code) {
            plan.tax_countries.push(StoredValue {
                id: available_id(
                    &format!("country-{}", c.code),
                    plan.tax_countries.iter().map(|r| r.id.as_str()),
                ),
                name: c.name.into(),
            });
        }
    }
    plan.meta.reference_defaults_version = 1;
}
fn chain(plan: &mut FinancePlan, id: &str, name: &str, aliases: &[&str]) -> String {
    if let Some(chain) = plan.chains.iter().find(|c| {
        aliases
            .iter()
            .any(|name| c.name.trim().eq_ignore_ascii_case(name))
    }) {
        return chain.id.clone();
    }
    let id = available_id(id, plan.chains.iter().map(|c| c.id.as_str()));
    plan.chains.push(Chain {
        id: id.clone(),
        name: name.into(),
    });
    id
}

fn available_id<'a>(preferred: &str, existing: impl Iterator<Item = &'a str>) -> String {
    let ids: std::collections::BTreeSet<_> = existing.collect();
    let mut candidate = preferred.to_string();
    let mut suffix = 1;
    while ids.contains(candidate.as_str()) {
        candidate = format!("{preferred}-{suffix}");
        suffix += 1;
    }
    candidate
}
