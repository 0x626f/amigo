//! Markdown notebook UI. Filesystem work and native pickers belong to the shell.
use amigo_core::journal::{
    Document, Entry, Mode, Preferences,
    formatting::{self, Format},
};
use amigo_ui::design::{self, Icon, colors};
use egui::{self, RichText};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use std::{collections::BTreeSet, time::Instant};

#[derive(Clone, Debug)]
pub enum Action {
    Open(String),
    FollowLink { from: String, target: String },
    Refresh,
    Search(String),
    NewNote(String),
    NewFolder(String),
    Rename { old: String, name: String },
    Trash(String),
    Import(String),
    Export(String),
    Reload,
    SaveCopy,
}
pub struct Editor {
    pub document: Document,
    pub saved_text: String,
    pub changed_at: Instant,
    pub conflict: bool,
    selection: std::ops::Range<usize>,
    context_selection: Option<std::ops::Range<usize>>,
    preview_source: String,
    revision: u64,
    preview_revision: u64,
    preview_links: Vec<String>,
    preview_cache: CommonMarkCache,
}
impl Editor {
    pub fn new(document: Document) -> Self {
        Self {
            saved_text: document.text.clone(),
            document,
            changed_at: Instant::now(),
            conflict: false,
            selection: 0..0,
            context_selection: None,
            preview_source: String::new(),
            revision: 0,
            preview_revision: u64::MAX,
            preview_links: vec![],
            preview_cache: CommonMarkCache::default(),
        }
    }
    pub fn dirty(&self) -> bool {
        self.document.text != self.saved_text
    }
}
#[derive(Default)]
pub struct JournalTab {
    pub entries: Vec<Entry>,
    pub editor: Option<Editor>,
    pub busy: bool,
    pub loading: bool,
    pub error: String,
    pub loaded: bool,
    search: String,
    searched: String,
    searched_at: Option<Instant>,
    pub matches: Option<Vec<Entry>>,
    expanded: BTreeSet<String>,
    folder: String,
    dialog: Option<NameDialog>,
    title_edit: Option<TitleEdit>,
}
struct TitleEdit {
    path: String,
    value: String,
    focus: bool,
}
struct NameDialog {
    kind: NameKind,
    value: String,
    error: String,
}
enum NameKind {
    Note,
    Folder,
    Rename(String),
    Delete(String),
}
impl JournalTab {
    pub fn open(&mut self, document: Document) {
        self.folder = document
            .path
            .rsplit_once('/')
            .map_or(String::new(), |(parent, _)| parent.into());
        self.title_edit = None;
        self.editor = Some(Editor::new(document));
        self.error.clear();
    }
    pub fn dirty(&self) -> bool {
        self.editor.as_ref().is_some_and(Editor::dirty)
    }
    pub fn apply_search(&mut self, query: &str, entries: Vec<Entry>) {
        if self.search.trim() == query.trim() {
            self.matches = Some(entries);
        }
    }

    pub fn sidebar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        design::eyebrow(ui, "NOTEBOOK");
        ui.add_space(5.);
        if ui
            .add(
                design::input(&mut self.search)
                    .id(egui::Id::new("journal-search"))
                    .hint_text("Search notes…")
                    .desired_width(ui.available_width()),
            )
            .changed()
        {
            self.searched_at = Some(Instant::now());
        }
        if self.search != self.searched
            && self
                .searched_at
                .is_some_and(|t| t.elapsed().as_millis() >= 250)
            && !self.busy
        {
            self.searched = self.search.clone();
            action = Some(if self.search.trim().is_empty() {
                self.matches = None;
                Action::Refresh
            } else {
                Action::Search(self.search.clone())
            });
        }
        if self.searched_at.is_some() && self.search != self.searched {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
        ui.horizontal(|ui| {
            if design::small_button(ui, "+ Note").clicked() {
                self.dialog = Some(NameDialog {
                    kind: NameKind::Note,
                    value: self.prefix("Untitled"),
                    error: String::new(),
                });
            }
            if design::icon_button(
                ui,
                egui::Id::new("journal-new-folder"),
                Icon::Folder,
                "New Folder",
                28.,
            )
            .clicked()
            {
                self.dialog = Some(NameDialog {
                    kind: NameKind::Folder,
                    value: self.prefix("New folder"),
                    error: String::new(),
                });
            }
            if design::icon_button(
                ui,
                egui::Id::new("journal-refresh"),
                Icon::Refresh,
                "Refresh Notes",
                28.,
            )
            .clicked()
            {
                action = Some(Action::Refresh);
            }
        });
        ui.add_space(6.);
        let entries = self.matches.as_ref().unwrap_or(&self.entries).clone();
        if !self.folder.is_empty() && design::small_button(ui, "All Notes").clicked() {
            self.folder.clear();
        }
        for entry in entries {
            let parent = entry.path.rsplit_once('/').map_or("", |(p, _)| p);
            if self.matches.is_none()
                && parent
                    .split('/')
                    .scan(String::new(), |path, part| {
                        if !path.is_empty() {
                            path.push('/');
                        }
                        path.push_str(part);
                        Some(path.clone())
                    })
                    .any(|p| !p.is_empty() && !self.expanded.contains(&p))
            {
                continue;
            }
            let depth = if self.matches.is_some() {
                0
            } else {
                entry.path.matches('/').count()
            };
            ui.horizontal(|ui| {
                let label = if self.matches.is_some() {
                    entry.path.strip_suffix(".md").unwrap_or(&entry.path)
                } else {
                    entry
                        .path
                        .rsplit('/')
                        .next()
                        .unwrap_or("")
                        .strip_suffix(".md")
                        .unwrap_or(entry.path.rsplit('/').next().unwrap_or(""))
                };
                let selected = self
                    .editor
                    .as_ref()
                    .is_some_and(|e| e.document.path == entry.path);
                let row = design::nav_row_with_options(
                    ui,
                    label,
                    if entry.folder {
                        Icon::Folder
                    } else {
                        Icon::Book
                    },
                    selected,
                    design::NavRowOptions {
                        indent: depth as f32 * 12.,
                        disclosure: entry.folder.then(|| self.expanded.contains(&entry.path)),
                        ..Default::default()
                    },
                );
                ui.interact(
                    row.rect,
                    egui::Id::new(("journal-entry", &entry.path)),
                    egui::Sense::hover(),
                );
                if row.clicked() {
                    if entry.folder {
                        if !self.expanded.remove(&entry.path) {
                            self.expanded.insert(entry.path.clone());
                        }
                        self.folder = entry.path.clone();
                    } else {
                        action = Some(Action::Open(entry.path.clone()));
                    }
                }
                row.context_menu(|ui| {
                    if design::button(ui, "Rename / Move…").clicked() {
                        self.dialog = Some(NameDialog {
                            kind: NameKind::Rename(entry.path.clone()),
                            value: entry.path.clone(),
                            error: String::new(),
                        });
                        ui.close();
                    }
                    if design::button(ui, "Move to Trash…").clicked() {
                        self.dialog = Some(NameDialog {
                            kind: NameKind::Delete(entry.path.clone()),
                            value: String::new(),
                            error: String::new(),
                        });
                        ui.close();
                    }
                });
            });
        }
        if self.entries.is_empty() && self.loaded {
            design::hint(ui, "Create a note or import Markdown files.");
        }
        action
    }
    fn prefix(&self, name: &str) -> String {
        if self.folder.is_empty() {
            name.into()
        } else {
            format!("{}/{name}", self.folder)
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui, preferences: &mut Preferences) -> Option<Action> {
        let mut action = None;
        let title_width = (ui.available_width() - 270.).max(0.);
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(title_width, 38.),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let response = ui.add(
                        egui::Label::new(RichText::new("Journal").size(32.).strong())
                            .halign(egui::Align::Min),
                    );
                    ui.interact(
                        response.rect,
                        egui::Id::new("journal-page-title"),
                        egui::Sense::hover(),
                    );
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                mode_buttons(ui, &mut preferences.mode, "journal-mode");
                ui.separator();
                for (key, icon, label, candidate) in [
                    (
                        "import",
                        Icon::Import,
                        "Import Markdown",
                        Some(Action::Import(self.folder.clone())),
                    ),
                    ("rename", Icon::Edit, "Rename Note", None),
                    ("delete", Icon::Trash, "Move Note to Trash", None),
                ] {
                    if ui
                        .add_enabled_ui(key == "import" || self.editor.is_some(), |ui| {
                            design::icon_button(
                                ui,
                                egui::Id::new(("journal", key)),
                                icon,
                                label,
                                32.,
                            )
                        })
                        .inner
                        .clicked()
                    {
                        if key == "rename" {
                            let path = self.editor.as_ref().unwrap().document.path.clone();
                            self.dialog = Some(NameDialog {
                                kind: NameKind::Rename(path.clone()),
                                value: path,
                                error: String::new(),
                            });
                        } else if key == "delete" {
                            self.dialog = Some(NameDialog {
                                kind: NameKind::Delete(
                                    self.editor.as_ref().unwrap().document.path.clone(),
                                ),
                                value: String::new(),
                                error: String::new(),
                            });
                        } else {
                            action = candidate;
                        }
                    }
                }
            });
        });
        ui.add_space(10.);
        if !self.error.is_empty() {
            ui.colored_label(colors(ui).red, &self.error);
            if self.editor.as_ref().is_some_and(|e| e.conflict) {
                ui.horizontal(|ui| {
                    if design::button(ui, "Reload from Disk…").clicked() {
                        action = Some(Action::Reload);
                    }
                    if design::button(ui, "Save My Edits as Copy").clicked() {
                        action = Some(Action::SaveCopy);
                    }
                });
            }
        }
        if self.busy {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(40));
        }
        let width = ui.available_width();
        let height = ui.available_height().max(100.);
        let palette = colors(ui);
        let notebook = egui::Frame::new()
            .fill(palette.card)
            .stroke(egui::Stroke::new(1., palette.hover_line))
            .corner_radius(6)
            .inner_margin(0)
            .show(ui, |ui| {
                ui.set_min_size(egui::vec2(width - 2., height - 2.));
                ui.set_max_size(egui::vec2(width - 2., height - 2.));
                if self.editor.is_some() {
                    action = self.note_header(ui).or(action.take());
                    let Some(editor) = &mut self.editor else {
                        return;
                    };
                    let width = ui.available_width();
                    let height = ui.available_height().max(80.);
                    ui.add_enabled_ui(!self.loading, |ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(width, height),
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 0.;
                                match preferences.mode {
                                    Mode::Source => {
                                        editor_pane(ui, width, height, |ui, width, height| {
                                            source(editor, preferences, ui, width, height)
                                        })
                                    }
                                    Mode::Preview => {
                                        editor_pane(ui, width, height, |ui, width, height| {
                                            preview(editor, ui, width, height)
                                        })
                                    }
                                    Mode::Split => {
                                        let half = (width - 1.) / 2.;
                                        editor_pane(ui, half, height, |ui, width, height| {
                                            source(editor, preferences, ui, width, height)
                                        });
                                        let (divider, _) = ui.allocate_exact_size(
                                            egui::vec2(1., height),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().line_segment(
                                            [divider.center_top(), divider.center_bottom()],
                                            egui::Stroke::new(1., palette.hover_line),
                                        );
                                        editor_pane(ui, half, height, |ui, width, height| {
                                            preview(editor, ui, width, height)
                                        });
                                    }
                                }
                            },
                        );
                    });
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(80.);
                        design::icon(ui, Icon::Book, colors(ui).muted, 44.);
                        ui.heading("Your Journal");
                        design::hint(ui, "Create a Markdown note or import existing .md files.");
                    });
                }
            });
        ui.interact(
            notebook.response.rect,
            egui::Id::new("journal-notebook"),
            egui::Sense::hover(),
        );
        let Some(editor) = &mut self.editor else {
            return action;
        };
        if let Some(target) = editor
            .preview_links
            .iter()
            .find(|link| editor.preview_cache.get_link_hook(link) == Some(true))
            .cloned()
        {
            editor.preview_cache.add_link_hook(&target);
            action = Some(Action::FollowLink {
                from: editor.document.path.clone(),
                target,
            });
        }
        action
    }
    fn note_header(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let keys = design::ActionKeys::capture(ui.ctx());
        let editor = self.editor.as_ref()?;
        let path = editor.document.path.clone();
        let filename = path.rsplit('/').next().unwrap_or(&path);
        let name = filename.strip_suffix(".md").unwrap_or(filename);
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 46.), egui::Sense::hover());
        let title_rect = egui::Rect::from_center_size(
            rect.center(),
            egui::vec2((width - 170.).clamp(80., 500.), 34.),
        );
        let mut action = None;
        if let Some(edit) = &mut self.title_edit {
            let id = egui::Id::new("journal-title-edit");
            let response = ui.put(
                title_rect,
                design::input(&mut edit.value)
                    .id(id)
                    .font(egui::FontId::proportional(20.))
                    .horizontal_align(egui::Align::Center)
                    .interactive(!self.loading),
            );
            if edit.focus {
                response.request_focus();
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(edit.value.chars().count()),
                        )));
                    state.store(ui.ctx(), id);
                }
                edit.focus = false;
            }
            let valid = !edit.value.trim().is_empty() && !edit.value.contains(['/', '\\']);
            let mut save = false;
            let mut cancel = false;
            let controls = egui::Rect::from_min_size(
                egui::pos2(rect.right() - 78., rect.top() + 7.),
                egui::vec2(72., 32.),
            );
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(controls)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 4.;
                    save = ui
                        .add_enabled_ui(valid && !self.loading, |ui| {
                            design::icon_button(
                                ui,
                                egui::Id::new("journal-title-save"),
                                Icon::Check,
                                "Save Note Name",
                                32.,
                            )
                        })
                        .inner
                        .clicked();
                    cancel = design::icon_button(
                        ui,
                        egui::Id::new("journal-title-cancel"),
                        Icon::Close,
                        "Cancel Rename",
                        32.,
                    )
                    .clicked();
                },
            );
            let shortcut = keys.in_ui(ui, valid && !self.loading);
            if save || shortcut.confirm {
                let value = edit.value.trim();
                let new = edit.path.rsplit_once('/').map_or_else(
                    || value.to_owned(),
                    |(parent, _)| format!("{parent}/{value}"),
                );
                action = Some(Action::Rename {
                    old: edit.path.clone(),
                    name: new,
                });
                // Retain the typed name if the background rename fails.
                ui.memory_mut(|m| m.surrender_focus(id));
            } else if !self.loading && (cancel || shortcut.cancel) {
                self.title_edit = None;
                ui.memory_mut(|m| m.surrender_focus(id));
            }
        } else {
            let response = ui
                .put(
                    title_rect,
                    egui::Label::new(RichText::new(name).size(20.).strong())
                        .truncate()
                        .halign(egui::Align::Center)
                        .sense(egui::Sense::click()),
                )
                .on_hover_text("Double-click to rename note");
            ui.interact(
                response.rect,
                egui::Id::new("journal-note-title"),
                egui::Sense::hover(),
            );
            if response.double_clicked() && !self.loading {
                self.title_edit = Some(TitleEdit {
                    value: name.into(),
                    path,
                    focus: true,
                });
                ui.ctx().request_repaint();
            }
        }
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            egui::Stroke::new(1., colors(ui).hover_line),
        );
        action
    }
    pub fn dialogs(&mut self, ctx: &egui::Context) -> Option<Action> {
        let mut dialog = self.dialog.take()?;
        let keys = design::ActionKeys::capture(ctx);
        let mut close = false;
        let mut submit = false;
        let deleting = matches!(dialog.kind, NameKind::Delete(_));
        let title = match &dialog.kind {
            NameKind::Note => "New Note",
            NameKind::Folder => "New Folder",
            NameKind::Rename(_) => "Rename / Move",
            NameKind::Delete(_) => "Move to Trash",
        };
        let response = design::modal(ctx, egui::Id::new("journal-name-dialog")).show(ctx, |ui| {
            ui.set_width(design::dialog_width(ctx, 520.));
            close = design::dialog_heading(ui, title).clicked();
            if let NameKind::Delete(path) = &dialog.kind {
                ui.label(path);
                design::hint(
                    ui,
                    "The file or folder will be moved to journal/.trash. You can recover it there.",
                );
            } else {
                ui.label("Path inside Journal");
                ui.add(
                    design::input(&mut dialog.value)
                        .id(egui::Id::new("journal-note-name"))
                        .desired_width(f32::INFINITY),
                );
            }
            if !dialog.error.is_empty() {
                ui.colored_label(colors(ui).red, &dialog.error);
            }
            design::dialog_footer(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    submit = ui
                        .add_enabled_ui(deleting || !dialog.value.trim().is_empty(), |ui| {
                            design::dialog_primary(
                                ui,
                                if deleting { "Move to Trash" } else { "Save" },
                            )
                        })
                        .inner
                        .clicked();
                    close |= design::button(ui, "Cancel").clicked();
                });
            });
        });
        let shortcut = keys.dialog(&response, !deleting && !dialog.value.trim().is_empty());
        if close || shortcut.cancel {
            return None;
        }
        if submit || shortcut.confirm {
            return Some(match dialog.kind {
                NameKind::Note => Action::NewNote(dialog.value.trim().into()),
                NameKind::Folder => Action::NewFolder(dialog.value.trim().into()),
                NameKind::Rename(old) => Action::Rename {
                    old,
                    name: dialog.value.trim().into(),
                },
                NameKind::Delete(path) => Action::Trash(path),
            });
        }
        self.dialog = Some(dialog);
        None
    }
}
pub fn mode_buttons(ui: &mut egui::Ui, mode: &mut Mode, prefix: &str) {
    let mut modes = [
        (Mode::Source, Icon::Markdown, "Markdown"),
        (Mode::Split, Icon::Split, "Split"),
        (Mode::Preview, Icon::Preview, "Preview"),
    ];
    if ui.layout().main_dir == egui::Direction::RightToLeft {
        modes.reverse();
    }
    for (value, icon, label) in modes {
        if design::selected_icon_button(
            ui,
            egui::Id::new((prefix, value)),
            icon,
            label,
            32.,
            *mode == value,
        )
        .clicked()
        {
            *mode = value;
        }
    }
}
fn editor_pane(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    contents: impl FnOnce(&mut egui::Ui, f32, f32),
) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            egui::Frame::new().inner_margin(14).show(ui, |ui| {
                let width = (width - 28.).max(24.);
                let height = (height - 28.).max(24.);
                ui.set_min_size(egui::vec2(width, height));
                ui.set_max_size(egui::vec2(width, height));
                contents(ui, width, height);
            });
        },
    );
}
fn source(editor: &mut Editor, prefs: &Preferences, ui: &mut egui::Ui, width: f32, height: f32) {
    let id = egui::Id::new(("journal-editor", &editor.document.path));
    if !ui.is_enabled() {
        ui.memory_mut(|m| m.surrender_focus(id));
    }

    if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary))
        && !editor.selection.is_empty()
        && ui.ctx().read_response(id).is_some_and(|r| {
            ui.input(|i| i.pointer.interact_pos().is_some_and(|p| r.rect.contains(p)))
        })
    {
        editor.context_selection = Some(editor.selection.clone());
    }
    let mut format = None;
    if ui.is_enabled() && ui.memory(|m| m.has_focus(id)) && !egui::Popup::is_any_open(ui.ctx()) {
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::B)) {
            format = Some(Format::Bold);
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::I)) {
            format = Some(Format::Italic);
        }
    }
    egui::ScrollArea::both()
        .id_salt(("journal-source-scroll", &editor.document.path))
        .max_height(height)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            let font = egui::FontId::monospace(prefs.font_size.clamp(11., 28.));
            let ink = colors(ui).ink;
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                ui.fonts_mut(|f| {
                    f.layout_job(egui::text::LayoutJob::simple(
                        text.as_str().to_owned(),
                        font.clone(),
                        ink,
                        if prefs.word_wrap { wrap } else { f32::INFINITY },
                    ))
                })
            };
            let enabled = ui.is_enabled();
            let output = egui::TextEdit::multiline(&mut editor.document.text)
                .interactive(enabled)
                .id(id)
                .code_editor()
                .font(egui::FontId::monospace(prefs.font_size))
                .frame(egui::Frame::NONE)
                .desired_width(width)
                .min_size(egui::vec2(width, height - 12.))
                .layouter(&mut layouter)
                .show(ui);
            if output.response.changed() {
                editor.revision = editor.revision.wrapping_add(1);
                editor.changed_at = Instant::now();
            }
            if let Some(range) = &editor.context_selection {
                editor.selection = range.clone();
                let mut state = output.state.clone();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(range.start),
                        egui::text::CCursor::new(range.end),
                    )));
                state.store(ui.ctx(), id);
            } else if let Some(range) = output.cursor_range {
                let range = range.as_sorted_char_range();
                editor.selection = range.start.0..range.end.0;
            }
            output.response.context_menu(|ui| {
                ui.label(RichText::new("Format Selection").strong());
                for (label, kind) in [
                    ("Bold", Format::Bold),
                    ("Italic", Format::Italic),
                    ("Strikethrough", Format::Strike),
                    ("Inline Code", Format::Code),
                    ("Heading 1", Format::Heading(1)),
                    ("Heading 2", Format::Heading(2)),
                    ("Heading 3", Format::Heading(3)),
                    ("Bulleted List", Format::Bullet),
                    ("Numbered List", Format::Numbered),
                    ("Task List", Format::Task),
                    ("Quote", Format::Quote),
                    ("Code Block", Format::CodeBlock),
                    ("Link", Format::Link),
                    ("Table", Format::Table),
                    ("Horizontal Rule", Format::Rule),
                ] {
                    if ui
                        .push_id(("journal-format", label), |ui| design::button(ui, label))
                        .inner
                        .clicked()
                    {
                        format = Some(kind);
                        ui.close();
                    }
                }
                ui.separator();
                if design::button(ui, "Copy").clicked() {
                    ui.ctx().copy_text(
                        editor
                            .document
                            .text
                            .chars()
                            .skip(editor.selection.start)
                            .take(editor.selection.len())
                            .collect(),
                    );
                    ui.close();
                }
            });
            if !output.response.context_menu_opened()
                && !ui.input(|i| i.pointer.button_down(egui::PointerButton::Secondary))
            {
                editor.context_selection = None;
            }
            if let Some(kind) = format {
                let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                let old_range = egui::text::CCursorRange::two(
                    egui::text::CCursor::new(editor.selection.start),
                    egui::text::CCursor::new(editor.selection.end),
                );
                let mut undoer = state.undoer();
                undoer.add_undo(&(old_range, editor.document.text.clone()));
                editor.selection =
                    formatting::apply(&mut editor.document.text, editor.selection.clone(), kind);
                state.set_undoer(undoer);
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(editor.selection.start),
                        egui::text::CCursor::new(editor.selection.end),
                    )));
                state.store(ui.ctx(), id);
                editor.revision = editor.revision.wrapping_add(1);
                editor.changed_at = Instant::now();
                ui.memory_mut(|m| m.request_focus(id));
            }
        });
}
fn preview(editor: &mut Editor, ui: &mut egui::Ui, width: f32, height: f32) {
    if editor.preview_revision != editor.revision {
        editor.preview_revision = editor.revision;
        let (source, links) = markdown_links(&editor.document.text);
        editor.preview_source = source;
        editor.preview_links = links;
        editor.preview_cache = CommonMarkCache::default();
        for link in &editor.preview_links {
            editor.preview_cache.add_link_hook(link);
        }
    }
    ui.set_max_width(width);
    ui.set_max_height(height);
    ui.style_mut().spacing.scroll.bar_width = 0.;
    CommonMarkViewer::new()
        .enable_scroll_to_heading(true)
        .show_scrollable(
            ("journal-preview", &editor.document.path),
            ui,
            &mut editor.preview_cache,
            &editor.preview_source,
        );
}

fn markdown_links(text: &str) -> (String, Vec<String>) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let body = if let Some(rest) = text.strip_prefix("---\r\n") {
        rest.split_once("\r\n---\r\n")
            .map_or(text, |(_, body)| body)
    } else if let Some(rest) = text.strip_prefix("---\n") {
        rest.split_once("\n---\n").map_or(text, |(_, body)| body)
    } else {
        text
    };
    // Parser offsets keep wiki syntax inside code spans and fenced/indented blocks literal.
    let mut protected: Vec<_> = pulldown_cmark::Parser::new(body)
        .into_offset_iter()
        .filter_map(|(event, range)| {
            matches!(
                event,
                pulldown_cmark::Event::Code(_)
                    | pulldown_cmark::Event::Start(pulldown_cmark::Tag::CodeBlock(_))
            )
            .then_some(range)
        })
        .collect();
    protected.sort_by_key(|range| range.start);
    let mut output = String::new();
    let mut cursor = 0;
    for range in protected {
        if range.start < cursor {
            continue;
        }
        wiki_text(&body[cursor..range.start], &mut output);
        output.push_str(&body[range.clone()]);
        cursor = range.end;
    }
    wiki_text(&body[cursor..], &mut output);
    let mut hooks = BTreeSet::new();
    for event in pulldown_cmark::Parser::new(&output) {
        if let pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) = event {
            let url = dest_url.to_string();
            if url.starts_with("amigo-journal://") || (!url.contains(':') && !url.starts_with('#'))
            {
                hooks.insert(url);
            }
        }
    }
    (output, hooks.into_iter().collect())
}
fn wiki_text(mut rest: &str, output: &mut String) {
    while let Some(start) = rest.find("[[") {
        output.push_str(&rest[..start]);
        let Some(end) = rest[start + 2..].find("]]").map(|n| n + start + 2) else {
            output.push_str(&rest[start..]);
            return;
        };
        let content = &rest[start + 2..end];
        let (target, label) = content.split_once('|').unwrap_or((content, content));
        output.push_str(&format!(
            "[{}](amigo-journal://{})",
            label.replace('[', "\\[").replace(']', "\\]"),
            encode_target(target)
        ));
        rest = &rest[end + 2..];
    }
    output.push_str(rest);
}
fn encode_target(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._/".contains(b) {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Harness {
        action: Option<Action>,
        tab: JournalTab,
        prefs: Preferences,
    }
    fn frame(
        app: &mut Harness,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        size: egui::Vec2,
    ) -> egui::FullOutput {
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
    fn label(output: &egui::FullOutput, text: &str) -> egui::Rect {
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
            .find_map(|s| find(&s.shape, text))
            .unwrap_or_else(|| panic!("Missing {text}"))
    }
    #[test]
    fn context_formatting_and_undo_keep_unicode_selection_and_markdown() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut app = Harness {
            action: None,
            tab: JournalTab::default(),
            prefs: Preferences {
                mode: Mode::Source,
                ..Default::default()
            },
        };
        app.tab.open(Document {
            path: "Note.md".into(),
            text: "Hello 🌍".into(),
            fingerprint: "test".into(),
        });
        let size = egui::vec2(980., 640.);
        for _ in 0..4 {
            frame(&mut app, &ctx, vec![], size);
        }
        let id = egui::Id::new(("journal-editor", "Note.md"));
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(5),
            )));
        state.store(&ctx, id);
        ctx.memory_mut(|m| m.request_focus(id));
        frame(&mut app, &ctx, vec![], size);
        let pos = ctx.read_response(id).unwrap().rect.min + egui::vec2(12., 10.);
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
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
        let output = frame(&mut app, &ctx, vec![], size);
        let bold = label(&output, "Bold").center();
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(bold),
                    egui::Event::PointerButton {
                        pos: bold,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                size,
            );
        }
        assert_eq!(
            app.tab.editor.as_ref().unwrap().document.text,
            "**Hello** 🌍"
        );
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::Key {
                    key: egui::Key::Z,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                }],
                size,
            );
        }
        assert_eq!(app.tab.editor.as_ref().unwrap().document.text, "Hello 🌍");
    }
    #[test]
    fn exclusive_load_blocks_edits_but_autosave_does_not() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut app = Harness {
            action: None,
            tab: JournalTab::default(),
            prefs: Preferences {
                mode: Mode::Source,
                ..Default::default()
            },
        };
        app.tab.open(Document {
            path: "Note.md".into(),
            text: "Original".into(),
            fingerprint: "test".into(),
        });
        let size = egui::vec2(980., 640.);
        let id = egui::Id::new(("journal-editor", "Note.md"));
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![], size);
        }
        ctx.memory_mut(|m| m.request_focus(id));
        app.tab.busy = true;
        app.tab.loading = true;
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("Blocked".into())],
            size,
        );
        assert_eq!(app.tab.editor.as_ref().unwrap().document.text, "Original");
        app.tab.loading = false;
        frame(&mut app, &ctx, vec![], size);
        ctx.memory_mut(|m| m.request_focus(id));
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("Allowed".into())],
            size,
        );
        assert!(
            app.tab
                .editor
                .as_ref()
                .unwrap()
                .document
                .text
                .contains("Allowed")
        );
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
    #[test]
    fn notebook_keeps_title_borders_and_icon_controls_and_double_click_renames() {
        for size in [egui::vec2(680., 500.), egui::vec2(1160., 760.)] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            let mut app = Harness {
                action: None,
                tab: JournalTab::default(),
                prefs: Preferences::default(),
            };
            app.tab.open(Document {
                path: "Folder/Original.md".into(),
                text: "# Original\n\nBody".into(),
                fingerprint: "test".into(),
            });
            for _ in 0..4 {
                frame(&mut app, &ctx, vec![], size);
            }
            let title = ctx
                .read_response(egui::Id::new("journal-note-title"))
                .unwrap()
                .rect;
            let notebook = ctx
                .read_response(egui::Id::new("journal-notebook"))
                .unwrap()
                .rect;
            let heading = ctx
                .read_response(egui::Id::new("journal-page-title"))
                .unwrap()
                .rect;
            assert!((title.center().x - notebook.center().x).abs() < 1.);
            assert!(heading.left() < notebook.left() + 1.);
            assert!(notebook.right() <= size.x && notebook.bottom() <= size.y);
            assert!(
                ctx.read_response(egui::Id::new(("journal", "export")))
                    .is_none()
            );
            click(&mut app, &ctx, title.center(), size);
            click(&mut app, &ctx, title.center(), size);
            frame(&mut app, &ctx, vec![], size);
            assert!(app.tab.title_edit.is_some());
            assert_eq!(
                ctx.memory(|m| m.focused()),
                Some(egui::Id::new("journal-title-edit"))
            );
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::Text("Renamed 🌍".into())],
                size,
            );
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                size,
            );
            assert!(
                matches!(&app.action, Some(Action::Rename {old, name}) if old == "Folder/Original.md" && name == "Folder/Renamed 🌍")
            );
            assert_eq!(
                app.tab.editor.as_ref().unwrap().document.text,
                "# Original\n\nBody"
            );
            app.tab.title_edit = Some(TitleEdit {
                path: "Folder/Original.md".into(),
                value: "Discard me".into(),
                focus: true,
            });
            frame(&mut app, &ctx, vec![], size);
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                size,
            );
            assert!(app.tab.title_edit.is_none());
            assert!(app.action.is_none());
            for mode in [Mode::Source, Mode::Split, Mode::Preview] {
                let button = ctx
                    .read_response(egui::Id::new(("journal-mode", mode)))
                    .unwrap()
                    .rect;
                click(&mut app, &ctx, button.center(), size);
                assert_eq!(app.prefs.mode, mode);
            }
        }
    }
    #[test]
    fn folder_disclosure_opens_and_closes_nested_notes() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut tab = JournalTab {
            loaded: true,
            entries: vec![
                Entry {
                    path: "Folder".into(),
                    folder: true,
                },
                Entry {
                    path: "Folder/Note.md".into(),
                    folder: false,
                },
            ],
            ..Default::default()
        };
        fn render(tab: &mut JournalTab, ctx: &egui::Context, events: Vec<egui::Event>) {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(224., 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        tab.sidebar(ui);
                    });
                },
            );
            output.textures_delta.clear();
        }
        for _ in 0..3 {
            render(&mut tab, &ctx, vec![]);
        }
        for open in [true, false] {
            let folder = ctx
                .read_response(egui::Id::new(("journal-entry", "Folder")))
                .unwrap()
                .rect;
            for pressed in [true, false] {
                render(
                    &mut tab,
                    &ctx,
                    vec![
                        egui::Event::PointerMoved(folder.center()),
                        egui::Event::PointerButton {
                            pos: folder.center(),
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            render(&mut tab, &ctx, vec![]);
            render(&mut tab, &ctx, vec![]);
            assert_eq!(tab.expanded.contains("Folder"), open);
            assert_eq!(
                ctx.read_response(egui::Id::new(("journal-entry", "Folder/Note.md")))
                    .is_some(),
                open
            );
        }
        assert!(!tab.expanded.contains("Folder"));
    }
    #[test]
    fn every_mode_fits_both_window_sizes_and_cache_tracks_edits() {
        for size in [egui::vec2(680., 500.), egui::vec2(1160., 760.)] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            let mut app = Harness {
                action: None,
                tab: JournalTab::default(),
                prefs: Preferences::default(),
            };
            app.tab.open(Document{path:"Notebook.md".into(),text:"# Notebook\n\n**Bold** and _italic_.\n\n| A | B |\n| --- | --- |\n| one | two |\n\n- [ ] Task\n\n```rust\nlet n=1;\n```\n".into(),fingerprint:"test".into()});
            for mode in [Mode::Source, Mode::Split, Mode::Preview] {
                app.prefs.mode = mode;
                for _ in 0..4 {
                    frame(&mut app, &ctx, vec![], size);
                }
                let editor = app.tab.editor.as_ref().unwrap();
                if mode != Mode::Source {
                    assert_eq!(editor.preview_revision, editor.revision);
                    assert!(editor.preview_source.contains("Notebook"));
                }
            }
        }
    }
    #[test]
    fn wikilinks_and_relative_links_are_hooked_without_touching_code_or_front_matter() {
        let (source, links) = markdown_links(
            "---\ntitle: Private\n---\n[[Folder/Note|My note]] [Other](Other%20Note.md)\n\n```md\n[[code]]\n```\n",
        );
        let (inline, inline_links) =
            markdown_links("`[[literal]]` and [[Real]]\n\n    [[indented]]\n");
        assert!(inline.contains("`[[literal]]`"));
        assert!(inline.contains("    [[indented]]"));
        assert!(inline.contains("[Real](amigo-journal://Real)"));
        assert_eq!(inline_links, vec!["amigo-journal://Real"]);
        let (windows, _) = markdown_links("\u{feff}---\r\ntitle: Private\r\n---\r\n# Windows\r\n");
        assert_eq!(windows, "# Windows\r\n");
        assert!(!source.contains("title: Private"));
        assert!(source.contains("[My note](amigo-journal://Folder/Note)"));
        assert!(source.contains("[[code]]"));
        assert_eq!(
            links,
            vec!["Other%20Note.md", "amigo-journal://Folder/Note"]
        );
    }
}
