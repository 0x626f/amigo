//! Serialized graph storage and attachment jobs; graph edits remain responsive during saves.
use super::*;
use amigo_core::{
    attachments::Batch,
    squidy::{Board, Entry, Store},
};
use amigo_squidy::Action;
use std::collections::VecDeque;
#[derive(Default)]
pub(super) struct SquidyIo {
    initialized: bool,
    pub(super) saved: Option<Board>,
    fingerprint: String,
    job: Option<mpsc::Receiver<Result<Outcome, String>>>,
    pub(super) pending: VecDeque<Action>,
    pub(super) blocked: bool,
    saving: bool,
    pub(super) pending_setup: Option<design::DataAction>,
}
enum Outcome {
    Loaded(Board, Vec<Entry>, PathBuf, String),
    Saved(Board, Vec<Entry>, String),
    Attached(String, Batch),
    Done,
}
impl Amigo {
    pub(super) fn squidy_dirty(&self) -> bool {
        self.squidy
            .board
            .as_ref()
            .is_some_and(|b| Some(b) != self.squidy_io.saved.as_ref())
    }
    pub(super) fn squidy_busy(&self) -> bool {
        self.squidy_io.job.is_some() || !self.squidy_io.pending.is_empty()
    }
    pub(super) fn squidy_action(&mut self, action: Action, ctx: &egui::Context) {
        if self.squidy_io.blocked && self.squidy_dirty() && !matches!(action, Action::Refresh) {
            self.error="Resolve the Squidy save conflict before switching boards or starting another operation.".into();
            return;
        }
        self.squidy_io.pending.push_back(action);
        self.tick_squidy(ctx);
    }
    fn squidy_job(
        &mut self,
        ctx: &egui::Context,
        exclusive: bool,
        work: impl FnOnce(Store) -> Result<Outcome, String> + Send + 'static,
    ) {
        let root = self.data_path.clone();
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        self.squidy_io.job = Some(receiver);
        self.squidy_io.saving = !exclusive;
        self.squidy.busy = true;
        self.squidy.loading = exclusive;
        std::thread::spawn(move || {
            let result = Store::open(&root).and_then(work);
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
    fn squidy_result(&mut self, result: Result<Outcome, String>) {
        self.squidy.busy = false;
        self.squidy.loading = false;
        match result {
            Ok(Outcome::Loaded(board, entries, folder, fp)) => {
                self.library.settings.squidy_board = board.id.clone();
                if let Err(error) = self.library.save_settings() {
                    self.error = error;
                }
                self.squidy_io.saved = Some(board.clone());
                self.squidy_io.fingerprint = fp;
                self.squidy_io.initialized = true;
                self.squidy_io.blocked = false;
                self.squidy.loaded(board, entries, folder);
            }
            Ok(Outcome::Saved(board, entries, fp)) => {
                self.squidy_io.saved = Some(board);
                self.squidy_io.fingerprint = fp;
                self.squidy.boards = entries;
                self.squidy.error.clear();
            }
            Ok(Outcome::Attached(id, batch)) => self.squidy.attachment_result(&id, batch),
            Ok(Outcome::Done) => (),
            Err(error) => {
                if let Some(editor) = &mut self.squidy.editor {
                    editor.error = error.clone();
                }
                self.squidy.error = error;
                self.squidy_io.blocked |= self.squidy_io.saving || !self.squidy_io.initialized;
                self.squidy_io.pending.clear();
            }
        }
    }
    pub(super) fn tick_squidy(&mut self, ctx: &egui::Context) {
        if self.save_blocked {
            return;
        }
        if let Some(receiver) = self.squidy_io.job.take() {
            match receiver.try_recv() {
                Ok(result) => self.squidy_result(result),
                Err(mpsc::TryRecvError::Empty) => self.squidy_io.job = Some(receiver),
                Err(mpsc::TryRecvError::Disconnected) => self.squidy_result(Err(
                    "The Squidy worker stopped; your changes are preserved.".into(),
                )),
            }
        }
        if self.squidy_io.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(40));
            return;
        }
        if self.section == Section::Squidy
            && !self.squidy_io.initialized
            && !self.squidy_io.blocked
            && self.squidy_io.pending.is_empty()
        {
            self.squidy_io
                .pending
                .push_back(Action::Select(self.library.settings.squidy_board.clone()));
        }
        let refresh = self
            .squidy_io
            .pending
            .front()
            .is_some_and(|a| matches!(a, Action::Refresh));
        if !refresh && !self.squidy_io.blocked && self.squidy_dirty() {
            if self.squidy.changed_at.elapsed() >= Duration::from_millis(700)
                || !self.squidy_io.pending.is_empty()
            {
                let snapshot = self.squidy.board.as_ref().unwrap().clone();
                let expected = self.squidy_io.fingerprint.clone();
                self.squidy_job(ctx, false, move |store| {
                    let fp = store.save(&snapshot, Some(&expected))?;
                    Ok(Outcome::Saved(snapshot, store.entries()?, fp))
                });
                return;
            }
            ctx.request_repaint_after(Duration::from_millis(700));
        }
        let Some(action) = self.squidy_io.pending.pop_front() else {
            if !self.squidy_dirty()
                && let Some(action) = self.squidy_io.pending_setup.take()
            {
                self.start_setup_transfer(action, ctx);
            }
            return;
        };
        if matches!(&action, Action::Rename(_)) {
            if let Action::Rename(name) = action {
                match amigo_core::library::name(&name) {
                    Ok(name) => {
                        let Some(board) = &mut self.squidy.board else {
                            return;
                        };
                        if self.squidy.boards.iter().any(|entry| {
                            entry.id != board.id && entry.name.to_lowercase() == name.to_lowercase()
                        }) {
                            self.squidy.error = "A board with this name already exists.".into();
                            return;
                        }
                        board.name = name;
                        self.squidy.changed_at = Instant::now() - Duration::from_secs(1);
                        self.tick_squidy(ctx);
                    }
                    Err(error) => self.squidy.error = error,
                }
            }
            return;
        }
        let selected = self
            .squidy
            .board
            .as_ref()
            .map(|b| b.id.clone())
            .unwrap_or_default();
        let attached = self.squidy.editor.as_ref().map(|e| e.card.id.clone());
        self.squidy_job(ctx, true, move |store| {
            let load = |id: &str| -> Result<Outcome, String> {
                let mut entries = store.entries()?;
                if entries.is_empty() {
                    store.create("Personal")?;
                    entries = store.entries()?;
                }
                let id = entries
                    .iter()
                    .find(|e| e.id == id)
                    .unwrap_or(&entries[0])
                    .id
                    .clone();
                let (board, fp) = store.load(&id)?;
                Ok(Outcome::Loaded(board, entries, store.folder(&id)?, fp))
            };
            match action {
                Action::Select(id) => load(&id),
                Action::Refresh => load(&selected),
                Action::Create(name) => {
                    let board = store.create(&name)?;
                    load(&board.id)
                }
                Action::Delete(id) => {
                    store.delete(&id)?;
                    load("")
                }
                Action::Attach(card) if attached.as_deref() == Some(&card) => {
                    let paths = rfd::FileDialog::new()
                        .set_title("Attach Files to Squidy Card")
                        .pick_files()
                        .unwrap_or_default();
                    Ok(Outcome::Attached(
                        card,
                        Batch::import(store.folder(&selected)?, &paths)?,
                    ))
                }
                Action::OpenFile(file) => {
                    let path = amigo_core::attachments::resolve(&store.folder(&selected)?, &file)?;
                    amigo_tasks::open_attachment(&path)?;
                    Ok(Outcome::Done)
                }
                _ => Ok(Outcome::Done),
            }
        });
    }
    pub(super) fn flush_squidy(&mut self) -> bool {
        if let Some(receiver) = self.squidy_io.job.take() {
            match receiver.recv() {
                Ok(result) => self.squidy_result(result),
                Err(_) => return false,
            }
        }
        if !self.squidy_dirty() {
            return true;
        }
        if self.squidy_io.blocked {
            return false;
        }
        let snapshot = self.squidy.board.as_ref().unwrap().clone();
        self.squidy_io.saving = true;
        let result = Store::open(&self.data_path).and_then(|store| {
            let fp = store.save(&snapshot, Some(&self.squidy_io.fingerprint))?;
            Ok(Outcome::Saved(snapshot, store.entries()?, fp))
        });
        let success = result.is_ok();
        self.squidy_result(result);
        success
    }
}
#[cfg(test)]
#[path = "squidy_tests.rs"]
mod tests;
