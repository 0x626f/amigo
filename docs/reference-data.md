# Finance reference data

Fresh vaults include EUR, USD, UAH, USDC on Ethereum Mainnet, USDT on Ethereum Mainnet, USDT on Tron, and native ETH on Ethereum Mainnet. Currency symbols remain EUR/USD/UAH/USDC/USDT/ETH for calculations; record IDs distinguish networks. ETH is a native asset with no token-contract address.

Default metadata is added once using `meta.referenceDefaultsVersion`. Existing matching mappings, IDs, chain RPC URLs, and custom values are retained. Later edits and deletions of currency defaults are respected. This marker, token mappings, and native-asset flags are saved in plain and encrypted vaults through the existing persistence format.

Token defaults were checked on 18 October 2026 against the issuers’ primary documentation:

- USDC Ethereum: `0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48` — [Circle contract addresses](https://developers.circle.com/stablecoins/usdc-contract-addresses).
- USDT Ethereum: `0xdAC17F958D2ee523a2206206994597C13D831ec7` and USDT Tron: `TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t` — [Tether supported protocols](https://tether.to/en/supported-protocols/).
- ETH is Ethereum’s native currency — [Ethereum documentation](https://ethereum.org/developers/docs/intro-to-ethereum/).

Default chain entries use the Ethereum Mainnet endpoint from [PublicNode](https://www.publicnode.com/?network=Mainnet) and the mainnet HTTP endpoint documented by [TronGrid](https://www.trongrid.io/documents). Existing chain endpoints are preserved. The app’s exchange-rate refresh uses the [Currency API](https://github.com/fawazahmed0/exchange-api). Its documented daily and historical endpoints cover fiat and common cryptocurrencies; the alternate Cloudflare host is used when the primary CDN fails. Response dates must match historical requests. Unsupported or unavailable prices remain pending, and previously stored record snapshots are preserved. Creation timestamps and snapshots are stored with each financial record, including nested budget expenses.

## Country catalogue

`crates/core/src/countries.tsv` bundles the 249 ISO 3166-1 countries and territories. It stores alpha-2 code, alpha-3 code, a short English display name, and aliases. Names and codes follow [ISO 3166](https://www.iso.org/iso-3166-country-codes.html); the [UN M49 overview](https://unstats.un.org/unsd/methodology/m49/overview/) provides the corresponding country/area names and ISO codes.

The snapshot was extracted from the installed **iso-codes 4.20.1** `iso_3166-1.json` on 18 October 2026. Changes for Amigo: converted the data to TSV, omitted unrelated fields, sorted display names, selected shorter English names for some entries, and retained original names as aliases. This is an offline catalogue; updating it requires a new app build.

The original dataset comes from the [iso-codes project](https://salsa.debian.org/iso-codes-team/iso-codes), copyright the iso-codes contributors, and is distributed under LGPL-2.1-or-later. Its license is included at `crates/core/COUNTRY-DATA-LICENSE`. Distribution packages include that license, this notice, and the modified TSV source under `/usr/share/doc/amigo/`.

Country names and ISO codes can be searched in the income dropdown. Only choosing a listed entry changes the country field. The field remains optional. Stored Values → Countries displays the catalogue without edit/delete controls. Known imported country names and codes are normalized while retaining linked IDs; unrecognized historical values remain stored, but edited rows must choose a listed country or clear the field before saving.
