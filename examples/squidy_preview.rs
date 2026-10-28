//! Native Squidy and inline-media review using a fresh, synthetic library only.
use amigo::{Amigo, Section};
use amigo_core::{
    attachments,
    journal::{Document, Journal, Mode},
    library::{self, Library},
    media,
    model::{Task, Workspace},
    squidy::{Card, Link, Store},
};
use eframe::{App, egui};
use std::{path::PathBuf, time::Duration};

const SCENES: [&str; 8] = [
    "squidy-board",
    "squidy-card",
    "journal-media",
    "task-media",
    "squidy-path",
    "squidy-connection-bidirectional",
    "squidy-connection-directed",
    "squidy-path-result",
];
struct Preview {
    app: Amigo,
    output: PathBuf,
    card: String,
    note: Document,
    task: Task,
    step: usize,
    frames: u32,
    pending: bool,
    shortcut_received: bool,
    shortcut_announced: bool,
    requested_at: std::time::Instant,
    report_at: std::time::Instant,
}
impl App for Preview {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if matches!(self.step, 5..=7) {
            if self.frames == 1 {
                for pressed in [true, false] {
                    input.events.push(egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
            let response = if self.frames == 5 && self.step != 7 {
                self.app.squidy.board.as_ref().and_then(|board| {
                    ctx.read_response(egui::Id::new((
                        "squidy-edge",
                        &board.links[self.step - 5].id,
                    )))
                })
            } else if self.step == 6 && self.frames == 9 {
                ctx.read_response(egui::Id::new("squidy-link-swap"))
            } else if self.step == 7 && self.frames == 17 {
                ctx.read_response(egui::Id::new("squidy-find-path-confirm"))
            } else {
                None
            };
            if let Some(response) = response {
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
        if self.step == 0
            && self.frames == 5
            && std::env::args().any(|a| a == "--context-menu")
            && let Some(response) = ctx.read_response(egui::Id::new(("squidy-card", &self.card)))
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
        if std::env::args().any(|a| a == "--native-shortcut") {
            for event in &input.events {
                if matches!(
                    event,
                    egui::Event::Key { .. } | egui::Event::ModifiersChanged(_)
                ) {
                    println!("Native keyboard: {event:?}");
                }
            }
        }
        if matches!(self.step, 4 | 7) && self.frames == 5 {
            if self.step == 4 && std::env::args().any(|a| a == "--native-shortcut") {
                if !self.shortcut_announced {
                    println!("Ready for native Alt+F");
                    self.shortcut_announced = true;
                }
                self.shortcut_received |= input.events.iter().any(|event| {
                    matches!(
                        event, egui::Event::Key { key: egui::Key::F, pressed: true, modifiers, .. }
                            if modifiers.alt
                    )
                });
            } else {
                input
                    .events
                    .push(egui::Event::ModifiersChanged(egui::Modifiers::ALT));
                for pressed in [true, false] {
                    input.events.push(egui::Event::Key {
                        key: egui::Key::F,
                        physical_key: Some(egui::Key::F),
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::ALT,
                    });
                }
                input
                    .events
                    .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
            }
        }
        if matches!(self.step, 4 | 7)
            && [9, 13].contains(&self.frames)
            && !(self.step == 4
                && self.frames == 13
                && std::env::args().any(|a| a == "--path-selection"))
        {
            let name = if self.frames == 9 {
                "Field observations"
            } else {
                "Next expedition"
            };
            if let Some(card) = self.app.squidy.board.as_ref().and_then(|b| b.card(name))
                && let Some(response) = ctx.read_response(egui::Id::new(("squidy-card", &card.id)))
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
        if self.step == 1
            && self.frames == 9
            && std::env::args().any(|a| a == "--card-palette")
            && let Some(response) =
                ctx.read_response(egui::Id::new(("squidy-card-color", &self.card)))
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
        if self.report_at.elapsed() > Duration::from_secs(2) {
            println!(
                "Scene {} frame {} pending {} loading {}",
                self.step,
                self.frames,
                self.pending,
                self.app.is_page_loading()
            );
            self.report_at = std::time::Instant::now();
        }
        if let Some(image) = ui.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        }) {
            let path = self.output.join(format!("{}.png", SCENES[self.step]));
            let rgba: Vec<_> = image
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_array())
                .collect();
            image::save_buffer(
                &path,
                &rgba,
                image.width() as u32,
                image.height() as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            println!("Captured {}", path.display());
            self.app.squidy.editor = None;
            self.app.tasks.reset();
            egui::Popup::close_all(ui.ctx());
            self.step += 1;
            if self.step == 1 && std::env::args().any(|a| a == "--board-tools") {
                self.step = 4;
            }
            self.frames = 0;
            self.pending = false;
            if self.step == SCENES.len() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        if self.frames == 0 {
            self.app.section = match self.step {
                2 => Section::Journal,
                3 => Section::Tasks,
                _ => Section::Squidy,
            };
            match self.step {
                1 => {
                    self.app.squidy.edit_card(&self.card);
                }
                2 => self.app.journal.open(self.note.clone()),
                3 => {
                    self.app.tasks.edit_task(self.task.clone());
                }
                _ => (),
            }
        }
        self.app.ui(ui, frame);
        if !self.app.is_page_loading()
            && self.app.squidy.board.is_some()
            && !(self.step == 4
                && self.frames == 5
                && !self.shortcut_received
                && std::env::args().any(|a| a == "--native-shortcut"))
        {
            self.frames += 1;
        }
        if self.frames >= 50
            && (!self.pending || self.requested_at.elapsed() > Duration::from_secs(2))
        {
            if matches!(self.step, 5 | 6) {
                assert!(
                    ui.ctx()
                        .read_response(egui::Id::new("squidy-link-swap"))
                        .is_some(),
                    "Clicking an edge must open the connection editor"
                );
            }
            if self.step == 7 {
                assert!(
                    ui.ctx()
                        .read_response(egui::Id::new("squidy-path-result"))
                        .is_some(),
                    "Find Path must show the styled route summary"
                );
            }
            if self.step == 4 && !std::env::args().any(|a| a == "--path-selection") {
                assert!(
                    ui.ctx()
                        .read_response(egui::Id::new(("squidy-path-name", "From Card Name")))
                        .is_some(),
                    "Alt+F and two card selections must open the path dialog"
                );
            }
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            self.pending = true;
            self.requested_at = std::time::Instant::now();
        }
        ui.ctx().request_repaint();
    }
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.app.on_exit(gl);
    }
}
fn main() -> eframe::Result {
    let override_file = PathBuf::from(
        std::env::var_os("AMIGO_DATA_FILE").expect("Set an isolated AMIGO_DATA_FILE"),
    );
    let root = library::default_root();
    assert_eq!(
        root,
        override_file.with_extension("library"),
        "Remove AMIGO_HOME and other library overrides"
    );
    assert!(
        !root.exists(),
        "Use a fresh, nonexistent library; this preview never modifies an existing library"
    );
    let output = PathBuf::from(std::env::args().nth(1).expect("Pass screenshot directory"));
    std::fs::create_dir_all(&output).unwrap();
    let mut library = Library::open(root.clone(), None).unwrap();
    let photo = output.join("Ocean study.png");
    image::RgbaImage::from_fn(640, 120, |x, y| {
        let wave = ((x as f32 / 35.).sin() * 12. + 45.) as u32;
        if y < wave {
            image::Rgba([172, 218, 232, 255])
        } else {
            image::Rgba([20, (130 - y / 2) as u8, (170 + x / 12) as u8, 255])
        }
    })
    .save(&photo)
    .unwrap();
    let video = output.join("Field recording.mp4");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x120:rate=24",
            "-t",
            "2",
            "-c:v",
            "mpeg4",
            "-q:v",
            "4",
        ])
        .arg(&video)
        .status()
        .expect("Install FFmpeg for the media preview");
    assert!(status.success());
    let sources = [photo, video];
    let store = Store::open(&root).unwrap();
    let mut board = store.create("Ocean research").unwrap();
    let folder = store.folder(&board.id).unwrap();
    let mut files = attachments::Batch::import(folder, &sources).unwrap();
    let names = [
        "Field observations",
        "Working hypothesis",
        "Lab analysis",
        "Open questions",
        "Supporting evidence",
        "Next expedition",
    ];
    let briefs = [
        "Photos and recordings from the coast.",
        "Changing temperatures affect local habitats.",
        "Compare samples across three sites.",
        "Which seasonal patterns need more data?",
        "Published findings and source notes.",
        "Turn unanswered questions into a field plan.",
    ];
    let colors = [
        [64, 144, 184],
        [123, 110, 193],
        [52, 150, 130],
        [209, 150, 65],
        [91, 144, 87],
        [197, 108, 126],
    ];
    let mut card_id = String::new();
    for (index, ((name, brief), color)) in names.into_iter().zip(briefs).zip(colors).enumerate() {
        let mut card = Card::new(name);
        card.brief = brief.into();
        card.color = color;
        card.position = Some([(index % 3) as f32 * 370., (index / 3) as f32 * 260.]);
        card.data =
            "## Research notes\n\nKeep evidence, interpretations, and next steps together.\n"
                .into();
        if index == 0 {
            card_id = card.id.clone();
            card.attachments = files.files.clone();
            for file in &card.attachments {
                card.data.push_str(&format!(
                    "\n{}\n",
                    media::embed(&file.name, &media::target(file, 2))
                ));
            }
        }
        board.put_card(card).unwrap();
    }
    for (from, to, title, color) in [
        (0, 1, "supports", [61, 147, 116]),
        (0, 1, "questions", [203, 145, 65]),
        (1, 2, "tests", [106, 126, 191]),
        (0, 3, "raises", [203, 145, 65]),
        (3, 4, "investigates", [112, 147, 117]),
        (4, 5, "informs", [61, 147, 116]),
        (2, 5, "guides", [106, 126, 191]),
    ] {
        let mut link = Link::new(names[from], names[to], title);
        link.color = color;
        link.directed = matches!(title, "questions" | "tests" | "guides");
        board.put_link(link).unwrap();
    }
    let compact = std::env::args().any(|a| a == "--compact");
    board.zoom = if compact { 0.60 } else { 0.94 };
    board.pan = [26., 70.];
    store.save(&board, None).unwrap();
    files.keep();
    library.settings.squidy_board = board.id;
    library.settings.journal.mode = Mode::Preview;
    library.settings.dark_mode = std::env::args().any(|a| a == "--dark");
    library.save_settings().unwrap();
    let journal = Journal::open(&root).unwrap();
    journal.create_folder("Research").unwrap();
    let mut files = attachments::Batch::import(journal.media_folder().unwrap(), &sources).unwrap();
    let mut text =
        "# Field journal\n\nPhotos and video stay beside your Markdown notes.\n".to_owned();
    for file in &files.files {
        text.push_str(&format!(
            "\n{}\n",
            media::embed(&file.name, &format!("../.media/{}", media::target(file, 0)))
        ));
    }
    let note = journal.create("Research/Field journal", &text).unwrap();
    files.keep();
    let task_folder = library.board_path(&library.settings.board).unwrap();
    let mut files = attachments::Batch::import(task_folder, &sources).unwrap();
    let mut workspace: Workspace = library.load_board(&library.settings.board).unwrap();
    let mut task = Task::new(1, &workspace.task_statuses[0].id);
    task.title = "Review field evidence".into();
    task.description =
        "## Expedition review\n\nCompare the photo and recording before planning the next visit.\n"
            .into();
    task.attachments = files.files.clone();
    for file in &task.attachments {
        task.description.push_str(&format!(
            "\n{}\n",
            media::embed(&file.name, &media::target(file, 0))
        ));
    }
    workspace.tasks.push(task.clone());
    library
        .save_board(&library.settings.board, &workspace)
        .unwrap();
    files.keep();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(if compact {
            [980., 640.]
        } else {
            [1440., 900.]
        }),
        ..Default::default()
    };
    eframe::run_native(
        "Amigo Squidy preview",
        options,
        Box::new(move |cc| {
            let mut app = Amigo::new(cc);
            app.workspace.dark_mode = std::env::args().any(|a| a == "--dark");
            app.section = Section::Squidy;
            Ok(Box::new(Preview {
                app,
                output,
                card: card_id,
                note,
                task,
                step: if std::env::args().any(|a| a == "--native-shortcut") {
                    4
                } else {
                    0
                },
                frames: 0,
                pending: false,
                shortcut_received: false,
                shortcut_announced: false,
                requested_at: std::time::Instant::now(),
                report_at: std::time::Instant::now(),
            }))
        }),
    )
}
