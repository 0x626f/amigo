use super::*;
use crate::{journal::Journal, model::id};
const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/journal-forms.pdf");
#[test]
fn binary_pdf_lifecycle_preserves_bytes_extensions_and_external_changes() {
    let root = std::env::temp_dir().join(format!("amigo-pdf-test-{}", id()));
    let library = crate::library::Library::open(root.clone(), None).unwrap();
    let journal = Journal::open(&root).unwrap();
    journal.create_folder("Work").unwrap();
    std::fs::write(root.join("journal/Work/Форма.PDF"), FIXTURE).unwrap();
    let mut document = journal.read("Work/Форма.PDF").unwrap();
    assert!(document.text.is_empty());
    assert_eq!(document.pdf.as_ref().unwrap().bytes.as_ref(), FIXTURE);
    assert!(
        journal
            .entries()
            .unwrap()
            .iter()
            .any(|entry| entry.path == document.path)
    );
    assert!(journal.create("Empty.pdf", "").is_err());
    assert!(
        journal
            .save(&document.path, "broken", &document.fingerprint)
            .is_err()
    );
    assert!(journal.rename(&document.path, "Work/Form.md").is_err());
    document.path = journal.rename(&document.path, "Work/Form").unwrap();
    assert_eq!(document.path, "Work/Form.pdf");
    journal
        .export(&document.path, &root.join("export.pdf"))
        .unwrap();
    assert_eq!(std::fs::read(root.join("export.pdf")).unwrap(), FIXTURE);
    journal.trash("Work").unwrap();
    let folder = journal
        .entries()
        .unwrap()
        .into_iter()
        .find(|e| e.folder && e.path != "Trash")
        .unwrap();
    assert_eq!(journal.restore(&folder.path).unwrap(), "Work");
    assert_eq!(
        journal
            .read("Work/Form.pdf")
            .unwrap()
            .pdf
            .unwrap()
            .bytes
            .as_ref(),
        FIXTURE
    );
    // An unchanged PDF snapshot still must check the current disk fingerprint.
    std::fs::write(
        root.join("journal/Work/Form.pdf"),
        [FIXTURE, b"\n% external change"].concat(),
    )
    .unwrap();
    assert!(
        journal
            .save_document(&document)
            .unwrap_err()
            .contains("outside Amigo")
    );
    assert_eq!(journal.search("Form.pdf").unwrap().len(), 1);
    assert!(
        journal
            .follow_link("Other.md", "Work/Form.pdf")
            .unwrap()
            .pdf
            .is_some()
    );
    let archive = crate::backup::Archive::capture(&library, crate::backup::Scope::Setup).unwrap();
    let destination = crate::library::Library::open(root.join("restored"), None).unwrap();
    archive.import(&destination).unwrap();
    let restored = Journal::open(&destination.root)
        .unwrap()
        .read("Work/Form.pdf")
        .unwrap();
    assert_eq!(
        restored.pdf.unwrap().bytes.as_ref(),
        [FIXTURE, b"\n% external change"].concat()
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn invalid_pdf_headers_and_paths_are_rejected_without_writing() {
    assert!(validate_header(b"not a PDF").is_err());
    assert!(validate_header(&vec![0; MAX_PDF_BYTES as usize + 1]).is_err());
    let root = std::env::temp_dir().join(format!("amigo-pdf-invalid-{}", id()));
    let journal = Journal::open(&root).unwrap();
    std::fs::write(root.join("journal/bad.pdf"), b"not a PDF").unwrap();
    assert!(journal.read("bad.pdf").is_err());
    assert!(journal.read("../outside.pdf").is_err());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
#[ignore = "requires Poppler GLib, Cairo and Pango"]
fn native_pdf_forms_annotations_rotation_conversion_and_import_round_trip() {
    let mut pdf = PdfDocument::new(FIXTURE.to_vec());
    let page = render(&pdf, 0, 1.).unwrap();
    assert_eq!(page.count, 2);
    assert_eq!(page.pixels, [300, 400]);
    assert!(page.text.contains("Journal PDF fixture"));
    assert_eq!(page.fields.len(), 5);
    assert_eq!(page.annotations[0].text, "Existing note");
    assert!(page.can_fill && page.can_annotate && page.can_copy);
    let name = page.fields.iter().find(|f| f.name == "Your name").unwrap();
    assert_eq!(name.rect, [30., 75., 240., 100.]);
    pdf.edits
        .fields
        .insert(name.id, Value::Text("Español Привет".into()));
    let checkbox = page.fields.iter().find(|f| f.name == "accept").unwrap();
    pdf.edits.fields.insert(checkbox.id, Value::Button(true));
    let choice = page.fields.iter().find(|f| f.name == "language").unwrap();
    pdf.edits.fields.insert(
        choice.id,
        Value::Choice {
            selected: vec![2],
            text: None,
        },
    );
    pdf.edits.annotations.insert(
        (0, page.annotations[0].index),
        Some("Edited existing note".into()),
    );
    pdf.edits.added.push(Annotation {
        id: id(),
        page: 0,
        rect: [30., 340., 240., 385.],
        text: "Added text español".into(),
        note: false,
        color: None,
    });
    pdf.edits.added.push(Annotation {
        id: id(),
        page: 1,
        rect: [30., 40., 150., 80.],
        text: "Rotated annotation".into(),
        note: false,
        color: None,
    });
    let updated = save(&pdf).unwrap();
    let reopened = PdfDocument::new(updated);
    let page = render(&reopened, 0, 1.).unwrap();
    assert_eq!(
        page.fields.iter().find(|f| f.id == name.id).unwrap().value,
        Value::Text("Español Привет".into())
    );
    assert_eq!(
        page.fields
            .iter()
            .find(|f| f.id == checkbox.id)
            .unwrap()
            .value,
        Value::Button(true)
    );
    assert_eq!(
        page.fields
            .iter()
            .find(|f| f.id == choice.id)
            .unwrap()
            .value,
        Value::Choice {
            selected: vec![2],
            text: None
        }
    );
    assert!(
        page.annotations
            .iter()
            .any(|a| a.text == "Edited existing note")
    );
    assert!(
        page.annotations
            .iter()
            .any(|a| a.text == "Added text español")
    );
    assert_eq!(find(&reopened, "SECOND", 0).unwrap(), Some(1));
    assert_eq!(find(&reopened, "not found", 0).unwrap(), None);
    let rotated = render(&reopened, 1, 1.).unwrap();
    assert_eq!(rotated.size, [400., 300.]);
    assert!(
        rotated.fields[0].rect.iter().all(|v| *v >= 0.),
        "{:?}",
        rotated.fields[0].rect
    );
    assert_eq!(
        rotated
            .annotations
            .iter()
            .find(|a| a.text == "Rotated annotation")
            .unwrap()
            .rect,
        [30., 40., 150., 80.]
    );
    let mut removed = reopened.clone();
    removed.edits.annotations.insert(
        (
            0,
            page.annotations
                .iter()
                .find(|a| a.text == "Edited existing note")
                .unwrap()
                .index,
        ),
        None,
    );
    let removed = PdfDocument::new(save(&removed).unwrap());
    let removed_page = render(&removed, 0, 1.).unwrap();
    assert!(
        !removed_page
            .annotations
            .iter()
            .any(|a| a.text == "Edited existing note")
    );
    assert!(
        removed_page
            .annotations
            .iter()
            .any(|a| a.text == "Added text español")
    );
    let readonly = page.fields.iter().find(|f| f.name == "readonly").unwrap();
    let mut invalid = reopened.clone();
    invalid
        .edits
        .fields
        .insert(readonly.id, Value::Text("replace".into()));
    assert!(save(&invalid).unwrap_err().contains("read-only"));
    assert!(render(&reopened, 9, 1.).is_err());
    assert!(render(&reopened, 0, f64::NAN).is_err());
    assert!(validate(b"%PDF-1.7\nbroken").is_err());
    let root = std::env::temp_dir().join(format!("amigo-pdf-native-{}", id()));
    let journal = Journal::open(&root).unwrap();
    journal.create_folder("Work").unwrap();
    let source = root.join("Form.PDF");
    std::fs::write(&source, FIXTURE).unwrap();
    let imported = journal
        .import(&[source.clone(), source.clone()], "Work")
        .unwrap();
    assert_eq!(imported[0].path, "Work/Form.pdf");
    assert_eq!(imported[1].path, "Work/Form (1).pdf");
    assert_eq!(std::fs::read(&source).unwrap(), FIXTURE);
    let bad = root.join("Bad.pdf");
    std::fs::write(&bad, b"%PDF-1.7\ninvalid").unwrap();
    let count = journal.entries().unwrap().len();
    assert!(journal.import(&[source, bad], "Work").is_err());
    assert_eq!(journal.entries().unwrap().len(), count);
    let original="# Español Привет\n\n**Bold** and _italic_ with [link](https://example.com).\n\n- First\n- Second\n\n| One | Two |\n| --- | --- |\n| Español | Привет |\n\n```rust\nfn main() {}\n```\n".to_owned()+&"\nAnother long paragraph with Unicode café Привет.\n".repeat(100);
    let note = journal.create("Work/Report", &original).unwrap();
    let converted = journal.convert_to_pdf(&note.path).unwrap();
    assert_eq!(converted.path, "Work/Report.pdf");
    let converted_pdf = converted.pdf.unwrap();
    let page = render(&converted_pdf, 0, 1.).unwrap();
    assert!(page.count > 1);
    assert!(page.text.contains("Español"));
    assert!(page.text.contains("Привет"));
    assert!(!page.text.contains("**"));
    assert!(find(&converted_pdf, "fn main", 0).unwrap().is_some());
    assert_eq!(
        journal.convert_to_pdf(&note.path).unwrap().path,
        "Work/Report (1).pdf"
    );
    assert_eq!(journal.read(&note.path).unwrap().text, original);
    if let Some(dir) = std::env::var_os("AMIGO_PDF_TEST_OUTPUT") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("filled.pdf"), reopened.bytes.as_ref()).unwrap();
        std::fs::write(dir.join("converted.pdf"), converted_pdf.bytes.as_ref()).unwrap();
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires Poppler GLib, Cairo and Pango"]
fn encrypted_pdf_password_stays_session_only_and_saved_pdf_remains_encrypted() {
    let bytes = include_bytes!("../../../tests/fixtures/journal-encrypted.pdf");
    validate(bytes).unwrap();
    let mut pdf = PdfDocument::new(bytes.to_vec());
    assert!(render(&pdf, 0, 1.).is_err());
    pdf.password = "wrong".into();
    assert!(render(&pdf, 0, 1.).is_err());
    pdf.password = "secret-test".into();
    let page = render(&pdf, 0, 1.).unwrap();
    assert!(page.text.contains("Journal PDF fixture"));
    pdf.edits.added.push(Annotation {
        id: id(),
        page: 0,
        rect: [20., 20., 60., 60.],
        text: "Private note".into(),
        note: true,
        color: None,
    });
    let mut reopened = PdfDocument::new(save(&pdf).unwrap());
    assert!(render(&reopened, 0, 1.).is_err());
    reopened.password = "secret-test".into();
    assert!(
        render(&reopened, 0, 1.)
            .unwrap()
            .annotations
            .iter()
            .any(|a| a.text == "Private note")
    );
}

#[test]
#[ignore = "requires Poppler GLib, Cairo and Pango"]
fn native_pdf_radio_multi_select_and_editable_choices_round_trip() {
    let mut pdf = PdfDocument::new(FIXTURE.to_vec());
    let page = render(&pdf, 1, 1.).unwrap();
    let radio: Vec<_> = page
        .fields
        .iter()
        .filter(|f| matches!(f.kind, FieldKind::Button { radio: true }))
        .collect();
    assert_eq!(radio.len(), 2);
    assert_eq!(radio[0].group, radio[1].group);
    let was_on = radio
        .iter()
        .find(|r| r.value == Value::Button(true))
        .unwrap();
    let was_off = radio
        .iter()
        .find(|r| r.value == Value::Button(false))
        .unwrap();
    pdf.edits.fields.insert(was_on.id, Value::Button(false));
    pdf.edits.fields.insert(was_off.id, Value::Button(true));
    let multiple = page
        .fields
        .iter()
        .find(|f| f.name == "multi-choice")
        .unwrap();
    pdf.edits.fields.insert(
        multiple.id,
        Value::Choice {
            selected: vec![1, 2],
            text: None,
        },
    );
    let editable = page
        .fields
        .iter()
        .find(|f| f.name == "editable-choice")
        .unwrap();
    pdf.edits.fields.insert(
        editable.id,
        Value::Choice {
            selected: vec![1],
            text: None,
        },
    );
    let mut reopened = PdfDocument::new(save(&pdf).unwrap());
    let page = render(&reopened, 1, 1.).unwrap();
    assert_eq!(
        page.fields
            .iter()
            .find(|f| f.id == was_off.id)
            .unwrap()
            .value,
        Value::Button(true)
    );
    assert_eq!(
        page.fields
            .iter()
            .find(|f| f.id == was_on.id)
            .unwrap()
            .value,
        Value::Button(false)
    );
    assert!(
        matches!(&page.fields.iter().find(|f|f.id==multiple.id).unwrap().value,Value::Choice{selected,..} if selected==&vec![1,2])
    );
    assert!(
        matches!(&page.fields.iter().find(|f|f.id==editable.id).unwrap().value,Value::Choice{selected,..} if selected==&vec![1])
    );
    reopened.edits.fields.insert(
        editable.id,
        Value::Choice {
            selected: vec![],
            text: Some("Otro Привет".into()),
        },
    );
    let reopened = PdfDocument::new(save(&reopened).unwrap());
    let page = render(&reopened, 1, 1.).unwrap();
    assert!(
        matches!(&page.fields.iter().find(|f|f.id==editable.id).unwrap().value,Value::Choice{text:Some(text),..} if text=="Otro Привет")
    );
}

#[test]
#[ignore = "requires Poppler GLib, Cairo and Pango"]
fn native_note_colors_change_pixels_and_survive_pdf_save() {
    let mut document = PdfDocument::new(FIXTURE.to_vec());
    let before = render(&document, 0, 1.).unwrap();
    let note = before.annotations.iter().find(|a| a.note).unwrap();
    let key = (0, note.index);
    let blue = [30, 110, 245];
    document.edits.annotation_colors.insert(key, blue);
    document.edits.added.push(Annotation {
        id: id(),
        page: 0,
        rect: [30., 340., 55., 365.],
        text: "Colored new note".into(),
        note: true,
        color: Some([240, 70, 90]),
    });
    let updated = render(&document, 0, 1.).unwrap();
    assert_ne!(before.rgba, updated.rgba);
    assert_eq!(
        updated
            .annotations
            .iter()
            .find(|a| a.index == key.1)
            .unwrap()
            .color,
        Some(blue)
    );
    let reopened = PdfDocument::new(save(&document).unwrap());
    let page = render(&reopened, 0, 1.).unwrap();
    assert_eq!(
        page.annotations
            .iter()
            .find(|a| a.text == note.text)
            .unwrap()
            .color,
        Some(blue)
    );
    assert_eq!(
        page.annotations
            .iter()
            .find(|a| a.text == "Colored new note")
            .unwrap()
            .color,
        Some([240, 70, 90])
    );
    let mut red = document.clone();
    red.edits.annotation_colors.insert(key, [245, 30, 30]);
    assert_ne!(
        updated.rgba,
        render(&red, 0, 1.).unwrap().rgba,
        "color changes must be visible on the page"
    );
    let mut green = document.clone();
    green.edits.added[0].color = Some([30, 245, 30]);
    assert_ne!(
        updated.rgba,
        render(&green, 0, 1.).unwrap().rgba,
        "new note colors must change the page pixels too"
    );
    // A removed colored annotation stays removed rather than being recreated by its color edit.
    document.edits.annotations.insert(key, None);
    let removed = PdfDocument::new(save(&document).unwrap());
    assert!(
        !render(&removed, 0, 1.)
            .unwrap()
            .annotations
            .iter()
            .any(|a| a.text == note.text)
    );
}
