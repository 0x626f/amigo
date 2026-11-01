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
