//! Capture Journal and shared settings using an isolated synthetic library.
use amigo::{Amigo, Section, SettingsPage};
use amigo_core::{
    journal::{Document, Journal, Mode},
    library::{self, Library},
};
use eframe::{App, egui};
use std::path::PathBuf;
struct Preview {
    app: Amigo,
    output: PathBuf,
    step: usize,
    frames: u32,
    pending: bool,
    board: String,
    vault: String,
    code_document: Document,
}
const SCENES: [&str; 12] = [
    "journal-split",
    "journal-preview",
    "journal-source",
    "app-settings",
    "update-settings",
    "journal-settings",
    "board-settings",
    "vault-settings",
    "journal-rename",
    "journal-selection",
    "journal-stacked",
    "journal-code",
];
impl App for Preview {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if self.step == 9
            && self.frames == 5
            && let Some(response) = ctx.read_response(egui::Id::new(("journal-mode", Mode::Split)))
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
        if self.step == 9
            && self.frames == 9
            && let Some(editor) = self.app.journal.editor.as_ref()
        {
            let id = egui::Id::new(("journal-editor", &editor.document.path));
            if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
                let byte = editor.document.text.find("your ideas").unwrap();
                let start = editor.document.text[..byte].chars().count();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(start),
                        egui::text::CCursor::new(start + 10),
                    )));
                state.store(ctx, id);
                ctx.memory_mut(|m| m.request_focus(id));
            }
        }
        if self.step == 10 && [5, 6, 10, 11].contains(&self.frames) {
            let id = if self.frames <= 6 {
                "journal-view-options"
            } else {
                "journal-layout-stacked"
            };
            if let Some(response) = ctx.read_response(egui::Id::new(id)) {
                let pos = response.rect.center();
                input.events.push(egui::Event::PointerMoved(pos));
                {
                    let pressed = [5, 10].contains(&self.frames);
                    input.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
        }
        if self.step == 11
            && self.frames == 5
            && let Some(response) =
                ctx.read_response(egui::Id::new(("journal-mode", Mode::Preview)))
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
        if self.step == 8
            && [5, 7].contains(&self.frames)
            && let Some(response) = ctx.read_response(egui::Id::new("journal-note-title"))
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
        if self.frames == 5 && self.step < 3 {
            let mode = [Mode::Split, Mode::Preview, Mode::Source][self.step];
            if let Some(response) = ctx.read_response(egui::Id::new(("journal-mode", mode))) {
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
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Some(image) = ui.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        }) {
            let path = self.output.join(format!("{}.png", SCENES[self.step]));
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            image::save_buffer(
                &path,
                &bytes,
                image.width() as u32,
                image.height() as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            println!("Captured {}", path.display());
            self.app.close_settings();
            self.step += 1;
            self.frames = 0;
            self.pending = false;
            if self.step == SCENES.len() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        if self.frames == 0 {
            self.app.section = Section::Journal;
            if self.step == 9
                && let Some(editor) = self.app.journal.editor.as_ref()
            {
                self.app.journal.open(editor.document.clone());
            }
            if self.step == 11 {
                self.app.journal.open(self.code_document.clone());
            }
            let page = match self.step {
                3 => Some(SettingsPage::Tasks),
                4 => Some(SettingsPage::Updates),
                5 => Some(SettingsPage::Journal),
                6 => Some(SettingsPage::Board(self.board.clone())),
                7 => Some(SettingsPage::Vault(self.vault.clone())),
                _ => None,
            };
            if let Some(page) = page {
                self.app.open_settings(page);
            }
        }
        self.app.ui(ui, frame);
        if !self.app.is_page_loading() {
            self.frames += 1;
        }
        if self.frames >= 30 && !self.pending {
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
    let text = "---\ntags: [planning, journal]\n---\n# A place to think\n\nKeep your ideas close, in plain **Markdown**.\n\n## Today's focus\n\n- [x] Bring settings into one place\n- [x] Keep notes as portable `.md` files\n- [ ] Plan the next small improvement\n\n> Start with a clear thought. Leave room for the next one.\n\n## Working notes\n\nUse **bold** for decisions, *italics* for emphasis, and `code` for details.\nSelect text and right-click to format it.\n\n| View | Best for |\n| --- | --- |\n| Markdown | Writing without distraction |\n| Preview | Reading the finished note |\n| Split | Seeing changes as you write |\n\n## Next steps\n\nOpen [[Projects/Ideas|project ideas]] or [the daily note](Daily.md).\n";
    journal.create_folder("Projects").unwrap();
    journal
        .create(
            "Projects/Ideas",
            "# Project ideas\n\n- A quiet place for everyday planning\n",
        )
        .unwrap();
    journal
        .create("Daily", "# Daily note\n\nA fresh page.\n")
        .unwrap();
    let document = journal.create("Welcome", text).unwrap();
    let code_document = journal.create("Code", "# Code and tables\n\nReadable code blocks keep their language and a copy action.\n\n```rust\npub fn greeting(name: &str) -> String {\n    let message = \"Hello from Amigo\";\n    format!(\"{message}, {name}!\")\n}\n```\n\n## Planning together\n\n| Feature | State | Notes |\n| --- | --- | --- |\n| **Editor** | Ready | Syntax, line numbers, selection toolbar |\n| **Preview** | Ready | Clear headings and real bold text |\n\n> Plain Markdown, kept in your own notebook.\n").unwrap();
    let entries = journal.entries().unwrap();
    let board = library.settings.board.clone();
    let vault = library.settings.vault.clone();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(
            if std::env::args().any(|a| a == "--compact") {
                [980., 640.]
            } else {
                [1440., 900.]
            },
        ),
        ..Default::default()
    };
    eframe::run_native(
        "Amigo Journal preview",
        options,
        Box::new(move |cc| {
            let mut app = Amigo::new(cc);
            app.workspace.dark_mode = std::env::args().any(|a| a == "--dark");
            app.section = Section::Journal;
            app.journal.open(document);
            app.journal.entries = entries;
            app.journal.loaded = true;
            Ok(Box::new(Preview {
                app,
                output,
                step: 0,
                frames: 0,
                pending: false,
                board,
                vault,
                code_document,
            }))
        }),
    )
}
