# Verification

Choose checks for the actual change. Documentation/skill-only work does not require compiling the application. A style-only edit does not automatically need new interaction tests.

## Rust checks

From the workspace root:

```sh
cargo fmt --all --check
cargo check --workspace --locked
cargo test -p amigo-finance --locked
cargo test -p amigo-tasks --locked
cargo test -p amigo-core --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Run the affected crate first, then the workspace suite when shared behavior, the model, storage, or shell composition changes. Use `cargo test --workspace --locked` for that full suite. Run `cargo build --release --locked` when an updated release binary or distribution verification is part of the task. `justfile` exposes equivalent run/test/check/build/package recipes.

## Test ownership

| Behavior | Existing tests |
| --- | --- |
| Calculations, normalization, reference migration | `crates/core/src/model_tests.rs`, `audit.rs` |
| Plain/encrypted library, attachment files | `crates/core/src/library_tests.rs`, `storage.rs` |
| Inline fields, keyboard batching, selector geometry | `crates/finance/src/views/tests.rs` |
| Finance settings and rates | `views/settings.rs`, `rates.rs`, `crates/finance/src/lib.rs` |
| Task dialogs, gestures, filters, attachments | `crates/tasks/src/tests.rs`, `attachments.rs` |
| Theme/hover/search/calendar primitives | `crates/ui/src/` module tests |
| App composition, autosave, switching, stale sync | `src/app.rs`, `src/app/library_ui.rs`, `src/app/sync.rs` |

Headless UI tests feed egui pointer/key events and inspect committed data, draft retention, popup state, focus, and geometry. Settle layout/scroll animation before reading a response rectangle. Use widget IDs based on record identity, not a row’s index.

Use temporary libraries for storage and encrypted-vault tests. Failed validation/cancellation should preserve existing data. Confirm stale worker results cannot apply after a context switch. Keep current/live-rate tests and independent Node Web Crypto interoperability tests optional; the regular suite is deterministic and offline.

## Native preview

`examples/preview.rs` uses synthetic data and the same public UI APIs as Amigo:

```sh
cargo build --locked --example preview
AMIGO_DATA_FILE=/tmp/amigo-review-UNIQUE/workspace.json \
  target/debug/examples/preview /tmp/amigo-review-UNIQUE/screens --compact --dark
```

Replace `amigo-review-UNIQUE` with a fresh temporary directory for the run. A compatible desktop session is required. The override puts the preview library beside the file with a `.library` extension; do not point it at the user’s library.

Use scene flags from `README.md`: `--finance-inline`, `--finance-country-scenes`, `--finance-reference-scenes`, `--structure-scenes`, `--accounts`, `--wallet-chart`, `--task-drag`, and `--touchpad-drag`. Check both themes and compact/default widths for visual changes. Screenshots complement, rather than replace, event-driven behavior tests.

## Distribution changes

If dependencies, bundled assets, licences, or packaging logic change, inspect `scripts/distribute.py`, `packaging/amigo.spec.in`, and the README’s distribution instructions. Runtime-loaded libraries may need explicit packaging entries even when ELF scanning finds no direct dependency. Desktop portal file selection also needs a host portal backend.

The release helper checks run with:

```sh
python3 -B -m unittest discover -s scripts -p 'test_*.py'
```

Preparing packages and uploading a GitHub draft release are distinct actions. Skills describing code organization do not authorize publishing or sending messages.
