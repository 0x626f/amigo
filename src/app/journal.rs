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
        reveal: Option<String>,
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
                reveal,
            }) => {
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
                    self.journal.open(document);
                }
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
        let searching = self
            .journal_io
            .pending
            .front()
            .is_some_and(|action| matches!(action, Action::Search(_)));
        if !searching
            && self.journal.editor.as_ref().is_some_and(|e| {
                e.dirty()
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
            && !self
                .journal_io
                .pending
                .front()
                .is_some_and(|a| matches!(a, Action::SaveCopy | Action::Reload | Action::Search(_)))
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
                Action::NewFolder(name) => {
                    journal.create_folder(&name)?;
                    reveal = Some(name);
                }
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
                Action::Restore(path) => {
                    let new = journal.restore(&path)?;
                    if let Some(current) = current {
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
                    clear = current
                        .is_some_and(|d| d.path == path || d.path.starts_with(&format!("{path}/")));
                }
                Action::OpenExternal => {
                    return Ok(
                        match rfd::FileDialog::new()
                            .set_title("Open External Markdown File")
                            .add_filter("Markdown", &["md"])
                            .pick_file()
                        {
                            Some(source) => Outcome::ExternalDestination {
                                source,
                                folder: String::new(),
                                error: String::new(),
                            },
                            None => Outcome::Cancelled,
                        },
                    );
                }
                Action::StoreExternal { source, folder } => {
                    match journal.import(std::slice::from_ref(&source), &folder) {
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
                reveal,
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
            let save = text_rect_optional(&output, "Save").unwrap().center();
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
        assert!(crate::app::tests::text_rect_optional(&output, "Open External File").is_some());
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
}
