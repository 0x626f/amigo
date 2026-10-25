//! Library-wide reference catalogues. Vaults retain portable snapshots, not separate editors.
use crate::{library::Library, model::*, storage::atomic_write};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Mutex,
};

// Record autosaves and settings confirmations can touch usage at the same time.
static WRITER: Mutex<()> = Mutex::new(());
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FinanceSettings {
    version: u32,
    pub currencies: Vec<Currency>,
    pub chains: Vec<Chain>,
    pub countries: Vec<StoredValue>,
    vaults: BTreeMap<String, VaultReferences>,
    history: ReferenceHistory,
}
impl Default for FinanceSettings {
    fn default() -> Self {
        Self {
            version: 1,
            currencies: vec![],
            chains: vec![],
            countries: vec![],
            vaults: BTreeMap::new(),
            history: ReferenceHistory::default(),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct ReferenceHistory {
    currencies: Vec<Currency>,
    chains: Vec<Chain>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct VaultReferences {
    // The original entries identify aliases even when another vault reused an ID.
    currencies: Vec<(Currency, String)>,
    chains: Vec<(Chain, String)>,
    countries: Vec<(StoredValue, String)>,
    used: BTreeSet<String>,
}
fn read(root: &Path) -> Result<FinanceSettings> {
    let path = root.join("finance.json");
    if !path.exists() {
        return Ok(FinanceSettings::default());
    }
    let settings: FinanceSettings =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Cannot read shared Finance settings: {e}"))?;
    if settings.version != 1 {
        return Err(
            "Unsupported shared Finance settings version. Existing files have been preserved."
                .into(),
        );
    }
    Ok(settings)
}
fn write(root: &Path, settings: &FinanceSettings) -> Result<()> {
    atomic_write(
        &root.join("finance.json"),
        &serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?,
    )
}
fn key(value: &str) -> String {
    value.trim().to_lowercase()
}
fn address_key(address: &str) -> String {
    if address.starts_with("0x") {
        address.to_lowercase()
    } else {
        address.to_owned()
    }
}
fn remap(value: &mut String, aliases: &BTreeMap<String, String>) {
    if let Some(id) = aliases.get(value) {
        *value = id.clone();
    }
}
impl FinanceSettings {
    pub fn plan(&self) -> FinancePlan {
        let mut plan = FinancePlan::empty();
        self.apply(&mut plan);
        // The editor's scratch plan must not protect an unused reporting currency.
        if let Some(currency) = self
            .currencies
            .iter()
            .find(|c| self.vaults.values().any(|s| s.used.contains(&c.id)))
        {
            plan.meta.reporting_currency = currency.name.clone();
            plan.meta.reporting_currency_id = currency.id.clone();
        }
        plan
    }
    pub fn apply(&self, plan: &mut FinancePlan) {
        plan.currencies = self.currencies.clone();
        plan.chains = self.chains.clone();
        plan.tax_countries = self.countries.clone();
        plan.normalize();
    }
    fn merge(&mut self, vault: &str, plan: &mut FinancePlan) {
        let chain_history: Vec<_> = self
            .history
            .chains
            .iter()
            .map(|r| (r.clone(), r.id.clone()))
            .chain(self.vaults.values().flat_map(|s| s.chains.iter().cloned()))
            .collect();
        let currency_history: Vec<_> = self
            .history
            .currencies
            .iter()
            .map(|r| (r.clone(), r.chain_id.clone(), r.id.clone()))
            .chain(self.vaults.values().flat_map(|s| {
                s.currencies.iter().map(|(original, target)| {
                    let chain = s
                        .chains
                        .iter()
                        .find(|(c, _)| c.id == original.chain_id)
                        .map(|(_, id)| id.clone())
                        .unwrap_or_else(|| original.chain_id.clone());
                    (original.clone(), chain, target.clone())
                })
            }))
            .collect();
        let source = self.vaults.entry(vault.into()).or_default();
        let mut chains = BTreeMap::new();
        for row in &plan.chains {
            let target = if let Some((_, target)) =
                source.chains.iter().find(|(original, _)| original == row)
            {
                target.clone()
            } else if let Some(existing) = self
                .chains
                .iter()
                .find(|c| c.id == row.id && source.chains.iter().any(|(_, id)| id == &row.id))
            {
                existing.id.clone()
            } else if let Some((_, target)) = chain_history.iter().find(|(original, target)| {
                key(&original.name) == key(&row.name) && self.chains.iter().any(|c| c.id == *target)
            }) {
                target.clone()
            } else if let Some(existing) =
                self.chains.iter().find(|c| key(&c.name) == key(&row.name))
            {
                existing.id.clone()
            } else {
                let mut added = row.clone();
                if self.chains.iter().any(|c| c.id == added.id) {
                    added.id = id();
                }
                self.chains.push(added.clone());
                added.id
            };
            if !source.chains.iter().any(|(original, _)| original == row) {
                source.chains.push((row.clone(), target.clone()));
            }
            chains.insert(row.id.clone(), target);
        }
        let mut currencies = BTreeMap::new();
        for row in &plan.currencies {
            let target =
                if let Some((_, target)) = source
                    .currencies
                    .iter()
                    .find(|(original, _)| original == row)
                {
                    target.clone()
                } else if let Some(existing) = self.currencies.iter().find(|c| {
                    c.id == row.id && source.currencies.iter().any(|(_, id)| id == &row.id)
                }) {
                    existing.id.clone()
                } else {
                    let mut added = row.clone();
                    remap(&mut added.chain_id, &chains);
                    if let Some((_, _, target)) =
                        currency_history.iter().find(|(original, chain, target)| {
                            key(&original.name) == key(&added.name)
                                && *chain == added.chain_id
                                && address_key(&original.address) == address_key(&added.address)
                                && original.native_asset == added.native_asset
                                && self.currencies.iter().any(|c| c.id == *target)
                        })
                    {
                        target.clone()
                    } else if let Some(existing) = self.currencies.iter().find(|c| {
                        key(&c.name) == key(&added.name)
                            && c.chain_id == added.chain_id
                            && address_key(&c.address) == address_key(&added.address)
                            && c.native_asset == added.native_asset
                    }) {
                        existing.id.clone()
                    } else {
                        if self.currencies.iter().any(|c| c.id == added.id) {
                            added.id = id();
                        }
                        self.currencies.push(added.clone());
                        added.id
                    }
                };
            if !source
                .currencies
                .iter()
                .any(|(original, _)| original == row)
            {
                source.currencies.push((row.clone(), target.clone()));
            }
            currencies.insert(row.id.clone(), target);
        }
        let mut countries = BTreeMap::new();
        for row in &plan.tax_countries {
            let target = if let Some((_, target)) = source
                .countries
                .iter()
                .find(|(original, _)| original == row)
            {
                target.clone()
            } else if let Some(existing) = self
                .countries
                .iter()
                .find(|c| key(&c.name) == key(&row.name))
            {
                existing.id.clone()
            } else {
                let mut added = row.clone();
                if self.countries.iter().any(|c| c.id == added.id) {
                    added.id = id();
                }
                self.countries.push(added.clone());
                added.id
            };
            if !source.countries.iter().any(|(original, _)| original == row) {
                source.countries.push((row.clone(), target.clone()));
            }
            countries.insert(row.id.clone(), target);
        }
        remap(&mut plan.meta.reporting_currency_id, &currencies);
        for r in &mut plan.income {
            remap(&mut r.currency_id, &currencies);
            remap(&mut r.tax_country_id, &countries);
        }
        for r in &mut plan.expenses {
            remap(&mut r.currency_id, &currencies);
        }
        for r in &mut plan.savings {
            remap(&mut r.currency_id, &currencies);
        }
        for r in &mut plan.fx_rates {
            remap(&mut r.currency_id, &currencies);
        }
        for r in &mut plan.budgets {
            remap(&mut r.currency_id, &currencies);
            for e in &mut r.expenses {
                remap(&mut e.currency_id, &currencies);
            }
        }
        source.used = used_references(plan);
        self.apply(plan);
    }
    pub fn used(&self, library: &Library) -> BTreeSet<String> {
        let mut used: BTreeSet<_> = library
            .vaults
            .iter()
            .filter_map(|v| self.vaults.get(&v.entry.id))
            .flat_map(|s| s.used.iter().cloned())
            .collect();
        if library
            .vaults
            .iter()
            .any(|v| !self.vaults.contains_key(&v.entry.id))
        {
            // Unopened encrypted or unreadable vaults cannot supply their usage yet.
            used.extend(self.currencies.iter().map(|r| r.id.clone()));
            used.extend(self.chains.iter().map(|r| r.id.clone()));
        }
        used
    }
}
pub fn used_references(plan: &FinancePlan) -> BTreeSet<String> {
    let mut used = BTreeSet::from([plan.meta.reporting_currency_id.clone()]);
    used.extend(
        plan.income
            .iter()
            .flat_map(|r| [r.currency_id.clone(), r.tax_country_id.clone()]),
    );
    used.extend(plan.expenses.iter().map(|r| r.currency_id.clone()));
    used.extend(plan.savings.iter().map(|r| r.currency_id.clone()));
    used.extend(plan.fx_rates.iter().map(|r| r.currency_id.clone()));
    for budget in &plan.budgets {
        used.insert(budget.currency_id.clone());
        used.extend(budget.expenses.iter().map(|r| r.currency_id.clone()));
    }
    used.remove("");
    used
}
impl Library {
    pub fn finance_settings(&self) -> Result<FinanceSettings> {
        read(&self.root)
    }
    pub(crate) fn initialize_finance_settings(&self) -> Result<()> {
        let _guard = WRITER.lock().map_err(|e| e.to_string())?;
        let mut shared = read(&self.root)?;
        let previous = shared.clone();
        for vault in &self.vaults {
            if !vault.encrypted
                && let Ok(mut plan) = self.load_plain_vault(vault)
            {
                shared.merge(&vault.entry.id, &mut plan);
            }
        }
        if shared.currencies.is_empty() && !self.root.join("finance.json").exists() {
            let plan = FinancePlan::empty();
            shared.currencies = plan.currencies;
            shared.chains = plan.chains;
            shared.countries = plan.tax_countries;
        }
        if shared != previous || !self.root.join("finance.json").exists() {
            write(&self.root, &shared)?;
        }
        Ok(())
    }
    /// Adopt legacy references, remap collisions, and hydrate the current shared catalogue.
    pub fn share_finance_references(&self, vault: &str, plan: &mut FinancePlan) -> Result<()> {
        let _guard = WRITER.lock().map_err(|e| e.to_string())?;
        let mut shared = read(&self.root)?;
        let previous = shared.clone();
        let mut hydrated = plan.clone();
        shared.merge(vault, &mut hydrated);
        if shared != previous {
            write(&self.root, &shared)?;
        }
        *plan = hydrated;
        Ok(())
    }
    pub fn save_finance_settings(&self, plan: &FinancePlan) -> Result<()> {
        let _guard = WRITER.lock().map_err(|e| e.to_string())?;
        let mut shared = read(&self.root)?;
        let used = shared.used(self);
        if shared
            .currencies
            .iter()
            .any(|r| used.contains(&r.id) && !plan.currencies.iter().any(|v| v.id == r.id))
        {
            return Err(
                "This currency is in use by a vault. Reassign its records before removing it."
                    .into(),
            );
        }
        if plan
            .currencies
            .iter()
            .any(|r| !r.chain_id.is_empty() && !plan.chains.iter().any(|v| v.id == r.chain_id))
            || shared
                .chains
                .iter()
                .any(|r| used.contains(&r.id) && !plan.chains.iter().any(|v| v.id == r.id))
        {
            return Err("This chain is in use. Reassign its currencies before removing it.".into());
        }
        for row in &shared.currencies {
            if !shared.history.currencies.contains(row) {
                shared.history.currencies.push(row.clone());
            }
        }
        for row in &shared.chains {
            if !shared.history.chains.contains(row) {
                shared.history.chains.push(row.clone());
            }
        }
        shared.currencies = plan.currencies.clone();
        shared.chains = plan.chains.clone();
        shared.countries = plan.tax_countries.clone();
        write(&self.root, &shared)
    }
    pub(crate) fn forget_finance_vault(&self, vault: &str) -> Result<()> {
        let _guard = WRITER.lock().map_err(|e| e.to_string())?;
        let mut shared = read(&self.root)?;
        shared.vaults.remove(vault);
        write(&self.root, &shared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backup::{Archive, Scope},
        library::VaultKey,
    };
    use serde_json::json;
    struct Temp(std::path::PathBuf);
    impl Temp {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("amigo-shared-finance-{}", id())))
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn legacy(chain: &str, symbol: &str, country: &str) -> FinancePlan {
        plan_from_value(json!({
            "meta":{"referenceDefaultsVersion":1,"reportingCurrency":"USD"},
            "chains":[{"id":"same-chain","name":chain}],
            "currencies":[{"id":"usd-alias","name":"USD"},{"id":"same-token","name":symbol,"chainId":"same-chain","address":"TAbc"}],
            "taxCountries":[{"id":"same-country","name":country}],
            "income":[{"id":"income","currency":"USD","currencyId":"usd-alias","grossAmount":50,"taxCountry":country,"taxCountryId":"same-country"}],
            "expenses":[{"id":"expense","currency":symbol,"currencyId":"same-token","amount":5}],
            "savings":[{"id":"saving","currency":symbol,"currencyId":"same-token","amount":10}],
            "budgets":[{"id":"budget","currency":symbol,"currencyId":"same-token","label":"Trip","expenses":[{"id":"nested","label":"Hotel","currency":symbol,"currencyId":"same-token","amount":2}]}],
            "fxRates":[{"id":"rate","currency":symbol,"currencyId":"same-token","date":"2026-10-01","rate":2}]
        })).unwrap()
    }
    #[test]
    fn shared_catalogue_merges_duplicates_and_remaps_collisions_without_changing_records() {
        let temp = Temp::new();
        let mut library = Library::open(temp.0.clone(), None).unwrap();
        let original_a = legacy("Network A", "AAA", "Spain");
        let original_b = legacy("Network B", "BBB", "France");
        let a = library.create_vault("A", &original_a, None).unwrap();
        let b = library.create_vault("B", &original_b, None).unwrap();
        // Existing plain vaults migrate together when the library next opens.
        let library = Library::open(temp.0.clone(), None).unwrap();
        let shared = library.finance_settings().unwrap();
        assert_eq!(
            shared.currencies.iter().filter(|r| r.name == "USD").count(),
            1
        );
        let mut plans = Vec::new();
        for (vault, original, name) in [
            (&a, &original_a, "Network A"),
            (&b, &original_b, "Network B"),
        ] {
            let mut plan = library.load_plain_vault(vault).unwrap();
            library
                .share_finance_references(&vault.entry.id, &mut plan)
                .unwrap();
            let currency = plan
                .currencies
                .iter()
                .find(|r| r.name == original.expenses[0].currency)
                .unwrap();
            assert_eq!(
                plan.chains
                    .iter()
                    .find(|r| r.id == currency.chain_id)
                    .unwrap()
                    .name,
                name
            );
            for linked in [
                &plan.expenses[0].currency_id,
                &plan.savings[0].currency_id,
                &plan.budgets[0].currency_id,
                &plan.budgets[0].expenses[0].currency_id,
                &plan.fx_rates[0].currency_id,
            ] {
                assert_eq!(linked, &currency.id);
            }
            assert_eq!(plan.expenses[0].id, original.expenses[0].id);
            assert_eq!(plan.expenses[0].audit, original.expenses[0].audit);
            assert_eq!(plan.expenses[0].amount, original.expenses[0].amount);
            assert_eq!(plan.income[0].tax_country, original.income[0].tax_country);
            assert_eq!(
                plan.tax_countries
                    .iter()
                    .find(|r| r.id == plan.income[0].tax_country_id)
                    .unwrap()
                    .name,
                original.income[0].tax_country
            );
            library.save_vault(vault, &plan, None).unwrap();
            let mut reloaded = library.load_plain_vault(vault).unwrap();
            library
                .share_finance_references(&vault.entry.id, &mut reloaded)
                .unwrap();
            assert_eq!(plan, reloaded);
            plans.push(plan);
        }
        assert_ne!(
            plans[0].expenses[0].currency_id,
            plans[1].expenses[0].currency_id
        );
        assert_ne!(
            plans[0].income[0].tax_country_id,
            plans[1].income[0].tax_country_id
        );
    }
    #[test]
    fn shared_renames_and_deletions_survive_stale_vault_snapshots_and_setup_restore() {
        let temp = Temp::new();
        let mut library = Library::open(temp.0.clone(), None).unwrap();
        let original = legacy("Custom network", "AAA", "Spain");
        let vault = library.create_vault("Old", &original, None).unwrap();
        let mut plan = original.clone();
        library
            .share_finance_references(&vault.entry.id, &mut plan)
            .unwrap();
        let mut settings = library.finance_settings().unwrap().plan();
        let chain_id = settings
            .chains
            .iter()
            .find(|r| r.name == "Custom network")
            .unwrap()
            .id
            .clone();
        settings
            .chains
            .iter_mut()
            .find(|r| r.id == chain_id)
            .unwrap()
            .name = "Renamed network".into();
        settings.currencies.retain(|r| r.name != "ETH");
        library.save_finance_settings(&settings).unwrap();
        let mut stale = original;
        library
            .share_finance_references(&vault.entry.id, &mut stale)
            .unwrap();
        assert_eq!(
            stale.chains.iter().find(|r| r.id == chain_id).unwrap().name,
            "Renamed network"
        );
        assert!(!stale.currencies.iter().any(|r| r.name == "ETH"));
        let reopened = Library::open(temp.0.clone(), None).unwrap();
        assert_eq!(reopened.finance_settings().unwrap().plan(), settings);
        let archive = Archive::capture(&reopened, Scope::Setup).unwrap();
        let destination = Temp::new();
        let target = Library::open(destination.0.clone(), None).unwrap();
        archive.import(&target).unwrap();
        assert_eq!(
            Library::open(destination.0.clone(), None)
                .unwrap()
                .finance_settings()
                .unwrap()
                .plan(),
            settings
        );
    }
    #[test]
    fn encrypted_vaults_adopt_shared_catalogues_on_unlock_and_protect_used_currencies() {
        let temp = Temp::new();
        let mut library = Library::open(temp.0.clone(), None).unwrap();
        let original = legacy("Private network", "PRV", "Spain");
        let key = VaultKey::new("shared-references-test").unwrap();
        let vault = library
            .create_vault("Encrypted", &original, Some(&key))
            .unwrap();
        let shared = library.finance_settings().unwrap();
        assert!(shared.used(&library).contains(&shared.currencies[0].id));
        let path = library.vault_path(&vault.entry.id).unwrap();
        let (mut plan, _) =
            VaultKey::unlock(&path, &vault.entry.id, "shared-references-test").unwrap();
        library
            .share_finance_references(&vault.entry.id, &mut plan)
            .unwrap();
        let mut settings = library.finance_settings().unwrap().plan();
        settings.currencies.retain(|r| r.name != "PRV");
        assert!(
            library
                .save_finance_settings(&settings)
                .unwrap_err()
                .contains("in use")
        );
        library.save_vault(&vault, &plan, Some(&key)).unwrap();
        let mut reloaded = key.load(&path, &vault.entry.id).unwrap();
        library
            .share_finance_references(&vault.entry.id, &mut reloaded)
            .unwrap();
        assert_eq!(reloaded, plan);
        assert!(!path.join("records").exists());
        let global = std::fs::read_to_string(temp.0.join("finance.json")).unwrap();
        assert!(
            !global.contains("\"grossAmount\"")
                && !global.contains("\"amount\"")
                && !global.contains("\"audit\"")
        );
    }
    #[test]
    fn first_unlock_after_a_shared_rename_reuses_the_original_currency_and_network() {
        let mut shared = FinanceSettings::default();
        let original = legacy("Original network", "AAA", "Spain");
        let mut plain = original.clone();
        shared.merge("plain", &mut plain);
        let currency_id = plain.expenses[0].currency_id.clone();
        let chain_id = shared
            .currencies
            .iter()
            .find(|c| c.id == currency_id)
            .unwrap()
            .chain_id
            .clone();
        shared
            .currencies
            .iter_mut()
            .find(|c| c.id == currency_id)
            .unwrap()
            .name = "NEW".into();
        shared
            .chains
            .iter_mut()
            .find(|c| c.id == chain_id)
            .unwrap()
            .name = "Renamed network".into();
        let mut unlocked = original;
        shared.merge("encrypted-first-unlock", &mut unlocked);
        assert_eq!(unlocked.expenses[0].currency_id, currency_id);
        assert_eq!(unlocked.expenses[0].currency, "NEW");
        assert_eq!(
            unlocked
                .chains
                .iter()
                .find(|c| c.id == chain_id)
                .unwrap()
                .name,
            "Renamed network"
        );
        assert!(!shared.currencies.iter().any(|c| c.name == "AAA"));
        assert!(!shared.chains.iter().any(|c| c.name == "Original network"));
    }

    #[test]
    fn case_sensitive_token_addresses_are_not_merged() {
        let mut shared = FinanceSettings::default();
        let mut a = legacy("Tron", "USDT", "Spain");
        let mut b = a.clone();
        b.currencies
            .iter_mut()
            .find(|r| r.name == "USDT")
            .unwrap()
            .address = "Tabc".into();
        shared.merge("a", &mut a);
        shared.merge("b", &mut b);
        assert_eq!(
            shared
                .currencies
                .iter()
                .filter(|r| r.name == "USDT")
                .count(),
            2
        );
        assert_ne!(a.expenses[0].currency_id, b.expenses[0].currency_id);
    }
}
