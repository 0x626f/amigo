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

## Shared settings, Journal, and updates

The shell owns one app settings window in `src/app/settings.rs`. Tasks, Calendar, Finance, boards, vaults, and categories embed their existing draft editors there. Keep refresh preferences in one source; navigating pages retains drafts, closing discards unconfirmed edits. The app settings button sits beside the brand; setup import/export actions live inside App Settings, with appearance at the bottom of the sidebar. Feature actions live in their toolbar or, for Journal deletion, in tree-row action menus.

`amigo-journal` owns Markdown editor/preview state; `amigo-core::journal` owns safe relative file operations under `<library>/journal`; `src/app/journal.rs` serializes background I/O. Save before navigation, setup backup, restart, and close. Never overwrite external modifications on conflict; preserve draft text and offer recovery. Preview rendering leaves stored Markdown unchanged; intentional preview edits project back into the original source spans and share source-mode Undo, autosave, and conflict handling. Preserve front matter, Markdown formatting, code fences/languages, link destinations, Unicode, and line endings. Freeze the parsed snapshot while a preview input owns focus, and give each table cell its own stable input ID. Ordinary link clicks edit their label; Ctrl/Command-click follows the link. Search is a read-only background action and must not save or discard a conflicting editor draft. External-file picking returns to a destination dialog; only Save and Open copies the source into Journal root or a chosen folder. Copy failures retain the source, current note, and destination choice. Notebook header creation targets root, while each regular folder’s menu actions target that folder. Three-dot and right-click menus share one implementation; blank space between children targets their folder and empty root space targets root. Keep the destination separate from the name draft, preserve current edits until confirmation, and reveal newly created children. Tree trash actions require confirmation and clear stale folder expansions after deletion. Journal always creates a visible `Trash` folder and migrates legacy `.trash` contents without overwriting existing items. Protect the default folder from rename/deletion and reject re-trashing its contents. Trash descendant menus offer permanent deletion with explicit irreversible-deletion confirmation; delete through the background core file boundary, recursively for folders, and clear an affected open editor only after success. Folder trash moves preserve the complete subtree. Restore menu actions return notes or whole folder trees to their saved original location, recreating missing parents and keeping both items on name conflicts; older entries without origin metadata restore at Journal root. Persist origin records before trash moves and include them in setup backups. Update affected open-note paths and reveal restored parents after success. Keep deleted notes out of ordinary searches, link fallback, and new-note/import destinations.

`amigo-connect::updates` fetches stable GitHub release metadata and verifies streamed downloads. The shell owns update preferences, popups, and native installer helpers in `src/app/updates/`. Keep installers outside core/connect, verify digest/size/platform before execution, and preserve the library across application replacement. Develop CI checks all six native targets without publishing a release.

Journal formatting inspects parsed Markdown before applying a style. Toggle existing emphasis and code markers, normalize mixed styled/plain selections, replace heading/list prefixes, and preserve indentation, checked tasks, Unicode selections, and line endings. Protect fenced code from text/block-style commands; language changes reuse existing fences. Toolbar and context-menu selection states come from the same analysis. Keep ignored actions out of revision/undo history and preserve formatting Undo through the editor input path.

Preview begins read-only unless Journal's `preview_editable` preference is enabled. The header pencil toggles editing for the current note in Preview or Split mode, and new notes use the saved default. While editing, keep a visible formatting bar, contextual formatting menus, and source-compatible keyboard shortcuts. Map rendered character selections back to Markdown source spans; preserve selection and focus through style toggles, menus, Undo, and link-dialog cancellation. Links use an explicit destination dialog in Preview. Read-only mode permits selection/copy and ordinary link navigation, but no text, task-checkbox, or divider mutations. Keep table-cell formatting inline so headings, lists, or code fences cannot break the table structure.

Journal list newlines share `amigo_core::journal::formatting::list_newline` in Source and editable Preview. Enter continues the original bullet/delimiter/indentation, increments numbers, and resets new task checkboxes. Enter on an empty item exits immediately; nested empty items return to the parent level first. Shift+Enter keeps a hard break inside the item. Protect code, HTML, and YAML metadata. Render empty list/task items with a real input and preserve their insertion anchor; a bare saved marker needs a separating space before its first payload. Preview recognizes typed list prefixes without waiting for blur. Structural edits and Undo/Redo reparse and restore the caret, including blank paragraphs between existing blocks. Keep popups, modals, loading, read-only mode, and code/table editors in control of their own keys.
