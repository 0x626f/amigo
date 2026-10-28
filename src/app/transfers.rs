//! File selection and archive work stay off the rendering thread.
use super::*;
use amigo_core::backup::{self, Archive, Scope};
use design::DataAction;

#[derive(Default)]
pub(super) struct Transfers {
    dialog: Option<TransferDialog>,
    job: Option<TransferJob>,
}

struct TransferDialog {
    scope: Scope,
    action: DataAction,
    name: String,
    path: String,
    reviewed: Option<(Archive, String)>,
    error: String,
}
struct TransferJob {
    receiver: mpsc::Receiver<Result<TransferResult, String>>,
}
enum TransferResult {
    Cancelled,
    Picked(Option<PathBuf>),
    Reviewed(Archive, String),
    SetupReviewed(Archive, String, PathBuf),
    Complete { reload: bool, message: String },
}
impl Transfers {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
}
impl Amigo {
    pub(super) fn start_setup_transfer(&mut self, action: DataAction, ctx: &egui::Context) {
        if self.transfers.busy() || self.vault_job.is_some() {
            return;
        }
        if self.tasks.file_operation_in_progress() {
            self.error =
                "A task file operation is still running. Try again when it finishes.".into();
            return;
        }
        if self.squidy.has_draft() {
            self.error =
                "Save or cancel the Squidy card or connection before transferring the setup."
                    .into();
            return;
        }
        if self.squidy_io.blocked && self.squidy_dirty() {
            self.error = self.squidy.error.clone();
            return;
        }
        if self.squidy_dirty() || self.squidy_busy() {
            self.squidy_io.pending_setup = Some(action);
            self.squidy.changed_at = Instant::now() - Duration::from_secs(1);
            return;
        }
        if self.journal.dirty() || self.journal.busy {
            self.journal_io.pending_setup = Some(action);
            return;
        }
        if !self.save_local() {
            return;
        }
        self.open_data_dialog(Scope::Setup, action);
        let library = self.library.clone();
        self.transfer_job(ctx, move || {
            let picker = rfd::FileDialog::new().add_filter("Amigo files", &["amigo"]);
            let path = match action {
                DataAction::Export => picker
                    .set_title("Export Amigo Setup")
                    .set_file_name("amigo-setup.amigo")
                    .save_file(),
                _ => picker.set_title("Import Amigo Setup").pick_file(),
            };
            let Some(mut path) = path else {
                return Ok(TransferResult::Cancelled);
            };
            if action == DataAction::Export {
                if path.extension().is_none() {
                    path.set_extension("amigo");
                }
                let archive = Archive::capture(&library, Scope::Setup)?;
                archive.review(&library)?;
                archive.write(&path, &library.root)?;
                Ok(TransferResult::Complete {
                    reload: false,
                    message: format!("Exported {}", path.display()),
                })
            } else {
                let archive = Archive::read(&path)?;
                if archive.scope != Scope::Setup {
                    return Err("Choose an Amigo Setup file.".into());
                }
                let summary = archive.review(&library)?;
                Ok(TransferResult::SetupReviewed(archive, summary, path))
            }
        });
    }
    pub(super) fn handle_tasks_action(&mut self, action: TasksAction) {
        let data = match action {
            TasksAction::RenameBoardRequested => {
                self.prompt_board_rename();
                return;
            }
            TasksAction::RenameBoard(name) => {
                if let Err(error) = self
                    .library
                    .rename_board(&self.library.settings.board.clone(), &name)
                {
                    self.error = error;
                }
                return;
            }
            TasksAction::ImportBoard => DataAction::Import,
            TasksAction::ExportBoard => DataAction::Export,
            TasksAction::DeleteBoard => DataAction::Delete,
        };
        self.open_data_dialog(Scope::Board(self.library.settings.board.clone()), data);
    }
    pub(super) fn open_data_dialog(&mut self, scope: Scope, action: DataAction) {
        if self.transfers.busy() || self.vault_job.is_some() {
            return;
        }
        let name = match &scope {
            Scope::Board(id) => self
                .library
                .boards
                .iter()
                .find(|b| b.id == *id)
                .map(|b| b.name.clone())
                .unwrap_or_default(),
            Scope::Vault(id) => self
                .library
                .vaults
                .iter()
                .find(|v| v.entry.id == *id)
                .map(|v| v.entry.name.clone())
                .unwrap_or_default(),
            Scope::Calendar => self.organizer.name.clone(),
            _ => scope.label().into(),
        };
        self.transfers.dialog = Some(TransferDialog {
            scope,
            action,
            name,
            path: String::new(),
            reviewed: None,
            error: String::new(),
        });
    }
    fn transfer_job(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce() -> Result<TransferResult, String> + Send + 'static,
    ) {
        let (sender, receiver) = mpsc::channel();
        self.transfers.job = Some(TransferJob { receiver });
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(work());
            ctx.request_repaint();
        });
    }
    fn transfer_result(&mut self, result: Result<TransferResult, String>) {
        match result {
            Ok(TransferResult::Cancelled) => self.transfers.dialog = None,
            Ok(TransferResult::SetupReviewed(archive, summary, path)) => {
                if let Some(dialog) = &mut self.transfers.dialog {
                    dialog.path = path.to_string_lossy().into_owned();
                    dialog.reviewed = Some((archive, summary));
                    dialog.error.clear();
                }
            }
            Ok(TransferResult::Picked(Some(path))) => {
                if let Some(dialog) = &mut self.transfers.dialog {
                    dialog.path = path.to_string_lossy().into_owned();
                    dialog.reviewed = None;
                    dialog.error.clear();
                }
            }
            Ok(TransferResult::Picked(None)) => (),
            Ok(TransferResult::Reviewed(archive, summary)) => {
                if let Some(dialog) = &mut self.transfers.dialog {
                    dialog.reviewed = Some((archive, summary));
                    dialog.error.clear();
                }
            }
            Ok(TransferResult::Complete { reload, message }) => {
                if reload {
                    let section = self.section;
                    let notifications = std::mem::take(&mut self.notifications);
                    let updates = std::mem::take(&mut self.updates);
                    let app = Self::load_library(self.data_path.clone(), None);
                    *self = app;
                    self.updates = updates;
                    self.section = section;
                    self.notifications = notifications;
                }
                self.transfers.dialog = None;
                self.message = message;
            }
            Err(error) => {
                if self
                    .transfers
                    .dialog
                    .as_ref()
                    .is_some_and(|dialog| dialog.scope == Scope::Setup && dialog.reviewed.is_none())
                {
                    self.transfers.dialog = None;
                    self.error = error;
                    return;
                }
                if let Some(dialog) = &mut self.transfers.dialog {
                    dialog.error = error;
                } else {
                    self.error = error;
                }
            }
        }
    }
    pub(super) fn poll_transfer(&mut self) {
        let Some(job) = self.transfers.job.take() else {
            return;
        };
        match job.receiver.try_recv() {
            Ok(result) => self.transfer_result(result),
            Err(mpsc::TryRecvError::Empty) => self.transfers.job = Some(job),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.transfer_result(Err("The file worker stopped. Try again.".into()))
            }
        }
    }
    pub(super) fn finish_transfer(&mut self) {
        if let Some(job) = self.transfers.job.take() {
            let result = job
                .receiver
                .recv()
                .unwrap_or_else(|_| Err("The file worker stopped.".into()));
            self.transfer_result(result);
        }
    }
    pub(super) fn transfer_ui(&mut self, ctx: &egui::Context) {
        if self.transfers.busy() {
            return;
        }
        let Some(mut dialog) = self.transfers.dialog.take() else {
            return;
        };
        let keys = design::ActionKeys::capture(ctx);
        let mut close = false;
        let mut browse = false;
        let mut submit = false;
        let ready = dialog.action != DataAction::Import || dialog.reviewed.is_some();
        let verb = match dialog.action {
            DataAction::Import => "Import",
            DataAction::Export => "Export",
            DataAction::Delete => "Delete",
        };
        let submit_label = if !ready {
            "Review Import".into()
        } else if dialog.scope == Scope::Setup && dialog.action == DataAction::Import {
            "Replace All Data".into()
        } else {
            format!("{verb} {}", dialog.scope.label())
        };
        let valid = dialog.action == DataAction::Delete || !dialog.path.trim().is_empty();
        let modal = design::modal(ctx, egui::Id::new("data-transfer-dialog")).show(ctx, |ui| {
            ui.set_width(design::dialog_width(ctx, 640.0));
            close = design::settings_heading(ui, &format!("{verb} {}", dialog.scope.label())).clicked();
            design::dialog_body(ui, "data-transfer-body", |ui| {
            ui.label(RichText::new(&dialog.name).strong());
            match dialog.action {
                DataAction::Import => {
                    hint(ui, match dialog.scope {
                        Scope::Setup => "This replaces ALL existing boards, vaults, calendars, attachments, and app settings. Export your current setup first if you want to keep it.",
                        Scope::Calendar => "This replaces Calendar events, subscriptions, and Calendar settings. Other app data and notification credentials stay in place.",
                        _ => "This adds the imported item, or replaces the matching item if it already exists. Other boards and vaults stay in place.",
                    });
                    if let Some((_, summary)) = &dialog.reviewed { ui.add_space(10.0); ui.label(RichText::new(summary).strong()); }
                }
                DataAction::Export => {
                    hint(ui, "Save a portable .amigo file. Password protected vaults retain their encryption.");
                    if matches!(dialog.scope, Scope::Calendar | Scope::Setup) { hint(ui, "This file includes connection credentials. Keep it private."); }
                }
                DataAction::Delete => {
                    hint(ui, "Delete this item and all of its data permanently? This cannot be undone.");
                    if matches!(dialog.scope, Scope::Board(_)) { hint(ui, "Task attachments are also deleted. If this is your last board, an empty Personal board is created."); }
                    else if dialog.scope==Scope::Calendar{hint(ui,"Calendar events and subscriptions are deleted. Other app data and Telegram credentials stay in place.");}
                    else { hint(ui, "If this is your last vault, an empty Personal vault is created."); }
                }
            }
            if dialog.action != DataAction::Delete && !(dialog.scope == Scope::Setup && dialog.reviewed.is_some()) {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let response = ui.add(design::input(&mut dialog.path).id(egui::Id::new("data-transfer-path")).hint_text("Path to an Amigo file (.amigo)").desired_width((ui.available_width() - 120.0).max(100.0)));
                    if response.changed() { dialog.reviewed = None; dialog.error.clear(); }
                    browse = design::button(ui, "Browse…").clicked();
                });
            }
            if !dialog.error.is_empty() { ui.colored_label(colors(ui).red, &dialog.error); }
            });
            design::dialog_footer(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    submit = ui.add_enabled_ui(valid, |ui| {
                        if dialog.action == DataAction::Delete || (dialog.action == DataAction::Import && ready) {
                            design::dialog_destructive(ui, &submit_label)
                        } else {
                            design::dialog_primary(ui, &submit_label)
                        }
                    }).inner.clicked();
                    close |= design::button(ui, "Cancel").clicked();
                });
            });
        });
        // Destructive confirmation requires clicking its explicit button; Enter cannot replace data.
        let shortcut = keys.dialog(
            &modal,
            valid && (dialog.action == DataAction::Export || !ready),
        );
        close |= shortcut.cancel;
        submit |= shortcut.confirm;
        if close {
            return;
        }
        let scope = dialog.scope.clone();
        let action = dialog.action;
        let path = PathBuf::from(dialog.path.trim());
        let reviewed = if submit && ready {
            dialog.reviewed.take()
        } else {
            None
        };
        self.transfers.dialog = Some(dialog);
        if browse {
            self.transfer_job(ctx, move || {
                let picker = rfd::FileDialog::new()
                    .add_filter("Amigo files", &["amigo"])
                    .set_title(format!("{verb} {}", scope.label()));
                let selected = if action == DataAction::Export {
                    picker
                        .set_file_name(format!(
                            "{}.amigo",
                            scope.label().to_lowercase().replace(' ', "-")
                        ))
                        .save_file()
                } else {
                    picker.pick_file()
                };
                Ok(TransferResult::Picked(selected))
            });
        } else if submit && valid {
            if self.tasks.file_operation_in_progress() {
                self.transfer_result(Err(
                    "A task file operation is still running. Try again when it finishes.".into(),
                ));
                return;
            }
            if !self.save_local() {
                let error = self.error.clone();
                self.transfer_result(Err(error));
                return;
            }
            let library = self.library.clone();
            if action == DataAction::Import && !ready {
                self.transfer_job(ctx, move || {
                    let archive = Archive::read(&path)?;
                    if !archive.scope.same_kind(&scope) {
                        return Err(format!("Choose an Amigo {} file.", scope.label()));
                    }
                    let summary = archive.review(&library)?;
                    Ok(TransferResult::Reviewed(archive, summary))
                });
            } else {
                self.finance.cancel_rates();
                self.pause_calendar_connections();
                self.page_loader.cancel();
                self.transfer_job(ctx, move || match action {
                    DataAction::Export => {
                        let mut path = path;
                        if path.extension().is_none() {
                            path.set_extension("amigo");
                        }
                        let archive = Archive::capture(&library, scope)?;
                        archive.review(&library)?;
                        archive.write(&path, &library.root)?;
                        Ok(TransferResult::Complete {
                            reload: false,
                            message: format!("Exported {}", path.display()),
                        })
                    }
                    DataAction::Import => {
                        let archive = reviewed
                            .ok_or("Review the Amigo file before importing it.")?
                            .0;
                        archive.import(&library)?;
                        Ok(TransferResult::Complete {
                            reload: true,
                            message: format!("{} imported.", scope.label()),
                        })
                    }
                    DataAction::Delete => {
                        backup::delete(&library, &scope)?;
                        Ok(TransferResult::Complete {
                            reload: true,
                            message: format!("{} deleted.", scope.label()),
                        })
                    }
                });
            }
        }
    }
}

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
