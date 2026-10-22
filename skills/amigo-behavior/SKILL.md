---
name: amigo-behavior
description: "Implement or review shared interaction and functionality in the Amigo Rust desktop application: keyboard confirmation, drafts, filtering, persistence, background work, and cross-view consistency. Use when Amigo features must follow the existing behavior contract."
---

# Amigo shared behavior

Preserve these shared behaviors when adding or fixing a Amigo feature. An explicit user request can change a convention; update its implementation, relevant regression coverage, and documentation together.

Resolve directory symlinks first; the Amigo repository root is two directories above this skill. Check the current source before assuming implementation details.

## Keyboard ownership

Use [ActionKeys](../../crates/ui/src/keyboard.rs), re-exported through `amigo_ui::design`. Capture keys before rendering widgets, because a single-line input can surrender focus on Enter. Use `in_ui`, `in_popup`, or `dialog` for the actual input owner, and `multiline` for text that accepts newlines.

| Context | Confirm | Cancel |
| --- | --- | --- |
| Active dialog | Enter confirms when valid | Escape discards its draft |
| Finance inline editing | Enter saves all pending eligible rows across views | Escape discards the active row only |
| Pointer click on a row checkmark | Saves that row only | Row cross discards that row |
| Open dropdown/calendar | Owns its selection interaction | Closes before the underlying edit |
| Deletion prompt | Confirms only that deletion | Dismisses the prompt first |
| Search/filter menu | Enter accepts | Escape restores its snapshot |
| Multiline description/comment | Enter inserts a newline; Ctrl/Command+Enter confirms | Escape cancels the active editor |

An open popup or top modal takes priority over background edits. Preserve native activation of a focused button. Avoid raw global `key_pressed(Enter)` handlers that bypass this routing or let one key act on multiple layers.

Enter on a finance row’s focused Save checkmark also follows the batch-save contract; pointer activation of the same checkmark saves one row.

## Draft and validation contract

Persistent state changes after confirmation, not every intermediate keystroke. Keep unfinished rows and dialog drafts keyed by stable record IDs. Failed validation retains editable content and explains the error. A batch finance save commits valid rows and retains invalid ones; it does not batch-confirm deletion prompts.

New finance rows remain reachable under current filters and receive focus/scroll into view. Sorting or navigating does not attach a draft to a different record. Saving a child comment updates its parent row draft; it does not implicitly save every row. Opening shared settings from view settings retains the parent settings draft.

## Common app behavior

- Board status ordering supports drag handles with insertion feedback and arrow controls. Reordering changes the settings draft; Save Board/Enter commits it, Cancel/Escape discards it. Escape during a drag cancels the gesture first. Status IDs, properties and task assignments survive reordering.
- Calendar starts in Day view, with Day, Week, Month, Agenda selectors in that order.

- Initial search and currency filters are empty; Income/Expenses date filters are empty. Wallet has its own monthly period, defaulting to the current month, with no All dates option. Clear Filters restores these defaults. Task category/priority options match any selected value within a group; groups and search combine.
- Search expands on click and collapses on blur. Keep its query lifecycle in `SearchField`, not a second ad hoc implementation.
- Confirmed data autosaves through the shell, including before board/vault switches and on exit. Drafts remain transient. Do not add routine import/export steps or “All changes saved” banners.
- Periodic directory polling is disabled. Calendar settings own URL subscriptions, Telegram notifications, and Calendar refresh timing (60 seconds by default); Finance settings own exchange refresh timing (60 seconds by default).
- Network calls, password derivation, and large attachment copies run off the UI thread. Poll results without blocking and request repaint on completion. Results for a previous vault, board, or closed editor must not modify the new context.

Read [Finance behavior](references/finance.md) when touching finance records, calculations, or reference lists. Read [Tasks and persistence](references/tasks-and-storage.md) when touching Kanban, attachments, library operations, or sync.

## Verification

Exercise the interaction through the existing headless egui harness, not just a direct call to a save function. Cover the affected key/popup/modal priority and the saved state after success or cancellation. Persistence changes also need a temporary-library round trip, including encryption where relevant. Keep live network checks optional and separate from deterministic tests.
