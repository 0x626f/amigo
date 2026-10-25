# Finance behavior

Read the current `crates/finance/src/views/inline.rs`, `crates/core/src/model.rs`, and `crates/core/src/audit.rs` for the affected record type.

## Editing and references

Currencies, Chains, and Countries are global Finance child pages; Clients, Accounts, Expenses, and Savings belong to each vault. Canonical shared catalogues live in `finance.json`; vault reference lists are portable snapshots hydrated from the shared catalogue on open/unlock. Merge legacy mappings, remap conflicting IDs, preserve rename aliases, and protect references used by other vaults. Encrypted vaults contribute their catalogues when unlocked. Shared settings must work while the selected vault is locked. Do not add a generic Stored Values page or type selector. Expenses owns its categories; Savings owns its separate categories. Obsolete expense and savings labels, the generic labels catalogue, chain RPC URLs, and budget label IDs are ignored on load and omitted on save. Old savings labels migrate once to categories at the JSON import boundary. Reuse reference tables and currency dialogs with separate settings editors per global page and per vault page. Preserve unfinished drafts across settings pages, discard them when settings closes, and commit only confirmed rows to their owning shared catalogue or vault. Settings edits must not alter transaction filters or confirm background drafts. Render currency details after the parent settings modal so it owns keyboard input.

- Ordinary Income, Expense, Saving, Budget, Account, Chain, and stored-value records use inline editing. Currency/token details use their dedicated dialog. Create inserts a draft, not an immediate persisted record.
- Currency fields select available currency IDs. A symbol can occur on multiple networks; preserve the chosen ID and network metadata.
- Country fields use the bundled ISO country/territory catalogue. None is allowed for optional countries and clears the stored value. Preserve imported historical data during migration; edited rows must validate their country selection.
- Expense categories come from Expenses Settings; Savings Category comes from Savings Settings. They are separate catalogues. A typed transaction field must not silently create a category. Renames retain IDs and assignments; a category in use needs reassignment before removal.
- Accounts share stable IDs with the existing Income/Savings account reference list. Manage accounts only in Settings → Finance → the vault → Accounts, with Label as the sole editable property. ID and audit data remain internal; legacy Country, Principal, and Comment fields are ignored when loading and omitted when saving. Keep Stored Values, Income, and Savings references consistent instead of creating a second unrelated account catalogue.
- Values in use cannot be deleted while retaining dangling links. Preserve metadata during renames and normalization; resolve IDs before falling back to names.

Finance drafts share the `Record` enum and `Editor` map in `crates/finance/src/views/mod.rs`. Batch confirmation is in `inline::confirm_pending`. It applies transaction edits before reference renames, then normalizes the plan. Retain this ordering when extending it so renames cannot be overwritten by another pending row.

## Comments and dates

Saving and budget-expense comments use a large multiline popup. Apply Comment or Ctrl/Command+Enter updates the pending parent record. Escape or clicking outside discards the comment draft. Confirm the row afterward to persist it.

Editable dates use ISO strings and a calendar inside the input. Date filters display full month names such as September 2026. Saved dated records sort newest first, with genuinely undated legacy records last. Do not re-sort unfinished rows in a way that moves the current edit between keystrokes.

## Budgets and Wallet

A budget plans a trip or project. Its columns are Label (free text), Allowance, Currency, Spent, Remaining. Each budget owns expense subrows with Label, Amount, Currency, Comment. Creating or editing a subrow changes its parent draft.

Spent converts each nested expense into the parent budget currency using the expense’s creation-date rate. Remaining is allowance minus spent. If any required conversion is unavailable, show Pending FX and no partial spent/remaining result. Nested expenses also count toward Wallet spending and history; the allowance itself is not spending. Do not restore the former monthly expense-label target calculation.

Wallet uses `wallet_month`, independent of the other views’ `month` filter, for monthly income, expenses, balance and commitments. Default and Clear Filters use the current month; there is no All dates option. Savings remain independent of this period, excluding Deduct balances.

Wallet’s history defaults to the last 12 calendar months and can extend to 24/36/60 or all history. Its range is independent of the summary Date filter. Actual includes received income and all scheduled standalone expenses; nested budget expenses count as recorded spending. Recurring expenses start in their due month. Preserve these distinctions when adding summaries or plots.

## Creation dates and exchange snapshots

Income, Expense, Saving, Budget, and BudgetExpense carry `RecordAudit`. New records receive a creation timestamp, distinct from an editable transaction/due date. Copies that create new records receive fresh audits. Account reference records also retain their creation metadata but have no intrinsic transaction currency.

Capture dated rates for USD, EUR, enabled reporting currency, and a parent budget’s currency where applicable. Record snapshots store quote units per base unit; the shared rate cache stores USD per one source unit. Preserve that direction in calculations.

Once a snapshot has a value, later refreshes do not overwrite it. A changed reporting or record currency may need an additional historical pair. An unavailable rate stays pending and retries during exchange sync. Do not substitute a current or future price for a new record’s missing historical snapshot, or assume an observed stablecoin price equals one dollar. Same-currency conversion is one.

Legacy files lack true creation timestamps. Their inferred dates and legacy approximate conversion fallback are explicitly marked; preserve this migration distinction. A missing conversion is excluded from Wallet totals and visibly reported. Use the existing formatter to suppress displayed negative zero without changing stored amounts.

`rates.rs` owns public-provider requests and merging. Keep response-date checks, bounded historical batches, alternate-host fallback, and deterministic parsing tests. Fetch failures preserve existing rates and snapshots. Keep API-specific details out of ordinary product flows.
