//! Squidy research boards and graph interaction.
use amigo_core::{
    attachments::Batch,
    model::TaskAttachment,
    squidy::{Board, Card, Entry, GraphPath, Link},
};
use amigo_ui::design::{self, Icon};
use egui::{self, RichText};
use std::{collections::BTreeMap, path::PathBuf, time::Instant};
mod canvas;
mod controls;
mod dialogs;
mod path_result;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub enum Action {
    Select(String),
    Create(String),
    Rename(String),
    Delete(String),
    Refresh,
    Attach(String),
    OpenFile(TaskAttachment),
}
pub struct CardDraft {
    pub card: Card,
    pub original: Option<String>,
    pub error: String,
    pub batches: Vec<Batch>,
    pub focus: bool,
    pub data_editor: amigo_ui::markdown::InlineEditor,
}
#[derive(Clone)]
enum Picking {
    Connection(Option<String>),
    Path(Option<String>),
}
enum NameDialog {
    Create(String),
    Rename(String),
    Refresh,
}
enum Delete {
    Card(String),
    Link(String),
    Board(String),
}
struct Query {
    from: String,
    to: String,
    include: String,
    exclude: String,
    directed: bool,
}
impl Default for Query {
    fn default() -> Self {
        Self {
            from: String::new(),
            to: String::new(),
            include: String::new(),
            exclude: String::new(),
            directed: true,
        }
    }
}
pub struct SquidyTab {
    pub board: Option<Board>,
    pub boards: Vec<Entry>,
    pub busy: bool,
    pub loading: bool,
    pub error: String,
    pub root: Option<PathBuf>,
    pub changed_at: Instant,
    pub editor: Option<CardDraft>,
    link: Option<Link>,
    link_focus: bool,
    name_dialog: Option<NameDialog>,
    name_focus: bool,
    delete: Option<Delete>,
    delete_armed: bool,
    query: Query,
    path_open: bool,
    picking: Option<Picking>,
    pub path: Option<GraphPath>,
    pub path_message: String,
    positions: BTreeMap<String, [f32; 2]>,
    layout_dirty: bool,
    selected: Option<String>,
    drag: Option<(String, egui::Vec2)>,
    viewport: egui::Rect,
    action: Option<Action>,
}
impl Default for SquidyTab {
    fn default() -> Self {
        Self {
            board: None,
            boards: vec![],
            busy: false,
            loading: false,
            error: String::new(),
            root: None,
            changed_at: Instant::now(),
            editor: None,
            link: None,
            link_focus: false,
            name_dialog: None,
            name_focus: false,
            delete: None,
            delete_armed: false,
            query: Query::default(),
            path_open: false,
            picking: None,
            path: None,
            path_message: String::new(),
            positions: BTreeMap::new(),
            layout_dirty: true,
            selected: None,
            drag: None,
            viewport: egui::Rect::NOTHING,
            action: None,
        }
    }
}
impl SquidyTab {
    pub fn loaded(&mut self, board: Board, boards: Vec<Entry>, root: PathBuf) {
        self.board = Some(board);
        self.boards = boards;
        self.root = Some(root);
        self.editor = None;
        self.link = None;
        self.layout_dirty = true;
        self.selected = None;
        self.picking = None;
        self.path_open = false;
        self.path = None;
        self.path_message.clear();
        self.error.clear();
    }
    pub fn has_draft(&self) -> bool {
        self.editor.is_some() || self.link.is_some()
    }
    fn modal_open(&self) -> bool {
        self.has_draft() || self.name_dialog.is_some() || self.delete.is_some() || self.path_open
    }
    fn changed(&mut self) {
        self.changed_at = Instant::now();
    }
    fn contents_changed(&mut self) {
        self.changed();
        self.layout_dirty = true;
        self.path = None;
        self.path_message.clear();
    }
    fn confirm_delete(&mut self, delete: Delete) {
        self.delete = Some(delete);
        self.delete_armed = false;
    }
    pub fn attachment_result(&mut self, id: &str, mut batch: Batch) {
        let Some(editor) = self.editor.as_mut().filter(|e| e.card.id == id) else {
            return;
        };
        for file in &batch.files {
            if amigo_core::media::kind(&file.name).is_some() {
                editor.card.data.push_str(&format!(
                    "\n\n{}\n",
                    amigo_core::media::embed(&file.name, &amigo_core::media::target(file, 2))
                ));
            }
            editor.card.attachments.push(file.clone());
        }
        if batch.files.is_empty() {
            batch.keep();
        } else {
            editor.batches.push(batch);
        }
    }
    pub fn sidebar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let blocked = self.modal_open() || self.loading;
        ui.add_enabled_ui(!blocked, |ui| {
            ui.horizontal(|ui| {
                design::eyebrow(ui, "BOARDS");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if design::icon_button(
                        ui,
                        egui::Id::new("squidy-new-board"),
                        Icon::Plus,
                        "New Squidy Board",
                        28.,
                    )
                    .clicked()
                    {
                        self.name_dialog = Some(NameDialog::Create(String::new()));
                        self.name_focus = true;
                    }
                });
            });
            for entry in &self.boards {
                if design::nav_row(
                    ui,
                    &entry.name,
                    Icon::Link,
                    self.board
                        .as_ref()
                        .is_some_and(|board| board.id == entry.id),
                    None,
                )
                .clicked()
                {
                    self.action = Some(Action::Select(entry.id.clone()));
                }
            }
            if let Some(board) = &self.board {
                ui.add_space(12.);
                if design::button(ui, "Board Options").clicked() {
                    self.name_dialog = Some(NameDialog::Rename(board.name.clone()));
                    self.name_focus = true;
                }
            }
        });
        self.action.take()
    }
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        if self.board.is_none() {
            if self.loading {
                ui.spinner();
                ui.label("Opening Squidy boards…");
            }
            if !self.error.is_empty() {
                ui.colored_label(design::colors(ui).red, &self.error);
                if design::button(ui, "Retry").clicked() {
                    return Some(Action::Refresh);
                }
            }
            return None;
        }
        if self.layout_dirty {
            self.positions = self.board.as_ref().unwrap().positions();
            self.layout_dirty = false;
        }
        if ui.is_enabled()
            && !self.modal_open()
            && !self.loading
            && !egui::Popup::is_any_open(ui.ctx())
            && ui.memory(|memory| memory.top_modal_layer().is_none())
        {
            if !ui.ctx().text_edit_focused()
                && ui.input_mut(|i| {
                    // Match the physical key too: Alt/Option and keyboard layouts can
                    // change the logical character reported by the native backend.
                    let mut pressed = false;
                    i.events.retain(|event| {
                        let matches = matches!(event, egui::Event::Key {
                            key, physical_key, pressed: true, modifiers, ..
                        } if (*key == egui::Key::F || *physical_key == Some(egui::Key::F))
                            && modifiers.alt && !modifiers.ctrl && !modifiers.command
                            && !modifiers.shift && !modifiers.mac_cmd);
                        if matches && let egui::Event::Key { repeat, .. } = event {
                            pressed |= !repeat;
                        }
                        !matches
                    });
                    pressed
                })
            {
                self.begin_path_selection();
            }
            if self.picking.is_some()
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                self.picking = None;
            }
        }
        let blocked = self.modal_open() || self.loading;
        ui.add_enabled_ui(!blocked, |ui| {
            if !self.error.is_empty() {
                ui.colored_label(design::colors(ui).red, &self.error);
            }
            self.path_result(ui);
            self.canvas(ui);
        });
        self.action.take()
    }
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            if design::icon_button(
                ui,
                egui::Id::new("squidy-new-card"),
                Icon::Plus,
                "New Card",
                32.,
            )
            .clicked()
            {
                self.new_card(None);
            }
            ui.add_enabled_ui(!self.board.as_ref().unwrap().cards.is_empty(), |ui| {
                let connecting = matches!(self.picking, Some(Picking::Connection(_)));
                if design::selected_icon_button(
                    ui,
                    egui::Id::new("squidy-link-cards"),
                    Icon::Link,
                    "Link Cards: select two cards (Esc to cancel)",
                    32.,
                    connecting,
                )
                .clicked()
                {
                    self.picking = if connecting {
                        None
                    } else {
                        Some(Picking::Connection(None))
                    };
                    self.drag = None;
                    self.error.clear();
                }
                let finding = matches!(self.picking, Some(Picking::Path(_)));
                if design::selected_icon_button(
                    ui,
                    egui::Id::new("squidy-find-path-open"),
                    Icon::Route,
                    "Find Path (Alt+F): select two cards (Esc to cancel)",
                    32.,
                    finding,
                )
                .clicked()
                {
                    if finding {
                        self.picking = None;
                    } else {
                        self.begin_path_selection();
                    }
                }
            });
        });
    }
    fn board_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 180.).max(0.);
            ui.add_sized(
                [width, 32.],
                egui::Label::new(
                    RichText::new(&self.board.as_ref().unwrap().name)
                        .size(design::PAGE_TITLE_SIZE)
                        .strong(),
                )
                .truncate(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if design::icon_button(
                    ui,
                    egui::Id::new("squidy-refresh"),
                    Icon::Refresh,
                    "Refresh Squidy Board",
                    28.,
                )
                .clicked()
                {
                    self.name_dialog = Some(NameDialog::Refresh);
                }
                if design::small_button(ui, "+")
                    .on_hover_text("Zoom In")
                    .clicked()
                {
                    self.zoom(1.25, self.viewport.center());
                }
                ui.label(format!("{:.0}%", self.board.as_ref().unwrap().zoom * 100.));
                if design::small_button(ui, "−")
                    .on_hover_text("Zoom Out")
                    .clicked()
                {
                    self.zoom(0.8, self.viewport.center());
                }
            });
        });
    }
    pub fn dialogs(&mut self, ctx: &egui::Context) -> Option<Action> {
        if self.delete.is_none() {
            self.card_dialog(ctx);
            self.link_dialog(ctx);
            self.name_dialog(ctx);
            self.path_dialog(ctx);
        }
        self.delete_dialog(ctx);
        self.action.take()
    }
    pub fn new_card(&mut self, position: Option<[f32; 2]>) {
        self.picking = None;
        let mut card = Card::new("");
        card.position = position;
        self.editor = Some(CardDraft {
            card,
            original: None,
            error: String::new(),
            batches: vec![],
            focus: true,
            data_editor: Default::default(),
        });
    }
    pub fn edit_card(&mut self, id: &str) {
        if let Some(card) = self
            .board
            .as_ref()
            .and_then(|b| b.cards.iter().find(|c| c.id == id))
        {
            self.editor = Some(CardDraft {
                card: card.clone(),
                original: Some(id.into()),
                error: String::new(),
                batches: vec![],
                focus: true,
                data_editor: Default::default(),
            });
        }
    }
    fn new_link(&mut self, from: &str, to: &str) {
        self.link_focus = true;
        self.picking = None;
        self.error.clear();
        self.link = Some(Link::new(from, to, ""));
    }
    fn begin_path_selection(&mut self) {
        self.picking = Some(Picking::Path(None));
        self.drag = None;
        self.error.clear();
    }
    fn picked_card(&mut self, name: String, alt: bool) {
        self.drag = None;
        match self.picking.take() {
            Some(Picking::Path(None)) => self.picking = Some(Picking::Path(Some(name))),
            Some(Picking::Path(Some(from))) => {
                self.query.from = from;
                self.query.to = name;
                self.path_open = true;
                self.error.clear();
            }
            Some(Picking::Connection(None)) => self.picking = Some(Picking::Connection(Some(name))),
            Some(Picking::Connection(Some(from))) => self.new_link(&from, &name),
            None if alt => self.picking = Some(Picking::Connection(Some(name))),
            None => (),
        }
    }
    fn zoom(&mut self, factor: f32, anchor: egui::Pos2) {
        let board = self.board.as_mut().unwrap();
        let zoom = (board.zoom * factor).clamp(0.15, 3.);
        let local = anchor - self.viewport.min;
        let world = (local - egui::vec2(board.pan[0], board.pan[1])) / board.zoom;
        let pan = local - world * zoom;
        board.pan = [pan.x, pan.y];
        board.zoom = zoom;
        self.changed();
    }
}
