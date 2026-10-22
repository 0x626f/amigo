# Tasks and persistence

Read this when a change touches gestures, attachments, board/vault lifecycle, or background synchronization.

## Tasks

Task statuses determine Kanban columns and their order; categories are a separate filterable catalogue. Default statuses are starting values, not a hardcoded set of columns. Stable status/category IDs preserve assignments through renames. Removing a used status requires reassignment; removing a task category leaves its tasks Uncategorized.

Click/tap opens the task dialog after the system double-click interval. Keep existing mouse dragging, double-click-and-hold dragging, and touchpad tap-and-drag support in `crates/tasks/src/cards.rs`. Touchpads may produce one synthesized primary-button hold followed by movement rather than two complete mouse clicks. Process pointer events in order, including press/release/press/move sequences batched in one frame. Hold the task until release; Escape cancels the gesture. Preserve reordering within a column and movement between columns.

## Attachments

The task editor accepts arbitrary regular file types through the native picker or a drop. Save copies them into the board’s attachment directory, with metadata in the task’s Markdown front matter. Keep originals untouched.

The copy worker must not freeze the UI. Preserve the current `AttachmentBatch` cleanup behavior: failed multi-file imports, canceled drafts, dropped receivers, and late results remove uncommitted copies. Successful task confirmation retains them. Removing an attachment updates the draft first; managed unused copies are pruned after the board is saved.

Use `amigo_core::attachments` for import and path resolution. Record IDs and filenames are data, not arbitrary paths. Reject traversal and links that escape the board’s attachment root. Open the resolved copy through the system association; Open With selects an executable and passes the file as an argument, without shell interpolation. Keep the existing colored file-type icons. Native file selection uses the Linux desktop portal.

## Library and vaults

`library::default_root` defaults to `~/.amigo`, with `AMIGO_HOME` and `AMIGO_DATA_FILE` overrides. The earlier `~/.clerk` and `~/Clerk` libraries move there on first launch only when the destination is absent; preserve the complete folder, encrypted payloads and credentials. Legacy `CLERK_HOME`/`CLERK_DATA_FILE` overrides remain supported, with new Amigo overrides taking precedence. Keep previews/tests on fresh temporary libraries.

- A board has stable folder identity, `board.json`, task Markdown files, and its own attachment files.
- A plain vault has metadata plus per-record Markdown directories and reference data. An encrypted vault has an authenticated encrypted payload in place of plaintext records.
- Plain vaults open directly. Encrypted vaults prompt for a password; their names remain visible while locked. Password work runs on a worker. Passwords are not persisted, and derived keys stay only while unlocked.
- Switching boards/vaults saves confirmed changes first. Board and vault selections are independent. A lock or switch makes old async results obsolete.

Storage writes use the existing atomic-write helpers. Migrations retain IDs, verify their output, and preserve source data on failure. Keep legacy creation-date/category/account migrations idempotent and persisted once even without a successful network refresh.

## Sync and failures

The shell owns autosave in `src/app.rs`; periodic directory polling is disabled until a later implementation. Restart to load external edits. Any future directory watcher must preserve drafts, filters, open editors, and selection, and reject reads made stale by local edits.

Malformed external files encountered on load produce an error and are preserved. Do not overwrite them with an empty/default model. Finance exchange refresh timing belongs in Finance settings; Calendar refresh timing and notifications belong in Calendar settings. Exchange failures preserve saved data. Preserve interrupted-encryption recovery and ensure conversion does not leave a readable legacy plaintext copy.

For regression coverage, use the existing task pointer-event tests, attachment cancellation tests, temporary-library tests, and shell sync tests. Keep these tests offline; opening real applications or using a real user library is not part of the normal test suite.
