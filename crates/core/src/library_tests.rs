use super::*;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("amigo-library-test-{}", id())))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> Workspace {
    let mut w = Workspace {
        dark_mode: true,
        ..Workspace::default()
    };
    w.task_categories.push(StoredValue {
        id: "work".into(),
        name: "Work".into(),
    });
    let mut task = Task::new(7, "in-progress");
    task.id = "../../arbitrary-import-id".into();
    task.title = "A task with \"quotes\" and 日本語".into();
    task.description = "First paragraph.\n\n- A Markdown list\n- Another item\n".into();
    task.category_id = "work".into();
    task.labels = vec!["One".into(), "Two".into()];
    task.due_date = "2026-12-01".into();
    w.tasks.push(task);
    w.tasks.push(Task::new(9, "done"));
    w.tasks[1].title = "Second".into();
    w.finance=plan_from_value(json!({"version":2,"meta":{"name":"Private finance","reportingCurrency":"EUR"},
        "income":[{"id":"z","source":"SENSITIVE-VALUE-9182","grossAmount":"1200.50","currency":"EUR","date":"2026-10-01"},
                  {"id":"a","source":"Second","grossAmount":200,"currency":"EUR","date":"2026-10-02"}],
        "expenses":[{"id":"expense-1","title":"Rent","amount":"900","currency":"EUR"}],
        "savings":[{"id":"saving-z","source":"Reserve","amount":1000,"currency":"EUR"},{"id":"saving-a","source":"Cash","amount":10,"currency":"EUR"}],
        "budgets":[{"id":"budget","label":"Living","amount":1000,"month":"2026-10","currency":"EUR","expenses":[{"id":"hotel","label":"Hotel","amount":100,"currency":"USD","comment":"Booked for the trip","paid":true},{"id":"taxi","label":"Taxi","amount":20,"currency":"EUR","paid":false}]}],
        "fxRates":[{"id":"rate","date":"2026-10-01","currency":"EUR","rate":1.1}]})).unwrap();
    w.normalize();
    w
}
fn all_files(root: &Path) -> Vec<PathBuf> {
    let mut files = vec![];
    for e in fs::read_dir(root).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            files.extend(all_files(&p));
        } else {
            files.push(p);
        }
    }
    files
}

#[test]
fn attachments_are_board_owned_binary_copies_and_survive_source_removal_and_reload() {
    let temp = Temp::new();
    let mut lib = Library::open(temp.0.join("library"), None).unwrap();
    let mut workspace = fixture();
    let board = lib.create_board("Attachments", &workspace).unwrap();
    let root = lib.board_path(&board.id).unwrap();
    let source = temp.0.join("Résumé 2026.unknown");
    let bytes = [0, 255, 128, 17, 10, 0];
    fs::write(&source, bytes).unwrap();
    let attachment = crate::attachments::import(&root, &source).unwrap();
    workspace.tasks[0].attachments.push(attachment.clone());
    lib.save_board(&board.id, &workspace).unwrap();
    fs::remove_file(source).unwrap();
    let loaded = lib.load_board(&board.id).unwrap();
    assert_eq!(
        loaded.tasks[0].attachments.as_slice(),
        std::slice::from_ref(&attachment)
    );
    let file = crate::attachments::resolve(&root, &attachment).unwrap();
    assert!(file.starts_with(root.join("attachments")));
    assert_eq!(fs::read(&file).unwrap(), bytes);
    assert_eq!(attachment.size, 6);
    let bad = TaskAttachment {
        name: "../../outside".into(),
        ..attachment.clone()
    };
    assert!(crate::attachments::resolve(&root, &bad).is_err());
    let bad = TaskAttachment {
        id: "../outside".into(),
        ..attachment.clone()
    };
    assert!(crate::attachments::resolve(&root, &bad).is_err());
    #[cfg(unix)]
    {
        let outside = temp.0.join("outside.txt");
        fs::write(&outside, "outside").unwrap();
        let link = root
            .join("attachments")
            .join(&attachment.id)
            .join("outside.txt");
        std::os::unix::fs::symlink(outside, link).unwrap();
        let bad = TaskAttachment {
            name: "outside.txt".into(),
            ..attachment
        };
        assert!(crate::attachments::resolve(&root, &bad).is_err());
    }
    workspace.tasks[0].attachments.clear();
    lib.save_board(&board.id, &workspace).unwrap();
    assert!(!file.exists());
}

#[test]
fn boards_are_markdown_folders_with_roundtrip_editing_and_deletion() {
    let temp = Temp::new();
    let mut lib = Library::open(temp.0.clone(), None).unwrap();
    let w = fixture();
    let board = lib.create_board("Work", &w).unwrap();
    let path = lib.board_path(&board.id).unwrap();
    assert_eq!(md_files(&path).unwrap().len(), 2);
    let loaded = lib.load_board(&board.id).unwrap();
    assert_eq!(loaded.tasks, w.tasks);
    assert_eq!(loaded.task_statuses, w.task_statuses);
    assert_eq!(loaded.task_categories, w.task_categories);
    assert!(
        lib.load_board(&lib.settings.board)
            .unwrap()
            .tasks
            .is_empty()
    );
    let file = path.join(filename(&w.tasks[0].id));
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("# A task"));
    assert!(text.contains("- A Markdown list"));
    fs::write(
        &file,
        text.replace(
            "# A task with \"quotes\" and 日本語",
            "# Edited outside Amigo",
        )
        .replace("First paragraph.", "Changed description."),
    )
    .unwrap();
    let mut loaded = lib.load_board(&board.id).unwrap();
    assert_eq!(loaded.tasks[0].title, "Edited outside Amigo");
    assert!(
        loaded.tasks[0]
            .description
            .starts_with("Changed description.")
    );
    loaded.tasks.remove(1);
    lib.save_board(&board.id, &loaded).unwrap();
    assert_eq!(md_files(&path).unwrap().len(), 1);
    lib.rename_board(&board.id, "Renamed / name").unwrap();
    assert!(path.exists());
    assert_eq!(
        lib.boards.iter().find(|b| b.id == board.id).unwrap().name,
        "Renamed / name"
    );
    fs::write(file, "broken Markdown").unwrap();
    assert!(lib.load_board(&board.id).is_err());
    assert!(lib.board_path("../escape").is_err());
}

#[test]
fn plain_vault_preserves_all_finance_data_and_record_order() {
    let temp = Temp::new();
    let mut lib = Library::open(temp.0.clone(), None).unwrap();
    let w = fixture();
    let vault = lib.create_vault("Accounts", &w.finance, None).unwrap();
    let path = lib.vault_path(&vault.entry.id).unwrap();
    assert!(path.join("records/income").is_dir());
    let accounts = md_files(&path.join("records/accounts")).unwrap();
    let text = std::fs::read_to_string(&accounts[0]).unwrap();
    let (account, _) = parse_markdown(&text).unwrap();
    assert!(account.get("label").is_some());
    for removed in ["country", "principal", "comment"] {
        assert!(account.get(removed).is_none());
    }
    assert_eq!(lib.load_plain_vault(&vault).unwrap(), w.finance);
    let mut plan = w.finance;
    plan.income.remove(0);
    plan.savings.reverse();
    lib.save_vault(&vault, &plan, None).unwrap();
    assert_eq!(lib.load_plain_vault(&vault).unwrap(), plan);
    assert_eq!(md_files(&path.join("records/income")).unwrap().len(), 1);
    lib.rename_vault(&vault.entry.id, "Household").unwrap();
    let reopened = Library::open(temp.0.clone(), None).unwrap();
    assert_eq!(reopened.load_plain_vault(&vault).unwrap(), plan);
}

#[test]
fn stopped_regular_expenses_keep_their_cutoff_after_vault_reload() {
    let temp = Temp::new();
    let mut lib = Library::open(temp.0.clone(), None).unwrap();
    let mut plan = FinancePlan::empty();
    plan.meta.reporting_currency = "EUR".into();
    let mut expense = Expense::new("EUR");
    expense.title = "Rent".into();
    expense.due_date = "2026-09-30".into();
    expense.amount = Amount::Number(100.0);
    expense.regular = true;
    expense.applicable = true;
    let key = expense.id.clone();
    plan.expenses.push(expense);
    plan.normalize();
    let vault = lib.create_vault("Household", &plan, None).unwrap();
    plan.remove_expense(&key, "2026-10").unwrap();
    lib.save_vault(&vault, &plan, None).unwrap();
    let reopened = Library::open(temp.0.clone(), None).unwrap();
    let saved = reopened.load_plain_vault(&vault).unwrap();
    assert_eq!(saved, plan);
    assert_eq!(saved.totals("2026-09", false).1, 100.0);
    assert_eq!(saved.totals("2026-10", false).1, 100.0);
    assert_eq!(saved.totals("2026-11", false).1, 0.0);
    // The same metadata must survive encrypted storage too.
    let password = "regular expense test password";
    let encryption_key = VaultKey::new(password).unwrap();
    let encrypted = lib
        .create_vault("Protected", &plan, Some(&encryption_key))
        .unwrap();
    let (saved, _) = VaultKey::unlock(
        &lib.vault_path(&encrypted.entry.id).unwrap(),
        &encrypted.entry.id,
        password,
    )
    .unwrap();
    assert_eq!(saved, plan);
    assert_eq!(saved.totals("2026-11", false).1, 0.0);
}

#[test]
fn cancelled_expenses_keep_their_date_and_history_after_vault_reload() {
    let temp = Temp::new();
    let mut lib = Library::open(temp.0.clone(), None).unwrap();
    let mut plan = FinancePlan::empty();
    plan.meta.reporting_currency = "EUR".into();
    let mut expense = Expense::new("EUR");
    expense.title = "Rent".into();
    expense.due_date = "2026-09-30".into();
    expense.amount = Amount::Number(100.0);
    expense.regular = true;
    expense.applicable = true;
    let key = expense.id.clone();
    plan.expenses.push(expense);
    plan.normalize();
    let vault = lib.create_vault("Household", &plan, None).unwrap();
    plan.cancel_expense(&key, "2026-10-21").unwrap();
    lib.save_vault(&vault, &plan, None).unwrap();
    let reopened = Library::open(temp.0.clone(), None).unwrap();
    let saved = reopened.load_plain_vault(&vault).unwrap();
    assert_eq!(saved, plan);
    assert_eq!(saved.totals("2026-09", false).1, 100.0);
    assert_eq!(saved.totals("2026-10", false).1, 0.0);
    assert_eq!(saved.expenses[0].cancelled_on, "2026-10-21");
    assert_eq!(saved.totals("2026-11", false).1, 0.0);
    // The same metadata must survive encrypted storage too.
    let password = "regular expense test password";
    let encryption_key = VaultKey::new(password).unwrap();
    let encrypted = lib
        .create_vault("Protected", &plan, Some(&encryption_key))
        .unwrap();
    let (saved, _) = VaultKey::unlock(
        &lib.vault_path(&encrypted.entry.id).unwrap(),
        &encrypted.entry.id,
        password,
    )
    .unwrap();
    assert_eq!(saved, plan);
    assert_eq!(saved.totals("2026-11", false).1, 0.0);
}

#[test]
fn migration_is_automatic_verified_and_does_not_leave_a_plaintext_legacy_copy() {
    let temp = Temp::new();
    fs::create_dir_all(&temp.0).unwrap();
    let legacy = temp.0.join("workspace.json");
    let w = fixture();
    storage::save_workspace(&legacy, &w).unwrap();
    let root = temp.0.join("library");
    let lib = Library::open(root.clone(), Some(&legacy)).unwrap();
    assert_eq!(lib.load_board(&lib.settings.board).unwrap().tasks, w.tasks);
    assert_eq!(lib.load_plain_vault(&lib.vaults[0]).unwrap(), w.finance);
    assert!(lib.settings.dark_mode);
    let receipt: Value = read(&legacy).unwrap();
    assert_eq!(receipt["migratedTo"], json!(root));
    assert!(
        !fs::read_to_string(&legacy)
            .unwrap()
            .contains("SENSITIVE-VALUE-9182")
    );
    let reopened = Library::open(root, Some(&legacy)).unwrap();
    assert_eq!(reopened.boards.len(), 1);
    assert_eq!(reopened.vaults.len(), 1);
}

#[test]
fn invalid_or_partial_libraries_are_preserved() {
    let temp = Temp::new();
    fs::create_dir_all(&temp.0).unwrap();
    let legacy = temp.0.join("old.json");
    fs::write(&legacy, "not valid JSON").unwrap();
    let root = temp.0.join("library");
    assert!(Library::open(root.clone(), Some(&legacy)).is_err());
    assert_eq!(fs::read_to_string(&legacy).unwrap(), "not valid JSON");
    assert!(!root.exists());
    fs::create_dir(&root).unwrap();
    fs::write(root.join("note.md"), "Keep me").unwrap();
    assert!(Library::open(root.clone(), None).is_err());
    assert_eq!(fs::read_to_string(root.join("note.md")).unwrap(), "Keep me");
}

#[test]
fn encrypted_vault_requires_password_authenticates_data_and_saves_without_plaintext() {
    let temp = Temp::new();
    let mut lib = Library::open(temp.0.clone(), None).unwrap();
    let mut plan = fixture().finance;
    let key = VaultKey::new("correct horse test password").unwrap();
    let vault = lib.create_vault("Secret", &plan, Some(&key)).unwrap();
    let path = lib.vault_path(&vault.entry.id).unwrap();
    assert!(!path.join("records").exists());
    assert!(lib.load_plain_vault(&vault).is_err());
    for file in all_files(&temp.0) {
        assert!(
            !fs::read_to_string(file)
                .unwrap()
                .contains("SENSITIVE-VALUE-9182")
        );
    }
    assert!(VaultKey::unlock(&path, &vault.entry.id, "wrong").is_err());
    let (opened, key) =
        VaultKey::unlock(&path, &vault.entry.id, "correct horse test password").unwrap();
    assert_eq!(opened, plan);
    let before = fs::read(path.join("vault.enc")).unwrap();
    plan.income[0].gross_amount = Amount::Number(9000.0);
    lib.save_vault(&vault, &plan, Some(&key)).unwrap();
    assert_ne!(before, fs::read(path.join("vault.enc")).unwrap());
    let (saved, _) =
        VaultKey::unlock(&path, &vault.entry.id, "correct horse test password").unwrap();
    assert_eq!(saved, plan);
    assert_eq!(key.load(&path, &vault.entry.id).unwrap(), plan);
    assert!(key.load(&path, "another-vault").is_err());
    assert!(lib.save_vault(&vault, &plan, None).is_err());
    let mut envelope: Value = read(&path.join("vault.enc")).unwrap();
    envelope["data"] = json!(STANDARD.encode([0; 32]));
    write(&path.join("vault.enc"), &envelope).unwrap();
    assert!(key.load(&path, &vault.entry.id).is_err());
    assert!(VaultKey::unlock(&path, &vault.entry.id, "correct horse test password").is_err());
    assert!(
        !fs::read_to_string(temp.0.join("library.json"))
            .unwrap()
            .contains("password")
    );
}

#[test]
fn encrypting_existing_vault_removes_plaintext_and_interrupted_conversion_is_recoverable() {
    let temp = Temp::new();
    let mut lib = Library::open(temp.0.clone(), None).unwrap();
    let plan = fixture().finance;
    let key = VaultKey::new("vault test password").unwrap();
    let vault = lib.create_vault("Household", &plan, None).unwrap();
    let path = lib.vault_path(&vault.entry.id).unwrap();
    assert!(path.join("records").exists());
    let encrypted = lib.encrypt_vault(&vault.entry.id, &plan, &key).unwrap();
    assert!(encrypted.encrypted);
    assert!(!path.join("records").exists());
    for file in all_files(&temp.0) {
        assert!(
            !fs::read_to_string(file)
                .unwrap()
                .contains("SENSITIVE-VALUE-9182")
        );
    }
    let other = lib.create_vault("Interrupted", &plan, None).unwrap();
    let other_path = lib.vault_path(&other.entry.id).unwrap();
    storage::atomic_write(
        &other_path.join("vault.enc"),
        &key.seal(&other.entry.id, &plan).unwrap(),
    )
    .unwrap();
    let mut recovered = Library::open(temp.0.clone(), None).unwrap();
    assert!(
        recovered
            .vaults
            .iter()
            .find(|v| v.entry.id == other.entry.id)
            .unwrap()
            .encrypted
    );
    assert!(other_path.join("records").exists()); // Don't delete data before authenticated unlock.
    let (decoded, _) =
        VaultKey::unlock(&other_path, &other.entry.id, "vault test password").unwrap();
    assert_eq!(decoded, plan);
    recovered.complete_encryption(&other.entry.id).unwrap();
    assert!(!other_path.join("records").exists());
}

#[test]
fn changing_vault_password_authenticates_the_old_key_and_keeps_all_records() {
    let temp = Temp::new();
    let mut library = Library::open(temp.0.clone(), None).unwrap();
    let mut plan = fixture().finance;
    let old_key = VaultKey::new("original vault password").unwrap();
    let new_key = VaultKey::new("replacement vault password").unwrap();
    let vault = library
        .create_vault("Protected", &plan, Some(&old_key))
        .unwrap();
    let path = library.vault_path(&vault.entry.id).unwrap();
    let encrypted = fs::read(path.join("vault.enc")).unwrap();
    let metadata = fs::read(path.join("vault.json")).unwrap();
    plan.income[0].gross_amount = Amount::Number(4321.0);
    assert!(
        library
            .change_vault_password(&vault.entry.id, &plan, &new_key, &new_key)
            .is_err()
    );
    assert_eq!(fs::read(path.join("vault.enc")).unwrap(), encrypted);
    library
        .change_vault_password(&vault.entry.id, &plan, &old_key, &new_key)
        .unwrap();
    assert!(VaultKey::unlock(&path, &vault.entry.id, "original vault password").is_err());
    let (reopened, _) =
        VaultKey::unlock(&path, &vault.entry.id, "replacement vault password").unwrap();
    assert_eq!(reopened, plan);
    assert_eq!(fs::read(path.join("vault.json")).unwrap(), metadata);
    assert!(!path.join("records").exists());
    let after = fs::read(path.join("vault.enc")).unwrap();
    assert!(
        library
            .change_vault_password(&vault.entry.id, &plan, &old_key, &new_key)
            .is_err()
    );
    assert_eq!(fs::read(path.join("vault.enc")).unwrap(), after);
}

#[test]
fn failed_selection_write_keeps_the_original_board_and_vault_active() {
    let temp = Temp::new();
    let mut lib = Library::open(temp.0.clone(), None).unwrap();
    let original_board = lib.settings.board.clone();
    let original_vault = lib.settings.vault.clone();
    let board = lib.create_board("Other", &Workspace::default()).unwrap();
    let vault = lib
        .create_vault("Other", &FinancePlan::empty(), None)
        .unwrap();
    let settings = temp.0.join("library.json");
    fs::rename(&settings, temp.0.join("settings-backup.json")).unwrap();
    fs::create_dir(&settings).unwrap();
    assert!(lib.select_board(&board.id).is_err());
    assert_eq!(lib.settings.board, original_board);
    assert!(lib.select_vault(&vault.entry.id).is_err());
    assert_eq!(lib.settings.vault, original_vault);
}

#[test]
fn amigo_library_paths_support_legacy_overrides_without_changing_the_environment() {
    let resolve = |values: &[(&str, &str)]| {
        root_from_environment(|key| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| std::ffi::OsString::from(value))
        })
    };
    assert_eq!(
        resolve(&[("HOME", "/home/example")]),
        PathBuf::from("/home/example/.amigo")
    );
    assert_eq!(
        resolve(&[("USERPROFILE", r"C:\Users\Example")]),
        PathBuf::from(r"C:\Users\Example").join(".amigo")
    );
    assert_eq!(
        resolve(&[("HOME", ""), ("USERPROFILE", r"C:\Users\Example")]),
        PathBuf::from(r"C:\Users\Example").join(".amigo")
    );
    assert_eq!(
        resolve(&[("HOMEDRIVE", "D:"), ("HOMEPATH", r"\Users\Example")]),
        PathBuf::from(r"D:\Users\Example").join(".amigo")
    );
    assert_eq!(
        resolve(&[("HOME", "/existing"), ("USERPROFILE", "/other")]),
        PathBuf::from("/existing").join(".amigo")
    );
    assert_eq!(
        resolve(&[("CLERK_HOME", "/old-custom")]),
        PathBuf::from("/old-custom")
    );
    assert_eq!(
        resolve(&[("CLERK_DATA_FILE", "/tmp/old.json")]),
        PathBuf::from("/tmp/old.library")
    );
    assert_eq!(
        resolve(&[
            ("AMIGO_DATA_FILE", "/tmp/new.json"),
            ("CLERK_HOME", "/old-custom")
        ]),
        PathBuf::from("/tmp/new.library")
    );
    assert_eq!(
        resolve(&[
            ("AMIGO_HOME", "/new-custom"),
            ("CLERK_HOME", "/old-custom"),
            ("AMIGO_DATA_FILE", "/tmp/new.json")
        ]),
        PathBuf::from("/new-custom")
    );
}

#[test]
fn windows_line_endings_preserve_tasks_and_plain_vault_records() {
    let temp = Temp::new();
    let mut library = Library::open(temp.0.clone(), None).unwrap();
    let workspace = fixture();
    let board = library.create_board("Windows edits", &workspace).unwrap();
    let vault = library
        .create_vault("Windows finance", &workspace.finance, None)
        .unwrap();
    for file in all_files(&temp.0) {
        if file.extension().is_some_and(|extension| extension == "md") {
            let text = fs::read_to_string(&file).unwrap();
            fs::write(&file, text.replace('\n', "\r\n")).unwrap();
        }
    }
    assert_eq!(
        library.load_board(&board.id).unwrap().tasks,
        workspace.tasks
    );
    assert_eq!(library.load_plain_vault(&vault).unwrap(), workspace.finance);
    library.save_board(&board.id, &workspace).unwrap();
    library
        .save_vault(&vault, &workspace.finance, None)
        .unwrap();
    assert_eq!(
        library.load_board(&board.id).unwrap().tasks,
        workspace.tasks
    );
    assert_eq!(library.load_plain_vault(&vault).unwrap(), workspace.finance);
}

#[test]
fn rebrand_moves_the_entire_clerk_library_and_preserves_encrypted_data() {
    let temp = Temp::new();
    let previous = temp.0.join(".clerk");
    let root = temp.0.join(".amigo");
    let mut lib = Library::open(previous.clone(), None).unwrap();
    let workspace = fixture();
    let board = lib.create_board("Work", &workspace).unwrap();
    let plain = lib.create_vault("Plain", &workspace.finance, None).unwrap();
    let key = VaultKey::new("rebrand-test-password").unwrap();
    let encrypted = lib
        .create_vault("Protected", &workspace.finance, Some(&key))
        .unwrap();
    lib.select_board(&board.id).unwrap();
    lib.select_vault(&encrypted.entry.id).unwrap();
    let credentials = crate::credentials::Credentials {
        telegram_token: "synthetic-test-token".into(),
        ..Default::default()
    };
    credentials.save(&previous).unwrap();
    crate::organizer::Organizer::default()
        .save(&previous)
        .unwrap();
    for (name, bytes) in [
        (
            format!("boards/{}/attachments/file.bin", board.id),
            b"\0attachment".as_slice(),
        ),
        ("backups/retained.txt".to_string(), b"backup".as_slice()),
    ] {
        let path = previous.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    let before: Vec<_> = all_files(&previous)
        .iter()
        .map(|path| {
            (
                path.strip_prefix(&previous).unwrap().to_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect();
    move_previous_libraries(&temp.0, &root).unwrap();
    assert!(!previous.exists());
    for (path, bytes) in before {
        assert_eq!(fs::read(root.join(path)).unwrap(), bytes);
    }
    let migrated = Library::open(root.clone(), None).unwrap();
    assert_eq!(migrated.settings.board, board.id);
    assert_eq!(migrated.settings.vault, encrypted.entry.id);
    assert_eq!(
        migrated.load_board(&board.id).unwrap().tasks,
        workspace.tasks
    );
    assert_eq!(
        migrated.load_plain_vault(&plain).unwrap(),
        workspace.finance
    );
    let (opened, _) = VaultKey::unlock(
        &migrated.vault_path(&encrypted.entry.id).unwrap(),
        &encrypted.entry.id,
        "rebrand-test-password",
    )
    .unwrap();
    assert_eq!(opened, workspace.finance);
    assert!(crate::credentials::Credentials::load(&root).unwrap() == credentials);
    assert!(crate::organizer::Organizer::load(&root).is_ok());
    Library::open(previous.clone(), None).unwrap();
    move_previous_libraries(&temp.0, &root).unwrap();
    assert!(
        previous.join("library.json").is_file(),
        "An existing Amigo library must not be replaced"
    );
    assert_eq!(
        Library::open(root, None).unwrap().settings.vault,
        encrypted.entry.id
    );
}

#[test]
fn moving_previous_default_library_preserves_data_and_never_replaces_an_existing_library() {
    let temp = Temp::new();
    let previous = temp.0.join("Clerk");
    let root = temp.0.join(".amigo");
    let mut library = Library::open(previous.clone(), None).unwrap();
    let workspace = fixture();
    let board = library.create_board("Work", &workspace).unwrap();
    let vault = library
        .create_vault("Accounts", &workspace.finance, None)
        .unwrap();
    move_previous_library(&previous, &root).unwrap();
    assert!(!previous.exists());
    let moved = Library::open(root.clone(), None).unwrap();
    assert_eq!(moved.load_board(&board.id).unwrap().tasks, workspace.tasks);
    assert_eq!(moved.load_plain_vault(&vault).unwrap(), workspace.finance);
    Library::open(previous.clone(), None).unwrap();
    move_previous_library(&previous, &root).unwrap();
    assert!(previous.join("library.json").exists());
    assert!(
        Library::open(root, None)
            .unwrap()
            .boards
            .iter()
            .any(|b| b.id == board.id)
    );
}
