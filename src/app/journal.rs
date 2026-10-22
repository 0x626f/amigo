//! Serialized notebook file jobs; the editor remains interactive while autosaving.
use super::*;
use amigo_core::journal::{Document, Entry, Journal};
use amigo_journal::Action;
use std::collections::VecDeque;
#[derive(Default)]
pub(super) struct JournalIo {
    job: Option<Job>,
    pending: VecDeque<Action>,
    pub pending_setup: Option<design::DataAction>,
    reload_confirmation: bool,
    close_requested: bool,
}
struct Job {
    receiver: mpsc::Receiver<Result<Outcome, String>>,
}
enum Outcome {
    Updated {
        entries: Vec<Entry>,
        document: Option<Document>,
        clear: bool,
    },
    Saved {
        path: String,
        text: String,
        fingerprint: String,
    },
    Search {
        query: String,
        entries: Vec<Entry>,
    },
    Exported,
}
impl Amigo {
    pub(super) fn journal_action(&mut self, action: Action, ctx: &egui::Context) {
        if matches!(action, Action::Reload) && self.journal.dirty() {
            self.journal_io.reload_confirmation = true;
            return;
        }
        self.journal_io.pending.push_back(action);
        self.tick_journal(ctx);
    }
    fn start_journal_save(&mut self, ctx: &egui::Context) {
        let Some(editor) = self
            .journal
            .editor
            .as_ref()
            .filter(|e| e.dirty() && !e.conflict)
        else {
            return;
        };
        let path = editor.document.path.clone();
        let text = editor.document.text.clone();
        let expected = editor.document.fingerprint.clone();
        self.journal_job(ctx, false, move |journal| {
            let fingerprint = journal.save(&path, &text, &expected)?;
            Ok(Outcome::Saved {
                path,
                text,
                fingerprint,
            })
        });
    }
    fn journal_job(
        &mut self,
        ctx: &egui::Context,
        exclusive: bool,
        work: impl FnOnce(Journal) -> Result<Outcome, String> + Send + 'static,
    ) {
        let root = self.data_path.clone();
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        self.journal_io.job = Some(Job { receiver });
        self.journal.busy = true;
        self.journal.loading = exclusive;
        std::thread::spawn(move || {
            let result = Journal::open(&root).and_then(work);
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
    fn journal_result(&mut self, result: Result<Outcome, String>) {
        self.journal.busy = false;
        self.journal.loading = false;
        match result {
            Ok(Outcome::Saved {
                path,
                text,
                fingerprint,
            }) => {
                if let Some(editor) = &mut self.journal.editor
                    && editor.document.path == path
                {
                    editor.saved_text = text;
                    editor.document.fingerprint = fingerprint;
                    editor.conflict = false;
                }
                self.journal.error.clear();
            }
            Ok(Outcome::Updated {
                entries,
                document,
                clear,
            }) => {
                self.journal.entries = entries;
                self.journal.loaded = true;
                self.journal.matches = None;
                if clear {
                    self.journal.editor = None;
                }
                if let Some(document) = document {
                    self.journal.open(document);
                }
                self.journal.error.clear();
            }
            Ok(Outcome::Search { query, entries }) => self.journal.apply_search(&query, entries),
            Ok(Outcome::Exported) => self.message = "Markdown note exported.".into(),
            Err(error) => {
                if let Some(editor) = &mut self.journal.editor
                    && editor.dirty()
                {
                    editor.conflict = true;
                }
                self.journal.error = error;
                self.journal_io.pending.clear();
                self.journal_io.pending_setup = None;
            }
        }
    }
    pub(super) fn tick_journal(&mut self, ctx: &egui::Context) {
        if self.save_blocked {
            return;
        }
        if let Some(job) = self.journal_io.job.take() {
            match job.receiver.try_recv() {
                Ok(result) => self.journal_result(result),
                Err(mpsc::TryRecvError::Empty) => self.journal_io.job = Some(job),
                Err(mpsc::TryRecvError::Disconnected) => self.journal_result(Err(
                    "The Journal worker stopped. Your editor contents are preserved.".into(),
                )),
            }
        }
        if self.journal_io.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(40));
            return;
        }
        if self.section == Section::Journal
            && !self.journal.loaded
            && self.journal_io.pending.is_empty()
        {
            self.journal_io.pending.push_back(Action::Refresh);
        }
        if self.journal.editor.as_ref().is_some_and(|e| {
            e.dirty()
                && !e.conflict
                && (e.changed_at.elapsed() >= Duration::from_millis(700)
                    || !self.journal_io.pending.is_empty()
                    || self.journal_io.pending_setup.is_some())
        }) {
            self.start_journal_save(ctx);
            return;
        }
        if self.journal.dirty()
            && !self
                .journal_io
                .pending
                .front()
                .is_some_and(|a| matches!(a, Action::SaveCopy | Action::Reload))
        {
            ctx.request_repaint_after(Duration::from_millis(700));
            return;
        }
        let Some(action) = self.journal_io.pending.pop_front() else {
            if let Some(action) = self.journal_io.pending_setup.take() {
                self.start_setup_transfer(action, ctx);
            }
            return;
        };
        let current = self.journal.editor.as_ref().map(|e| e.document.clone());
        let exclusive = !matches!(action, Action::Search(_) | Action::Export(_));
        self.journal_job(ctx, exclusive, move |journal| {
            let mut document = None;
            let mut clear = false;
            match action {
                Action::Open(path) => document = Some(journal.read(&path)?),
                Action::FollowLink { from, target } => {
                    document = Some(journal.follow_link(&from, &target)?)
                }
                Action::NewNote(name) => {
                    document = Some(journal.create(
                        &name,
                        &format!(
                                "# {}\n\n",
                                std::path::Path::new(&name)
                                    .file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                            ),
                    )?)
                }
                Action::NewFolder(name) => journal.create_folder(&name)?,
                Action::Rename { old, name } => {
                    let new = journal.rename(&old, &name)?;
                    if let Some(current) = current {
                        if current.path == old {
                            document = Some(journal.read(&new)?);
                        } else if let Some(suffix) = current.path.strip_prefix(&format!("{old}/")) {
                            document = Some(journal.read(&format!("{new}/{suffix}"))?);
                        }
                    }
                }
                Action::Trash(path) => {
                    journal.trash(&path)?;
                    clear = current
                        .is_some_and(|d| d.path == path || d.path.starts_with(&format!("{path}/")));
                }
                Action::Import(folder) => {
                    if let Some(paths) = rfd::FileDialog::new()
                        .set_title("Import Markdown Notes")
                        .add_filter("Markdown", &["md"])
                        .pick_files()
                    {
                        document = journal.import(&paths, &folder)?.into_iter().next();
                    }
                }
                Action::Export(path) => {
                    if let Some(mut destination) = rfd::FileDialog::new()
                        .set_title("Export Markdown Note")
                        .set_file_name(
                            std::path::Path::new(&path)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy(),
                        )
                        .save_file()
                    {
                        if destination.extension().is_none() {
                            destination.set_extension("md");
                        }
                        journal.export(&path, &destination)?;
                        return Ok(Outcome::Exported);
                    }
                }
                Action::Search(query) => {
                    return Ok(Outcome::Search {
                        entries: journal.search(&query)?,
                        query,
                    });
                }
                Action::SaveCopy => {
                    if let Some(current) = current {
                        let stem = std::path::Path::new(&current.path)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy();
                        document = Some(journal.create(
                            &format!(
                                "{stem} (recovered {})",
                                chrono::Local::now().format("%Y-%m-%d %H-%M-%S")
                            ),
                            &current.text,
                        )?);
                    }
                }
                Action::Reload => {
                    if let Some(current) = current {
                        document = Some(journal.read(&current.path)?);
                    }
                }
                Action::Refresh => {
                    if let Some(current) = current {
                        document = Some(journal.read(&current.path)?);
                    }
                }
            }
            Ok(Outcome::Updated {
                entries: journal.entries()?,
                document,
                clear,
            })
        });
    }
    pub(super) fn journal_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && (self.journal.busy || self.journal.dirty())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.journal_io.close_requested = true;
            self.journal_io.pending.clear();
        }
        if self.journal_io.close_requested {
            if !self.journal.busy && !self.journal.dirty() {
                if self.save_local() {
                    self.journal_io.close_requested = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            } else if !self.journal.busy
                && self.journal.editor.as_ref().is_some_and(|e| !e.conflict)
            {
                self.start_journal_save(ctx);
            }
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }
    pub(super) fn journal_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(action) = self.journal.dialogs(ctx) {
            self.journal_action(action, ctx);
        }
        if self.journal_io.close_requested {
            let mut cancel = false;
            let mut copy = false;
            let mut discard = false;
            let response=design::modal(ctx,egui::Id::new("journal-close-confirm")).show(ctx,|ui|{
                ui.set_width(design::dialog_width(ctx,540.));
                cancel=design::dialog_heading(ui,"Unsaved Journal Changes").clicked();
                if self.journal.busy{ui.spinner();ui.label("Saving your note before closing…");}
                else{ui.colored_label(colors(ui).red,&self.journal.error);ui.label("Your note could not be saved. Keep your edits as a separate note, or return to the editor.");}
                design::dialog_footer(ui,|ui|{
                    cancel|=design::button(ui,"Return to Journal").clicked();
                    if !self.journal.busy{copy=design::primary(ui,"Save Copy and Close").clicked();discard=design::button(ui,"Discard Edits and Close").clicked();}
                });
            });
            if cancel || design::dialog_should_close(&response) {
                self.journal_io.close_requested = false;
            }
            if copy {
                self.journal_io.pending.push_back(Action::SaveCopy);
            }
            if discard {
                if let Some(editor) = &mut self.journal.editor {
                    editor.document.text = editor.saved_text.clone();
                }
                self.journal_io.close_requested = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if self.journal_io.reload_confirmation {
            let mut close = false;
            let mut reload = false;
            let modal =
                design::modal(ctx, egui::Id::new("journal-reload-confirm")).show(ctx, |ui| {
                    ui.set_width(design::dialog_width(ctx, 480.));
                    close = design::dialog_heading(ui, "Reload Note").clicked();
                    ui.label("Discard your editor changes and reload the file from disk?");
                    design::dialog_footer(ui, |ui| {
                        close |= design::button(ui, "Cancel").clicked();
                        reload = design::button(ui, "Discard Changes and Reload").clicked();
                    });
                });
            if close || design::dialog_should_close(&modal) {
                self.journal_io.reload_confirmation = false;
            }
            if reload {
                self.journal_io.reload_confirmation = false;
                self.journal_io.pending.push_back(Action::Reload);
            }
        }
    }
    pub(super) fn finish_journal(&mut self) -> bool {
        if let Some(job) = self.journal_io.job.take() {
            let result = job
                .receiver
                .recv()
                .unwrap_or_else(|_| Err("The Journal worker stopped.".into()));
            self.journal_result(result);
        }
        let Some(editor) = self.journal.editor.as_mut().filter(|e| e.dirty()) else {
            return true;
        };
        let result = Journal::open(&self.data_path).and_then(|journal| {
            journal.save(
                &editor.document.path,
                &editor.document.text,
                &editor.document.fingerprint,
            )
        });
        match result {
            Ok(fingerprint) => {
                editor.document.fingerprint = fingerprint;
                editor.saved_text = editor.document.text.clone();
                true
            }
            Err(error) => {
                self.error = error;
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wait(app: &mut Amigo, ctx: &egui::Context) {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            app.tick_journal(ctx);
            if app.journal_io.job.is_none() && app.journal_io.pending.is_empty() {
                break;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn journal_autosaves_switches_and_preserves_external_edits_in_recovery_copy() {
        let (mut app, ctx) = crate::app::tests::harness();
        app.section = Section::Journal;
        app.journal_action(Action::NewNote("Note".into()), &ctx);
        wait(&mut app, &ctx);
        app.journal.editor.as_mut().unwrap().document.text = "# Unicode 🌍\n\nFirst draft".into();
        app.journal_action(Action::NewNote("Next".into()), &ctx);
        wait(&mut app, &ctx);
        let store = Journal::open(&app.data_path).unwrap();
        assert!(store.read("Note.md").unwrap().text.contains("First draft"));
        app.journal_action(Action::Open("Note.md".into()), &ctx);
        wait(&mut app, &ctx);
        app.journal.editor.as_mut().unwrap().document.text = "My unsaved edits".into();
        std::fs::write(app.data_path.join("journal/Note.md"), "External edits").unwrap();
        app.start_journal_save(&ctx);
        wait(&mut app, &ctx);
        assert!(app.journal.editor.as_ref().unwrap().conflict);
        assert_eq!(store.read("Note.md").unwrap().text, "External edits");
        app.journal_action(Action::SaveCopy, &ctx);
        wait(&mut app, &ctx);
        assert!(!app.journal.dirty());
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.text,
            "My unsaved edits"
        );
        assert_eq!(store.read("Note.md").unwrap().text, "External edits");
        let archive =
            amigo_core::backup::Archive::capture(&app.library, amigo_core::backup::Scope::Setup)
                .unwrap();
        assert!(archive.review(&app.library).is_ok());
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
}
