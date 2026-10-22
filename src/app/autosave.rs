//! Serialize background autosaves; explicit context switches and exit flush them.
use super::*;

pub(super) struct SaveJob {
    receiver: mpsc::Receiver<Result<(), String>>,
    snapshot: Workspace,
}

fn write_records(
    library: &Library,
    snapshot: &Workspace,
    previous: &Workspace,
    vault: Option<&Vault>,
    key: Option<&VaultKey>,
) -> Result<(), String> {
    if snapshot.tasks != previous.tasks
        || snapshot.task_statuses != previous.task_statuses
        || snapshot.task_categories != previous.task_categories
        || snapshot.next_task != previous.next_task
    {
        library.save_board(&library.settings.board, snapshot)?;
    }
    if snapshot.finance != previous.finance {
        library.save_vault(vault.ok_or("No vault is open.")?, &snapshot.finance, key)?;
    }
    Ok(())
}

impl Amigo {
    fn finish_autosave(&mut self, job: SaveJob, result: Result<(), String>) -> bool {
        match result {
            Ok(()) => {
                // Later edits remain in the live workspace and will be saved next.
                self.last_saved = job.snapshot;
                true
            }
            Err(error) => {
                self.error = format!("Local save failed: {error}");
                false
            }
        }
    }

    pub(super) fn poll_autosave(&mut self) {
        let Some(job) = self.save_job.take() else {
            return;
        };
        match job.receiver.try_recv() {
            Ok(result) => {
                self.finish_autosave(job, result);
            }
            Err(mpsc::TryRecvError::Empty) => self.save_job = Some(job),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.finish_autosave(
                    job,
                    Err(
                        "The save worker stopped. Your edits are retained; saving will retry."
                            .into(),
                    ),
                );
            }
        }
    }

    pub(super) fn flush_autosave(&mut self) -> bool {
        let Some(job) = self.save_job.take() else {
            return true;
        };
        let result = job
            .receiver
            .recv()
            .unwrap_or_else(|_| Err("The save worker stopped. Your edits are retained.".into()));
        self.finish_autosave(job, result)
    }

    pub(super) fn start_autosave(&mut self, ctx: &egui::Context) {
        if self.save_blocked
            || self.save_job.is_some()
            || !self.save_organizer()
            || self.workspace == self.last_saved
        {
            return;
        }
        if self.workspace.finance != self.last_saved.finance && !self.vault_unlocked {
            self.error = "The vault is locked; finance has not been saved.".into();
            return;
        }
        self.workspace.normalize();
        // Settings remain on their existing serialized path; a stale record job
        // must never overwrite newer selections or synchronization preferences.
        if self.library.settings.dark_mode != self.workspace.dark_mode {
            let previous_theme = self.library.settings.dark_mode;
            self.library.settings.dark_mode = self.workspace.dark_mode;
            if let Err(error) = self.library.save_settings() {
                self.library.settings.dark_mode = previous_theme;
                self.error = format!("Local save failed: {error}");
                return;
            }
        }
        let snapshot = self.workspace.clone();
        let writing = snapshot.clone();
        let previous = self.last_saved.clone();
        let library = self.library.clone();
        let vault = self.current_vault().cloned();
        let key = self.vault_key.clone();
        let ctx = ctx.clone();
        let (sender, receiver) = mpsc::channel();
        self.save_job = Some(SaveJob { receiver, snapshot });
        self.last_save = Instant::now();
        std::thread::spawn(move || {
            let result = write_records(&library, &writing, &previous, vault.as_ref(), key.as_ref());
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{harness, render};

    #[test]
    fn navigation_does_not_wait_for_autosave_and_later_edits_remain_dirty() {
        let (mut app, ctx) = harness();
        let _ = render(&mut app, &ctx, vec![]);
        let (sender, receiver) = mpsc::channel();
        let snapshot = app.workspace.clone();
        app.save_job = Some(SaveJob {
            receiver,
            snapshot: snapshot.clone(),
        });
        app.workspace.finance.meta.name = "Changed while saving".into();
        app.last_save = Instant::now() - Duration::from_secs(1);
        app.section = Section::Finance;
        let _ = render(&mut app, &ctx, vec![]);
        assert!(
            app.save_job.is_some(),
            "A pending worker must not block a navigation frame"
        );
        sender.send(Ok(())).unwrap();
        app.poll_autosave();
        assert_eq!(app.last_saved, snapshot);
        assert_eq!(app.workspace.finance.meta.name, "Changed while saving");
        app.start_autosave(&ctx);
        assert!(app.flush_autosave());
        assert_eq!(app.workspace, app.last_saved);
        assert_eq!(
            app.library
                .load_plain_vault(app.current_vault().unwrap())
                .unwrap(),
            app.workspace.finance
        );
        std::fs::remove_dir_all(app.data_path).unwrap();
    }

    #[test]
    fn failed_worker_preserves_edits_and_explicit_save_retries() {
        let (mut app, ctx) = harness();
        app.workspace.finance.meta.name = "Unsaved name".into();
        let (sender, receiver) = mpsc::channel();
        app.save_job = Some(SaveJob {
            receiver,
            snapshot: app.workspace.clone(),
        });
        sender.send(Err("Test write failure".into())).unwrap();
        app.poll_autosave();
        assert!(app.error.contains("Test write failure"));
        assert_ne!(app.workspace, app.last_saved);
        app.start_autosave(&ctx);
        assert!(app.save_local()); // Waits for the in-flight snapshot before proceeding.
        assert_eq!(app.workspace, app.last_saved);
        std::fs::remove_dir_all(app.data_path).unwrap();
    }

    #[test]
    fn encrypted_autosave_uses_the_unlocked_key_and_flushes_before_vault_switch() {
        let (mut app, ctx) = harness();
        let plain = app.current_vault().unwrap().clone();
        let plan = app.workspace.finance.clone();
        let key = VaultKey::new("autosave-test-password").unwrap();
        let vault = app
            .library
            .create_vault("Protected", &plan, Some(&key))
            .unwrap();
        app.switch_vault(vault.clone()).unwrap();
        app.workspace.finance = plan.clone();
        app.last_saved.finance = plan;
        app.vault_unlocked = true;
        app.vault_key = Some(key.clone());
        app.workspace.finance.meta.name = "Saved in encrypted vault".into();
        app.start_autosave(&ctx);
        assert!(app.save_job.is_some());
        app.switch_vault(plain).unwrap();
        assert!(app.save_job.is_none());
        let path = app.library.vault_path(&vault.entry.id).unwrap();
        assert_eq!(
            key.load(&path, &vault.entry.id).unwrap().meta.name,
            "Saved in encrypted vault"
        );
        assert!(!path.join("records").exists());
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
}
