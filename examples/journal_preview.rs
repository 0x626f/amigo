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
    trash_document: Document,
    entries: Vec<amigo_core::journal::Entry>,
}
const SCENES: [&str; 28] = [
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
    "journal-search",
    "journal-open-external",
    "journal-trash",
    "journal-trash-delete",
    "journal-tree-menu",
    "calendar-settings",
    "finance-settings",
    "expense-settings",
    "savings-settings",
    "currencies-settings",
    "chains-settings",
    "clients-settings",
    "accounts-settings",
    "countries-settings",
    "journal-new-folder",
    "journal-folder-rename",
];
impl App for Preview {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if matches!(self.step, 26 | 27) && self.frames == 1 {
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
        if matches!(self.step, 26 | 27) && self.frames == 5 {
            let id = if self.step == 26 {
                egui::Id::new("journal-new-folder")
            } else {
                egui::Id::new(("journal-folder-rename", "Projects"))
            };
            if let Some(response) = ctx.read_response(id) {
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
        if matches!(self.step, 26 | 27) && self.frames == 9 {
            input.events.push(egui::Event::Text(
                if self.step == 26 {
                    "Research"
                } else {
                    "Project archive"
                }
                .into(),
            ));
        }
        if self.step == 1 && std::env::args().any(|a| a == "--edit-preview") {
            let pos = if self.frames == 9 {
                ctx.read_response(egui::Id::new("journal-preview-edit"))
                    .map(|r| r.rect.center())
            } else if self.frames == 12 {
                ctx.read_response(egui::Id::new("journal-preview-pane"))
                    .map(|r| {
                        let content = (r.rect.width() - 48.).clamp(60., 820.);
                        egui::pos2(
                            r.rect.left() + (r.rect.width() - content) / 2. + 60.,
                            r.rect.top() + 76.,
                        )
                    })
            } else {
                None
            };
            if let Some(pos) = pos {
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
            if self.frames == 14 {
                for pressed in [true, false] {
                    input.events.push(egui::Event::Key {
                        key: egui::Key::A,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::COMMAND,
                    });
                }
            }
        }
        if matches!(self.step, 14 | 16) && self.frames == 1 {
            input.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        if ((self.step == 15 && self.frames == 5) || (self.step == 16 && self.frames == 5))
            && let Some(response) = ctx.read_response(egui::Id::new((
                if self.step == 15 {
                    "journal-note-trash"
                } else {
                    "journal-entry-menu"
                },
                if self.step == 16 {
                    "Projects"
                } else {
                    &self.trash_document.path
                },
            )))
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
        if self.step == 12
            && self.frames == 5
            && let Some(response) = ctx.read_response(egui::Id::new("journal-search-open"))
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
        if self.step == 12 && self.frames == 9 {
            input.events.push(egui::Event::Text("plan".into()));
        }
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
                let byte = editor.document.text.find("Markdown").unwrap();
                let start = editor.document.text[..byte].chars().count();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(start),
                        egui::text::CCursor::new(start + 8),
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
            egui::Popup::close_all(ui.ctx());
            self.app.close_settings();
            self.step += 1;
            if self.step == 17 && std::env::args().any(|a| a == "--folder-scenes") {
                self.step = 26;
            }
            if self.step == 3 && std::env::args().any(|a| a == "--list-editing") {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            if std::env::args().any(|a| a == "--settings") && self.step == 8 {
                self.step = 17;
            }
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
            if self.step == 13 {
                self.app.journal.choose_external_destination(
                    self.output.join("External.md"),
                    String::new(),
                    String::new(),
                );
            }
            if matches!(self.step, 14 | 15) {
                self.app.journal.open(self.trash_document.clone());
                // Keep the Trash controls in view in the compact preview too.
                self.app
                    .journal
                    .entries
                    .retain(|entry| amigo_core::journal::is_in_trash(&entry.path));
            }
            if self.step == 16 {
                self.app.journal.entries = self.entries.clone();
                self.app.journal.open(self.code_document.clone());
            }
            let page = match self.step {
                3 => Some(SettingsPage::Tasks),
                4 => Some(SettingsPage::Updates),
                5 => Some(SettingsPage::Journal),
                6 => Some(SettingsPage::Board(self.board.clone())),
                7 => Some(SettingsPage::Vault(self.vault.clone())),
                17 => Some(SettingsPage::Calendar),
                18 => Some(SettingsPage::Finance),
                19 => Some(SettingsPage::Expenses(self.vault.clone())),
                20 => Some(SettingsPage::Savings(self.vault.clone())),
                21 => Some(SettingsPage::FinanceReference(
                    amigo_finance::FinanceReference::Currencies,
                )),
                22 => Some(SettingsPage::FinanceReference(
                    amigo_finance::FinanceReference::Chains,
                )),
                23 => Some(SettingsPage::Reference(
                    self.vault.clone(),
                    amigo_finance::FinanceReference::Clients,
                )),
                24 => Some(SettingsPage::Reference(
                    self.vault.clone(),
                    amigo_finance::FinanceReference::Accounts,
                )),
                25 => Some(SettingsPage::FinanceReference(
                    amigo_finance::FinanceReference::Countries,
                )),
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
    std::fs::write(
        output.join("External.md"),
        "# An external Markdown note\n\nChoose a Journal folder for this copy.\n",
    )
    .unwrap();
    let library = Library::open(library::default_root(), None).unwrap();
    let journal = Journal::open(&library.root).unwrap();
    let text = "---\ntags: [planning, journal]\n---\n# A place to think\n\nKeep your ideas close, in plain **Markdown**.\n\n## Today's focus\n\n- [x] Bring settings into one place\n- [x] Keep notes as portable `.md` files\n- [ ] Plan the next small improvement\n\n> Start with a clear thought. Leave room for the next one.\n\n## Working notes\n\nUse **bold** for decisions, *italics* for emphasis, and `code` for details.\nEdit here or format Markdown in the source pane.\n\n| View | Best for |\n| --- | --- |\n| Markdown | Writing without distraction |\n| Preview | Writing in the finished note |\n| Split | Seeing changes as you write |\n\n## Next steps\n\nOpen [[Projects/Ideas|project ideas]] or [the daily note](Daily.md).\n";
    let text = if std::env::args().any(|a| a == "--list-editing") {
        "# Lists that keep flowing\n\n- First bullet\n- \n\n1. First numbered item\n2. \n\n- [x] A finished task\n- [ ] \n\n> Enter continues the list. Enter on the empty item finishes it.\n"
    } else {
        text
    };
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
    let code_document = journal.create("Code", "# Code and tables\n\nReadable code blocks keep their language and a copy action.\n\n```rust\npub fn greeting(name: &str) -> String {\n    let message = \"Hello from Amigo\";\n    format!(\"{message}, {name}!\")\n}\n```\n\n## Planning together\n\n| Feature | State | Notes |\n| --- | --- | --- |\n| **Editor** | Ready | Syntax, line numbers, selection toolbar |\n| **Preview** | Ready | Editable headings and formatted text |\n\n> Plain Markdown, kept in your own notebook.\n").unwrap();
    journal.create_folder("Discarded").unwrap();
    journal.create("Discarded/Old plan", "# Old plan\n\nThis note is in Trash. Restore the note or its folder, or remove it permanently.\n").unwrap();
    journal.trash("Discarded").unwrap();
    let entries = journal.entries().unwrap();
    let trash_document = journal
        .read(
            &entries
                .iter()
                .find(|entry| amigo_core::journal::is_in_trash(&entry.path) && !entry.folder)
                .unwrap()
                .path,
        )
        .unwrap();
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
            app.journal.entries = entries.clone();
            app.journal.loaded = true;
            Ok(Box::new(Preview {
                app,
                output,
                step: if std::env::args().any(|a| a == "--settings") {
                    3
                } else if std::env::args().any(|a| a == "--folder-scenes" || a == "--tree-menu") {
                    16
                } else if std::env::args().any(|a| a == "--trash") {
                    14
                } else {
                    0
                },
                frames: 0,
                pending: false,
                board,
                vault,
                code_document,
                trash_document,
                entries,
            }))
        }),
    )
}
