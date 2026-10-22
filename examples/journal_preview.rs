//! Capture Journal and shared settings using an isolated synthetic library.
use amigo::{Amigo, Section, SettingsPage};
use amigo_core::{
    journal::{Journal, Mode},
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
}
const SCENES: [&str; 8] = [
    "journal-split",
    "journal-preview",
    "journal-source",
    "app-settings",
    "update-settings",
    "journal-settings",
    "board-settings",
    "vault-settings",
];
impl App for Preview {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
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
            let page = match self.step {
                3 => Some(SettingsPage::General),
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
            }))
        }),
    )
}
