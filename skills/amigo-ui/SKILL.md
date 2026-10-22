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
- Base ordinary input sizing on `design::CONTROL_HEIGHT` and `design::input`. The current height is 38 logical pixels. Use `field_label` for readable, capitalized input titles.
- Keep text away from borders and align neighboring controls on the same baseline. Calendar buttons belong inside the right side of date inputs and must not increase the row height.
- Use spacing, soft surfaces, and section grouping for the current Apple-inspired desktop interface. Buttons have no decorative border; keyboard focus rings remain visible. Avoid adding redundant subtitles, decorative drop targets, save-status slogans, or task-count summaries.
- Preserve scrolling while hiding scrollbars with `ScrollBarVisibility::AlwaysHidden`.
- Keep the normal pointer over task cards. Kanban column accents are separate from status titles; title backgrounds and folder/drop-target illustrations are not part of the current design.

## Composition

The top-right toolbar is **Search → Settings → Filters → Create**. Reuse `SearchField` for click-to-expand and blur-to-collapse behavior. Settings open dialogs; filters open checkbox menus. Kanban column creation uses a plus icon.

Use `design::modal`, `dialog_heading`, `dialog_width`, `dialog_body_height`, and `dialog_footer`. Keep one window title with its close action, a body that can scroll, and an accessible footer. Use `dialog_section` and `property_row` where they clarify grouping. Avoid growing a dialog beyond the viewport when a child expands.

Finance tables share column boundaries between header and body. Reuse `Cells`, `column_widths`, and the existing row implementation. Center headings and values, including numbers, dates, currencies, and countries. Keep actions in the rightmost column with an empty heading. Computed values stay read-only. Use truncation, hover text, and expandable comment editing for long content.

For implementation examples and geometry checks, read [Control patterns](references/control-patterns.md).

## Motion and verification

Reuse `animate_toggle`, the animated palette, and `page_transition`. Animation IDs belong to stable entities, not row positions. Reverse transitions from their current value, request repaint while moving, and keep hit targets stationary. Theme changes update visuals without reinstalling fonts or rebuilding the font atlas.

Check affected screens in both themes and at the supported 980 × 640 minimum and 1440 × 900 default sizes. Verify focus, popup bounds, long values, and scrolling as well as appearance. Use the synthetic preview in `examples/preview.rs`; isolate its data from the real library. Prefer existing behavior/geometry tests when interaction changes; a cosmetic adjustment does not automatically require a new test.
