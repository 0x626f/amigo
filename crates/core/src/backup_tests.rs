use super::*;
use crate::{
    library::VaultKey,
    model::{Amount, FinancePlan, Income, Task},
    organizer::{CalendarEvent, CalendarSubscription},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("amigo-backup-test-{}", id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture(root: &Path) -> Library {
    let mut library = Library::open(root.into(), None).unwrap();
    let mut workspace = Workspace::default();
    let mut task = Task::new(1, "todo");
    task.title = "Task with 日本語 attachment".into();
    task.description = "Text\n\nSecond paragraph".into();
    let source = root.parent().unwrap().join(".binary-file");
    fs::write(&source, [0, 255, 8, 17]).unwrap();
    task.attachments.push(
        attachments::import(
            &library.board_path(&library.settings.board).unwrap(),
            &source,
        )
        .unwrap(),
    );
    workspace.tasks.push(task);
    library
        .save_board(&library.settings.board, &workspace)
        .unwrap();
    library.settings.dark_mode = true;
    library.save_settings().unwrap();
    library
}
fn snapshot(library: &Library) -> serde_json::Value {
    serde_json::to_value(Archive::capture(library, Scope::Setup).unwrap()).unwrap()
}

#[test]
fn board_round_trip_preserves_attachments_and_replaces_only_matching_board() {
    let temp = Temp::new();
    let source = fixture(&temp.0.join("source"));
    let mut dest = Library::open(temp.0.join("dest"), None).unwrap();
    let original_board = dest.settings.board.clone();
    let original_vault = dest.vaults[0].clone();
    let source_id = source.settings.board.clone();
    let archive = Archive::capture(&source, Scope::Board(source_id.clone())).unwrap();
    let path = temp.0.join("board.amigo");
    archive.write(&path, &source.root).unwrap();
    let archive = Archive::read(&path).unwrap();
    let before = snapshot(&dest);
    assert!(archive.review(&dest).unwrap().contains("1 tasks"));
    assert_eq!(snapshot(&dest), before);
    archive.import(&dest).unwrap();
    dest = Library::open(dest.root, None).unwrap();
    assert_eq!(dest.boards.len(), 2);
    assert!(dest.load_board(&original_board).unwrap().tasks.is_empty());
    assert_eq!(dest.vaults[0], original_vault);
    assert_eq!(dest.settings.board, source_id);
    let board = dest.load_board(&source_id).unwrap();
    assert_eq!(board.tasks, source.load_board(&source_id).unwrap().tasks);
    let attachment = &board.tasks[0].attachments[0];
    assert_eq!(
        fs::read(attachments::resolve(&dest.board_path(&source_id).unwrap(), attachment).unwrap())
            .unwrap(),
        [0, 255, 8, 17]
    );
    dest.save_board(&source_id, &Workspace::default()).unwrap();
    archive.import(&dest).unwrap();
    let reopened = Library::open(dest.root, None).unwrap();
    assert_eq!(reopened.boards.len(), 2);
    assert_eq!(reopened.load_board(&source_id).unwrap().tasks, board.tasks);
}

#[test]
fn encrypted_vault_exports_remain_encrypted_and_unlock_after_import() {
    let temp = Temp::new();
    let mut source = fixture(&temp.0.join("source"));
    let mut plan = FinancePlan::empty();
    let mut income = Income::new("EUR");
    income.source = "PRIVATE-VAULT-RECORD".into();
    income.gross_amount = Amount::Number(7000.0);
    plan.income.push(income);
    plan.normalize();
    let key = VaultKey::new("export password").unwrap();
    let vault = source.create_vault("Protected", &plan, Some(&key)).unwrap();
    let archive = Archive::capture(&source, Scope::Vault(vault.entry.id.clone())).unwrap();
    for file in archive.files.values() {
        assert!(
            !String::from_utf8_lossy(&STANDARD.decode(&file.data).unwrap())
                .contains("PRIVATE-VAULT-RECORD")
        );
    }
    let dest = Library::open(temp.0.join("dest"), None).unwrap();
    archive.review(&dest).unwrap();
    archive.import(&dest).unwrap();
    let dest = Library::open(dest.root, None).unwrap();
    let path = dest.vault_path(&vault.entry.id).unwrap();
    assert!(!path.join("records").exists());
    assert_eq!(
        fs::read(path.join("vault.enc")).unwrap(),
        fs::read(
            source
                .vault_path(&vault.entry.id)
                .unwrap()
                .join("vault.enc")
        )
        .unwrap()
    );
    assert!(VaultKey::unlock(&path, &vault.entry.id, "wrong password").is_err());
    assert_eq!(
        VaultKey::unlock(&path, &vault.entry.id, "export password")
            .unwrap()
            .0,
        plan
    );
}

#[test]
fn calendar_import_transfers_urls_and_events_without_overwriting_other_credentials() {
    let temp = Temp::new();
    let source = fixture(&temp.0.join("source"));
    let mut organizer = Organizer::default();
    organizer.events.push(CalendarEvent {
        title: "Portable event".into(),
        ..Default::default()
    });
    organizer.subscriptions.push(CalendarSubscription {
        id: "remote".into(),
        name: "Work".into(),
    });
    organizer.calendar_settings.refresh_interval_seconds = 120;
    organizer.save(&source.root).unwrap();
    let mut credentials = Credentials {
        telegram_token: "source-secret".into(),
        ..Default::default()
    };
    credentials.calendar_urls.insert(
        "remote".into(),
        "https://example.invalid/private-feed".into(),
    );
    credentials.save(&source.root).unwrap();
    let archive = Archive::capture(&source, Scope::Calendar).unwrap();
    let dest = fixture(&temp.0.join("dest"));
    let before = dest.load_board(&dest.settings.board).unwrap();
    Credentials {
        telegram_token: "destination-secret".into(),
        ..Default::default()
    }
    .save(&dest.root)
    .unwrap();
    archive.import(&dest).unwrap();
    assert_eq!(Organizer::load(&dest.root).unwrap(), organizer);
    let imported = Credentials::load(&dest.root).unwrap();
    assert_eq!(imported.telegram_token, "destination-secret");
    assert_eq!(imported.calendar_urls, credentials.calendar_urls);
    assert_eq!(dest.load_board(&dest.settings.board).unwrap(), before);
}

#[test]
fn setup_restore_replaces_everything_including_selection_theme_and_connections() {
    let temp = Temp::new();
    let mut source = fixture(&temp.0.join("source"));
    let second = source
        .create_board("Other board", &Workspace::default())
        .unwrap();
    source.select_board(&second.id).unwrap();
    source.refresh().unwrap();
    let credentials = Credentials {
        telegram_token: "portable-settings".into(),
        telegram_chat: "chat".into(),
        ..Default::default()
    };
    credentials.save(&source.root).unwrap();
    Organizer {
        events: vec![CalendarEvent {
            title: "Original event".into(),
            ..Default::default()
        }],
        ..Default::default()
    }
    .save(&source.root)
    .unwrap();
    let archive = Archive::capture(&source, Scope::Setup).unwrap();
    let mut dest = fixture(&temp.0.join("dest"));
    let unwanted = dest
        .create_board("Remove me", &Workspace::default())
        .unwrap();
    fs::write(dest.root.join("unwanted-file"), "remove").unwrap();
    archive.review(&dest).unwrap();
    assert!(dest.root.join("unwanted-file").exists());
    archive.import(&dest).unwrap();
    let dest = Library::open(dest.root, None).unwrap();
    assert_eq!(dest.boards, source.boards);
    assert!(!dest.boards.iter().any(|b| b.id == unwanted.id));
    assert!(!dest.root.join("unwanted-file").exists());
    assert!(dest.settings.dark_mode);
    assert_eq!(dest.settings.board, second.id);
    assert_eq!(snapshot(&dest), snapshot(&source));
    assert!(Credentials::load(&dest.root).unwrap() == credentials);
    assert!(!recovery_path(&dest.root).unwrap().exists());
}

#[test]
fn invalid_files_and_scopes_leave_the_entire_destination_unchanged() {
    let temp = Temp::new();
    let library = fixture(&temp.0.join("library"));
    let archive = Archive::capture(&library, Scope::Setup).unwrap();
    let before = snapshot(&library);
    for invalid in [
        "../escape",
        "/absolute",
        "boards/../escape",
        "boards\\escape",
        "C:/escape",
        "./escape",
    ] {
        let mut damaged = archive.clone();
        damaged.files.insert(
            invalid.into(),
            File {
                data: STANDARD.encode("bad"),
                sha256: STANDARD.encode(Sha256::digest(b"bad")),
            },
        );
        assert!(damaged.import(&library).is_err());
    }
    let mut damaged = archive.clone();
    damaged.files.get_mut("library.json").unwrap().data = STANDARD.encode("changed");
    assert!(damaged.import(&library).is_err());
    let mut damaged = archive.clone();
    damaged.version = 999;
    assert!(damaged.import(&library).is_err());
    let mut damaged = archive.clone();
    damaged.files.remove("library.json");
    assert!(damaged.import(&library).is_err());
    let mut damaged = archive.clone();
    damaged.files.get_mut("library.json").unwrap().data = STANDARD.encode("{}");
    damaged.files.get_mut("library.json").unwrap().sha256 = STANDARD.encode(Sha256::digest(b"{}"));
    assert!(damaged.import(&library).is_err());
    let mut damaged = archive.clone();
    damaged.scope = Scope::Board(library.settings.board.clone());
    assert!(damaged.import(&library).is_err());
    assert_eq!(snapshot(&library), before);
}

#[test]
fn deleting_selected_and_last_items_keeps_the_app_usable_and_other_data_intact() {
    let temp = Temp::new();
    let mut library = fixture(&temp.0.join("library"));
    let old_board = library.settings.board.clone();
    let other = library
        .create_board("Remaining", &Workspace::default())
        .unwrap();
    let vault_before = library.vaults.clone();
    delete(&library, &Scope::Board(old_board.clone())).unwrap();
    library = Library::open(library.root, None).unwrap();
    assert_eq!(library.settings.board, other.id);
    assert!(!library.board_path(&old_board).unwrap().exists());
    assert_eq!(library.vaults, vault_before);
    delete(&library, &Scope::Board(other.id)).unwrap();
    library = Library::open(library.root, None).unwrap();
    assert_eq!(library.boards.len(), 1);
    assert!(
        library
            .load_board(&library.settings.board)
            .unwrap()
            .tasks
            .is_empty()
    );
    let old_vault = library.settings.vault.clone();
    delete(&library, &Scope::Vault(old_vault.clone())).unwrap();
    library = Library::open(library.root, None).unwrap();
    assert_eq!(library.vaults.len(), 1);
    assert_ne!(library.settings.vault, old_vault);
    assert!(!library.vault_path(&old_vault).unwrap().exists());
    assert!(library.load_plain_vault(&library.vaults[0]).is_ok());
    let before = snapshot(&library);
    assert!(delete(&library, &Scope::Board("missing".into())).is_err());
    assert_eq!(snapshot(&library), before);
}

#[test]
fn interrupted_replacement_recovers_original_instead_of_initializing_an_empty_library() {
    let temp = Temp::new();
    let library = fixture(&temp.0.join("library"));
    let before = snapshot(&library);
    let rollback = format!("amigo-rollback-{}", id());
    atomic_write(&recovery_path(&library.root).unwrap(), rollback.as_bytes()).unwrap();
    fs::rename(&library.root, temp.0.join(&rollback)).unwrap();
    let recovered = Library::open(library.root, None).unwrap();
    assert_eq!(snapshot(&recovered), before);
    assert!(!temp.0.join(rollback).exists());
}

#[test]
fn failed_install_rolls_back_and_committed_restore_cleans_up_only_after_validation() {
    let temp = Temp::new();
    let library = fixture(&temp.0.join("library"));
    let before = snapshot(&library);
    assert!(commit(&library.root, &temp.0.join("missing-stage")).is_err());
    assert_eq!(snapshot(&library), before);
    assert!(!recovery_path(&library.root).unwrap().exists());

    let rollback = format!("amigo-rollback-{}", id());
    let old = temp.0.join(&rollback);
    private_dir(&old).unwrap();
    copy_library(&library, &old).unwrap();
    atomic_write(&recovery_path(&library.root).unwrap(), rollback.as_bytes()).unwrap();
    let settings = fs::read(library.root.join("library.json")).unwrap();
    fs::write(library.root.join("library.json"), "damaged").unwrap();
    assert!(Library::open(library.root.clone(), None).is_err());
    assert!(old.join("library.json").exists());
    fs::write(library.root.join("library.json"), settings).unwrap();
    let reopened = Library::open(library.root.clone(), None).unwrap();
    assert_eq!(snapshot(&reopened), before);
    assert!(!old.exists());
    assert!(!recovery_path(&library.root).unwrap().exists());
}

#[test]
fn backups_reject_missing_attachments_and_export_destinations_inside_the_library() {
    let temp = Temp::new();
    let library = fixture(&temp.0.join("library"));
    let archive = Archive::capture(&library, Scope::Setup).unwrap();
    assert!(
        archive
            .write(&library.root.join("backup.amigo"), &library.root)
            .is_err()
    );
    let mut incomplete = archive;
    incomplete
        .files
        .retain(|path, _| !path.contains("/attachments/"));
    let before = snapshot(&library);
    assert!(incomplete.import(&library).is_err());
    assert_eq!(snapshot(&library), before);
}

#[cfg(unix)]
#[test]
fn exports_reject_symlinks_without_reading_files_outside_the_library() {
    let temp = Temp::new();
    let library = fixture(&temp.0.join("library"));
    std::os::unix::fs::symlink("/etc/hostname", library.root.join("external")).unwrap();
    assert!(Archive::capture(&library, Scope::Setup).is_err());
}

#[test]
fn calendar_delete_preserves_journal_boards_vaults_and_telegram_and_setup_roundtrips_markdown() {
    let temp = Temp::new();
    let library = fixture(&temp.0.join("source"));
    let journal = crate::journal::Journal::open(&library.root).unwrap();
    let note = "---\ntags: [notes]\n---\n# Journal 🌍\n\n**Markdown**\n";
    journal.create("Note", note).unwrap();
    journal.create_folder("Projects").unwrap();
    journal.create_folder("Projects/Discarded").unwrap();
    journal
        .create("Projects/Discarded/Restore", "Restorable after backup")
        .unwrap();
    journal.trash("Projects/Discarded").unwrap();
    let trashed = journal
        .entries()
        .unwrap()
        .into_iter()
        .find(|entry| entry.folder && crate::journal::entry_name(&entry.path) == "Discarded")
        .unwrap()
        .path;
    let mut organizer = Organizer {
        name: "Work Calendar".into(),
        ..Default::default()
    };
    organizer.events.push(CalendarEvent::default());
    organizer.save(&library.root).unwrap();
    let mut credentials = Credentials {
        telegram_token: "secret-token".into(),
        telegram_chat: "chat".into(),
        ..Default::default()
    };
    credentials
        .calendar_urls
        .insert("feed".into(), "https://example.com/feed.ics".into());
    credentials.save(&library.root).unwrap();
    let board = library.load_board(&library.settings.board).unwrap();
    let vault = library.load_plain_vault(&library.vaults[0]).unwrap();
    delete(&library, &Scope::Calendar).unwrap();
    assert!(Organizer::load(&library.root).unwrap().events.is_empty());
    assert!(
        Credentials::load(&library.root)
            .unwrap()
            .calendar_urls
            .is_empty()
    );
    assert_eq!(
        Credentials::load(&library.root).unwrap().telegram_token,
        "secret-token"
    );
    let reloaded = Library::open(library.root.clone(), None).unwrap();
    assert_eq!(
        reloaded.load_board(&reloaded.settings.board).unwrap(),
        board
    );
    assert_eq!(
        reloaded.load_plain_vault(&reloaded.vaults[0]).unwrap(),
        vault
    );
    assert_eq!(journal.read("Note.md").unwrap().text, note);
    let archive = Archive::capture(&reloaded, Scope::Setup).unwrap();
    let dest = Library::open(temp.0.join("dest"), None).unwrap();
    archive.import(&dest).unwrap();
    assert_eq!(
        crate::journal::Journal::open(&dest.root)
            .unwrap()
            .read("Note.md")
            .unwrap()
            .text,
        note
    );
    let restored = crate::journal::Journal::open(&dest.root).unwrap();
    assert_eq!(restored.restore(&trashed).unwrap(), "Projects/Discarded");
    assert_eq!(
        restored.read("Projects/Discarded/Restore.md").unwrap().text,
        "Restorable after backup"
    );
}
