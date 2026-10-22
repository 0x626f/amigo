---
name: amigo-architecture
description: "Organize changes in Amigo’s Rust workspace across its desktop shell, Tasks, Calendar, Finance, and Journal tab crates, shared UI, and core model/storage. Use for Amigo refactors, new record types, persistence changes, or deciding where functionality belongs."
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
| `crates/calendar/src/` | Calendar views, event drafts, recurrence and reminder UI |
| `crates/connect/src/` | Calendar URL downloads, iCalendar parsing and Telegram transport; no GUI or persistence |
| `crates/core/src/organizer.rs`, `credentials.rs` | Calendar records, subscriptions, reminder receipts and encrypted URL/Telegram settings |
| `crates/finance/src/` | Finance views, inline drafts/validation/commit, categories, references, rate requests |
| `crates/ui/src/` | Shared visual primitives, keyboard routing, search, calendars, form/dialog layout |
| `crates/core/src/model.rs` | Serializable workspace/records, IDs, normalization, domain calculations |
| `crates/core/src/audit.rs` | Creation metadata and frozen dated FX snapshots |
| `crates/core/src/library.rs`, `storage.rs`, `attachments.rs` | Folder records, atomic writes, migration, encryption, safe attachment copies |
| `examples/preview.rs` | Synthetic native previews through public app/tab APIs |
| `scripts/`, `packaging/`, `justfile` | Build, verification, distribution, release recipes |

## Dependency direction

```text
amigo (shell) -> amigo-tasks, amigo-calendar, amigo-finance, amigo-journal, amigo-connect, amigo-ui, amigo-core
amigo-tasks   -> amigo-ui, amigo-core
amigo-finance -> amigo-ui, amigo-core
amigo-calendar -> amigo-ui, amigo-core
amigo-journal -> amigo-ui, amigo-core
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

The shell owns one app settings window in `src/app/settings.rs`. Tasks, Calendar, Finance, boards, vaults, and categories embed their existing draft editors there. Keep refresh preferences in one source; navigating pages retains drafts, closing discards unconfirmed edits. Data transfer/rename/delete actions live in tab toolbars; the sidebar action row contains Settings and setup import/export, with appearance at the bottom.

`amigo-journal` owns Markdown editor/preview state; `amigo-core::journal` owns safe relative file operations under `<library>/journal`; `src/app/journal.rs` serializes background I/O. Save before navigation, setup backup, restart, and close. Never overwrite external modifications on conflict; preserve draft text and offer recovery. Preview code must not mutate stored Markdown.

`amigo-connect::updates` fetches stable GitHub release metadata and verifies streamed downloads. The shell owns update preferences, popups, and native installer helpers in `src/app/updates/`. Keep installers outside core/connect, verify digest/size/platform before execution, and preserve the library across application replacement. Develop CI checks all six native targets without publishing a release.
