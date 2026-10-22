//! One application settings window. Feature editors keep their existing draft/validation logic.
use super::*;
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SettingsPage {
    #[default]
    Tasks,
    Board(String),
    Calendar,
    Finance,
    Vault(String),
    Expenses(String),
    Savings(String),
    Journal,
    Updates,
}
pub(super) struct AppSettings {
    page: SettingsPage,
    boards: BTreeMap<String, amigo_tasks::BoardSettingsDraft>,
    finance: BTreeMap<SettingsPage, amigo_finance::AppSettingsDraft>,
    exchange_interval: u64,
    journal: amigo_core::journal::Preferences,
    update_preferences: amigo_core::updates::Preferences,
}
impl Amigo {
    pub fn close_settings(&mut self) {
        self.app_settings = None;
        self.tasks.close_settings();
        self.finance.close_settings();
        self.productivity.settings = None;
    }

    pub fn open_settings(&mut self, page: SettingsPage) {
        if self.save_blocked {
            self.error = "Open the library before changing settings.".into();
            return;
        }
        let mut state = self.app_settings.take().unwrap_or_else(|| AppSettings {
            page: SettingsPage::Tasks,
            boards: BTreeMap::new(),
            finance: BTreeMap::new(),
            exchange_interval: self.library.settings.sync.exchange_rate_interval_seconds,
            journal: self.library.settings.journal.clone(),
            update_preferences: self.library.settings.updates.clone(),
        });
        self.stash_settings(&mut state);
        state.page = page;
        self.activate_settings(&mut state);
        self.app_settings = Some(state);
    }
    fn stash_settings(&mut self, state: &mut AppSettings) {
        match &state.page {
            SettingsPage::Board(id) => {
                if let Some(draft) = self.tasks.take_settings() {
                    state.boards.insert(id.clone(), draft);
                }
            }
            SettingsPage::Vault(_) | SettingsPage::Expenses(_) | SettingsPage::Savings(_) => {
                state
                    .finance
                    .insert(state.page.clone(), self.finance.take_settings());
            }
            _ => (),
        }
    }
    fn activate_settings(&mut self, state: &mut AppSettings) {
        let result = (|| -> Result<(), String> {
            match &state.page {
                SettingsPage::Board(id) => {
                    if self.library.settings.board != *id {
                        self.switch_board(id)?;
                    }
                    if let Some(draft) = state.boards.remove(id) {
                        self.tasks.restore_settings(draft);
                    } else {
                        self.tasks.configure_in_app(&self.workspace);
                    }
                }
                SettingsPage::Calendar => {
                    if self.productivity.settings.is_none() {
                        self.calendar_settings();
                    }
                }
                SettingsPage::Vault(id)
                | SettingsPage::Expenses(id)
                | SettingsPage::Savings(id) => {
                    if self.library.settings.vault != *id {
                        let vault = self
                            .library
                            .vaults
                            .iter()
                            .find(|v| v.entry.id == *id)
                            .cloned()
                            .ok_or("Vault no longer exists.")?;
                        self.switch_vault(vault)?;
                    }
                    if let Some(draft) = state.finance.remove(&state.page) {
                        self.finance.restore_settings(draft);
                    } else if self.vault_unlocked {
                        self.initialize_finance_settings(&state.page);
                    } else {
                        self.prompt_locked_vault();
                    }
                }
                _ => (),
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.error = error;
            state.page = SettingsPage::Tasks;
        }
    }
    fn initialize_finance_settings(&mut self, page: &SettingsPage) {
        match page {
            SettingsPage::Expenses(_) => self.finance.configure_categories_in_app(
                amigo_finance::FinanceView::Expenses,
                &self.workspace.finance,
            ),
            SettingsPage::Savings(_) => self.finance.configure_categories_in_app(
                amigo_finance::FinanceView::Savings,
                &self.workspace.finance,
            ),
            _ => self.finance.configure_in_app(&self.workspace.finance),
        }
    }
    pub(super) fn app_settings_ui(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.app_settings.take() else {
            return;
        };
        let keys = design::ActionKeys::capture(ctx);
        let mut close = false;
        let mut save = false;
        let mut next = None;
        let rows = self.settings_rows(&state.page);
        let modal = design::modal(ctx, egui::Id::new("app-settings")).show(ctx, |ui| {
            ui.set_width(design::dialog_width(ctx, 1100.0));
            close = design::settings_heading(ui, "Amigo Settings").clicked();
            ui.horizontal_top(|ui| {
                let height = design::dialog_body_height(ctx) + 70.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(190.0, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("app-settings-pages")
                            .max_height(height)
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                            )
                            .show(ui, |ui| {
                                ui.set_width(190.0);
                                for (page, label, icon, depth) in rows {
                                    ui.horizontal(|ui| {
                                        ui.push_id((&page, "settings-page"), |ui| {
                                            let response = ui
                                                .add_enabled_ui(!self.updates.installing(), |ui| {
                                                    design::nav_row_with_options(
                                                        ui,
                                                        &label,
                                                        icon,
                                                        state.page == page,
                                                        design::NavRowOptions {
                                                            indent: depth as f32 * 10.0,
                                                            ..Default::default()
                                                        },
                                                    )
                                                })
                                                .inner;
                                            ui.interact(
                                                response.rect,
                                                egui::Id::new(("settings-page", &page)),
                                                egui::Sense::hover(),
                                            );
                                            if response.clicked() {
                                                next = Some(page.clone());
                                            }
                                        });
                                    });
                                }
                            });
                    },
                );
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width().max(200.0));
                    if self.save_blocked {
                        ui.disable();
                    }
                    let result = self.settings_page_ui(&mut state, ui);
                    save |= result.0;
                    close |= result.1;
                });
            });
        });
        let preference_page = matches!(
            state.page,
            SettingsPage::Finance | SettingsPage::Journal | SettingsPage::Updates
        );
        let shortcut = keys.dialog(&modal, preference_page);
        close |= shortcut.cancel;
        if save || shortcut.confirm {
            self.save_app_preferences(&state);
        }
        if close {
            self.close_settings();
            return;
        }
        if let Some(page) = next
            && page != state.page
        {
            self.stash_settings(&mut state);
            state.page = page;
            self.activate_settings(&mut state);
        }
        self.app_settings = Some(state);
    }
    fn settings_rows(&self, page: &SettingsPage) -> Vec<(SettingsPage, String, Icon, u8)> {
        let mut rows = vec![(SettingsPage::Tasks, "Tasks".into(), Icon::Board, 0)];
        rows.extend(self.library.boards.iter().map(|board| {
            (
                SettingsPage::Board(board.id.clone()),
                board.name.clone(),
                Icon::Board,
                1,
            )
        }));
        rows.push((SettingsPage::Calendar, "Calendar".into(), Icon::Calendar, 0));
        rows.push((SettingsPage::Finance, "Finance".into(), Icon::Wallet, 0));
        for vault in &self.library.vaults {
            let id = vault.entry.id.clone();
            rows.push((
                SettingsPage::Vault(id.clone()),
                vault.entry.name.clone(),
                Icon::Wallet,
                1,
            ));
            if matches!(page, SettingsPage::Vault(v) | SettingsPage::Expenses(v) | SettingsPage::Savings(v) if *v == id)
            {
                rows.push((
                    SettingsPage::Expenses(id.clone()),
                    "Expenses".into(),
                    Icon::Expenses,
                    2,
                ));
                rows.push((
                    SettingsPage::Savings(id),
                    "Savings".into(),
                    Icon::Savings,
                    2,
                ));
            }
        }
        rows.push((SettingsPage::Journal, "Journal".into(), Icon::Book, 0));
        rows.push((SettingsPage::Updates, "Updates".into(), Icon::Refresh, 0));
        rows
    }
    fn settings_page_ui(&mut self, state: &mut AppSettings, ui: &mut egui::Ui) -> (bool, bool) {
        match state.page.clone() {
            SettingsPage::Tasks => {
                ui.heading("Tasks");
                design::hint(
                    ui,
                    "Choose a board in the list to configure its statuses and categories.",
                );
            }
            SettingsPage::Board(_) => {
                if !self.tasks.settings_open() {
                    self.tasks.configure_in_app(&self.workspace);
                }
                let name = self.current_board_name().to_owned();
                let outcome = self.tasks.settings_in(&mut self.workspace, &name, ui);
                if let Some(name) = outcome.rename {
                    self.handle_tasks_action(TasksAction::RenameBoard(name));
                }
                return (false, outcome.cancelled);
            }
            SettingsPage::Calendar => {
                if self.productivity.settings.is_none() {
                    self.calendar_settings();
                }
                return (false, self.calendar_settings_in(ui));
            }
            SettingsPage::Finance => {
                ui.heading("Finance");
                ui.add_space(16.0);
                design::dialog_section(ui, "Exchange Rates");
                ui.horizontal(|ui| {
                    ui.label("Refresh Every");
                    ui.add(
                        egui::DragValue::new(&mut state.exchange_interval)
                            .range(1..=86_400)
                            .suffix(" s"),
                    );
                });
                return preferences_footer(ui);
            }
            SettingsPage::Vault(_) | SettingsPage::Expenses(_) | SettingsPage::Savings(_) => {
                if !self.vault_unlocked {
                    ui.heading("Vault Locked");
                    if design::button(ui, "Unlock Vault").clicked() {
                        self.prompt_locked_vault();
                    }
                } else if matches!(state.page, SettingsPage::Vault(_)) {
                    if !self.finance.settings_open() {
                        self.initialize_finance_settings(&state.page);
                    }
                    let vault = self.current_vault().cloned();
                    if let Some(action) =
                        self.finance
                            .settings_in(&mut self.workspace.finance, vault.as_ref(), ui)
                    {
                        let lock = action == VaultAction::Lock;
                        self.handle_vault_action(action);
                        if lock {
                            return (false, true);
                        }
                    }
                } else {
                    if !self.finance.categories_open() {
                        self.initialize_finance_settings(&state.page);
                    }
                    self.finance.categories_in(&mut self.workspace.finance, ui);
                }
                return (false, self.finance.take_settings_cancelled());
            }
            SettingsPage::Journal => {
                ui.heading("Journal");
                ui.add_space(16.0);
                egui::ScrollArea::vertical()
                    .id_salt("journal-settings-fields")
                    .max_height((design::dialog_body_height(ui.ctx()) - 70.).max(180.))
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        amigo_ui::forms::field_label(ui, "Default Editor Mode");
                        ui.horizontal(|ui| {
                            amigo_journal::mode_buttons_with_layout(
                                ui,
                                &mut state.journal.mode,
                                "journal-default-mode",
                                state.journal.preview_layout,
                            )
                        });
                        egui::Grid::new("journal-font-sizes")
                            .spacing([24., 12.])
                            .show(ui, |ui| {
                                ui.label("Editor Font Size");
                                ui.add(
                                    egui::DragValue::new(&mut state.journal.font_size)
                                        .range(11.0..=28.0)
                                        .suffix(" px"),
                                );
                                ui.end_row();
                                ui.label("Preview Font Size");
                                ui.add(
                                    egui::DragValue::new(&mut state.journal.preview_font_size)
                                        .range(11.0..=28.0)
                                        .suffix(" px"),
                                );
                                ui.end_row();
                            });
                        ui.horizontal(|ui| {
                            ui.label("Split Layout");
                            ui.radio_value(
                                &mut state.journal.preview_layout,
                                amigo_core::journal::PreviewLayout::SideBySide,
                                "Side by Side",
                            );
                            ui.radio_value(
                                &mut state.journal.preview_layout,
                                amigo_core::journal::PreviewLayout::Stacked,
                                "Stacked",
                            );
                        });
                        design::checkbox(
                            ui,
                            &mut state.journal.sync_scroll,
                            "Synchronize Scrolling",
                        );
                        design::checkbox(ui, &mut state.journal.line_numbers, "Line Numbers");
                        design::checkbox(ui, &mut state.journal.word_wrap, "Wrap Long Lines");
                    });
                return preferences_footer(ui);
            }
            SettingsPage::Updates => {
                ui.heading("Updates");
                ui.label(format!("Installed version: {}", env!("CARGO_PKG_VERSION")));
                design::checkbox(
                    ui,
                    &mut state.update_preferences.automatic_checks,
                    "Check for Updates Automatically",
                );
                let result = preferences_footer(ui);
                self.updates_ui(ui);
                return result;
            }
        }
        (false, false)
    }
    fn save_app_preferences(&mut self, state: &AppSettings) {
        let result = match state.page {
            SettingsPage::Finance => self.save_exchange_interval(state.exchange_interval),
            SettingsPage::Journal => {
                let old = self.library.settings.journal.clone();
                self.library.settings.journal = state.journal.clone();
                self.library
                    .save_settings()
                    .inspect_err(|_| self.library.settings.journal = old)
            }
            SettingsPage::Updates => {
                let old = self.library.settings.updates.automatic_checks;
                self.library.settings.updates.automatic_checks =
                    state.update_preferences.automatic_checks;
                self.library
                    .save_settings()
                    .inspect_err(|_| self.library.settings.updates.automatic_checks = old)
            }
            _ => return,
        };
        match result {
            Ok(()) => self.message = "Settings updated.".into(),
            Err(error) => self.error = error,
        }
    }
}
fn preferences_footer(ui: &mut egui::Ui) -> (bool, bool) {
    design::dialog_footer(ui, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            (
                design::dialog_primary(ui, "Save Settings").clicked(),
                design::button(ui, "Cancel").clicked(),
            )
        })
        .inner
    })
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
