---
name: amigo-ui
description: "Create or change native egui UI elements in the Amigo Rust desktop application, including controls, dialogs, tables, themes, and responsive layout. Use for Amigo visual or layout work; shared interaction semantics and crate boundaries have separate project skills."
---

# Amigo UI

Follow Amigo’s Apple-inspired desktop visual language while implementing the requested change. Read the project [Apple design skill](../apple-design/SKILL.md) and its [Amigo adaptation](../apple-design/references/amigo.md) for Apple-style UI work. These are project conventions, not restrictions on an explicitly requested redesign.

Resolve the skill directory through any symlinks; the repository root is two directories above it. Work against the current Amigo checkout and confirm helper signatures before editing.

## Find the existing element

| Element | Existing implementation |
| --- | --- |
| Palette, typography, frames, icons, animation | [design.rs](../../crates/ui/src/design.rs) |
| Buttons, icon buttons, chips | [controls.rs](../../crates/ui/src/design/controls.rs) |
| Input labels and form fields | [forms.rs](../../crates/ui/src/forms.rs) |
| Dialog layout | [dialogs.rs](../../crates/ui/src/dialogs.rs) |
| Date and month inputs | [calendar.rs](../../crates/ui/src/calendar.rs) |
| Expanding search field | [search.rs](../../crates/ui/src/search.rs) |
| Finance cells, column widths, selectors, comment popup | [inline.rs](../../crates/finance/src/views/inline.rs) |

Reuse a helper before creating a new implementation. Keep a feature-specific helper within its tab; promote it into `amigo-ui` when reuse across features justifies doing so.

## Visual conventions

- Use `design::colors(ui)` and semantic palette roles for both themes. Shared components own their colors, padding, typography, and animation.
- Build text buttons with `design::button`, `small_button`, `primary`, or `design::Button`. Use `icon_button` with a stable ID and meaningful accessible label; it already provides hover animation, keyboard focus feedback, and a tooltip.
- Use `design::checkbox` and `small_checkbox` for rounded-square indicators: blue fill and a white tick when selected, a neutral outline when empty, and a visible keyboard focus ring. Keep the full label clickable and preserve row hit targets in both sizes.
- Base ordinary input sizing on `design::CONTROL_HEIGHT` and `design::input`. The current height is 38 logical pixels. Use `field_label` for readable, capitalized input titles.
- Keep text away from borders and align neighboring controls on the same baseline. Calendar buttons belong inside the right side of date inputs and must not increase the row height.
- Use spacing, soft surfaces, and section grouping for the current Apple-inspired desktop interface. Buttons have no decorative border; keyboard focus rings remain visible. Avoid adding redundant subtitles, decorative drop targets, save-status slogans, or task-count summaries.
- Preserve scrolling while hiding scrollbars with `ScrollBarVisibility::AlwaysHidden`.
- Keep the normal pointer over task cards. Kanban column accents are separate from status titles; title backgrounds and folder/drop-target illustrations are not part of the current design.

## Composition

The top-right toolbar is **Search → Settings → Filters → Create**. Reuse `SearchField` for click-to-expand and blur-to-collapse behavior. Settings open dialogs; filters open checkbox menus. Kanban column creation uses a plus icon.

Use `design::modal`, `dialog_heading`, `dialog_width`, `dialog_body_height`, and `dialog_footer`. Keep one window title with its close action, a body that can scroll, and an accessible footer. Use `dialog_section` and `property_row` where they clarify grouping. Settings preference lists use `settings_group`, `settings_row`, and `settings_separator` from `dialogs.rs`: softly filled rounded groups, inset palette-colored rules, and aligned label/control rows. Keep action footers outside scrolling groups. Avoid growing a dialog beyond the viewport when a child expands.

Currencies, Chains, and Countries appear directly under Finance, at the same hierarchy level as vault names. Each vault has Clients, Accounts, Expenses, and Savings child pages. Shared reference pages have no vault subtitle or password requirement. Keep vault expansion separate from the selected page: opening a global page must neither expand a collapsed vault nor collapse an expanded one. Keep expense categories on Expenses and savings categories on Savings; obsolete label catalogues and RPC URL fields are removed. Do not duplicate these lists in a Stored Values page or type dropdown. The Finance preferences page links to each vault. Keep page-specific search/create actions above shared tables, the Done footer outside scrolling content, and currency dialogs above the parent settings window. Allow the settings navigation to scroll and reveal the selected page when the expanded vault list exceeds its viewport.

Currencies use a left-aligned clickable list with quiet inset separators. Keep network names visible and open the existing currency-details dialog on activation; do not add a Symbol header, centered cells, or alternating table fills.

Finance tables share column boundaries between header and body. Reuse `Cells`, `column_widths`, and the existing row implementation. Center headings and values, including numbers, dates, currencies, and countries. Keep actions in the rightmost column with an empty heading. Computed values stay read-only. Use truncation, hover text, and expandable comment editing for long content.

For implementation examples and geometry checks, read [Control patterns](references/control-patterns.md).

## Motion and verification

Reuse `animate_toggle`, the animated palette, and `page_transition`. Animation IDs belong to stable entities, not row positions. Reverse transitions from their current value, request repaint while moving, and keep hit targets stationary. Theme changes update visuals without reinstalling fonts or rebuilding the font atlas.

Check affected screens in both themes and at the supported 980 × 640 minimum and 1440 × 900 default sizes. Verify focus, popup bounds, long values, and scrolling as well as appearance. Use the synthetic preview in `examples/preview.rs`; isolate its data from the real library. Prefer existing behavior/geometry tests when interaction changes; a cosmetic adjustment does not automatically require a new test.

## Settings and Journal layout

App settings starts with Tasks and has no General page. Settings navigation uses a separate soft surface with dividers between top-level categories; parents and their child pages remain grouped. Navigation rows keep equal outside edges, with hierarchy indentation inside the shared row. Apply separator strokes only within settings so ordinary unbordered controls keep their existing appearance. Group related Journal editor/preview options, finance security/exchange preferences, Calendar sources/notifications, and board lists; preserve scrolling and status drag targets at compact sizes. Reserve the actual action width and current item spacing when sizing inputs next to buttons; do not recalculate list widths from a previous row's expanded bounds.

Journal has no separate page heading. Its upper-left editor Open External Markdown File icon opens a native picker followed by a destination dialog offering Journal Root and existing folders. Keep the note name centered above the notebook. The editor fills the central workspace with no outside margin, rounded corners, or gap between header and content. It has no outer border, header separator, or line-number gutter rule; retain only the resizable pane divider in Split mode, in either orientation. Preserve content formatting such as code panels and table rules. Double-clicking the name starts inline rename as plain text with a collapsed caret at the end. Use a frameless title input with no background box or focus outline, and do not automatically select the title. Enter/check saves and Escape/cross cancels; there is no separate rename icon. The notebook header has Open External Markdown File at the leading edge and Markdown, Split, Preview, and view options at the trailing edge. Folder rows have one 22-pixel three-dot menu button; notes instead have a 22-pixel direct trash button with confirmation, including permanent deletion for notes inside Trash. Notes retain their right-click actions menu for Restore and other commands. The identical action menu opens by right-clicking a row or its related folder/root space. Folder menus group New Note and New Folder before Move to Trash; note menus offer Open Note and Move to Trash. Root space offers creation and refresh. Keep destructive actions separate and require confirmation. The default Trash folder remains visible at the end of the tree; the default folder’s menu only offers Refresh, while its contents offer Restore and Delete Permanently. Confirm these with “Delete Permanently” and explain that folders include their contents and cannot be recovered. Expand Trash to browse deleted items with their original display names. Place Restore before Delete Permanently in Trash item menus; restore a folder with its complete subtree. Restore uses the saved original parent, recreates missing parents, keeps both items on name conflicts, and falls back to Journal root for older items without origin metadata. Exclude Trash from ordinary searches and new-note/import destinations. Keep deletion confirmation separate from row navigation.

Notebook keeps its compact new-note, new-folder, refresh, and search icons and uses a slightly larger 13-pixel title. Header creation always targets Journal root. New Note and New Folder in each regular folder’s menu target that folder even when collapsed or a different note is open. Show the fixed destination separately from the editable child name. Keep creation controls out of Trash, and reveal the parent after creation. Put Journal folder disclosure arrows before the folder icon with a small gap between them, leaving room for names beside the row actions. Search opens a centered modal with a focused query, background results, arrow-key selection, Enter to open, and Escape to close; it never filters the notebook tree. Opening a result expands its ancestors. Keep late results from a previous query or a closed dialog from changing the current search.

Journal follows the linked JetBrains Markdown reference in the README. Keep source gutter/highlighting in `highlight.rs` and `panes.rs`, rendered block typography in `render.rs`, and selection actions in `toolbar.rs`. View icons belong at the right of the notebook header. Preserve both pane orientations, divider resizing, line-based bidirectional scroll sync, and independently configurable preview text size. Formatting actions share the Unicode-safe core transforms and editor undo history. Show detected styles as selected in both the floating toolbar and context menus; disable text styles inside code blocks. Cache selection analysis by document revision and character range. Test both scroll directions and popup ownership when changing selection controls.

Preview begins read-only unless Journal's `preview_editable` preference is enabled. The header pencil toggles editing for the current note in Preview or Split mode, and new notes use the saved default. While editing, keep a visible formatting bar, contextual formatting menus, and source-compatible keyboard shortcuts. Map rendered character selections back to Markdown source spans; preserve selection and focus through style toggles, menus, Undo, and link-dialog cancellation. Links use an explicit destination dialog in Preview. Read-only mode permits selection/copy and ordinary link navigation, but no text, task-checkbox, or divider mutations. Keep table-cell formatting inline so headings, lists, or code fences cannot break the table structure.

Keep empty list bullets, numbers, and task checkboxes visible in editable Preview. Their payload input must retain focus when Enter creates the next item. Blank continuation lines align with the item text, while exiting a list creates a normal paragraph at that position in the document. Avoid dropping empty items during Markdown parsing.
