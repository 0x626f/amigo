# Record lifecycle

Use this checklist selectively for persisted Finance changes; task-specific data follows the equivalent workspace/board path. Paths are relative to the repository root.

## Model and migration

1. Add persisted fields/types in `crates/core/src/model.rs` with backward-compatible serde defaults. Inspect existing camelCase serialization and legacy parsing in `plan_from_value`.
2. Keep stable record/reference IDs. Add normalization where needed, ensuring a second normalize/load does not regenerate IDs, reset edits, or recreate deliberately removed defaults.
3. Distinguish legacy inference from new data. Use creation audits for new financial records; do not restamp an existing record when editing or refreshing its rates. A copied transaction is a new record.
4. Share domain calculations through core methods. If a field participates in totals, update all relevant summaries/charts through those methods, including missing-rate behavior.
5. Inspect seed and migration markers. When a normalized upgrade must be persisted even without user edits, check the shell’s `last_saved` baseline handling on initial load and vault unlock. `meta.record_metadata_version` (currently `RECORD_METADATA_VERSION = 3`) coordinates this; update its writers and tests together if a new migration requires a version change.

## Inline Finance integration

The `Record` enum, `Editor`, ID/view routing, and creation defaults are in `crates/finance/src/views/mod.rs` and `crates/finance/src/lib.rs`.

Update applicable enum matches, including record ID, view, creation audit, first-field focus, validation, commit, and delete-reference guards. Add the cell order and headings together in `views/inline.rs` and the owning view. Use that view’s actual draft membership rule; Account records also appear under Stored Values, so their membership is not simply one view equality check.

Retain the Enter batch-save contract across views and offscreen rows. Keep record edits before reference renames in batch application. Validation errors remain in the draft map. A row’s checkmark click saves one row; popup/modal/deletion input takes priority.

Add creation through the appropriate toolbar/menu or section action. New rows should focus and scroll into view, including embedded Wallet tables. Preserve unfinished rows when filtering/sorting the saved records.

## Reference consistency

Names are editable display data; IDs maintain relationships. Update ID-based rename and in-use deletion logic rather than matching only current text.

Account details in `FinancePlan.accounts` and the older `income_accounts` reference list deliberately share IDs. Wallet account commits update both; normalization keeps Income/Savings links and account metadata consistent. Stored Values editing/deletion must use the same lifecycle, not resurrect a deleted account or recreate removed account properties.

Expense categories, savings categories, expense labels, and country entries are distinct catalogues. Do not resolve a free-form budget label through an expense-label catalogue. Country selection is constrained, but a migration preserves unrecognized historical data until an edit can validate it.

## Disk and encrypted payloads

Plain-vault record collections are enumerated by `RECORDS` in `crates/core/src/library.rs`. Add a new standalone collection to both plain writing/reading and record-order preservation through this list. Old vaults can lack the new directory; loading must still work.

References and transaction collections serialize through the same FinancePlan for encrypted vaults. Verify all new fields survive encrypted round trips and remain absent from plaintext while the vault is encrypted. Use existing atomic writes and interrupted-conversion recovery.

Board tasks use Markdown front matter for structured metadata and a heading/body for title/description. File attachments live in the board-owned attachment folder and are referenced by metadata, not embedded as large serialized byte arrays.

## Evidence to keep

Choose checks that prove the changed contract: legacy input migration, stable IDs on a second normalize, real inline edit/save/cancel, reference rename/delete, and plain/encrypted reload retaining the new values. Extend existing synthetic fixtures with representative data rather than reproducing implementation details in assertions.
