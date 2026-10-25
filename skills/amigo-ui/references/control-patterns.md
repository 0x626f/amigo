# Control patterns

Read this when implementing a new control, dialog, or finance table column. Paths below are relative to the Amigo repository root.

## Buttons and IDs

Use record identity in widget IDs so sorting or inserting rows cannot move focus or animation state to a different record:

```rust
use amigo_ui::design::{self, Icon};

let clicked = design::icon_button(
    ui,
    egui::Id::new(("feature-settings", record_id)),
    Icon::Settings,
    "Configure Record",
    design::CONTROL_HEIGHT,
)
.clicked();
```

Give repeated child fields distinct IDs, conventionally `(feature, record_id, field)`. Use `push_id` or explicit IDs for sibling controls. Tooltips complement accessible labels; they do not replace them.

## Inputs and selectors

- Start a text field with `design::input`; use `forms::text_field` when a labeled vertical field is appropriate. Keep a single draft value while typing.
- Use `date_input`, `month_input`, or `table_date_input` from `amigo_ui::forms`. The table variant handles centered values and the internal calendar affordance.
- In a finance table, use the existing `currency`, `country_input`, and `category` helpers. Currency selection stores stable IDs as well as symbols. Country selection comes from the bundled catalogue, with None clearing the optional value. Category selection uses the view’s configured list.
- Use `centered_dropdown` for the current full-width selector design: centered text, reserved arrow space, bounded popup, hidden scrollbars. Keep selection separate from search text.
- Use the finance `comment` popup for long editable comments. Its Apply action updates the row draft; row confirmation persists it. A tooltip alone is insufficient for editing large comments.
- Use the shared money formatter for displayed values; it suppresses negative zero after rounding. Do not round the stored amount merely to fix its presentation.

## Dialogs

Keep the draft separate from persisted state. Use the existing heading, body-height, and footer helpers instead of manually reproducing their margins. Put confirmation and cancellation actions in the footer. Display validation close to its field or row.

Size adjacent properties explicitly within the available width. A nested `vertical` UI can expand to all remaining horizontal space and push later buttons outside the dialog. When reserving space for file actions or fixed-width properties, use a bounded `allocate_ui_with_layout` and account for gaps, icons, and buttons.

For a child dialog opened from settings, retain the parent draft and route input to the child. Rendering the parent again in the same frame that the child closes can make one Escape dismiss both. Follow the existing finance settings routing in `crates/finance/src/lib.rs`.

Keyboard ownership comes from `amigo_ui::design::ActionKeys`; look at `crates/ui/src/keyboard.rs` and the existing dialog using the same interaction before wiring confirmation.

## Tables and layout

When adding a column, update the header list and `record_cells` in the same order, including the unnamed actions cell. Adjust `column_widths` only if the content needs a meaningful weight or minimum width. Keep computed and editable cells inside those shared bounds.

Nested budget expenses reuse the table cell primitives. Settings → Finance → the vault → Accounts is a Label-only table using the same account IDs as Income/Savings references. Wallet has no account list. Test these embedded tables after scrolling; settling layout and scroll animation may require several frames.

Current table typography intentionally becomes more compact at narrow widths. Keep the complete heading available on hover and ensure full values can be read or edited. A compact header is not permission to clip a control.

## Meaningful checks

Existing examples are in `crates/finance/src/views/tests.rs`, `crates/tasks/src/tests.rs`, and `crates/ui/src/design/controls.rs`.

- Drive real pointer/key events for changes to selection, editing, or focus.
- Compare header and cell rectangles when changing alignment or widths. Check the rightmost actions and internal date-picker button too.
- For centered text, use the rendered galley’s actual rectangle translated by its paint position; its local origin can be offset.
- Include long labels/comments and more rows than fit vertically when these affect the change.
- Use the same public shell/tab APIs in previews. Build with `cargo build --locked --example preview` and run with a fresh `AMIGO_DATA_FILE` under a temporary directory. Read `README.md` for the scene flags; `--compact` and `--dark` cover the main layout variants.

Dropdown captions reserve an arrow gutter and truncate within it. Keep at least an 8-pixel visible gap before the arrow; short currency codes must fit at compact table widths. Shared ComboBoxes use a 10-pixel icon gap, while the compact date picker uses 6 pixels to retain centered dates. Settings groups use 16-pixel horizontal and 12-pixel vertical insets with 8 pixels between rows.

Journal preview uses frameless text inputs with rendered Markdown typography. Keep paragraph separation, task-checkbox gutters, and top-aligned table cells; give every cell a distinct input ID. The preview stays flush with the workspace and the split divider remains its only outer pane border.
