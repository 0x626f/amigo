//! Markdown notebook UI. Filesystem work and native pickers belong to the shell.
use amigo_core::journal::{
    Document, Entry, Mode, Preferences, PreviewLayout, TRASH_FOLDER,
    formatting::{self, Format},
    is_in_trash,
};
use amigo_ui::design::{self, Icon, colors};
use egui::{self, RichText};
mod dialogs;
mod editing;
mod highlight;
mod panes;
mod preview_format;
mod render;
mod scroll;
mod search;
mod sidebar;
mod toolbar;
use std::{collections::BTreeSet, time::Instant};

#[derive(Clone, Debug)]
pub enum Action {
    Open(String),
    FollowLink {
        from: String,
        target: String,
    },
    Refresh,
    Search(String),
    NewNote(String),
    NewFolder(String),
    Rename {
        old: String,
        name: String,
    },
    Trash(String),
    Restore(String),
    DeletePermanently(String),
    OpenExternal,
    AttachMedia,
    StoreExternal {
        source: std::path::PathBuf,
        folder: String,
    },
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
    format_state: Option<(u64, std::ops::Range<usize>, formatting::SelectionState)>,
    preview_editing: Option<bool>,
    preview_tools: preview_format::Tools,
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
            format_state: None,
            preview_editing: None,
            preview_tools: Default::default(),
        }
    }
    fn formatting_state(&mut self) -> formatting::SelectionState {
        if self
            .format_state
            .as_ref()
            .is_none_or(|(revision, range, _)| {
                *revision != self.revision || *range != self.selection
            })
        {
            self.format_state = Some((
                self.revision,
                self.selection.clone(),
                formatting::SelectionState::new(&self.document.text, self.selection.clone()),
            ));
        }
        self.format_state.as_ref().unwrap().2.clone()
    }
    pub fn dirty(&self) -> bool {
        self.document.text != self.saved_text
    }
    pub fn append_markdown(&mut self, ctx: &egui::Context, text: &str) {
        let id = egui::Id::new(("journal-editor", &self.document.path));
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        let mut undoer = state.undoer();
        let range = egui::text::CCursorRange::one(egui::text::CCursor::new(
            self.document.text.chars().count(),
        ));
        undoer.add_undo(&(range, self.document.text.clone()));
        state.set_undoer(undoer);
        state.store(ctx, id);
        self.document.text.push_str(text);
        self.revision = self.revision.wrapping_add(1);
        self.changed_at = Instant::now();
    }
}
#[derive(Default)]
pub struct JournalTab {
    pub media_root: Option<std::path::PathBuf>,
    pub entries: Vec<Entry>,
    pub editor: Option<Editor>,
    pub busy: bool,
    pub loading: bool,
    pub error: String,
    pub loaded: bool,
    search: Option<search::SearchDialog>,
    external_file: Option<dialogs::ExternalFileDialog>,
    expanded: BTreeSet<String>,
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
    focus: bool,
    pending: bool,
}
impl NameDialog {
    fn new(kind: NameKind, value: impl Into<String>) -> Self {
        Self {
            kind,
            value: value.into(),
            error: String::new(),
            focus: true,
            pending: false,
        }
    }
}
enum NameKind {
    Note(String),
    Folder(String),
    RenameFolder(String),
    Delete(String),
}
impl JournalTab {
    pub fn open(&mut self, document: Document) {
        let folder = document
            .path
            .rsplit_once('/')
            .map_or(String::new(), |(parent, _)| parent.into());
        self.expand_folder(&folder);
        self.title_edit = None;
        self.editor = Some(Editor::new(document));
        self.error.clear();
    }
    pub fn folder_edit_result(&mut self, error: Option<&str>) {
        if let Some(dialog) = &mut self.dialog
            && dialog.pending
        {
            if let Some(error) = error {
                dialog.pending = false;
                dialog.focus = true;
                dialog.error = error.into();
            } else {
                self.dialog = None;
            }
        }
    }
    pub fn renamed_folder(&mut self, old: &str, new: &str) {
        let prefix = format!("{old}/");
        self.expanded = self
            .expanded
            .iter()
            .map(|path| {
                if path == old {
                    new.into()
                } else if let Some(suffix) = path.strip_prefix(&prefix) {
                    format!("{new}/{suffix}")
                } else {
                    path.clone()
                }
            })
            .collect();
    }
    pub fn expand_folder(&mut self, folder: &str) {
        let mut parent = String::new();
        for part in folder.split('/').filter(|part| !part.is_empty()) {
            if !parent.is_empty() {
                parent.push('/');
            }
            parent.push_str(part);
            self.expanded.insert(parent.clone());
        }
    }
    pub fn dirty(&self) -> bool {
        self.editor.as_ref().is_some_and(Editor::dirty)
    }
    pub fn close_note(&mut self) {
        self.editor = None;
        self.title_edit = None;
    }
    pub fn update_entries(&mut self, entries: Vec<Entry>) {
        self.entries = entries;
        let folders: BTreeSet<_> = self
            .entries
            .iter()
            .filter(|entry| entry.folder)
            .map(|entry| entry.path.as_str())
            .collect();
        self.expanded.retain(|path| folders.contains(path.as_str()));
    }
    pub fn show(&mut self, ui: &mut egui::Ui, preferences: &mut Preferences) -> Option<Action> {
        let location = self.media_root.as_ref().and_then(|root| {
            self.editor
                .as_ref()
                .map(|editor| amigo_ui::media::Location {
                    root: root.clone(),
                    base: root.join(
                        std::path::Path::new(&editor.document.path)
                            .parent()
                            .unwrap_or(std::path::Path::new("")),
                    ),
                })
        });
        amigo_ui::media::set_location(ui.ctx(), location);
        let mut action = None;
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
            .corner_radius(0)
            .inner_margin(0)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                ui.set_min_size(egui::vec2(width, height));
                ui.set_max_size(egui::vec2(width, height));
                action = self.note_header(ui, preferences).or(action.take());
                if self.editor.is_some() {
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
                        design::hint(ui, "Create a note or open an external Markdown file.");
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
        let has_note = self.editor.is_some();
        let path = self
            .editor
            .as_ref()
            .map(|editor| editor.document.path.clone())
            .unwrap_or_default();
        let filename = amigo_core::journal::entry_name(&path);
        let name = filename.strip_suffix(".md").unwrap_or(filename);
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 46.), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0, colors(ui).field);
        let show_edit = has_note && preferences.mode != Mode::Source;
        let controls_width = if show_edit { 176. } else { 140. };
        let controls_left = rect.right() - controls_width - 12.;
        let rename_controls_left = controls_left - 76.;
        let title_width = (width - 2. * (controls_width + 24.)).clamp(80., 500.);
        let mut title_center = rect.center();
        if self.title_edit.is_some() {
            title_center.x = title_center
                .x
                .min(rename_controls_left - 8. - title_width / 2.);
        }
        let title_rect = egui::Rect::from_center_size(title_center, egui::vec2(title_width, 34.));
        let mut action = None;
        let open_file =
            egui::Rect::from_min_size(rect.min + egui::vec2(12., 7.), egui::vec2(32., 32.));
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(open_file)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                if ui
                    .add_enabled_ui(!self.busy, |ui| {
                        design::icon_button(
                            ui,
                            egui::Id::new("journal-open-external"),
                            Icon::OpenFile,
                            "Open External Markdown File",
                            32.,
                        )
                    })
                    .inner
                    .clicked()
                {
                    action = Some(Action::OpenExternal);
                }
            },
        );
        let attach =
            egui::Rect::from_min_size(rect.min + egui::vec2(48., 7.), egui::vec2(32., 32.));
        ui.scope_builder(egui::UiBuilder::new().max_rect(attach), |ui| {
            if ui
                .add_enabled_ui(has_note && !self.busy && !is_in_trash(&path), |ui| {
                    design::icon_button(
                        ui,
                        egui::Id::new("journal-attach-media"),
                        Icon::Plus,
                        "Attach Photo or Video",
                        32.,
                    )
                })
                .inner
                .clicked()
            {
                action = Some(Action::AttachMedia);
            }
        });
        if let Some(edit) = &mut self.title_edit {
            let id = egui::Id::new("journal-title-edit");
            if edit.focus {
                let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(
                        egui::text::CCursor::new(edit.value.chars().count()),
                    )));
                state.store(ui.ctx(), id);
            }
            let response = ui.put(
                title_rect,
                design::input(&mut edit.value)
                    .id(id)
                    .frame(egui::Frame::NONE)
                    .min_size(egui::vec2(0., title_rect.height()))
                    .font(egui::FontId::proportional(20.))
                    .horizontal_align(egui::Align::Center)
                    .interactive(!self.loading),
            );
            if edit.focus {
                response.request_focus();
                edit.focus = false;
            }
            let valid = !edit.value.trim().is_empty() && !edit.value.contains(['/', '\\']);
            let mut save = false;
            let mut cancel = false;
            let controls = egui::Rect::from_min_size(
                egui::pos2(rename_controls_left, rect.top() + 7.),
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
        } else if has_note {
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
            egui::pos2(controls_left, rect.top() + 7.),
            egui::vec2(controls_width, 32.),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(controls)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 4.;
                if show_edit {
                    let editor = self.editor.as_mut().unwrap();
                    let editing = editor
                        .preview_editing
                        .unwrap_or(preferences.preview_editable);
                    if design::selected_icon_button(
                        ui,
                        egui::Id::new("journal-preview-edit"),
                        Icon::Edit,
                        if editing {
                            "Finish Editing Preview"
                        } else {
                            "Edit Preview"
                        },
                        32.,
                        editing,
                    )
                    .clicked()
                        && !self.loading
                    {
                        editor.preview_editing = Some(!editing);
                        editor.preview_tools = Default::default();
                        editor.preview = render::Preview::document(&editor.document.text);
                        editor.preview_revision = editor.revision;
                    }
                }
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
                design::menu(&menu).show(|ui| {
                    ui.set_min_width(220.);
                    design::eyebrow(ui, "PREVIEW LAYOUT");
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
                    design::eyebrow(ui, "MARKDOWN EDITOR");
                    design::checkbox(ui, &mut preferences.line_numbers, "Line Numbers");
                    design::checkbox(ui, &mut preferences.word_wrap, "Soft Wraps");
                    ui.separator();
                    design::eyebrow(ui, "SCROLLING");
                    design::checkbox(ui, &mut preferences.sync_scroll, "Synchronize Scrolling");
                });
            },
        );
        ui.advance_cursor_after_rect(rect);
        action
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
    editing::Prepared::new(text).text
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
