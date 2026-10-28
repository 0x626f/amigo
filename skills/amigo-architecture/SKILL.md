---
name: amigo-architecture
description: "Organize changes in Amigo’s Rust workspace across its desktop shell, Tasks, Calendar, Finance, Journal, and Squidy tab crates, shared UI, and core model/storage. Use for Amigo refactors, new record types, persistence changes, or deciding where functionality belongs."
---

# Amigo code organization

Keep ownership clear while completing the requested change. These boundaries describe Amigo’s current architecture; adapt them deliberately when the user requests an architectural change.

Resolve any skill-directory symlink first. The repository root is two directories above this skill. Use the current workspace manifests and implementation as the source of truth.

## Ownership map

| Location | Owns |
| --- | --- |
| `src/main.rs` | Native eframe startup, title, viewport bounds |
| `src/app.rs` | Workspace ownership, tab composition, navigation, autosave |
| `src/app/library_ui.rs` | Board/vault switching, library dialogs, password jobs, lock/encrypt actions |
| `src/app/sync.rs`, `src/app/organizer/` | Exchange refresh clock, Calendar subscription/notification workers, tab settings coordination; periodic directory polling is disabled |
| `crates/tasks/src/` | Kanban views, gestures, task drafts, board settings, attachment UI/jobs |
| `crates/squidy/src/`, `src/app/squidy.rs` | Graph canvas and dialogs; shell serializes board and attachment jobs |
| `crates/core/src/squidy.rs`, `media.rs` | Named multigraphs, path filters, Markdown/.link snapshots, safe relative media references |
| `crates/calendar/src/` | Calendar views, event drafts, recurrence and reminder UI |
| `crates/connect/src/` | Calendar URL downloads, iCalendar parsing and Telegram transport; no GUI or persistence |
| `crates/core/src/organizer.rs`, `credentials.rs` | Calendar records, subscriptions, reminder receipts and encrypted URL/Telegram settings |
| `crates/finance/src/` | Finance views, inline drafts/validation/commit, categories, references, rate requests |
| `crates/ui/src/` | Shared visual primitives, keyboard routing, search, calendars, form/dialog layout |
| `crates/ui/src/markdown.rs`, `media.rs` | Description previews, asynchronous image decoding, inline FFmpeg playback and process lifecycle |
| `crates/core/src/model.rs` | Serializable workspace/records, IDs, normalization, domain calculations |
| `crates/core/src/finance_settings.rs` | Library-wide currencies/chains/countries, reference migrations and cross-vault usage |
| `crates/core/src/audit.rs` | Creation metadata and frozen dated FX snapshots |
| `crates/core/src/library.rs`, `storage.rs`, `attachments.rs` | Folder records, atomic writes, migration, encryption, safe attachment copies |
| `examples/preview.rs` | Synthetic native previews through public app/tab APIs |
| `scripts/`, `packaging/`, `justfile` | Build, verification, distribution, release recipes |

## Dependency direction

```text
amigo (shell) -> amigo-tasks, amigo-calendar, amigo-finance, amigo-journal, amigo-squidy, amigo-connect, amigo-ui, amigo-core
amigo-tasks   -> amigo-ui, amigo-core
amigo-finance -> amigo-ui, amigo-core
amigo-calendar -> amigo-ui, amigo-core
amigo-journal -> amigo-ui, amigo-core
amigo-squidy -> amigo-ui, amigo-core
amigo-ui -> amigo-core
amigo-connect -> amigo-core
```

Shared crates do not depend on the tabs or shell. The tabs do not depend on each other. `amigo-core` stays independent of egui. Reused dependency versions belong in workspace dependencies where appropriate.

Tabs own transient UI state and operate on the shared model. They return typed actions, such as `TasksAction::RenameBoard` or `VaultAction::Lock`, for operations owned by the shell. Keep filesystem selection, vault access, and shared app lifecycle out of arbitrary view closures.

## Place a change

- Extend an existing feature module before creating another abstraction. A small one-off widget can remain near its caller; reusable visual behavior belongs in `amigo-ui`.
- Keep drafts, selection, focus IDs, filters, receiver handles, and modal state in tab/shell state. Put persisted business data in core records. Do not serialize egui state or derived encryption keys.
- Put calculations used by multiple screens in core methods so the table, Wallet, and chart agree. Keep provider/network orchestration in Finance’s rates module and the tab’s worker lifecycle.
- Make persistence changes through the core library/storage boundary. Do not write files directly from a rendering closure. Attachment UI requests work; the core attachment module owns copying and safe resolution.
- Keep `main.rs` small. Add feature modules with descriptive names instead of growing startup or a single view into the whole application.

For a new or changed persisted record, read [Record lifecycle](references/record-lifecycle.md). For choosing tests, previews, or packaging checks, read [Verification](references/verification.md).

## Keep contracts connected

An added field may affect normalization, reference renames, validation, drafts, keyboard confirmation, plain records, encrypted payloads, migration markers, background results, and previews. Follow the actual callers; do not expose a new public API solely to make a unit test convenient.

Preserve existing user data and stable IDs through refactors. Derive labels and totals without creating a second independently editable source of truth. Update the relevant README section and preview fixture when observable behavior changes. Avoid unrelated refactors or changes to release configuration during an ordinary feature task.

## Calendar subscriptions

The shell owns serial calendar download and notification workers. The calendar tab
returns typed settings and refresh actions. Subscriptions use stable
IDs; secret URLs stay in encrypted Credentials. Apply a download only while its
subscription ID and URL still match the request. Failed refreshes preserve cached
events and user-selected reminders.

Only local records and configured URL sources are active. Removed account/mail
fields are retained as opaque legacy data on saves; they do not activate integrations
or generate notifications. Keep synthetic previews offline and isolated from the
real library.

## Shared settings, Journal, and updates

The shell owns one app settings window in `src/app/settings.rs`. Tasks, Calendar, Finance, boards, vaults, and categories embed their existing draft editors there. Keep refresh preferences in one source; navigating pages retains drafts, closing discards unconfirmed edits. The app settings button sits beside the brand; setup import/export actions live inside App Settings, with appearance at the bottom of the sidebar. Feature actions live in their toolbar or, for Journal deletion, in tree-row action menus.

`amigo-journal` owns Markdown editor/preview state; `amigo-core::journal` owns safe relative file operations under `<library>/journal`; `src/app/journal.rs` serializes background I/O. Save before navigation, setup backup, restart, and close. Never overwrite external modifications on conflict; preserve draft text and offer recovery. Preview rendering leaves stored Markdown unchanged; intentional preview edits project back into the original source spans and share source-mode Undo, autosave, and conflict handling. Preserve front matter, Markdown formatting, code fences/languages, link destinations, Unicode, and line endings. Freeze the parsed snapshot while a preview input owns focus, and give each table cell its own stable input ID. Ordinary link clicks edit their label; Ctrl/Command-click follows the link. Search is a read-only background action and must not save or discard a conflicting editor draft. External-file picking returns to a destination dialog; only Save and Open copies the source into Journal root or a chosen folder. Copy failures retain the source, current note, and destination choice. Notebook header creation targets root, while each regular folder’s menu actions target that folder. Three-dot and right-click menus share one implementation; blank space between children targets their folder and empty root space targets root. Keep the destination separate from the name draft, preserve current edits until confirmation, and reveal newly created children. Tree trash actions require confirmation and clear stale folder expansions after deletion. Journal always creates a visible `Trash` folder and migrates legacy `.trash` contents without overwriting existing items. Protect the default folder from rename/deletion and reject re-trashing its contents. Trash descendant menus offer permanent deletion with explicit irreversible-deletion confirmation; delete through the background core file boundary, recursively for folders, and clear an affected open editor only after success. Folder trash moves preserve the complete subtree. Restore menu actions return notes or whole folder trees to their saved original location, recreating missing parents and keeping both items on name conflicts; older entries without origin metadata restore at Journal root. Persist origin records before trash moves and include them in setup backups. Update affected open-note paths and reveal restored parents after success. Keep deleted notes out of ordinary searches, link fallback, and new-note/import destinations.

`amigo-connect::updates` fetches stable GitHub release metadata and verifies streamed downloads. The shell owns update preferences, popups, and native installer helpers in `src/app/updates/`. Keep installers outside core/connect, verify digest/size/platform before execution, and preserve the library across application replacement. Develop CI checks all six native targets without publishing a release.

## Squidy and shared media

Squidy links refer to unique card **names**; stable internal IDs identify files and UI state. Renaming a card rewrites every incident edge, including parallel edges. Each edge persists its `directed` flag: new edges are bidirectional, while legacy `.link` entries without the flag retain their directed behavior. Render arrows at both ends for bidirectional edges and from `from` to `to` for directed edges. The connection editor owns direction and endpoint swapping as draft edits; path search respects each edge unless its direction override is disabled. Keep graph data in Markdown bodies, card properties in front matter, connections in `.link`, and view state in board metadata. Preserve saved manual positions when computing unpositioned cards. Serialize snapshot writes through the shell and check fingerprints before replacing content; conflicts retain both local state and external files. Setup transfer/update/close coordination includes pending graph saves.

Tasks, Journal, and Squidy share inline media in `amigo-ui`. Keep decoding and FFmpeg work off the UI thread, media resolution scoped to the owning notebook/board, and player processes stopped when hidden or on exit. Journal's shared `.media/attachments` references remain rooted in the notebook across Trash/restore moves. Provisional attachment batches roll back on cancellation or stale results. The README documents file formats and runtime requirements; `examples/squidy_preview.rs` verifies synthetic graph/media scenes without touching an existing library.
