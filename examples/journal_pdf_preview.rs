//! Synthetic PDF reader / forms scenes. Always requires an isolated library.
use amigo::{Amigo, Section};
use amigo_core::{
    journal::{Document, Journal},
    library::{self, Library},
};
use eframe::{App, egui};
use std::path::PathBuf;
struct Preview {
    app: Amigo,
    output: PathBuf,
    scene: usize,
    frames: usize,
    pending: bool,
    external: Document,
    filename: String,
}
impl App for Preview {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if self.scene == 5 {
            if self.frames == 4 {
                input.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            if (10..=12).contains(&self.frames)
                && let Some(row) =
                    ctx.read_response(egui::Id::new(("journal-entry", &self.filename)))
            {
                let pos = row.rect.center();
                input
                    .events
                    .push(egui::Event::PointerMoved(if self.frames == 12 {
                        pos + egui::vec2(12., 0.)
                    } else {
                        pos
                    }));
                if self.frames == 11 {
                    input.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
            if self.frames == 14
                && let Some(row) = ctx.read_response(egui::Id::new(("journal-entry", "Documents")))
            {
                input
                    .events
                    .push(egui::Event::PointerMoved(row.rect.center()));
            }
        }
        if self.scene == 6 && self.frames == 4 {
            if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            // Keep screenshot timing deterministic rather than waiting for a wall-clock tooltip delay.
            ctx.style_mut_of(ctx.theme(), |style| {
                style.interaction.tooltip_delay = 0.;
                style.interaction.show_tooltips_only_when_still = false;
            });
            input.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        if ((self.scene == 6 && self.frames == 10)
            || (self.scene == 7 && [8, 10].contains(&self.frames)))
            && let Some(note) =
                ctx.read_response(egui::Id::new(("pdf-added-annotation", "preview-note")))
        {
            let pos = note.rect.center();
            input.events.push(egui::Event::PointerMoved(pos));
            if self.scene == 7 {
                for pressed in [true, false] {
                    input.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
        }
        if ((self.scene == 7 && self.frames == 16) || (self.scene == 8 && self.frames == 4))
            && let Some(button) = ctx.read_response(if self.scene == 7 {
                egui::Id::new(("pdf-note-color", "Green"))
            } else {
                egui::Id::new("pdf-annotation-save")
            })
        {
            let pos = button.rect.center();
            input.events.push(egui::Event::PointerMoved(pos));
            for pressed in [true, false] {
                input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
        if self.scene == 0
            && self.frames == 12
            && std::env::args().any(|a| a == "--long-names")
            && let Some(response) =
                ctx.read_response(egui::Id::new(("journal-entry", &self.filename)))
        {
            input
                .events
                .push(egui::Event::PointerMoved(response.rect.center()));
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                delta: egui::vec2(-1500., 0.),
                modifiers: egui::Modifiers::NONE,
            });
        }
        if self.scene == 2
            && self.frames == 12
            && std::env::args().any(|a| a == "--long-names")
            && let Some(response) = ctx.read_response(egui::Id::new("journal-note-title"))
        {
            input
                .events
                .push(egui::Event::PointerMoved(response.rect.center()));
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                delta: egui::vec2(-1500., 0.),
                modifiers: egui::Modifiers::NONE,
            });
        }
        if self.scene == 4
            && self.frames == 4
            && let Some(response) = ctx.read_response(egui::Id::new(("journal-pdf-page", 1usize)))
        {
            input
                .events
                .push(egui::Event::PointerMoved(response.rect.center()));
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                delta: egui::vec2(0., 10_000.),
                modifiers: egui::Modifiers::NONE,
            });
        }
        if self.scene == 4
            && self.frames == 14
            && let Some(response) =
                ctx.read_response(egui::Id::new(("pdf-added-annotation", "preview-text")))
        {
            let pos = response.rect.center();
            input.events.push(egui::Event::PointerMoved(pos));
            for pressed in [true, false] {
                input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
        if self.scene == 3
            && self.frames == 12
            && let Some(response) = ctx.read_response(egui::Id::new(("journal-pdf-page", 0usize)))
        {
            input
                .events
                .push(egui::Event::PointerMoved(response.rect.center()));
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                delta: egui::vec2(0., -700.),
                modifiers: egui::Modifiers::NONE,
            });
        }
        if self.scene == 1
            && self.frames == 4
            && let Some(response) = ctx.read_response(egui::Id::new(("pdf-tool", "Fill Forms")))
        {
            let pos = response.rect.center();
            input.events.push(egui::Event::PointerMoved(pos));
            for pressed in [true, false] {
                input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.scene >= 9 {
            self.app.journal.close_note();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if let Some(image) = ui.input(|i| {
            i.events.iter().find_map(|event| {
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        }) {
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            let path = self.output.join(match self.scene {
                0 => "journal-pdf-reader.png",
                1 => "journal-pdf-forms.png",
                2 => "journal-pdf-external.png",
                3 => "journal-pdf-scrolled.png",
                4 => "journal-pdf-remove-menu.png",
                5 => "journal-tree-drag.png",
                6 => "journal-pdf-note-hover.png",
                7 => "journal-pdf-note-open.png",
                _ => "journal-pdf-note-color.png",
            });
            image::save_buffer(
                &path,
                &bytes,
                image.width() as u32,
                image.height() as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            println!("Captured {}", path.display());
            self.scene += 1;
            self.frames = 0;
            self.pending = false;
            if self.scene == 2 {
                self.app.journal.open(self.external.clone());
            }
            if self.scene == 9 {
                // Discard only this isolated preview's temporary PDF draft on exit.
                self.app.journal.close_note();
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        self.app.ui(ui, frame);
        self.frames += 1;
        if self.frames > 50
            && !self.pending
            && ui
                .ctx()
                .read_response(egui::Id::new("journal-pdf-page"))
                .is_some()
        {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            self.pending = true;
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(30));
    }
}
fn main() -> eframe::Result {
    assert!(
        std::env::var_os("AMIGO_DATA_FILE").is_some(),
        "Set an isolated AMIGO_DATA_FILE"
    );
    let output = PathBuf::from(std::env::args().nth(1).expect("Pass screenshot directory"));
    std::fs::create_dir_all(&output).unwrap();
    let library = Library::open(library::default_root(), None).unwrap();
    let journal = Journal::open(&library.root).unwrap();
    journal
        .create(
            "Welcome",
            "# PDF documents\n\nImport a PDF or convert this note to PDF.\n",
        )
        .unwrap();
    journal.create_folder("Documents").unwrap();
    let filename=if std::env::args().any(|a|a=="--long-names"){"Application with a very long document name containing español and Привет and the final words at the right edge.pdf"}else{"Application.pdf"}.to_owned();
    std::fs::write(
        library.root.join("journal").join(&filename),
        include_bytes!("../crates/core/tests/fixtures/journal-forms.pdf"),
    )
    .unwrap();
    let mut document = journal.read(&filename).unwrap();
    let external_path = output.join(if std::env::args().any(|a| a == "--long-names") {
        filename.as_str()
    } else {
        "External.pdf"
    });
    std::fs::write(
        &external_path,
        include_bytes!("../crates/core/tests/fixtures/journal-forms.pdf"),
    )
    .unwrap();
    let mut external = Journal::read_external(&external_path).unwrap();
    for doc in [&mut document, &mut external] {
        doc.pdf
            .as_mut()
            .unwrap()
            .edits
            .added
            .push(amigo_core::journal::pdf::Annotation {
                id: "preview-text".into(),
                page: 0,
                rect: [30., 340., 240., 385.],
                text: "Borderless added text".into(),
                note: false,
                color: None,
            });
    }
    for doc in [&mut document, &mut external] {
        doc.pdf
            .as_mut()
            .unwrap()
            .edits
            .added
            .push(amigo_core::journal::pdf::Annotation {
                id: "preview-note".into(),
                page: 0,
                rect: [250., 340., 280., 365.],
                text: "A PDF note: hover to read, double-click to open. Español / Привет.".into(),
                note: true,
                color: Some([80, 150, 245]),
            });
    }
    let entries = journal.entries().unwrap();
    let size = if std::env::args().any(|a| a == "--compact") {
        [980., 640.]
    } else {
        [1440., 900.]
    };
    eframe::run_native(
        "Amigo Journal PDF preview",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size(size),
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut app = Amigo::new(cc);
            app.workspace.dark_mode = std::env::args().any(|a| a == "--dark");
            app.section = Section::Journal;
            app.journal.open(document);
            app.journal.update_entries(entries);
            app.journal.loaded = true;
            Ok(Box::new(Preview {
                app,
                output,
                scene: 0,
                frames: 0,
                pending: false,
                external,
                filename,
            }))
        }),
    )
}
