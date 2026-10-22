//! Exchange refresh belongs to Finance. Directory polling is intentionally disabled.
use super::*;

pub(super) struct SyncState {
    last_rates: Instant,
}
impl Default for SyncState {
    fn default() -> Self {
        Self {
            last_rates: Instant::now(),
        }
    }
}
impl Amigo {
    pub(super) fn save_exchange_interval(&mut self, seconds: u64) -> Result<(), String> {
        if self.save_blocked {
            return Err("Open the library before saving settings.".into());
        }
        let previous = self.library.settings.sync.exchange_rate_interval_seconds;
        self.library.settings.sync.exchange_rate_interval_seconds = seconds.clamp(1, 86_400);
        if let Err(error) = self.library.save_settings() {
            self.library.settings.sync.exchange_rate_interval_seconds = previous;
            return Err(error);
        }
        self.finance.exchange_interval_seconds =
            self.library.settings.sync.exchange_rate_interval_seconds;
        self.sync.last_rates = Instant::now();
        Ok(())
    }
    pub(super) fn tick_sync(&mut self, ctx: &egui::Context, now: Instant) {
        let interval = Duration::from_secs(
            self.library
                .settings
                .sync
                .exchange_rate_interval_seconds
                .clamp(1, 86_400),
        );
        if now.saturating_duration_since(self.sync.last_rates) >= interval {
            self.sync.last_rates = now;
            if !self.save_blocked
                && self.vault_unlocked
                && self.vault_job.is_none()
                && !self.workspace.finance.tracked_currencies().is_empty()
            {
                self.finance.sync_rates(&self.workspace.finance, ctx);
            }
        }
        ctx.request_repaint_after(
            interval.saturating_sub(now.saturating_duration_since(self.sync.last_rates)),
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::harness;
    #[test]
    fn finance_refresh_setting_persists_and_old_directory_setting_is_removed() {
        let (mut app, _) = harness();
        app.handle_vault_action(VaultAction::SaveSettings {
            vault_name: None,
            exchange_interval_seconds: 120,
        });
        let reopened = Amigo::load(app.data_path.clone());
        assert_eq!(reopened.finance.exchange_interval_seconds, 120);
        let mut legacy = serde_json::to_value(&app.library.settings).unwrap();
        legacy["sync"]["directory_interval_seconds"] = 5.into();
        let migrated: amigo_core::library::Settings = serde_json::from_value(legacy).unwrap();
        let serialized = serde_json::to_string(&migrated).unwrap();
        assert!(!serialized.contains("directory_interval"));
        assert_eq!(migrated.sync.exchange_rate_interval_seconds, 120);
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn timers_do_not_reload_directories_and_locked_vaults_do_not_sync_rates() {
        let (mut app, ctx) = harness();
        let baseline = app.workspace.clone();
        let mut external = baseline.clone();
        let mut task = Task::new(1, "todo");
        task.title = "External task".into();
        external.tasks.push(task);
        app.library
            .save_board(&app.library.settings.board, &external)
            .unwrap();
        app.vault_unlocked = false;
        app.tick_sync(&ctx, Instant::now() + Duration::from_secs(86_400));
        assert_eq!(app.workspace, baseline);
        assert!(!app.finance.rates_loading());
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
}
