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
    external_navigation: Option<Action>,
    external_saving: bool,
}
struct Job {
    receiver: mpsc::Receiver<Result<Outcome, String>>,
}
enum Outcome {
    MediaAttached {
        path: String,
        batch: amigo_core::attachments::Batch,
    },
    Updated {
        entries: Vec<Entry>,
        document: Option<Document>,
        clear: bool,
        reveal: Option<String>,
        renamed: Option<(String, String)>,
        folder_edit: bool,
    },
    Moved {
        entries: Vec<Entry>,
        old: String,
        new: String,
    },
    Saved {
        path: String,
        text: String,
        fingerprint: String,
        pdf_edits: Option<amigo_core::journal::pdf::Edits>,
    },
    Search {
        query: String,
        entries: Vec<Entry>,
    },
    ExternalDestination {
        source: PathBuf,
        folder: String,
        error: String,
    },
    Cancelled,
    Exported,
}
impl Amigo {
    pub(super) fn journal_action(&mut self, action: Action, ctx: &egui::Context) {
        if matches!(action, Action::CancelExternalSave) {
            self.journal_io.external_navigation = None;
            self.journal_io.external_saving = false;
            return;
        }
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
            .filter(|e| e.dirty() && !e.conflict && e.document.external.is_none())
        else {
            return;
        };
        let document = editor.document.clone();
        self.journal_job(ctx, false, move |journal| {
            let fingerprint = journal.save_document(&document)?;
            Ok(Outcome::Saved {
                path: document.path,
                text: document.text,
                pdf_edits: document.pdf.map(|pdf| pdf.edits),
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
    fn journal_result(&mut self, result: Result<Outcome, String>, ctx: Option<&egui::Context>) {
        self.journal.busy = false;
        self.journal.loading = false;
        match result {
            Ok(Outcome::MediaAttached { path, mut batch }) => {
                if let Some(editor) = &mut self.journal.editor
                    && editor.document.path == path
                {
                    let levels = std::path::Path::new(&path)
                        .parent()
                        .unwrap_or(std::path::Path::new(""))
                        .components()
                        .count();
                    let mut markdown = String::new();
                    for file in &batch.files {
                        let target = format!(
                            "{}.media/{}",
                            "../".repeat(levels),
                            amigo_core::media::target(file, 0)
                        );
                        markdown.push_str(&format!(
                            "\n\n{}\n",
                            amigo_core::media::embed(&file.name, &target)
                        ));
                    }
                    // Repaint will project the appended source into Preview and Split modes.
                    if let Some(ctx) = ctx {
                        editor.append_markdown(ctx, &markdown);
                    } else {
                        editor.document.text.push_str(&markdown);
                    }
                    batch.keep();
                }
            }
            Ok(Outcome::Saved {
                path,
                text,
                fingerprint,
                pdf_edits,
            }) => {
                if let Some(editor) = &mut self.journal.editor
                    && editor.document.path == path
                {
                    editor.saved_text = text;
                    editor.saved_pdf_edits = pdf_edits;
                    editor.document.fingerprint = fingerprint;
                    editor.conflict = false;
                }
                self.journal.error.clear();
            }
            Ok(Outcome::Updated {
                entries,
                document,
                clear,
                reveal,
                renamed,
                folder_edit,
            }) => {
                if let Some((old, new)) = renamed {
                    self.journal.renamed_folder(&old, &new);
                }
                if folder_edit {
                    self.journal.folder_edit_result(None);
                }
                self.journal.update_entries(entries);
                if let Some(path) = reveal {
                    let folder = if self
                        .journal
                        .entries
                        .iter()
                        .any(|entry| entry.path == path && entry.folder)
                    {
                        path.as_str()
                    } else {
                        path.rsplit_once('/').map_or("", |(parent, _)| parent)
                    };
                    self.journal.expand_folder(folder);
                }
                self.journal.loaded = true;
                if clear {
                    self.journal.close_note();
                }
                if let Some(document) = document {
                    let saved = document.external.is_none();
                    self.journal.open(document);
                    if saved && self.journal_io.external_saving {
                        self.journal_io.external_saving = false;
                        if let Some(action) = self.journal_io.external_navigation.take() {
                            self.journal_io.pending.push_front(action);
                        }
                    }
                }
                self.journal.error.clear();
            }
            Ok(Outcome::Moved { entries, old, new }) => {
                self.journal.renamed_folder(&old, &new);
                if let Some(editor) = &mut self.journal.editor
                    && editor.document.external.is_none()
                    && editor.document.path == old
                {
                    if let Some(ctx) = ctx {
                        let old_id = egui::Id::new(("journal-editor", &old));
                        let new_id = egui::Id::new(("journal-editor", &new));
                        if let Some(state) = egui::TextEdit::load_state(ctx, old_id) {
                            state.store(ctx, new_id);
                        }
                    }
                    editor.document.path = new.clone();
                }
                self.journal.update_entries(entries);
                self.journal
                    .expand_folder(new.rsplit_once('/').map_or("", |(p, _)| p));
                self.journal.move_result(None);
                self.journal.error.clear();
            }
            Ok(Outcome::Search { query, entries }) => self.journal.apply_search(&query, entries),
            Ok(Outcome::ExternalDestination {
                source,
                folder,
                error,
            }) => {
                self.journal
                    .choose_external_destination(source, folder, error);
            }
            Ok(Outcome::Cancelled) => (),
            Ok(Outcome::Exported) => self.message = "Document exported.".into(),
            Err(error) => {
                self.journal.folder_edit_result(Some(&error));
                self.journal.move_result(Some(&error));
                if let Some(editor) = &mut self.journal.editor
                    && editor.dirty()
                    && editor.document.external.is_none()
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
        self.journal.media_root = Some(self.data_path.join("journal"));
        if self.save_blocked {
            return;
        }
        if let Some(job) = self.journal_io.job.take() {
            match job.receiver.try_recv() {
                Ok(result) => self.journal_result(result, Some(ctx)),
                Err(mpsc::TryRecvError::Empty) => self.journal_io.job = Some(job),
                Err(mpsc::TryRecvError::Disconnected) => self.journal_result(
                    Err("The Journal worker stopped. Your editor contents are preserved.".into()),
                    Some(ctx),
                ),
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
        let external_dirty = self
            .journal
            .editor
            .as_ref()
            .is_some_and(|e| e.document.external.is_some() && e.dirty());
        if self.journal_io.external_navigation.is_some() && !self.journal_io.external_saving {
            return;
        }
        if external_dirty
            && self.journal_io.pending.front().is_some_and(|a| {
                matches!(
                    a,
                    Action::Open(_)
                        | Action::OpenExternal
                        | Action::OpenExternalFile(_)
                        | Action::NewNote(_)
                        | Action::FollowLink { .. }
                        | Action::ConvertToPdf(_)
                )
            })
        {
            self.journal_io.external_navigation = self.journal_io.pending.pop_front();
            return;
        }
        let searching = self
            .journal_io
            .pending
            .front()
            .is_some_and(|action| matches!(action, Action::Search(_)));
        if !searching
            && self.journal.editor.as_ref().is_some_and(|e| {
                e.dirty()
                    && e.document.external.is_none()
                    && !e.conflict
                    && (e.changed_at.elapsed() >= Duration::from_millis(700)
                        || !self.journal_io.pending.is_empty()
                        || self.journal_io.pending_setup.is_some())
            })
        {
            self.start_journal_save(ctx);
            return;
        }
        if self.journal.dirty()
            && !self.journal_io.pending.front().is_some_and(|a| {
                matches!(a, Action::SaveCopy | Action::Reload | Action::Search(_))
                    || external_dirty
                        && matches!(
                            a,
                            Action::SaveToJournal
                                | Action::StoreExternal { .. }
                                | Action::Move { .. }
                                | Action::NewFolder(_)
                                | Action::Trash(_)
                                | Action::Restore(_)
                                | Action::DeletePermanently(_)
                                | Action::Rename { .. }
                                | Action::Refresh
                        )
            })
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
            let mut reveal = None;
            let mut renamed = None;
            let folder_edit = matches!(
                &action,
                Action::NewNote(_) | Action::NewFolder(_) | Action::Rename { .. }
            );
            match action {
                Action::AttachMedia => {
                    let Some(current) = current else {
                        return Ok(Outcome::Cancelled);
                    };
                    let paths = rfd::FileDialog::new()
                        .set_title("Attach Photo or Video")
                        .add_filter(
                            "Photos and Videos",
                            &[
                                "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "mp4",
                                "m4v", "mov", "webm", "mkv", "avi", "ogv",
                            ],
                        )
                        .pick_files()
                        .unwrap_or_default();
                    if paths.is_empty() {
                        return Ok(Outcome::Cancelled);
                    }
                    let root = Journal::media_folder(&journal)?;
                    return Ok(Outcome::MediaAttached {
                        path: current.path,
                        batch: amigo_core::attachments::Batch::import(root, &paths)?,
                    });
                }
                Action::Open(path) => document = Some(journal.read(&path)?),
                Action::FollowLink { from, target } => {
                    document = Some(
                        if let Some(source) = current.as_ref().and_then(|d| d.external.as_ref()) {
                            let target = amigo_core::journal::external_link(source, &target)?;
                            journal.open_file(&target)?
                        } else {
                            journal.follow_link(&from, &target)?
                        },
                    )
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
                Action::NewFolder(name) => {
                    journal.create_folder(&name)?;
                    reveal = Some(name);
                }
                Action::Rename { old, name } => {
                    let new = journal.rename(&old, &name)?;
                    renamed = Some((old.clone(), new.clone()));
                    if let Some(current) = current.filter(|d| d.external.is_none()) {
                        if current.path == old {
                            document = Some(journal.read(&new)?);
                        } else if let Some(suffix) = current.path.strip_prefix(&format!("{old}/")) {
                            document = Some(journal.read(&format!("{new}/{suffix}"))?);
                        }
                    }
                }
                Action::Trash(path) => {
                    journal.trash(&path)?;
                    clear = current.is_some_and(|d| {
                        d.external.is_none()
                            && (d.path == path || d.path.starts_with(&format!("{path}/")))
                    });
                }
                Action::Restore(path) => {
                    let new = journal.restore(&path)?;
                    if let Some(current) = current.filter(|d| d.external.is_none()) {
                        if current.path == path {
                            document = Some(journal.read(&new)?);
                        } else if let Some(suffix) = current.path.strip_prefix(&format!("{path}/"))
                        {
                            document = Some(journal.read(&format!("{new}/{suffix}"))?);
                        }
                    }
                    reveal = Some(new);
                }
                Action::DeletePermanently(path) => {
                    journal.delete_permanently(&path)?;
                    clear = current.is_some_and(|d| {
                        d.external.is_none()
                            && (d.path == path || d.path.starts_with(&format!("{path}/")))
                    });
                }
                Action::OpenExternal => {
                    if let Some(source) = rfd::FileDialog::new()
                        .set_title("Open Markdown or PDF File")
                        .add_filter("Journal documents", &["md", "pdf"])
                        .add_filter("Markdown", &["md"])
                        .add_filter("PDF", &["pdf"])
                        .pick_file()
                    {
                        document = Some(journal.open_file(&source)?);
                    }
                }
                Action::OpenExternalFile(source) => document = Some(journal.open_file(&source)?),
                Action::SaveToJournal => {
                    let Some(source) = current.and_then(|d| d.external) else {
                        return Ok(Outcome::Cancelled);
                    };
                    return Ok(Outcome::ExternalDestination {
                        source,
                        folder: String::new(),
                        error: String::new(),
                    });
                }
                Action::CancelExternalSave => return Ok(Outcome::Cancelled),
                Action::Move { path, folder } => {
                    let new = journal.move_file(&path, &folder)?;
                    return Ok(Outcome::Moved {
                        entries: journal.entries()?,
                        old: path,
                        new,
                    });
                }
                Action::StoreExternal { source, folder } => {
                    let canonical_source = source.canonicalize().unwrap_or_else(|_| source.clone());
                    let result = if let Some(draft) =
                        current.filter(|d| d.external.as_ref() == Some(&canonical_source))
                    {
                        journal.store_document(&draft, &folder).map(|d| vec![d])
                    } else {
                        journal.import(std::slice::from_ref(&source), &folder)
                    };
                    match result {
                        Ok(documents) => document = documents.into_iter().next(),
                        Err(error) => {
                            return Ok(Outcome::ExternalDestination {
                                source,
                                folder,
                                error,
                            });
                        }
                    }
                }
                Action::ConvertToPdf(path) => {
                    document = Some(journal.convert_to_pdf(&path)?);
                }
                Action::Export(path) => {
                    if let Some(mut destination) = rfd::FileDialog::new()
                        .set_title("Export Document")
                        .set_file_name(
                            std::path::Path::new(&path)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy(),
                        )
                        .save_file()
                    {
                        if destination.extension().is_none() {
                            destination.set_extension(if amigo_core::journal::is_pdf(&path) {
                                "pdf"
                            } else {
                                "md"
                            });
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
                        document = Some(journal.save_copy(&current)?);
                    }
                }
                Action::Reload => {
                    if let Some(current) = current {
                        document = Some(if let Some(source) = current.external {
                            Journal::read_external(&source)?
                        } else {
                            journal.read(&current.path)?
                        });
                    }
                }
                Action::Refresh => {
                    if let Some(current) = current.filter(|d| d.external.is_none()) {
                        document = Some(journal.read(&current.path)?);
                    }
                }
            }
            Ok(Outcome::Updated {
                entries: journal.entries()?,
                document,
                clear,
                reveal,
                renamed,
                folder_edit,
            })
        });
    }
    pub(super) fn journal_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && (self.journal.busy
                || self.journal.dirty()
                || self.squidy_busy()
                || self.squidy_dirty())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.journal_io.close_requested = true;
            self.journal_io.external_navigation = None;
            self.journal_io.external_saving = false;
            self.journal_io.pending.clear();
            self.squidy_io.pending.clear();
            self.squidy.changed_at = Instant::now() - Duration::from_secs(1);
        }
        if self.journal_io.close_requested {
            if !self.journal.busy
                && !self.journal.dirty()
                && !self.squidy_busy()
                && !self.squidy_dirty()
            {
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
        if self.journal_io.external_navigation.is_some() && !self.journal_io.external_saving {
            let mut save = false;
            let mut discard = false;
            let mut cancel = false;
            let modal=design::modal(ctx,egui::Id::new("journal-external-unsaved")).show(ctx,|ui|{
                ui.set_width(design::dialog_width(ctx,540.));
                cancel=design::dialog_heading(ui,"Unsaved External File").clicked();
                ui.label("Save your edits to Journal before opening another document? The original file has not been changed.");
                design::dialog_footer(ui,|ui|{discard=design::dialog_destructive(ui,"Discard Edits and Continue").clicked();save=design::dialog_primary(ui,"Save to Journal…").clicked();cancel|=design::button(ui,"Cancel").clicked();});
            });
            if cancel || design::dialog_should_close(&modal) {
                self.journal_io.external_navigation = None;
            } else if save {
                self.journal_io.external_saving = true;
                self.journal_io.pending.push_front(Action::SaveToJournal);
            } else if discard {
                self.journal.close_note();
                if let Some(action) = self.journal_io.external_navigation.take() {
                    self.journal_io.pending.push_front(action);
                }
            }
        }
        if self.journal_io.close_requested {
            let mut cancel = false;
            let mut copy = false;
            let mut discard = false;
            let response=design::modal(ctx,egui::Id::new("journal-close-confirm")).show(ctx,|ui|{
                ui.set_width(design::dialog_width(ctx,760.));
                cancel=design::dialog_heading(ui,"Unsaved Workspace Changes").clicked();
                if self.journal.busy||self.squidy_busy(){ui.spinner();ui.label("Saving your changes before closing…");}
                else if self.journal.editor.as_ref().is_some_and(|e|e.document.external.is_some()&&e.dirty()){ui.label("This file is open temporarily. Save your edits to Journal before closing, or return to the editor.");}
                else{ui.colored_label(colors(ui).red,if self.squidy_dirty(){&self.squidy.error}else{&self.journal.error});ui.label("Your changes could not be saved. Return to the editor to preserve or resolve them.");}
                design::dialog_footer(ui,|ui|{
                    if !self.journal.busy&&!self.squidy_busy(){discard=design::dialog_destructive(ui,"Discard Edits and Close").clicked();}
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                        if !self.journal.busy&&!self.squidy_busy()&&self.journal.dirty(){copy=design::dialog_primary(ui,"Save Copy and Close").clicked();}
                        cancel|=design::button(ui,"Return to Editor").clicked();
                    });
                });
            });
            if cancel || design::dialog_should_close(&response) {
                self.journal_io.close_requested = false;
            }
            if copy {
                self.journal_io.pending.push_back(Action::SaveCopy);
            }
            if discard {
                self.squidy.board = self.squidy_io.saved.clone();
                if let Some(editor) = &mut self.journal.editor {
                    editor.document.text = editor.saved_text.clone();
                    if let Some(pdf) = &mut editor.document.pdf {
                        pdf.edits = editor.saved_pdf_edits.clone().unwrap_or_default();
                    }
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
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            reload = design::dialog_destructive(ui, "Discard Changes and Reload")
                                .clicked();
                            close |= design::button(ui, "Cancel").clicked();
                        });
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
            self.journal_result(result, None);
        }
        let Some(editor) = self.journal.editor.as_mut().filter(|e| e.dirty()) else {
            return true;
        };
        let result = Journal::open(&self.data_path)
            .and_then(|journal| journal.save_document(&editor.document));
        match result {
            Ok(fingerprint) => {
                editor.document.fingerprint = fingerprint;
                editor.saved_text = editor.document.text.clone();
                editor.saved_pdf_edits = editor.document.pdf.as_ref().map(|pdf| pdf.edits.clone());
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
    #[test]
    fn external_save_matches_alias_paths_to_the_edited_draft() {
        let (mut app, ctx) = crate::app::tests::harness();
        let source = app.data_path.join("External.md");
        std::fs::write(&source, "Original external source").unwrap();
        std::fs::create_dir(app.data_path.join("spare")).unwrap();
        app.journal_action(Action::OpenExternalFile(source.clone()), &ctx);
        wait(&mut app, &ctx);
        app.journal.editor.as_mut().unwrap().document.text = "Keep my edited draft".into();
        app.journal_action(
            Action::StoreExternal {
                source: app.data_path.join("spare/../External.md"),
                folder: String::new(),
            },
            &ctx,
        );
        wait(&mut app, &ctx);
        assert!(app.journal.error.is_empty(), "{}", app.journal.error);
        assert_eq!(
            Journal::open(&app.data_path)
                .unwrap()
                .read("External.md")
                .unwrap()
                .text,
            "Keep my edited draft"
        );
        assert_eq!(
            std::fs::read_to_string(source).unwrap(),
            "Original external source"
        );
        assert!(!app.journal.dirty());
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn saving_an_external_draft_resumes_its_deferred_document_navigation() {
        let (mut app, ctx) = crate::app::tests::harness();
        let source = app.data_path.join("External.md");
        std::fs::write(&source, "Original").unwrap();
        app.journal_action(Action::OpenExternalFile(source.clone()), &ctx);
        wait(&mut app, &ctx);
        app.journal.editor.as_mut().unwrap().document.text = "Keep these edits".into();
        app.journal_action(Action::NewNote("Next".into()), &ctx);
        wait(&mut app, &ctx);
        assert!(app.journal_io.external_navigation.is_some());
        app.journal_io.external_saving = true;
        app.journal_action(Action::SaveToJournal, &ctx);
        wait(&mut app, &ctx);
        app.journal_action(
            Action::StoreExternal {
                source: source.clone(),
                folder: String::new(),
            },
            &ctx,
        );
        wait(&mut app, &ctx);
        assert!(app.journal_io.external_navigation.is_none());
        assert!(!app.journal_io.external_saving);
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.path,
            "Next.md"
        );
        assert_eq!(
            Journal::open(&app.data_path)
                .unwrap()
                .read("External.md")
                .unwrap()
                .text,
            "Keep these edits"
        );
        assert_eq!(std::fs::read_to_string(source).unwrap(), "Original");
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn opening_external_files_keeps_them_temporary_until_save_to_journal() {
        let (mut app, ctx) = crate::app::tests::harness();
        let journal = Journal::open(&app.data_path).unwrap();
        journal.create_folder("Work").unwrap();
        journal
            .create("External", "Existing notebook note")
            .unwrap();
        let source = app.data_path.join("External.md");
        std::fs::write(&source, "External original").unwrap();
        app.journal_action(Action::OpenExternalFile(source.clone()), &ctx);
        wait(&mut app, &ctx);
        assert!(
            app.journal
                .editor
                .as_ref()
                .unwrap()
                .document
                .external
                .is_some()
        );
        assert_eq!(app.journal.entries.iter().filter(|e| !e.folder).count(), 1);
        app.journal.editor.as_mut().unwrap().document.text = "Edited temporary note".into();
        app.start_journal_save(&ctx);
        assert!(app.journal_io.job.is_none());
        assert_eq!(
            journal.read("External.md").unwrap().text,
            "Existing notebook note"
        );
        app.journal_action(Action::Refresh, &ctx);
        wait(&mut app, &ctx);
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.text,
            "Edited temporary note"
        );
        app.journal_action(Action::NewNote("Next".into()), &ctx);
        wait(&mut app, &ctx);
        assert!(app.journal_io.external_navigation.is_some());
        assert!(journal.read("Next.md").is_err());
        app.journal_action(Action::CancelExternalSave, &ctx);
        assert!(app.journal_io.external_navigation.is_none());
        assert!(app.journal.dirty());
        app.journal_action(Action::SaveToJournal, &ctx);
        wait(&mut app, &ctx);
        assert!(
            app.journal
                .editor
                .as_ref()
                .unwrap()
                .document
                .external
                .is_some()
        );
        app.journal_action(
            Action::StoreExternal {
                source: source.clone(),
                folder: "Work".into(),
            },
            &ctx,
        );
        wait(&mut app, &ctx);
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.path,
            "Work/External.md"
        );
        assert!(!app.journal.dirty());
        assert!(
            app.journal
                .editor
                .as_ref()
                .unwrap()
                .document
                .external
                .is_none()
        );
        assert_eq!(
            journal.read("Work/External.md").unwrap().text,
            "Edited temporary note"
        );
        assert_eq!(
            std::fs::read_to_string(&source).unwrap(),
            "External original"
        );
        assert_eq!(
            journal.read("External.md").unwrap().text,
            "Existing notebook note"
        );
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn moving_an_open_note_saves_edits_updates_its_path_and_preserves_the_editor() {
        let (mut app, ctx) = crate::app::tests::harness();
        let journal = Journal::open(&app.data_path).unwrap();
        journal.create_folder("Work").unwrap();
        let document = journal.create("Note", "Original").unwrap();
        app.journal.open(document);
        let editor = app.journal.editor.as_mut().unwrap();
        editor.document.text = "Latest draft".into();
        editor.changed_at = Instant::now() - Duration::from_secs(5);
        let changed_at = editor.changed_at;
        app.journal_action(
            Action::Move {
                path: "Note.md".into(),
                folder: "Work".into(),
            },
            &ctx,
        );
        wait(&mut app, &ctx);
        let editor = app.journal.editor.as_ref().unwrap();
        assert_eq!(editor.document.path, "Work/Note.md");
        assert_eq!(editor.document.text, "Latest draft");
        assert_eq!(editor.changed_at, changed_at);
        assert!(!editor.dirty());
        assert!(app.journal.error.is_empty());
        assert!(journal.read("Note.md").is_err());
        assert_eq!(journal.read("Work/Note.md").unwrap().text, "Latest draft");
        journal.create("Note", "Keep existing destination").unwrap();
        app.journal_action(
            Action::Move {
                path: "Work/Note.md".into(),
                folder: String::new(),
            },
            &ctx,
        );
        wait(&mut app, &ctx);
        assert!(!app.journal.error.is_empty());
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.path,
            "Work/Note.md"
        );
        assert_eq!(
            journal.read("Note.md").unwrap().text,
            "Keep existing destination"
        );
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    #[ignore = "requires Poppler GLib, Cairo and Pango"]
    fn external_pdf_forms_save_the_draft_only_when_explicitly_imported() {
        use amigo_core::journal::pdf::{self, Value};
        let (mut app, ctx) = crate::app::tests::harness();
        let journal = Journal::open(&app.data_path).unwrap();
        let source = app.data_path.join("External.pdf");
        let original = include_bytes!("../../crates/core/tests/fixtures/journal-forms.pdf");
        std::fs::write(&source, original).unwrap();
        app.journal_action(Action::OpenExternalFile(source.clone()), &ctx);
        wait(&mut app, &ctx);
        let page = pdf::render(
            app.journal
                .editor
                .as_ref()
                .unwrap()
                .document
                .pdf
                .as_ref()
                .unwrap(),
            0,
            1.,
        )
        .unwrap();
        let id = page
            .fields
            .iter()
            .find(|f| f.name == "Your name")
            .unwrap()
            .id;
        app.journal
            .editor
            .as_mut()
            .unwrap()
            .document
            .pdf
            .as_mut()
            .unwrap()
            .edits
            .fields
            .insert(id, Value::Text("Temporary form value".into()));
        app.start_journal_save(&ctx);
        assert!(app.journal_io.job.is_none());
        assert!(journal.read("External.pdf").is_err());
        app.journal_action(
            Action::StoreExternal {
                source: source.clone(),
                folder: String::new(),
            },
            &ctx,
        );
        wait(&mut app, &ctx);
        assert!(!app.journal.dirty());
        assert!(
            app.journal
                .editor
                .as_ref()
                .unwrap()
                .document
                .external
                .is_none()
        );
        assert_eq!(std::fs::read(&source).unwrap(), original);
        let saved = journal.read("External.pdf").unwrap();
        assert_eq!(
            pdf::render(saved.pdf.as_ref().unwrap(), 0, 1.)
                .unwrap()
                .fields
                .iter()
                .find(|f| f.id == id)
                .unwrap()
                .value,
            Value::Text("Temporary form value".into())
        );
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn pdf_save_results_preserve_edits_made_while_worker_was_saving() {
        use amigo_core::journal::pdf::{PdfDocument, Value};
        let (mut app, ctx) = crate::app::tests::harness();
        let mut pdf = PdfDocument::new(
            include_bytes!("../../crates/core/tests/fixtures/journal-forms.pdf").to_vec(),
        );
        pdf.edits.fields.insert(10, Value::Text("First".into()));
        let saved = pdf.edits.clone();
        app.journal.open(Document {
            external: None,
            path: "Form.pdf".into(),
            text: String::new(),
            fingerprint: "original".into(),
            pdf: Some(pdf),
        });
        app.journal
            .editor
            .as_mut()
            .unwrap()
            .document
            .pdf
            .as_mut()
            .unwrap()
            .edits
            .fields
            .insert(10, Value::Text("Second".into()));
        app.journal_result(
            Ok(Outcome::Saved {
                path: "Form.pdf".into(),
                text: String::new(),
                fingerprint: "saved".into(),
                pdf_edits: Some(saved),
            }),
            Some(&ctx),
        );
        assert!(app.journal.dirty());
        assert_eq!(
            app.journal
                .editor
                .as_ref()
                .unwrap()
                .document
                .pdf
                .as_ref()
                .unwrap()
                .edits
                .fields[&10],
            Value::Text("Second".into())
        );
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    #[ignore = "requires Poppler GLib, Cairo and Pango"]
    fn pdf_edits_autosave_before_navigation_conflicts_recover_and_markdown_converts() {
        use amigo_core::journal::pdf::{self, Value};
        let (mut app, ctx) = crate::app::tests::harness();
        let journal = Journal::open(&app.data_path).unwrap();
        let fixture = include_bytes!("../../crates/core/tests/fixtures/journal-forms.pdf");
        std::fs::write(app.data_path.join("journal/Form.pdf"), fixture).unwrap();
        app.journal_action(Action::Open("Form.pdf".into()), &ctx);
        wait(&mut app, &ctx);
        let page = pdf::render(
            app.journal
                .editor
                .as_ref()
                .unwrap()
                .document
                .pdf
                .as_ref()
                .unwrap(),
            0,
            1.,
        )
        .unwrap();
        let field = page
            .fields
            .iter()
            .find(|f| f.name == "Your name")
            .unwrap()
            .id;
        app.journal
            .editor
            .as_mut()
            .unwrap()
            .document
            .pdf
            .as_mut()
            .unwrap()
            .edits
            .fields
            .insert(field, Value::Text("Saved form value".into()));
        app.journal_action(Action::NewNote("Next".into()), &ctx);
        wait(&mut app, &ctx);
        let reopened = journal.read("Form.pdf").unwrap();
        let page = pdf::render(reopened.pdf.as_ref().unwrap(), 0, 1.).unwrap();
        assert_eq!(
            page.fields.iter().find(|f| f.id == field).unwrap().value,
            Value::Text("Saved form value".into())
        );
        app.journal_action(Action::Open("Form.pdf".into()), &ctx);
        wait(&mut app, &ctx);
        app.journal
            .editor
            .as_mut()
            .unwrap()
            .document
            .pdf
            .as_mut()
            .unwrap()
            .edits
            .fields
            .insert(field, Value::Text("Recovery value".into()));
        std::fs::write(app.data_path.join("journal/Form.pdf"), fixture).unwrap();
        app.start_journal_save(&ctx);
        wait(&mut app, &ctx);
        assert!(app.journal.editor.as_ref().unwrap().conflict);
        app.journal_action(Action::SaveCopy, &ctx);
        wait(&mut app, &ctx);
        assert!(!app.journal.dirty());
        assert!(
            app.journal
                .editor
                .as_ref()
                .unwrap()
                .document
                .path
                .ends_with(".pdf")
        );
        assert_eq!(
            journal
                .read("Form.pdf")
                .unwrap()
                .pdf
                .unwrap()
                .bytes
                .as_ref(),
            fixture
        );
        let recovery = app
            .journal
            .editor
            .as_ref()
            .unwrap()
            .document
            .pdf
            .as_ref()
            .unwrap();
        assert_eq!(
            pdf::render(recovery, 0, 1.)
                .unwrap()
                .fields
                .iter()
                .find(|f| f.id == field)
                .unwrap()
                .value,
            Value::Text("Recovery value".into())
        );
        app.journal_action(Action::ConvertToPdf("Next.md".into()), &ctx);
        wait(&mut app, &ctx);
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.path,
            "Next.pdf"
        );
        assert!(app.journal.editor.as_ref().unwrap().document.pdf.is_some());
        assert!(journal.read("Next.md").unwrap().text.contains("Next"));
        assert!(app.journal.error.is_empty(), "{}", app.journal.error);
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn media_picker_results_insert_portable_markdown_preserve_undo_and_ignore_closed_notes() {
        use amigo_core::{
            attachments::{self, Batch},
            media,
        };
        let (mut app, ctx) = crate::app::tests::harness();
        let journal = Journal::open(&app.data_path).unwrap();
        journal.create_folder("Research").unwrap();
        let document = journal.create("Research/Note", "# Original\n").unwrap();
        app.journal.open(document.clone());
        let source = app.data_path.join("Photo ✓ (1).png");
        std::fs::write(&source, b"image data").unwrap();
        let folder = journal.media_folder().unwrap();
        let batch = Batch::import(folder.clone(), std::slice::from_ref(&source)).unwrap();
        let file = batch.files[0].clone();
        app.journal_result(
            Ok(Outcome::MediaAttached {
                path: document.path.clone(),
                batch,
            }),
            Some(&ctx),
        );
        let editor = app.journal.editor.as_ref().unwrap();
        let expected = format!("../.media/{}", media::target(&file, 0));
        assert!(
            editor
                .document
                .text
                .contains(&media::embed(&file.name, &expected))
        );
        assert!(editor.dirty());
        let mut history =
            egui::TextEdit::load_state(&ctx, egui::Id::new(("journal-editor", &document.path)))
                .unwrap()
                .undoer();
        let range = egui::text::CCursorRange::one(egui::text::CCursor::new(
            editor.document.text.chars().count(),
        ));
        assert_eq!(
            history
                .undo(&(range, editor.document.text.clone()))
                .unwrap()
                .1,
            document.text
        );
        let saved = editor.document.text.clone();
        assert!(app.finish_journal());
        assert_eq!(journal.read(&document.path).unwrap().text, saved);
        assert_eq!(
            media::resolve(
                &app.data_path.join("journal"),
                &app.data_path.join("journal/Research"),
                &expected
            )
            .unwrap(),
            attachments::resolve(&folder, &file).unwrap()
        );
        app.journal.close_note();
        let batch = Batch::import(folder.clone(), &[source]).unwrap();
        let cancelled = batch.files[0].clone();
        app.journal_result(
            Ok(Outcome::MediaAttached {
                path: document.path,
                batch,
            }),
            Some(&ctx),
        );
        assert!(attachments::resolve(&folder, &cancelled).is_err());
        assert!(attachments::resolve(&folder, &file).is_ok());
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
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
    fn creation_menus_and_header_buttons_save_to_their_destination_and_reveal_children() {
        use crate::app::tests::{click, render, text_rect_optional};
        let (mut app, ctx) = crate::app::tests::harness();
        app.section = Section::Journal;
        let journal = Journal::open(&app.data_path).unwrap();
        journal.create_folder("Work").unwrap();
        journal.create_folder("Work/Deep").unwrap();
        journal
            .create("Work/Deep/Current", "Keep this note")
            .unwrap();
        app.journal_action(Action::Open("Work/Deep/Current.md".into()), &ctx);
        wait(&mut app, &ctx);
        for (id, parent, expected) in [
            (
                "journal-entry-new-note",
                Some("Work/Deep"),
                "Work/Deep/Untitled.md",
            ),
            (
                "journal-entry-new-folder",
                Some("Work/Deep"),
                "Work/Deep/New folder",
            ),
            ("journal-new-note", None, "Untitled.md"),
            ("journal-new-folder", None, "New folder"),
        ] {
            for _ in 0..5 {
                render(&mut app, &ctx, vec![]);
            }
            if id == "journal-entry-new-folder" {
                let row = ctx
                    .read_response(egui::Id::new(("journal-entry", "Work/Deep")))
                    .unwrap()
                    .rect
                    .center();
                click(&mut app, &ctx, row);
                for _ in 0..5 {
                    render(&mut app, &ctx, vec![]);
                }
                assert!(
                    ctx.read_response(egui::Id::new(("journal-entry", "Work/Deep/Untitled.md")))
                        .is_none()
                );
            }
            if let Some(parent) = parent {
                let menu = ctx
                    .read_response(egui::Id::new(("journal-entry-menu", parent)))
                    .unwrap()
                    .rect
                    .center();
                click(&mut app, &ctx, menu);
                for _ in 0..5 {
                    render(&mut app, &ctx, vec![]);
                }
            }
            let button = match parent {
                Some(parent) => egui::Id::new((id, parent)),
                None => egui::Id::new(id),
            };
            let pos = ctx.read_response(button).unwrap().rect.center();
            click(&mut app, &ctx, pos);
            let mut output = render(&mut app, &ctx, vec![]);
            for _ in 0..4 {
                output = render(&mut app, &ctx, vec![]);
            }
            assert!(text_rect_optional(&output, parent.unwrap_or("Journal Root")).is_some());
            let confirm = if id.contains("folder") {
                "journal-folder-confirm"
            } else {
                "journal-note-confirm"
            };
            let save = ctx
                .read_response(egui::Id::new(confirm))
                .unwrap()
                .rect
                .center();
            click(&mut app, &ctx, save);
            wait(&mut app, &ctx);
            assert!(app.journal.error.is_empty(), "{}", app.journal.error);
            assert!(
                journal
                    .entries()
                    .unwrap()
                    .iter()
                    .any(|entry| entry.path == expected)
            );
            for _ in 0..5 {
                render(&mut app, &ctx, vec![]);
            }
            assert!(ctx.read_response(egui::Id::new(confirm)).is_none());
            assert!(
                ctx.read_response(egui::Id::new(("journal-entry", expected)))
                    .is_some()
            );
        }
        let reopened = Journal::open(&app.data_path).unwrap();
        assert!(reopened.read("Untitled.md").is_ok());
        assert!(reopened.read("Work/Deep/Untitled.md").is_ok());
        assert_eq!(
            reopened.read("Work/Deep/Current.md").unwrap().text,
            "Keep this note"
        );
        assert!(app.data_path.join("journal/New folder").is_dir());
        assert!(app.data_path.join("journal/Work/Deep/New folder").is_dir());
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn restore_menu_recovers_folder_trees_and_keeps_open_notes_at_the_new_path() {
        use crate::app::tests::{click, render};
        let (mut app, ctx) = crate::app::tests::harness();
        app.section = Section::Journal;
        let journal = Journal::open(&app.data_path).unwrap();
        for folder in ["Projects", "Projects/Work", "Projects/Work/Deep"] {
            journal.create_folder(folder).unwrap();
        }
        journal
            .create("Projects/Work/Deep/Note", "Nested original")
            .unwrap();
        app.journal_action(Action::Open("Projects/Work/Deep/Note.md".into()), &ctx);
        wait(&mut app, &ctx);
        app.journal_action(Action::Trash("Projects/Work".into()), &ctx);
        wait(&mut app, &ctx);
        assert!(app.journal.editor.is_none());
        assert!(
            !app.journal
                .entries
                .iter()
                .any(|entry| entry.path.starts_with("Projects/Work"))
        );
        let folder = journal
            .entries()
            .unwrap()
            .into_iter()
            .find(|entry| entry.folder && amigo_core::journal::entry_name(&entry.path) == "Work")
            .unwrap()
            .path;
        let note = format!("{folder}/Deep/Note.md");
        app.journal_action(Action::Open(note), &ctx);
        wait(&mut app, &ctx);
        app.journal.editor.as_mut().unwrap().document.text = "Edited while in Trash".into();
        for _ in 0..5 {
            render(&mut app, &ctx, vec![]);
        }
        let menu = ctx
            .read_response(egui::Id::new(("journal-entry-menu", &folder)))
            .unwrap()
            .rect
            .center();
        click(&mut app, &ctx, menu);
        for _ in 0..5 {
            render(&mut app, &ctx, vec![]);
        }
        let restore = ctx
            .read_response(egui::Id::new(("journal-entry-restore", &folder)))
            .unwrap()
            .rect
            .center();
        click(&mut app, &ctx, restore);
        wait(&mut app, &ctx);
        assert!(app.journal.error.is_empty(), "{}", app.journal.error);
        let editor = app.journal.editor.as_ref().unwrap();
        assert_eq!(editor.document.path, "Projects/Work/Deep/Note.md");
        assert_eq!(editor.document.text, "Edited while in Trash");
        assert!(!editor.dirty());
        assert!(
            !app.journal
                .entries
                .iter()
                .any(|entry| entry.path == folder || entry.path.starts_with(&format!("{folder}/")))
        );
        for _ in 0..5 {
            render(&mut app, &ctx, vec![]);
        }
        assert!(
            ctx.read_response(egui::Id::new((
                "journal-entry",
                "Projects/Work/Deep/Note.md"
            )))
            .is_some()
        );
        // Restoring an unrelated note retains the current editor and reveals its parent.
        journal
            .create("Projects/Other", "Restore independently")
            .unwrap();
        journal.trash("Projects/Other.md").unwrap();
        app.journal_action(Action::Refresh, &ctx);
        wait(&mut app, &ctx);
        let other = journal
            .entries()
            .unwrap()
            .into_iter()
            .find(|entry| amigo_core::journal::entry_name(&entry.path) == "Other.md")
            .unwrap()
            .path;
        app.journal_action(Action::Restore(other), &ctx);
        wait(&mut app, &ctx);
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.path,
            "Projects/Work/Deep/Note.md"
        );
        let reopened = Journal::open(&app.data_path).unwrap();
        assert_eq!(
            reopened.read("Projects/Work/Deep/Note.md").unwrap().text,
            "Edited while in Trash"
        );
        assert_eq!(
            reopened.read("Projects/Other.md").unwrap().text,
            "Restore independently"
        );
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn journal_fills_the_workspace_in_every_mode_and_split_orientation() {
        use crate::app::tests::render_sized;
        use amigo_core::journal::{Mode, PreviewLayout};
        let (mut app, ctx) = crate::app::tests::harness();
        app.section = Section::Journal;
        app.journal_action(Action::NewNote("Layout".into()), &ctx);
        wait(&mut app, &ctx);
        for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
            for layout in [PreviewLayout::SideBySide, PreviewLayout::Stacked] {
                for mode in [Mode::Source, Mode::Preview, Mode::Split] {
                    app.library.settings.journal.mode = mode;
                    app.library.settings.journal.preview_layout = layout;
                    for _ in 0..5 {
                        render_sized(&mut app, &ctx, vec![], size);
                    }
                    let notebook = ctx
                        .read_response(egui::Id::new("journal-notebook"))
                        .unwrap()
                        .rect;
                    assert!((notebook.left() - 224.).abs() < 1., "{notebook:?}");
                    assert!(
                        notebook.top().abs() < 1.
                            && (notebook.right() - size.x).abs() < 1.
                            && (notebook.bottom() - size.y).abs() < 1.,
                        "{notebook:?}"
                    );
                    let source = ctx.read_response(egui::Id::new("journal-source-pane"));
                    let preview = ctx.read_response(egui::Id::new("journal-preview-pane"));
                    let first = if mode == Mode::Preview {
                        preview.as_ref()
                    } else {
                        source.as_ref()
                    }
                    .unwrap()
                    .rect;
                    assert!((first.top() - 46.).abs() < 1., "{first:?}");
                    if mode == Mode::Split {
                        let source = source.unwrap().rect;
                        let preview = preview.unwrap().rect;
                        if layout == PreviewLayout::SideBySide {
                            assert!((preview.left() - source.right() - 5.).abs() < 1.);
                        } else {
                            assert!((preview.top() - source.bottom() - 5.).abs() < 1.);
                        }
                    }
                }
            }
        }
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn permanent_deletion_updates_the_tree_and_closes_only_affected_notes() {
        let (mut app, ctx) = crate::app::tests::harness();
        app.section = Section::Journal;
        let journal = Journal::open(&app.data_path).unwrap();
        journal.create("Keep", "Active note").unwrap();
        journal.create_folder("Work").unwrap();
        journal.create_folder("Work/Deep").unwrap();
        journal
            .create("Work/Deep/Note", "Deleted folder note")
            .unwrap();
        journal.create("Removed", "Deleted note").unwrap();
        journal.trash("Work").unwrap();
        journal.trash("Removed.md").unwrap();
        let entries = journal.entries().unwrap();
        let folder = entries
            .iter()
            .find(|e| amigo_core::journal::entry_name(&e.path) == "Work")
            .unwrap()
            .path
            .clone();
        let note = entries
            .iter()
            .find(|e| amigo_core::journal::entry_name(&e.path) == "Removed.md")
            .unwrap()
            .path
            .clone();
        app.journal_action(Action::Open("Keep.md".into()), &ctx);
        wait(&mut app, &ctx);
        app.journal_action(Action::DeletePermanently(note.clone()), &ctx);
        wait(&mut app, &ctx);
        assert!(app.journal.error.is_empty());
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.path,
            "Keep.md"
        );
        assert!(!app.journal.entries.iter().any(|e| e.path == note));
        app.journal_action(Action::Open(format!("{folder}/Deep/Note.md")), &ctx);
        wait(&mut app, &ctx);
        app.journal_action(Action::DeletePermanently(folder.clone()), &ctx);
        wait(&mut app, &ctx);
        assert!(app.journal.error.is_empty());
        assert!(app.journal.editor.is_none());
        assert!(
            !app.journal
                .entries
                .iter()
                .any(|e| e.path.starts_with(&folder))
        );
        let reopened = Journal::open(&app.data_path).unwrap();
        assert_eq!(reopened.entries().unwrap().len(), 2);
        assert_eq!(reopened.read("Keep.md").unwrap().text, "Active note");
        app.journal_action(Action::Open("Keep.md".into()), &ctx);
        wait(&mut app, &ctx);
        for path in ["Keep.md", "Trash"] {
            app.journal_action(Action::DeletePermanently(path.into()), &ctx);
            wait(&mut app, &ctx);
            assert!(!app.journal.error.is_empty());
            assert_eq!(
                app.journal.editor.as_ref().unwrap().document.path,
                "Keep.md"
            );
        }
        assert!(app.data_path.join("journal/Trash").is_dir());
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn external_files_copy_to_root_or_nested_folders_without_overwriting_sources_or_notes() {
        let (mut app, ctx) = crate::app::tests::harness();
        app.section = Section::Journal;
        app.journal_action(Action::NewNote("Current".into()), &ctx);
        wait(&mut app, &ctx);
        app.journal.editor.as_mut().unwrap().document.text = "Unsaved current note".into();
        let journal = Journal::open(&app.data_path).unwrap();
        journal.create_folder("Work").unwrap();
        journal.create_folder("Work/Deep").unwrap();
        let source = app.data_path.join("Outside.md");
        let content = "# External 🌍\n\nA portable note.\n";
        std::fs::write(&source, content).unwrap();
        for (folder, expected) in [
            ("", "Outside.md"),
            ("Work/Deep", "Work/Deep/Outside.md"),
            ("", "Outside (1).md"),
        ] {
            app.journal_action(
                Action::StoreExternal {
                    source: source.clone(),
                    folder: folder.into(),
                },
                &ctx,
            );
            wait(&mut app, &ctx);
            assert_eq!(app.journal.editor.as_ref().unwrap().document.path, expected);
            assert_eq!(journal.read(expected).unwrap().text, content);
            assert_eq!(std::fs::read_to_string(&source).unwrap(), content);
            assert!(!app.journal.dirty());
        }
        assert_eq!(
            journal.read("Current.md").unwrap().text,
            "Unsaved current note"
        );
        assert_eq!(journal.read("Outside.md").unwrap().text, content);
        let reopened = Amigo::load(app.data_path.clone());
        let journal = Journal::open(&reopened.data_path).unwrap();
        assert_eq!(journal.read("Work/Deep/Outside.md").unwrap().text, content);
        app.journal_action(Action::Open("Work/Deep/Outside.md".into()), &ctx);
        wait(&mut app, &ctx);
        app.journal_action(Action::Trash("Work".into()), &ctx);
        wait(&mut app, &ctx);
        assert!(app.journal.editor.is_none());
        assert!(journal.read("Work/Deep/Outside.md").is_err());
        assert_eq!(std::fs::read_to_string(&source).unwrap(), content);
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn failed_external_copy_keeps_the_current_note_and_reopens_the_destination_dialog() {
        let (mut app, ctx) = crate::app::tests::harness();
        app.section = Section::Journal;
        app.journal_action(Action::NewNote("Current".into()), &ctx);
        wait(&mut app, &ctx);
        let current = app.journal.editor.as_ref().unwrap().document.clone();
        let source = app.data_path.join("Invalid.md");
        std::fs::write(&source, [0xff, 0xfe]).unwrap();
        app.journal_action(
            Action::StoreExternal {
                source: source.clone(),
                folder: String::new(),
            },
            &ctx,
        );
        wait(&mut app, &ctx);
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.path,
            current.path
        );
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.text,
            current.text
        );
        assert!(!app.journal.editor.as_ref().unwrap().conflict);
        assert_eq!(std::fs::read(&source).unwrap(), [0xff, 0xfe]);
        let size = egui::vec2(980., 640.);
        let mut output = crate::app::tests::render_sized(&mut app, &ctx, vec![], size);
        for _ in 0..4 {
            output = crate::app::tests::render_sized(&mut app, &ctx, vec![], size);
        }
        assert!(crate::app::tests::text_rect_optional(&output, "Save To Journal").is_some());
        assert!(crate::app::tests::text_rect_optional(&output, "Journal Root").is_some());
        assert!(
            Journal::open(&app.data_path)
                .unwrap()
                .read("Invalid.md")
                .is_err()
        );
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn search_dialog_finds_note_contents_in_the_worker_and_opens_the_result() {
        use crate::app::tests::{render_sized, text_rect_optional};
        let (mut app, ctx) = crate::app::tests::harness();
        app.section = Section::Journal;
        let journal = Journal::open(&app.data_path).unwrap();
        journal.create_folder("Work").unwrap();
        journal
            .create("Work/Found", "A unique notebookneedle in the content.")
            .unwrap();
        app.journal_action(Action::NewNote("Current".into()), &ctx);
        wait(&mut app, &ctx);
        let size = egui::vec2(980., 640.);
        for _ in 0..5 {
            render_sized(&mut app, &ctx, vec![], size);
        }
        let search = ctx
            .read_response(egui::Id::new("journal-search-open"))
            .unwrap()
            .rect
            .center();
        for pressed in [true, false] {
            render_sized(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(search),
                    egui::Event::PointerButton {
                        pos: search,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                size,
            );
        }
        for _ in 0..4 {
            render_sized(&mut app, &ctx, vec![], size);
        }
        render_sized(
            &mut app,
            &ctx,
            vec![egui::Event::Text("notebookneedle".into())],
            size,
        );
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            let output = render_sized(&mut app, &ctx, vec![], size);
            if text_rect_optional(&output, "Work/Found.md").is_some() {
                break;
            }
            assert!(
                Instant::now() < until,
                "search worker did not return the matching note"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.path,
            "Current.md"
        );
        render_sized(
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
        wait(&mut app, &ctx);
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.path,
            "Work/Found.md"
        );
        for _ in 0..4 {
            render_sized(&mut app, &ctx, vec![], size);
        }
        assert!(
            ctx.read_response(egui::Id::new(("journal-entry", "Work/Found.md")))
                .is_some()
        );
        assert!(
            ctx.read_response(egui::Id::new("journal-search-query"))
                .is_none()
        );
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn searching_does_not_save_or_discard_a_conflicting_draft() {
        let (mut app, ctx) = crate::app::tests::harness();
        app.section = Section::Journal;
        app.journal_action(Action::NewNote("Current".into()), &ctx);
        wait(&mut app, &ctx);
        let editor = app.journal.editor.as_mut().unwrap();
        editor.document.text = "Unsaved editor draft".into();
        editor.conflict = true;
        std::fs::write(
            app.data_path.join("journal/Current.md"),
            "External notebookneedle",
        )
        .unwrap();
        app.journal_action(Action::Search("notebookneedle".into()), &ctx);
        wait(&mut app, &ctx);
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.text,
            "Unsaved editor draft"
        );
        assert_eq!(
            Journal::open(&app.data_path)
                .unwrap()
                .read("Current.md")
                .unwrap()
                .text,
            "External notebookneedle"
        );
        assert!(app.journal.editor.as_ref().unwrap().conflict);
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn preview_edits_autosave_before_navigation_and_reopen_as_markdown() {
        use crate::app::tests::{click, render, text_rect};
        let (mut app, ctx) = crate::app::tests::harness();
        app.section = Section::Journal;
        app.library.settings.journal.mode = amigo_core::journal::Mode::Preview;
        app.library.settings.journal.preview_editable = true;
        let journal = Journal::open(&app.data_path).unwrap();
        journal
            .create("Editable", "# Heading\n\nA **bold** thought.\n")
            .unwrap();
        journal.create("Next", "Another note").unwrap();
        app.journal_action(Action::Open("Editable.md".into()), &ctx);
        wait(&mut app, &ctx);
        for _ in 0..4 {
            render(&mut app, &ctx, vec![]);
        }
        let output = render(&mut app, &ctx, vec![]);
        click(
            &mut app,
            &ctx,
            text_rect(&output, "A bold thought.").center(),
        );
        let id = ctx.memory(|m| m.focused()).unwrap();
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(2),
                egui::text::CCursor::new(6),
            )));
        state.store(&ctx, id);
        render(&mut app, &ctx, vec![egui::Event::Text("clear🌍".into())]);
        let expected = "# Heading\n\nA **clear🌍** thought.\n";
        assert_eq!(app.journal.editor.as_ref().unwrap().document.text, expected);
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(2),
                egui::text::CCursor::new(8),
            )));
        state.store(&ctx, id);
        render(&mut app, &ctx, vec![]);
        let italic = ctx
            .read_response(egui::Id::new(("journal-preview-format", "Italic")))
            .unwrap()
            .rect;
        click(&mut app, &ctx, italic.center());
        render(&mut app, &ctx, vec![]);
        let expected = app.journal.editor.as_ref().unwrap().document.text.clone();
        assert_ne!(expected, "# Heading\n\nA **clear🌍** thought.\n");
        app.journal_action(Action::Open("Next.md".into()), &ctx);
        wait(&mut app, &ctx);
        assert_eq!(journal.read("Editable.md").unwrap().text, expected);
        app.journal_action(Action::Open("Editable.md".into()), &ctx);
        wait(&mut app, &ctx);
        assert_eq!(app.journal.editor.as_ref().unwrap().document.text, expected);
        assert!(!app.journal.dirty());
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn preview_list_continuation_and_exit_save_as_portable_markdown() {
        use crate::app::tests::{click, render, text_rect};
        let (mut app, ctx) = crate::app::tests::harness();
        app.section = Section::Journal;
        app.library.settings.journal.mode = amigo_core::journal::Mode::Preview;
        app.library.settings.journal.preview_editable = true;
        let journal = Journal::open(&app.data_path).unwrap();
        journal.create("List", "- First").unwrap();
        journal.create("Next", "Other note").unwrap();
        app.journal_action(Action::Open("List.md".into()), &ctx);
        wait(&mut app, &ctx);
        for _ in 0..4 {
            render(&mut app, &ctx, vec![]);
        }
        let output = render(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, text_rect(&output, "First").center());
        let id = ctx.memory(|m| m.focused()).unwrap();
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(5),
            )));
        state.store(&ctx, id);
        for value in ["Second 🌍", "", "After"] {
            for pressed in [true, false] {
                render(
                    &mut app,
                    &ctx,
                    vec![egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
            for _ in 0..3 {
                render(&mut app, &ctx, vec![]);
            }
            if !value.is_empty() {
                render(&mut app, &ctx, vec![egui::Event::Text(value.into())]);
            }
        }
        let expected = "- First\n- Second 🌍\n\nAfter";
        assert_eq!(app.journal.editor.as_ref().unwrap().document.text, expected);
        app.journal_action(Action::Open("Next.md".into()), &ctx);
        wait(&mut app, &ctx);
        assert_eq!(journal.read("List.md").unwrap().text, expected);
        app.journal_action(Action::Open("List.md".into()), &ctx);
        wait(&mut app, &ctx);
        assert_eq!(app.journal.editor.as_ref().unwrap().document.text, expected);
        std::fs::remove_dir_all(app.data_path).unwrap();
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
    #[test]
    fn folder_rename_moves_the_subtree_preserves_saved_edits_and_retries_disk_validation() {
        use crate::app::tests::{click, render};
        let (mut app, ctx) = crate::app::tests::harness();
        app.section = Section::Journal;
        let journal = Journal::open(&app.data_path).unwrap();
        journal.create_folder("Work").unwrap();
        journal.create_folder("Work/Deep").unwrap();
        journal.create_folder("Work/Other").unwrap();
        journal.create("Work/Deep/Current", "Original").unwrap();
        journal
            .create("Work/Other/Reference", "Keep reference")
            .unwrap();
        app.journal_action(Action::Open("Work/Deep/Current.md".into()), &ctx);
        wait(&mut app, &ctx);
        app.journal.expand_folder("Work/Other");
        app.journal.editor.as_mut().unwrap().document.text = "Unsaved research 🌍".into();
        for _ in 0..5 {
            render(&mut app, &ctx, vec![]);
        }
        let rename = ctx
            .read_response(egui::Id::new(("journal-folder-rename", "Work")))
            .unwrap()
            .rect
            .center();
        click(&mut app, &ctx, rename);
        for _ in 0..5 {
            render(&mut app, &ctx, vec![]);
        }
        render(
            &mut app,
            &ctx,
            vec![egui::Event::Text("Research 🌍".into())],
        );
        let confirm = ctx
            .read_response(egui::Id::new("journal-folder-confirm"))
            .unwrap()
            .rect
            .center();
        click(&mut app, &ctx, confirm);
        wait(&mut app, &ctx);
        assert!(app.journal.error.is_empty(), "{}", app.journal.error);
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.path,
            "Research 🌍/Deep/Current.md"
        );
        let reopened = Journal::open(&app.data_path).unwrap();
        assert!(reopened.read("Work/Deep/Current.md").is_err());
        assert_eq!(
            reopened.read("Research 🌍/Deep/Current.md").unwrap().text,
            "Unsaved research 🌍"
        );
        assert_eq!(
            reopened
                .read("Research 🌍/Other/Reference.md")
                .unwrap()
                .text,
            "Keep reference"
        );
        for _ in 0..5 {
            render(&mut app, &ctx, vec![]);
        }
        assert!(
            ctx.read_response(egui::Id::new((
                "journal-entry",
                "Research 🌍/Other/Reference.md"
            )))
            .is_some()
        );
        // A backend name rejection retains the input and can be corrected in place.
        let rename = ctx
            .read_response(egui::Id::new(("journal-folder-rename", "Research 🌍")))
            .unwrap()
            .rect
            .center();
        click(&mut app, &ctx, rename);
        for _ in 0..5 {
            render(&mut app, &ctx, vec![]);
        }
        render(&mut app, &ctx, vec![egui::Event::Text("NUL".into())]);
        let confirm = ctx
            .read_response(egui::Id::new("journal-folder-confirm"))
            .unwrap()
            .rect
            .center();
        click(&mut app, &ctx, confirm);
        wait(&mut app, &ctx);
        assert!(!app.journal.error.is_empty());
        for _ in 0..5 {
            render(&mut app, &ctx, vec![]);
        }
        assert!(ctx.memory(|m| m.has_focus(egui::Id::new("journal-note-name"))));
        render(
            &mut app,
            &ctx,
            vec![egui::Event::Text("Final research".into())],
        );
        click(&mut app, &ctx, confirm);
        wait(&mut app, &ctx);
        assert!(app.journal.error.is_empty(), "{}", app.journal.error);
        assert_eq!(
            reopened
                .read("Final research/Deep/Current.md")
                .unwrap()
                .text,
            "Unsaved research 🌍"
        );
        assert!(
            app.journal
                .editor
                .as_ref()
                .unwrap()
                .document
                .path
                .starts_with("Final research/")
        );
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
}
