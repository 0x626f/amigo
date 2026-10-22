//! The Tasks tab: Kanban, filtering, task editing, and board settings.
use amigo_core::model::*;
use amigo_ui::design::{panel, primary};
use amigo_ui::{
    design::{self, Icon, colors},
    forms::*,
};
use egui::{self, RichText, Stroke, Vec2};
use std::collections::BTreeSet;
mod attachments;
mod board;
mod cards;
mod editor;
mod settings;
use settings::{BoardEditor, status_color};

struct TaskEditor {
    draft: Task,
    is_new: bool,
    delete_confirm: bool,
    focus_requested: bool,
    pending_attachments: Vec<std::path::PathBuf>,
    attachment_error: String,
    imported_attachments: Vec<attachments::AttachmentBatch>,
}
/// Tab-specific transient state. Persisted board data belongs to the shared workspace.
#[derive(Default)]
pub struct TasksTab {
    search: String,
    search_control: design::SearchField,
    priorities: BTreeSet<String>,
    categories: BTreeSet<String>,
    filter_snapshot: Option<(BTreeSet<String>, BTreeSet<String>, String)>,
    editor: Option<TaskEditor>,
    board_editor: Option<BoardEditor>,
    board_name: String,
    cards: cards::CardInteraction,
    board_folder: Option<std::path::PathBuf>,
    attachment_job: Option<attachments::FileJob>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum TasksAction {
    RenameBoard(String),
    ImportBoard,
    ExportBoard,
    DeleteBoard,
}
impl TasksTab {
    pub fn file_operation_in_progress(&mut self) -> bool {
        attachments::busy(self)
    }
    pub fn set_board_folder(&mut self, folder: Option<std::path::PathBuf>) {
        self.board_folder = folder;
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn cancel_card_interaction(&mut self) {
        self.cards.clear();
    }
    pub fn show(&mut self, workspace: &mut Workspace, board_name: &str, ui: &mut egui::Ui) {
        self.board_name = board_name.to_owned();
        let mut view = TasksUi {
            state: self,
            workspace,
            board_name,
        };
        view.tasks(ui);
    }
    pub fn manage_board(&mut self, workspace: &Workspace) {
        self.cards.clear();
        self.editor = None;
        self.board_editor = Some(BoardEditor::new(workspace));
    }
    pub fn new_task(&mut self, workspace: &Workspace, status: &str) {
        self.cards.clear();
        let status = if workspace.task_statuses.iter().any(|s| s.id == status) {
            status
        } else {
            workspace.default_task_status()
        };
        let mut draft = Task::new(workspace.next_task, status);
        if self.categories.len() == 1 {
            draft.category_id = self.categories.first().unwrap().clone();
        }
        self.editor = Some(TaskEditor {
            draft,
            is_new: true,
            delete_confirm: false,
            focus_requested: true,
            pending_attachments: vec![],
            attachment_error: String::new(),
            imported_attachments: vec![],
        });
    }
    pub fn edit_task(&mut self, task: Task) {
        self.cards.clear();
        self.editor = Some(TaskEditor {
            draft: task,
            is_new: false,
            delete_confirm: false,
            focus_requested: true,
            pending_attachments: vec![],
            attachment_error: String::new(),
            imported_attachments: vec![],
        });
    }
    fn task_matches_filters(&self, workspace: &Workspace, task: &Task) -> bool {
        let category_matches =
            self.categories.is_empty() || self.categories.contains(&task.category_id);
        let category_name = workspace
            .task_categories
            .iter()
            .find(|c| c.id == task.category_id)
            .map(|c| c.name.as_str())
            .unwrap_or("");
        let query = self.search.trim().to_lowercase();
        category_matches
            && (self.priorities.is_empty() || self.priorities.contains(&task.priority))
            && (query.is_empty()
                || format!("{} {} {}", task.title, task.description, category_name)
                    .to_lowercase()
                    .contains(&query))
    }

    pub fn dialogs(
        &mut self,
        workspace: &mut Workspace,
        ctx: &egui::Context,
    ) -> Option<TasksAction> {
        TasksUi {
            state: self,
            workspace,
            board_name: "",
        }
        .task_dialog(ctx);
        let outcome = settings::show(&mut self.board_editor, workspace, &self.board_name, ctx);
        if outcome.saved {
            self.editor = None;
            self.categories.retain(|id| {
                id.is_empty() || workspace.task_categories.iter().any(|c| c.id == *id)
            });
        }
        outcome
            .data_action
            .map(|action| match action {
                design::DataAction::Import => TasksAction::ImportBoard,
                design::DataAction::Export => TasksAction::ExportBoard,
                design::DataAction::Delete => TasksAction::DeleteBoard,
            })
            .or_else(|| outcome.rename.map(TasksAction::RenameBoard))
    }
}
struct TasksUi<'a> {
    state: &'a mut TasksTab,
    workspace: &'a mut Workspace,
    board_name: &'a str,
}
#[cfg(test)]
mod tests;
