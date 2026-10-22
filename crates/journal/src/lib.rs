//! Markdown notebook UI. Filesystem work and native pickers belong to the shell.
use amigo_core::journal::{
    Document, Entry, Mode, Preferences, PreviewLayout,
    formatting::{self, Format},
};
use amigo_ui::design::{self, Icon, colors};
use egui::{self, RichText};
mod highlight;
mod panes;
mod render;
mod scroll;
mod toolbar;
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
    preview: render::Preview,
    syntax: highlight::SourceCache,
    scroll: scroll::ScrollSync,
    preview_link: Option<String>,
    toolbar_menus: Vec<egui::Id>,
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
            preview: Default::default(),
            syntax: Default::default(),
            scroll: Default::default(),
            preview_link: None,
            toolbar_menus: vec![],
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
        let title_width = (ui.available_width() - 130.).max(0.);
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
                    action = self.note_header(ui, preferences).or(action.take());
                    let Some(editor) = &mut self.editor else {
                        return;
                    };
                    let width = ui.available_width();
                    let height = ui.available_height().max(80.);
                    ui.add_enabled_ui(!self.loading, |ui| {
                        panes::show(editor, preferences, ui, width, height)
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
        if let Some(target) = editor.preview_link.take() {
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(target));
            } else if let Some(anchor) = target.strip_prefix('#') {
                if let Some(y) = editor.preview.heading_y(anchor) {
                    editor.scroll.preview_target = Some(y);
                    if preferences.sync_scroll && preferences.mode == Mode::Split {
                        let line = editor.preview.line_for_y((y - 20.).max(0.));
                        editor.scroll.source_target = Some(editor.scroll.y_for_line(line) + 10.);
                    }
                    ui.ctx().request_repaint();
                }
            } else {
                action = Some(Action::FollowLink {
                    from: editor.document.path.clone(),
                    target,
                });
            }
        }
        action
    }
    fn note_header(&mut self, ui: &mut egui::Ui, preferences: &mut Preferences) -> Option<Action> {
        let keys = design::ActionKeys::capture(ui.ctx());
        let editor = self.editor.as_ref()?;
        let path = editor.document.path.clone();
        let filename = path.rsplit('/').next().unwrap_or(&path);
        let name = filename.strip_suffix(".md").unwrap_or(filename);
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 46.), egui::Sense::hover());
        let title_rect = egui::Rect::from_center_size(
            rect.center(),
            egui::vec2(
                (width
                    - if self.title_edit.is_some() {
                        500.
                    } else {
                        340.
                    })
                .clamp(80., 500.),
                34.,
            ),
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
                egui::pos2(rect.right() - 242., rect.top() + 7.),
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
        let controls = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 160., rect.top() + 7.),
            egui::vec2(154., 32.),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(controls)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 4.;
                mode_buttons_with_layout(
                    ui,
                    &mut preferences.mode,
                    "journal-mode",
                    preferences.preview_layout,
                );
                let menu = design::icon_button(
                    ui,
                    egui::Id::new("journal-view-options"),
                    Icon::More,
                    "Editor and Preview Options",
                    32.,
                );
                egui::Popup::menu(&menu).show(|ui| {
                    ui.label(RichText::new("Split Layout").strong());
                    let side = ui.radio_value(
                        &mut preferences.preview_layout,
                        PreviewLayout::SideBySide,
                        "Side by Side",
                    );
                    ui.interact(
                        side.rect,
                        egui::Id::new("journal-layout-side"),
                        egui::Sense::hover(),
                    );
                    if side.changed() {
                        ui.close();
                    }
                    let stacked = ui.radio_value(
                        &mut preferences.preview_layout,
                        PreviewLayout::Stacked,
                        "Stacked",
                    );
                    ui.interact(
                        stacked.rect,
                        egui::Id::new("journal-layout-stacked"),
                        egui::Sense::hover(),
                    );
                    if stacked.changed() {
                        ui.close();
                    }
                    ui.separator();
                    design::checkbox(ui, &mut preferences.sync_scroll, "Synchronize Scrolling");
                    design::checkbox(ui, &mut preferences.line_numbers, "Line Numbers");
                    design::checkbox(ui, &mut preferences.word_wrap, "Soft Wraps");
                });
            },
        );
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
    mode_buttons_with_layout(ui, mode, prefix, PreviewLayout::SideBySide);
}
pub fn mode_buttons_with_layout(
    ui: &mut egui::Ui,
    mode: &mut Mode,
    prefix: &str,
    layout: PreviewLayout,
) {
    let split_icon = if layout == PreviewLayout::Stacked {
        Icon::SplitStacked
    } else {
        Icon::Split
    };
    let mut modes = [
        (Mode::Source, Icon::Markdown, "Markdown"),
        (Mode::Split, split_icon, "Split"),
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
fn markdown_source(text: &str) -> String {
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
    output
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
mod tests;
