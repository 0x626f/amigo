use super::*;
use crate::{Action, JournalTab};
use amigo_core::journal::{
    Document, Preferences,
    pdf::{Field, FieldKind, Value},
};
struct Harness {
    action: Option<Action>,
    tab: JournalTab,
    prefs: Preferences,
    _pdf_sender: mpsc::Sender<Result<Loaded, String>>,
}
fn frame(
    app: &mut Harness,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    size: egui::Vec2,
) -> egui::FullOutput {
    let pdf = &mut app.tab.editor.as_mut().unwrap().pdf;
    if let Some(job) = &mut pdf.job {
        job.revision = pdf.revision;
        job.scale = pdf.zoom;
    }
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                app.action = app.tab.show(ui, &mut app.prefs);
            });
        },
    );
    output.textures_delta.clear();
    output
}
fn label_optional(output: &egui::FullOutput, text: &str) -> Option<egui::Rect> {
    fn find(shape: &egui::Shape, text: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == text => {
                Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
            }
            egui::Shape::Vec(items) => items.iter().find_map(|s| find(s, text)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .rev()
        .find_map(|s| find(&s.shape, text).filter(|rect| s.clip_rect.contains_rect(*rect)))
}
fn label(output: &egui::FullOutput, text: &str) -> egui::Rect {
    label_optional(output, text).unwrap_or_else(|| panic!("Missing {text}"))
}
fn click(app: &mut Harness, ctx: &egui::Context, pos: egui::Pos2, size: egui::Vec2) {
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            size,
        );
    }
}
fn mock() -> (Harness, egui::Context) {
    let ctx = egui::Context::default();
    design::install(&ctx);
    let mut tab = JournalTab::default();
    tab.open(Document {
        external: None,
        path: "Form.pdf".into(),
        text: String::new(),
        fingerprint: "test".into(),
        pdf: Some(PdfDocument::new(b"%PDF-1.7".to_vec())),
    });
    let texture = ctx.load_texture(
        "test-pdf",
        egui::ColorImage::filled([2, 2], egui::Color32::WHITE),
        Default::default(),
    );
    tab.editor.as_mut().unwrap().pdf.rendered = Some(Arc::new(Rendered {
        texture,
        page: Page {
            number: 0,
            count: 2,
            size: [300., 400.],
            pixels: [2, 2],
            rgba: vec![],
            text: "PDF page text".into(),
            can_annotate: true,
            can_fill: true,
            can_copy: true,
            can_add_text: true,
            fields: vec![
                Field {
                    id: 10,
                    name: "Your name".into(),
                    group: "name".into(),
                    rect: [30., 70., 240., 95.],
                    kind: FieldKind::Text {
                        multiline: false,
                        password: false,
                        max_len: 40,
                    },
                    value: Value::Text("Original".into()),
                    read_only: false,
                },
                Field {
                    id: 11,
                    name: "Accept terms".into(),
                    group: "accept".into(),
                    rect: [30., 120., 50., 140.],
                    kind: FieldKind::Button { radio: false },
                    value: Value::Button(false),
                    read_only: false,
                },
            ],
            annotations: vec![],
        },
    }));
    tab.editor.as_mut().unwrap().pdf.zoom = 1.;
    let (pdf_sender, receiver) = mpsc::channel();
    tab.editor.as_mut().unwrap().pdf.job = Some(Job {
        receiver,
        revision: 0,
        scale: 1.,
        cancel: Arc::new(AtomicBool::new(false)),
        searching: false,
        worker: None,
    });
    (
        Harness {
            tab,
            prefs: Preferences::default(),
            action: None,
            _pdf_sender: pdf_sender,
        },
        ctx,
    )
}
#[test]
fn pdf_forms_preserve_the_binary_draft_and_undo_each_action() {
    let (mut app, ctx) = mock();
    let size = egui::vec2(980., 640.);
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![], size);
    }
    let output = frame(&mut app, &ctx, vec![], size);
    let pos = label(&output, "Fill Forms").center();
    click(&mut app, &ctx, pos, size);
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![], size);
    }
    let output = frame(&mut app, &ctx, vec![], size);
    let pos = label(&output, "Checked").center();
    click(&mut app, &ctx, pos, size);
    let editor = app.tab.editor.as_ref().unwrap();
    assert!(editor.dirty());
    assert_eq!(
        editor.document.pdf.as_ref().unwrap().edits.fields[&11],
        Value::Button(true)
    );
    assert!(editor.document.text.is_empty());
    assert_eq!(
        editor.document.pdf.as_ref().unwrap().bytes.as_ref(),
        b"%PDF-1.7"
    );
    // Block debounce from starting a native worker; this is a deterministic UI test.
    app.tab.editor.as_mut().unwrap().pdf.changed = Some(Instant::now());
    let output = frame(&mut app, &ctx, vec![], size);
    let pos = label(&output, "Undo").center();
    click(&mut app, &ctx, pos, size);
    assert!(!app.tab.dirty());
    assert_eq!(app.tab.editor.as_ref().unwrap().pdf.redo.len(), 1);
    app.tab.editor.as_mut().unwrap().pdf.changed = Some(Instant::now());
    let output = frame(&mut app, &ctx, vec![], size);
    let pos = label(&output, "Redo").center();
    click(&mut app, &ctx, pos, size);
    assert!(app.tab.dirty());
    assert_eq!(app.tab.editor.as_ref().unwrap().pdf.undo.len(), 1);
}
#[test]
fn pdf_header_hides_markdown_modes_and_find_is_available() {
    let (mut app, ctx) = mock();
    let size = egui::vec2(980., 640.);
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![], size);
    }
    assert!(
        ctx.read_response(egui::Id::new((
            "journal-mode",
            amigo_core::journal::Mode::Source
        )))
        .is_none()
    );
    let input = egui::Event::Key {
        key: egui::Key::F,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::ALT,
    };
    frame(&mut app, &ctx, vec![input], size);
    let output = frame(&mut app, &ctx, vec![], size);
    assert!(label_optional(&output, "Find in PDF").is_some());
    assert!(ctx.memory(|m| m.has_focus(egui::Id::new("journal-pdf-find"))));
    assert!(!app.tab.dirty());
}

#[test]
fn clicking_pdf_text_field_focuses_the_input_and_alt_a_replaces_its_value() {
    let (mut app, ctx) = mock();
    let size = egui::vec2(980., 640.);
    app.tab.editor.as_mut().unwrap().pdf.tool = Tool::Forms;
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![], size);
    }
    let pos = ctx
        .read_response(egui::Id::new(("pdf-field", 10)))
        .unwrap()
        .rect
        .center();
    click(&mut app, &ctx, pos, size);
    let input = egui::Id::new(("pdf-field-input", 10));
    assert!(ctx.memory(|m| m.has_focus(input)));
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        }],
        size,
    );
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("María".into())],
        size,
    );
    assert_eq!(
        app.tab
            .editor
            .as_ref()
            .unwrap()
            .document
            .pdf
            .as_ref()
            .unwrap()
            .edits
            .fields[&10],
        Value::Text("María".into())
    );
    assert!(app.tab.dirty());
}
#[test]
fn dragging_pdf_annotation_uses_the_press_origin_and_commits_only_on_add() {
    let (mut app, ctx) = mock();
    let size = egui::vec2(980., 640.);
    app.tab.editor.as_mut().unwrap().pdf.tool = Tool::Text;
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![], size);
    }
    let rect = ctx
        .read_response(egui::Id::new("journal-pdf-page"))
        .unwrap()
        .rect;
    let start = rect.min + egui::vec2(25., 35.);
    let end = rect.min + egui::vec2(205., 95.);
    frame(
        &mut app,
        &ctx,
        vec![
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        size,
    );
    frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)], size);
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        size,
    );
    assert!(!app.tab.dirty());
    let draft = app.tab.editor.as_ref().unwrap().pdf.draft.as_ref().unwrap();
    assert_eq!(draft.rect, [25., 35., 205., 95.]);
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![], size);
    }
    assert!(ctx.memory(|m| m.has_focus(egui::Id::new("pdf-annotation-text"))));
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::Text("Annotation text".into())],
        size,
    );
    let output = frame(&mut app, &ctx, vec![], size);
    click(&mut app, &ctx, label(&output, "Add").center(), size);
    assert!(app.tab.dirty());
    let pdf = app
        .tab
        .editor
        .as_ref()
        .unwrap()
        .document
        .pdf
        .as_ref()
        .unwrap();
    assert_eq!(pdf.edits.added.len(), 1);
    assert_eq!(pdf.edits.added[0].text, "Annotation text");
}

#[test]
fn pdf_header_has_no_export_and_external_drafts_have_a_save_icon() {
    let (mut app, ctx) = mock();
    let size = egui::vec2(980., 640.);
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![], size);
    }
    let output = frame(&mut app, &ctx, vec![], size);
    assert!(label_optional(&output, "Export PDF").is_none());
    assert!(
        ctx.read_response(egui::Id::new("journal-save-external"))
            .is_none()
    );
    app.tab.editor.as_mut().unwrap().document.external = Some("/tmp/Form.pdf".into());
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![], size);
    }
    let save = ctx
        .read_response(egui::Id::new("journal-save-external"))
        .unwrap()
        .rect
        .center();
    click(&mut app, &ctx, save, size);
    assert!(matches!(app.action, Some(Action::SaveToJournal)));
}
#[test]
fn scrolling_pdf_pages_updates_the_active_page_and_keeps_neighbor_pages_visible() {
    let (mut app, ctx) = mock();
    let size = egui::vec2(980., 640.);
    let editor = &mut app.tab.editor.as_mut().unwrap().pdf;
    editor.tool = Tool::Forms;
    let first = editor.rendered.clone().unwrap();
    editor.cache.insert(0, first.clone());
    editor.cache.insert(
        1,
        Arc::new(Rendered {
            texture: first.texture.clone(),
            page: Page {
                number: 1,
                count: 2,
                size: [300., 400.],
                pixels: [2, 2],
                rgba: vec![],
                text: "Second page".into(),
                fields: vec![],
                annotations: vec![],
                can_annotate: true,
                can_fill: true,
                can_copy: true,
                can_add_text: true,
            },
        }),
    );
    editor.sizes = vec![[300., 400.]; 2];
    for _ in 0..4 {
        frame(&mut app, &ctx, vec![], size);
    }
    let output = frame(&mut app, &ctx, vec![], size);
    assert!(label_optional(&output, "Fill Forms").is_some());
    assert!(label_optional(&output, "Page Forms").is_some());
    assert!(label_optional(&output, "Page Text").is_none());
    assert!(label_optional(&output, "Copy Page Text").is_none());
    let page = ctx
        .read_response(egui::Id::new(("journal-pdf-page", 0usize)))
        .unwrap()
        .rect;
    frame(
        &mut app,
        &ctx,
        vec![
            egui::Event::PointerMoved(page.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                delta: egui::vec2(0., -10_000.),
                modifiers: egui::Modifiers::NONE,
            },
        ],
        size,
    );
    for _ in 0..8 {
        frame(&mut app, &ctx, vec![], size);
    }
    assert_eq!(app.tab.editor.as_ref().unwrap().pdf.page, 1);
    let output = frame(&mut app, &ctx, vec![], size);
    assert!(label_optional(&output, "Fill Forms").is_none());
    assert!(label_optional(&output, "Page Forms").is_none());
    assert_eq!(
        app.tab
            .editor
            .as_ref()
            .unwrap()
            .pdf
            .rendered
            .as_ref()
            .unwrap()
            .page
            .number,
        1
    );
    assert!(
        ctx.read_response(egui::Id::new(("journal-pdf-page", 1usize)))
            .is_some()
    );
    let second = ctx
        .read_response(egui::Id::new(("journal-pdf-page", 1usize)))
        .unwrap()
        .rect;
    let viewport = ctx
        .read_response(egui::Id::new("journal-pdf-viewport"))
        .unwrap()
        .rect;
    assert!(
        (second.bottom() - (viewport.bottom() - 16.)).abs() < 2.,
        "second={second:?} viewport={viewport:?}"
    );
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            phase: egui::TouchPhase::Move,
            delta: egui::vec2(0., 10_000.),
            modifiers: egui::Modifiers::NONE,
        }],
        size,
    );
    for _ in 0..8 {
        frame(&mut app, &ctx, vec![], size);
    }
    assert_eq!(app.tab.editor.as_ref().unwrap().pdf.page, 0);
    let output = frame(&mut app, &ctx, vec![], size);
    assert!(label_optional(&output, "Fill Forms").is_some());
    assert!(label_optional(&output, "Page Forms").is_some());
    assert!(!app.tab.dirty());
}

#[test]
fn closing_pdf_cancels_and_joins_its_worker() {
    let cancel = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let worker_finished = finished.clone();
    let worker = std::thread::spawn(move || {
        while !worker_cancel.load(Ordering::Relaxed) {
            std::thread::park_timeout(Duration::from_millis(1));
        }
        worker_finished.store(true, Ordering::Relaxed);
    });
    let (_sender, receiver) = mpsc::channel();
    let mut editor = PdfEditor::default();
    editor.job = Some(Job {
        receiver,
        revision: 0,
        scale: 1.,
        cancel,
        searching: false,
        worker: Some(worker),
    });
    drop(editor);
    assert!(finished.load(Ordering::Relaxed));
}

fn context_remove(app: &mut Harness, ctx: &egui::Context, id: egui::Id, size: egui::Vec2) {
    for _ in 0..4 {
        frame(app, ctx, vec![], size);
    }
    let pos = ctx.read_response(id).unwrap().rect.center();
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            size,
        );
    }
    for _ in 0..3 {
        frame(app, ctx, vec![], size);
    }
    let remove = ctx
        .read_response(id.with("remove"))
        .expect("annotation Remove menu")
        .rect
        .center();
    click(app, ctx, remove, size);
}
#[test]
fn pdf_annotation_context_remove_and_undo_work_for_existing_and_added_items() {
    use amigo_core::journal::pdf::ExistingAnnotation;
    let (mut app, ctx) = mock();
    let size = egui::vec2(980., 640.);
    let editor = app.tab.editor.as_mut().unwrap();
    Arc::get_mut(editor.pdf.rendered.as_mut().unwrap())
        .unwrap()
        .page
        .annotations
        .push(ExistingAnnotation {
            note: true,
            color: None,
            index: 5,
            rect: [30., 160., 180., 200.],
            text: "Existing note".into(),
            read_only: false,
        });
    let added = Annotation {
        id: "added-text".into(),
        page: 0,
        rect: [30., 240., 180., 280.],
        text: "Added text".into(),
        note: false,
        color: None,
    };
    editor
        .document
        .pdf
        .as_mut()
        .unwrap()
        .edits
        .added
        .push(added);
    editor.saved_pdf_edits = Some(editor.document.pdf.as_ref().unwrap().edits.clone());
    context_remove(
        &mut app,
        &ctx,
        egui::Id::new(("pdf-existing-annotation", 0usize, 5usize)),
        size,
    );
    assert_eq!(
        app.tab
            .editor
            .as_ref()
            .unwrap()
            .document
            .pdf
            .as_ref()
            .unwrap()
            .edits
            .annotations
            .get(&(0, 5)),
        Some(&None)
    );
    assert_eq!(
        app.tab
            .editor
            .as_ref()
            .unwrap()
            .document
            .pdf
            .as_ref()
            .unwrap()
            .edits
            .added
            .len(),
        1
    );
    let output = frame(&mut app, &ctx, vec![], size);
    click(&mut app, &ctx, label(&output, "Undo").center(), size);
    assert!(!app.tab.dirty());
    context_remove(
        &mut app,
        &ctx,
        egui::Id::new(("pdf-added-annotation", "added-text")),
        size,
    );
    assert!(
        app.tab
            .editor
            .as_ref()
            .unwrap()
            .document
            .pdf
            .as_ref()
            .unwrap()
            .edits
            .added
            .is_empty()
    );
    let output = frame(&mut app, &ctx, vec![], size);
    click(&mut app, &ctx, label(&output, "Undo").center(), size);
    assert_eq!(
        app.tab
            .editor
            .as_ref()
            .unwrap()
            .document
            .pdf
            .as_ref()
            .unwrap()
            .edits
            .added[0]
            .text,
        "Added text"
    );
    assert!(!app.tab.dirty());
}
#[test]
fn pdf_read_only_annotation_cannot_be_removed_with_the_context_menu() {
    use amigo_core::journal::pdf::ExistingAnnotation;
    let (mut app, ctx) = mock();
    let size = egui::vec2(980., 640.);
    Arc::get_mut(
        app.tab
            .editor
            .as_mut()
            .unwrap()
            .pdf
            .rendered
            .as_mut()
            .unwrap(),
    )
    .unwrap()
    .page
    .annotations
    .push(ExistingAnnotation {
        note: true,
        color: None,
        index: 5,
        rect: [30., 160., 180., 200.],
        text: "Locked note".into(),
        read_only: true,
    });
    context_remove(
        &mut app,
        &ctx,
        egui::Id::new(("pdf-existing-annotation", 0usize, 5usize)),
        size,
    );
    assert!(!app.tab.dirty());
    assert!(
        app.tab
            .editor
            .as_ref()
            .unwrap()
            .document
            .pdf
            .as_ref()
            .unwrap()
            .edits
            .annotations
            .is_empty()
    );
}

#[test]
fn pdf_toolbar_hides_forms_for_unloaded_and_stale_pages_and_has_no_page_text() {
    let (mut app, ctx) = mock();
    let editor = app.tab.editor.as_mut().unwrap();
    for page in [1, 0] {
        editor.pdf.page = page;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            editor
                .pdf
                .toolbar(ui, editor.document.pdf.as_mut().unwrap());
        });
        output.textures_delta.clear();
        assert_eq!(label_optional(&output, "Fill Forms").is_some(), page == 0);
        assert!(label_optional(&output, "Page Text").is_none());
    }
    editor.pdf.rendered = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        editor
            .pdf
            .toolbar(ui, editor.document.pdf.as_mut().unwrap());
    });
    output.textures_delta.clear();
    assert!(label_optional(&output, "Fill Forms").is_none());
    assert!(label_optional(&output, "Page Text").is_none());
}

#[test]
fn pdf_notes_hover_open_edit_and_undo_without_duplicates() {
    use amigo_core::journal::pdf::ExistingAnnotation;
    for added in [false, true] {
        let (mut app, ctx) = mock();
        ctx.style_mut_of(ctx.theme(), |s| {
            s.interaction.tooltip_delay = 0.;
            s.interaction.show_tooltips_only_when_still = false;
        });
        let size = egui::vec2(980., 640.);
        let note_rect = [30., 160., 180., 200.];
        let editor = app.tab.editor.as_mut().unwrap();
        let id = if added {
            editor
                .document
                .pdf
                .as_mut()
                .unwrap()
                .edits
                .added
                .push(Annotation {
                    id: "note-to-edit".into(),
                    page: 0,
                    rect: note_rect,
                    text: "Latest note español Привет".into(),
                    note: true,
                    color: None,
                });
            egui::Id::new(("pdf-added-annotation", "note-to-edit"))
        } else {
            Arc::get_mut(editor.pdf.rendered.as_mut().unwrap())
                .unwrap()
                .page
                .annotations
                .push(ExistingAnnotation {
                    note: true,
                    color: None,
                    index: 5,
                    rect: note_rect,
                    text: "Original note".into(),
                    read_only: false,
                });
            editor
                .document
                .pdf
                .as_mut()
                .unwrap()
                .edits
                .annotations
                .insert((0, 5), Some("Latest note español Привет".into()));
            egui::Id::new(("pdf-existing-annotation", 0usize, 5usize))
        };
        editor.saved_pdf_edits = Some(editor.document.pdf.as_ref().unwrap().edits.clone());
        for _ in 0..4 {
            frame(&mut app, &ctx, vec![], size);
        }
        let pos = ctx.read_response(id).unwrap().rect.center();
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)], size);
        let output = frame(&mut app, &ctx, vec![], size);
        assert!(label_optional(&output, "Latest note español Привет").is_some());
        click(&mut app, &ctx, pos, size);
        assert!(
            app.tab.editor.as_ref().unwrap().pdf.draft.is_none(),
            "single click doesn't open"
        );
        click(&mut app, &ctx, pos, size);
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![], size);
        }
        assert_eq!(
            app.tab
                .editor
                .as_ref()
                .unwrap()
                .pdf
                .draft
                .as_ref()
                .unwrap()
                .text,
            "Latest note español Привет"
        );
        assert!(!app.tab.dirty(), "viewing isn't an edit");
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::ALT,
            }],
            size,
        );
        let output = frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("Revised note 🌍".into())],
            size,
        );
        click(&mut app, &ctx, label(&output, "Save").center(), size);
        let editor = app.tab.editor.as_ref().unwrap();
        let edits = &editor.document.pdf.as_ref().unwrap().edits;
        if added {
            assert_eq!(edits.added.len(), 1);
            assert_eq!(edits.added[0].text, "Revised note 🌍");
            assert_eq!(edits.added[0].rect, note_rect);
            assert!(edits.added[0].note);
        } else {
            assert!(edits.added.is_empty());
            assert_eq!(
                edits.annotations.get(&(0, 5)),
                Some(&Some("Revised note 🌍".into()))
            );
        }
        assert!(app.tab.dirty());
        assert!(editor.pdf.draft.is_none());
        let output = frame(&mut app, &ctx, vec![], size);
        click(&mut app, &ctx, label(&output, "Undo").center(), size);
        assert!(!app.tab.dirty());
    }
}

#[test]
fn double_click_opens_readonly_pdf_notes_and_cancel_discards_an_edit() {
    use amigo_core::journal::pdf::ExistingAnnotation;
    for read_only in [false, true] {
        let (mut app, ctx) = mock();
        let size = egui::vec2(980., 640.);
        Arc::get_mut(
            app.tab
                .editor
                .as_mut()
                .unwrap()
                .pdf
                .rendered
                .as_mut()
                .unwrap(),
        )
        .unwrap()
        .page
        .annotations
        .push(ExistingAnnotation {
            note: true,
            color: None,
            index: 5,
            rect: [30., 160., 180., 200.],
            text: "Original note".into(),
            read_only,
        });
        for _ in 0..4 {
            frame(&mut app, &ctx, vec![], size);
        }
        let pos = ctx
            .read_response(egui::Id::new(("pdf-existing-annotation", 0usize, 5usize)))
            .unwrap()
            .rect
            .center();
        click(&mut app, &ctx, pos, size);
        click(&mut app, &ctx, pos, size);
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![], size);
        }
        let output = frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("Unsaved change".into())],
            size,
        );
        assert_eq!(
            app.tab.editor.as_ref().unwrap().pdf.annotation_read_only,
            read_only
        );
        if read_only {
            assert!(label_optional(&output, "Save").is_none());
            assert_eq!(
                app.tab
                    .editor
                    .as_ref()
                    .unwrap()
                    .pdf
                    .draft
                    .as_ref()
                    .unwrap()
                    .text,
                "Original note"
            );
        }
        let close = label(&output, if read_only { "Close" } else { "Cancel" }).center();
        click(&mut app, &ctx, close, size);
        let editor = app.tab.editor.as_ref().unwrap();
        assert!(editor.pdf.draft.is_none());
        assert!(editor.pdf.annotation_target.is_none());
        assert!(
            editor
                .document
                .pdf
                .as_ref()
                .unwrap()
                .edits
                .annotations
                .is_empty()
        );
        assert!(!app.tab.dirty());
    }
}

#[test]
fn pdf_note_color_changes_save_cancel_undo_and_respect_readonly_permissions() {
    use amigo_core::journal::pdf::ExistingAnnotation;
    for scenario in ["existing", "added", "new", "readonly", "denied"] {
        for cancel in [false, true] {
            let (mut app, ctx) = mock();
            let size = egui::vec2(980., 640.);
            let editor = app.tab.editor.as_mut().unwrap();
            let note = Annotation {
                id: "colored-note".into(),
                page: 0,
                rect: [30., 160., 180., 200.],
                text: "Keep my text".into(),
                note: true,
                color: None,
            };
            let readonly = matches!(scenario, "readonly" | "denied");
            let id = if scenario == "new" {
                editor.pdf.draft = Some(note);
                editor.pdf.annotation_focus = true;
                None
            } else if matches!(scenario, "added" | "denied") {
                if scenario == "denied" {
                    Arc::get_mut(editor.pdf.rendered.as_mut().unwrap())
                        .unwrap()
                        .page
                        .can_annotate = false;
                }
                editor.document.pdf.as_mut().unwrap().edits.added.push(note);
                Some(egui::Id::new(("pdf-added-annotation", "colored-note")))
            } else {
                Arc::get_mut(editor.pdf.rendered.as_mut().unwrap())
                    .unwrap()
                    .page
                    .annotations
                    .push(ExistingAnnotation {
                        index: 5,
                        rect: note.rect,
                        text: note.text,
                        read_only: readonly,
                        note: true,
                        color: None,
                    });
                Some(egui::Id::new(("pdf-existing-annotation", 0usize, 5usize)))
            };
            let before = editor.document.pdf.as_ref().unwrap().edits.clone();
            editor.saved_pdf_edits = Some(before.clone());
            for _ in 0..4 {
                frame(&mut app, &ctx, vec![], size);
            }
            if let Some(id) = id {
                let pos = ctx.read_response(id).unwrap().rect.center();
                click(&mut app, &ctx, pos, size);
                click(&mut app, &ctx, pos, size);
            }
            for _ in 0..3 {
                frame(&mut app, &ctx, vec![], size);
            }
            let blue = ctx
                .read_response(egui::Id::new(("pdf-note-color", "Blue")))
                .unwrap()
                .rect
                .center();
            click(&mut app, &ctx, blue, size);
            assert_eq!(
                app.tab
                    .editor
                    .as_ref()
                    .unwrap()
                    .pdf
                    .draft
                    .as_ref()
                    .unwrap()
                    .color,
                if readonly { None } else { Some([80, 150, 245]) }
            );
            assert!(!app.tab.dirty(), "color remains provisional until Save/Add");
            let output = frame(&mut app, &ctx, vec![], size);
            let button = if readonly {
                "Close"
            } else if cancel {
                "Cancel"
            } else if scenario == "new" {
                "Add"
            } else {
                "Save"
            };
            click(&mut app, &ctx, label(&output, button).center(), size);
            if readonly || cancel {
                assert_eq!(
                    app.tab
                        .editor
                        .as_ref()
                        .unwrap()
                        .document
                        .pdf
                        .as_ref()
                        .unwrap()
                        .edits,
                    before
                );
                assert!(!app.tab.dirty());
            } else {
                let edits = &app
                    .tab
                    .editor
                    .as_ref()
                    .unwrap()
                    .document
                    .pdf
                    .as_ref()
                    .unwrap()
                    .edits;
                if scenario == "existing" {
                    assert_eq!(edits.annotation_colors.get(&(0, 5)), Some(&[80, 150, 245]));
                    assert!(
                        edits.annotations.is_empty(),
                        "color-only changes keep the contents untouched"
                    );
                } else {
                    assert_eq!(edits.added.len(), 1);
                    assert_eq!(edits.added[0].color, Some([80, 150, 245]));
                    assert_eq!(edits.added[0].text, "Keep my text");
                }
                let output = frame(&mut app, &ctx, vec![], size);
                click(&mut app, &ctx, label(&output, "Undo").center(), size);
                assert_eq!(
                    app.tab
                        .editor
                        .as_ref()
                        .unwrap()
                        .document
                        .pdf
                        .as_ref()
                        .unwrap()
                        .edits,
                    before
                );
                assert!(!app.tab.dirty());
            }
        }
    }
}
