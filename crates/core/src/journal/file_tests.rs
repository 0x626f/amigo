use super::*;
fn library() -> (PathBuf, Journal) {
    let root = std::env::temp_dir().join(format!("amigo-journal-files-{}", id()));
    let journal = Journal::open(&root).unwrap();
    (root, journal)
}
#[test]
fn external_drafts_open_without_import_and_save_the_edited_snapshot() {
    let (root, journal) = library();
    journal.create_folder("Work").unwrap();
    let source = root.join("External.md");
    fs::write(&source, "Original").unwrap();
    let before = journal.entries().unwrap();
    let mut doc = journal.open_file(&source).unwrap();
    assert_eq!(journal.entries().unwrap(), before);
    assert_eq!(
        doc.external.as_deref(),
        Some(source.canonicalize().unwrap().as_path())
    );
    doc.text = "Edited temporary draft Привет".into();
    assert!(journal.save_document(&doc).is_err());
    assert_eq!(fs::read_to_string(&source).unwrap(), "Original");
    fs::write(&source, "Changed by another app").unwrap();
    let stored = journal.store_document(&doc, "Work").unwrap();
    assert_eq!(stored.path, "Work/External.md");
    assert!(stored.external.is_none());
    assert_eq!(stored.text, doc.text);
    assert_eq!(
        fs::read_to_string(&source).unwrap(),
        "Changed by another app"
    );
    assert_eq!(
        journal.store_document(&doc, "Work").unwrap().path,
        "Work/External (1).md"
    );
    assert!(journal.store_document(&doc, "Trash").is_err());
    assert!(
        journal
            .open_file(&root.join("journal/Work/External.md"))
            .unwrap()
            .external
            .is_none()
    );
    fs::remove_file(&source).unwrap();
    assert_eq!(journal.store_document(&doc, "").unwrap().text, doc.text);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn documents_move_between_folders_without_overwriting_or_changing_contents() {
    let (root, journal) = library();
    journal.create_folder("Work").unwrap();
    journal.create_folder("Archive").unwrap();
    journal
        .create("Work/Note", "# Unicode café Привет\n")
        .unwrap();
    journal.create("Archive/Note", "Keep destination").unwrap();
    assert!(journal.move_file("Work/Note.md", "Archive").is_err());
    assert_eq!(
        journal.read("Work/Note.md").unwrap().text,
        "# Unicode café Привет\n"
    );
    assert_eq!(
        journal.read("Archive/Note.md").unwrap().text,
        "Keep destination"
    );
    assert_eq!(journal.move_file("Work/Note.md", "").unwrap(), "Note.md");
    assert!(journal.read("Work/Note.md").is_err());
    assert_eq!(journal.move_file("Note.md", "").unwrap(), "Note.md");
    assert!(journal.move_file("Note.md", "Trash").is_err());
    assert!(journal.move_file("Note.md", "../outside").is_err());
    assert!(journal.move_file("Work", "Archive").is_err());
    let bytes = include_bytes!("../../tests/fixtures/journal-forms.pdf");
    let source = root.join("Form.pdf");
    fs::write(&source, bytes).unwrap();
    let pdf = journal.open_file(&source).unwrap();
    assert!(pdf.external.is_some());
    let saved = journal.store_document(&pdf, "Work").unwrap();
    assert!(saved.external.is_none());
    assert_eq!(fs::read(&source).unwrap(), bytes);
    assert_eq!(
        journal.move_file(&saved.path, "Archive").unwrap(),
        "Archive/Form.pdf"
    );
    assert_eq!(
        journal
            .read("Archive/Form.pdf")
            .unwrap()
            .pdf
            .unwrap()
            .bytes
            .as_ref(),
        bytes
    );
    assert!(journal.move_file("Archive/Form.pdf", "Missing").is_err());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn external_document_links_stay_in_the_opened_directory() {
    let (root, _) = library();
    let folder = root.join("external");
    fs::create_dir(&folder).unwrap();
    let source = folder.join("Note.md");
    fs::write(&source, "").unwrap();
    fs::write(folder.join("Other.md"), "").unwrap();
    fs::write(root.join("Outside.md"), "").unwrap();
    assert_eq!(
        external_link(&source, "Other#heading").unwrap(),
        folder.join("Other.md").canonicalize().unwrap()
    );
    fs::create_dir(folder.join("nested")).unwrap();
    assert_eq!(
        external_link(&folder.join("nested/../Note.md"), "Other.md").unwrap(),
        folder.join("Other.md").canonicalize().unwrap()
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("Outside.md"), folder.join("Escapes.md")).unwrap();
        assert!(external_link(&source, "Escapes.md").is_err());
    }
    for target in ["../Outside.md", "file:///etc/passwd", "/etc/passwd"] {
        assert!(external_link(&source, target).is_err());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tree_orders_sibling_folders_before_files_with_alphabetical_prefix_order() {
    let (root, journal) = library();
    for folder in ["Zebra", "beta", "beta/Notebook", "beta/Note"] {
        journal.create_folder(folder).unwrap();
    }
    for name in [
        "B",
        "A-long",
        "A",
        "aardvark",
        "Язык",
        "Я",
        "beta/A-long",
        "beta/A",
        "beta/Note/Child",
    ] {
        journal.create(name, "text").unwrap();
    }
    // PDF and Markdown siblings share the same file group.
    fs::write(root.join("journal/alpha.pdf"), b"%PDF-1.7").unwrap();
    let paths: Vec<_> = journal
        .entries()
        .unwrap()
        .into_iter()
        .map(|e| e.path)
        .collect();
    assert_eq!(
        paths,
        [
            "beta",
            "beta/Note",
            "beta/Note/Child.md",
            "beta/Notebook",
            "beta/A.md",
            "beta/A-long.md",
            "Trash",
            "Zebra",
            "A.md",
            "A-long.md",
            "aardvark.md",
            "alpha.pdf",
            "B.md",
            "Я.md",
            "Язык.md",
        ]
    );
    // Reloading keeps the same order without changing the files.
    assert_eq!(
        Journal::open(&root)
            .unwrap()
            .entries()
            .unwrap()
            .into_iter()
            .map(|e| e.path)
            .collect::<Vec<_>>(),
        paths
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn trash_tree_sorts_displayed_names_instead_of_storage_ids() {
    let (root, journal) = library();
    let trash = root.join("journal/Trash");
    let a = "ffffffff-ffff-ffff-ffff-ffffffffffff-A.md";
    let longer = "11111111-1111-1111-1111-111111111111-A-long.md";
    let z = "00000000-0000-0000-0000-000000000000-Z.md";
    for name in [z, longer, a] {
        fs::write(trash.join(name), "text").unwrap();
    }
    fs::create_dir(trash.join("ZZ folder")).unwrap();
    let names: Vec<_> = journal
        .entries()
        .unwrap()
        .iter()
        .map(|e| entry_name(&e.path).to_owned())
        .collect();
    assert_eq!(names, ["Trash", "ZZ folder", "A.md", "A-long.md", "Z.md"]);
    fs::remove_dir_all(root).unwrap();
}
