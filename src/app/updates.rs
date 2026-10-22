//! Nonblocking update discovery/download and native installation orchestration.
mod install;
use super::*;
use amigo_connect::updates::{self, Release};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
#[derive(Default)]
pub(super) struct Updates {
    pub enabled: bool,
    started: Option<Instant>,
    last_attempt: Option<Instant>,
    job: Option<Job>,
    release: Option<Release>,
    installation: Option<install::Installation>,
    package: Option<install::Package>,
    status: String,
    error: String,
    notice: bool,
    pending_install: bool,
    closing: bool,
}
struct Job {
    receiver: mpsc::Receiver<Result<Outcome, String>>,
    kind: Kind,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU64>,
    size: u64,
}
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Check,
    Download,
    Install,
}
enum Outcome {
    Checked(Option<Release>, install::Installation),
    Downloaded(install::Package),
    Installing,
}
impl Updates {
    pub(super) fn installing(&self) -> bool {
        self.pending_install || self.applying()
    }
    pub(super) fn applying(&self) -> bool {
        self.closing
            || self
                .job
                .as_ref()
                .is_some_and(|job| job.kind == Kind::Install)
    }
}
impl Drop for Updates {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}
impl Amigo {
    fn update_job(
        &mut self,
        ctx: &egui::Context,
        kind: Kind,
        size: u64,
        work: impl FnOnce(Arc<AtomicBool>, Arc<AtomicU64>) -> Result<Outcome, String> + Send + 'static,
    ) {
        if self.updates.job.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicU64::new(0));
        self.updates.job = Some(Job {
            receiver,
            kind,
            cancel: cancel.clone(),
            progress: progress.clone(),
            size,
        });
        self.updates.error.clear();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(work(cancel, progress));
            ctx.request_repaint();
        });
    }
    pub(super) fn check_updates(&mut self, ctx: &egui::Context) {
        self.updates.last_attempt = Some(Instant::now());
        self.updates.status = "Checking GitHub…".into();
        self.update_job(ctx, Kind::Check, 0, |_, _| {
            let installation = install::detect()?;
            let release = updates::latest(env!("CARGO_PKG_VERSION"), &installation.target)?;
            Ok(Outcome::Checked(release, installation))
        });
    }
    fn download_update(&mut self, ctx: &egui::Context) {
        let Some(asset) = self
            .updates
            .release
            .as_ref()
            .and_then(|r| r.package.clone())
        else {
            return;
        };
        self.updates.status = "Downloading update…".into();
        self.update_job(ctx, Kind::Download, asset.size, move |cancel, progress| {
            install::download(asset, cancel, progress).map(Outcome::Downloaded)
        });
    }
    fn install_update(&mut self, ctx: &egui::Context) {
        if self.updates.job.is_some() {
            return;
        }
        self.updates.pending_install = false;
        if self
            .journal
            .editor
            .as_ref()
            .is_some_and(|e| e.conflict && e.dirty())
        {
            self.updates.error =
                "Resolve the Journal file conflict before installing the update.".into();
            self.updates.pending_install = false;
            return;
        }
        if self.journal.busy || self.journal.dirty() {
            self.updates.pending_install = true;
            self.updates.status = "Saving Journal notes before updating…".into();
            return;
        }
        if !self.finance.drafts.is_empty() {
            self.updates.error =
                "Save or cancel your unfinished finance edits before restarting to update.".into();
            return;
        }
        if !self.save_local() {
            self.updates.error = self.error.clone();
            return;
        }
        let (Some(package), Some(installation)) = (
            self.updates.package.take(),
            self.updates.installation.clone(),
        ) else {
            return;
        };
        self.updates.pending_install = false;
        self.updates.status = "Installing update… Respond to any system permission prompt.".into();
        self.update_job(ctx, Kind::Install, 0, move |_, _| {
            install::apply(package, installation)?;
            Ok(Outcome::Installing)
        });
    }
    pub(super) fn tick_updates(&mut self, ctx: &egui::Context) {
        let started = *self.updates.started.get_or_insert_with(Instant::now);
        if let Some(job) = self.updates.job.take() {
            match job.receiver.try_recv() {
                Ok(Ok(Outcome::Checked(release, installation))) => {
                    self.updates.installation = Some(installation);
                    self.updates.package = None;
                    if let Some(release) = &release {
                        self.updates.status = format!("Amigo {} is available.", release.version);
                        if self.library.settings.updates.notified_version != release.version {
                            self.updates.notice = true;
                            self.library.settings.updates.notified_version =
                                release.version.clone();
                        }
                    } else {
                        self.updates.status = "You are using the latest release.".into();
                    }
                    self.updates.release = release;
                    self.library.settings.updates.last_checked = chrono::Utc::now().timestamp();
                    if let Err(error) = self.library.save_settings() {
                        self.updates.error = format!("Cannot save update preferences: {error}");
                    }
                }
                Ok(Ok(Outcome::Downloaded(package))) => {
                    self.updates.package = Some(package);
                    self.updates.status = "Update verified and ready to install.".into();
                }
                Ok(Ok(Outcome::Installing)) => {
                    self.updates.closing = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Ok(Err(error)) => {
                    self.updates.error = error;
                    self.updates.status.clear();
                    self.updates.pending_install = false;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    self.updates.job = Some(job);
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.updates.error = "The update worker stopped. Retry the update.".into();
                    self.updates.pending_install = false;
                }
            }
        }
        if self.updates.pending_install
            && !self.journal.busy
            && (!self.journal.dirty() || self.journal.editor.as_ref().is_some_and(|e| e.conflict))
        {
            self.install_update(ctx);
        }
        if self.updates.enabled
            && self.library.settings.updates.automatic_checks
            && self.updates.job.is_none()
            && !self.save_blocked
            && self.updates.package.is_none()
            && chrono::Utc::now()
                .timestamp()
                .saturating_sub(self.library.settings.updates.last_checked)
                >= 86_400
            && started.elapsed() >= Duration::from_secs(5)
            && self
                .updates
                .last_attempt
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(3600))
        {
            self.check_updates(ctx);
        }
        if self.updates.enabled {
            ctx.request_repaint_after(Duration::from_secs(5));
        }
    }
    pub(super) fn installation_ui(&self, ui: &mut egui::Ui) {
        if let Some(id) = ui.memory(|m| m.focused()) {
            ui.memory_mut(|m| m.surrender_focus(id));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.spinner();
                    ui.heading("Installing Update");
                    ui.label(&self.updates.status);
                    design::hint(ui, "Amigo will restart when installation finishes.");
                });
            });
        });
    }
    pub(super) fn updates_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.);
        if !self.updates.status.is_empty() {
            ui.label(&self.updates.status);
        }
        if !self.updates.error.is_empty() {
            ui.colored_label(colors(ui).red, &self.updates.error);
        }
        if let Some(job) = &self.updates.job {
            match job.kind {
                Kind::Download => {
                    ui.add(
                        egui::ProgressBar::new(
                            (job.progress.load(Ordering::Relaxed) as f64 / job.size.max(1) as f64)
                                as f32,
                        )
                        .show_percentage(),
                    );
                    if design::button(ui, "Cancel Download").clicked() {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                }
                _ => {
                    ui.spinner();
                }
            }
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        let busy = self.updates.job.is_some() || self.updates.closing;
        if ui
            .add_enabled_ui(!busy, |ui| design::button(ui, "Check for Updates"))
            .inner
            .clicked()
        {
            self.check_updates(ui.ctx());
        }
        if let Some(release) = self.updates.release.clone() {
            ui.hyperlink_to(format!("Release {}", release.version), &release.url);
            if release.package.is_none() {
                design::hint(
                    ui,
                    "This release does not include an automatic update package for your installation. See the release page for downloads.",
                );
            } else if self.updates.package.is_some() {
                if ui
                    .add_enabled_ui(!busy, |ui| design::primary(ui, "Install and Restart"))
                    .inner
                    .clicked()
                {
                    self.install_update(ui.ctx());
                }
            } else if ui
                .add_enabled_ui(!busy, |ui| design::primary(ui, "Download Update"))
                .inner
                .clicked()
            {
                self.download_update(ui.ctx());
            }
            ui.collapsing("Release Notes", |ui| {
                egui::ScrollArea::vertical()
                    .max_height(180.)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(&release.notes).wrap());
                    });
            });
        }
    }
    pub(super) fn update_notice(&mut self, ctx: &egui::Context) {
        if !self.updates.notice {
            return;
        }
        let Some(release) = &self.updates.release else {
            return;
        };
        let version = release.version.clone();
        let mut open = true;
        let mut view = false;
        egui::Window::new("Update Available")
            .id(egui::Id::new("update-notification"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [-20., -20.])
            .resizable(false)
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(format!("Amigo {version} is ready to download."));
                ui.horizontal(|ui| {
                    view = design::primary(ui, "View Update").clicked();
                    if design::button(ui, "Later").clicked() {
                        self.updates.notice = false;
                    }
                });
            });
        if !open {
            self.updates.notice = false;
        }
        if view {
            self.updates.notice = false;
            self.open_settings(SettingsPage::Updates);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_install_returns_to_editor_when_a_journal_save_conflicts() {
        let (mut app, ctx) = crate::app::tests::harness();
        app.journal.open(amigo_core::journal::Document {
            path: "Note.md".into(),
            text: "Original".into(),
            fingerprint: "test".into(),
        });
        let editor = app.journal.editor.as_mut().unwrap();
        editor.document.text = "Unsaved".into();
        editor.conflict = true;
        app.updates.pending_install = true;
        app.tick_updates(&ctx);
        assert!(!app.updates.installing());
        assert!(app.updates.error.contains("conflict"));
        assert_eq!(
            app.journal.editor.as_ref().unwrap().document.text,
            "Unsaved"
        );
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn update_notice_opens_the_single_settings_window() {
        let (mut app, ctx) = crate::app::tests::harness();
        app.updates.release = Some(Release {
            version: "9.0.0".into(),
            url: format!("{}/releases/tag/v9.0.0", updates::REPOSITORY),
            notes: "New features".into(),
            package: None,
        });
        app.updates.notice = true;
        for _ in 0..4 {
            crate::app::tests::render(&mut app, &ctx, vec![]);
        }
        let output = crate::app::tests::render(&mut app, &ctx, vec![]);
        let button = crate::app::tests::text_rect(&output, "View Update");
        crate::app::tests::click(&mut app, &ctx, button.center());
        assert!(!app.updates.notice);
        assert!(app.app_settings.is_some());
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
}
