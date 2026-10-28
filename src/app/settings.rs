//! One application settings window. Feature editors keep their existing draft/validation logic.
use super::*;
use amigo_finance::FinanceReference;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SettingsPage {
    #[default]
    Tasks,
    Board(String),
    Calendar,
    Finance,
    Vault(String),
    Reference(String, FinanceReference),
    FinanceReference(FinanceReference),
    Expenses(String),
    Savings(String),
    Journal,
    Updates,
}
pub(super) struct AppSettings {
    page: SettingsPage,
    reveal_page: bool,
    expanded_vaults: BTreeSet<String>,
    boards: BTreeMap<String, amigo_tasks::BoardSettingsDraft>,
    finance: BTreeMap<SettingsPage, amigo_finance::AppSettingsDraft>,
    references: BTreeMap<SettingsPage, FinanceTab>,
    shared_finance: FinancePlan,
    shared_saved: FinancePlan,
    shared_used: std::collections::BTreeSet<String>,
    shared_error: String,
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
        let shared = match self.library.finance_settings() {
            Ok(shared) => shared,
            Err(error) => {
                self.error = error;
                return;
            }
        };
        let mut state = self.app_settings.take().unwrap_or_else(|| AppSettings {
            page: SettingsPage::Tasks,
            reveal_page: true,
            expanded_vaults: BTreeSet::new(),
            boards: BTreeMap::new(),
            finance: BTreeMap::new(),
            references: BTreeMap::new(),
            shared_finance: shared.plan(),
            shared_saved: shared.plan(),
            shared_used: shared.used(&self.library),
            shared_error: String::new(),
            exchange_interval: self.library.settings.sync.exchange_rate_interval_seconds,
            journal: self.library.settings.journal.clone(),
            update_preferences: self.library.settings.updates.clone(),
        });
        self.stash_settings(&mut state);
        state.page = match page {
            SettingsPage::Reference(_, reference) if reference.is_global() => {
                SettingsPage::FinanceReference(reference)
            }
            page => page,
        };
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
        state.reveal_page = true;
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
                SettingsPage::FinanceReference(_) => {
                    let shared = self.library.finance_settings()?;
                    state.shared_finance = shared.plan();
                    state.shared_saved = state.shared_finance.clone();
                    state.shared_used = shared.used(&self.library);
                    if self.vault_unlocked {
                        state
                            .shared_used
                            .extend(amigo_core::finance_settings::used_references(
                                &self.workspace.finance,
                            ));
                        state
                            .shared_used
                            .extend(self.finance.pending_reference_ids());
                    }
                }
                SettingsPage::Calendar => {
                    if self.productivity.settings.is_none() {
                        self.calendar_settings();
                    }
                }
                SettingsPage::Vault(id)
                | SettingsPage::Reference(id, _)
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
                    state.expanded_vaults.insert(id.clone());
                    if matches!(state.page, SettingsPage::Reference(_, _)) {
                        if !self.vault_unlocked {
                            self.prompt_locked_vault();
                        }
                        return Ok(());
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
        let mut data_action = None;
        let rows = self.settings_rows(&state.expanded_vaults);
        let reveal = std::mem::take(&mut state.reveal_page);
        let modal = design::modal(ctx, egui::Id::new("app-settings")).show(ctx, |ui| {
            ui.set_width(design::dialog_width(ctx, 1100.0));
            ui.visuals_mut().widgets.noninteractive.bg_stroke =
                egui::Stroke::new(1., colors(ui).line);
            let heading = design::data_settings_heading(
                ui,
                "Amigo Settings",
                "Amigo Setup",
                "app-settings",
                false,
                !self.transfers.busy() && self.vault_job.is_none() && !self.updates.installing(),
            );
            close = heading.close.clicked();
            data_action = heading.data_action;
            ui.horizontal_top(|ui| {
                let height = design::dialog_body_height(ctx) + 70.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(190.0, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::Frame::new()
                            .fill(colors(ui).sidebar)
                            .corner_radius(10)
                            .inner_margin(8)
                            .show(ui, |ui| {
                                let navigation = egui::ScrollArea::vertical()
                                    .id_salt("app-settings-pages")
                                    .max_height(height - 16.)
                                    .auto_shrink([false, false])
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                                    )
                                    .show(ui, |ui| {
                                        ui.set_width(174.0);
                                        ui.spacing_mut().item_spacing.y = 2.;
                                        let mut first = true;
                                        for (page, label, icon, depth) in rows {
                                            if depth == 0 && !first {
                                                ui.add_space(4.);
                                                design::settings_separator(ui);
                                                ui.add_space(4.);
                                            }
                                            first = false;
                                            ui.horizontal(|ui| {
                                                ui.push_id((&page, "settings-page"), |ui| {
                                                    let response = ui
                                                        .add_enabled_ui(
                                                            !self.updates.installing(),
                                                            |ui| {
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
                                                            },
                                                        )
                                                        .inner;
                                                    ui.interact(
                                                        response.rect,
                                                        egui::Id::new(("settings-page", &page)),
                                                        egui::Sense::hover(),
                                                    );
                                                    if reveal && state.page == page {
                                                        response.scroll_to_me(Some(
                                                            egui::Align::Center,
                                                        ));
                                                    }
                                                    if response.clicked() {
                                                        next = Some(page.clone());
                                                    }
                                                });
                                            });
                                        }
                                    });
                                ui.interact(
                                    navigation.inner_rect,
                                    egui::Id::new("settings-navigation"),
                                    egui::Sense::hover(),
                                );
                            });
                    },
                );
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width().max(200.0));
                    if self.save_blocked {
                        ui.disable();
                    }
                    let result = self.settings_page_ui(&mut state, ui, &mut next);
                    save |= result.0;
                    close |= result.1;
                });
            });
        });
        if let SettingsPage::Reference(id, _) = &state.page
            && self.vault_unlocked
            && self.library.settings.vault == *id
            && let Some(tab) = state.references.get_mut(&state.page)
        {
            tab.reference_dialogs(&mut self.workspace.finance, ctx);
        }
        if let SettingsPage::FinanceReference(_) = state.page
            && let Some(tab) = state.references.get_mut(&state.page)
        {
            tab.reference_dialogs(&mut state.shared_finance, ctx);
        }
        if state.shared_finance != state.shared_saved {
            match self.library.save_finance_settings(&state.shared_finance) {
                Ok(()) => {
                    state.shared_saved = state.shared_finance.clone();
                    state.shared_error.clear();
                    if self.vault_unlocked {
                        match self.library.share_finance_references(
                            &self.library.settings.vault,
                            &mut self.workspace.finance,
                        ) {
                            Ok(()) => (),
                            Err(error) => {
                                state.shared_error = error;
                                close = false;
                            }
                        }
                    }
                }
                Err(error) => {
                    state.shared_error = error;
                    close = false;
                }
            }
        }
        let preference_page = matches!(
            state.page,
            SettingsPage::Finance | SettingsPage::Journal | SettingsPage::Updates
        );
        let shortcut = keys.dialog(&modal, preference_page);
        close |= shortcut.cancel;
        if save || shortcut.confirm {
            self.save_app_preferences(&state);
        }
        if !state.shared_error.is_empty() && state.shared_finance != state.shared_saved {
            close = false;
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
        if let Some(action) = data_action {
            self.start_setup_transfer(action, ctx);
        }
    }
    fn settings_rows(
        &self,
        expanded_vaults: &BTreeSet<String>,
    ) -> Vec<(SettingsPage, String, Icon, u8)> {
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
        for reference in FinanceReference::GLOBAL {
            rows.push((
                SettingsPage::FinanceReference(reference),
                reference.name().into(),
                reference.icon(),
                1,
            ));
        }
        for vault in &self.library.vaults {
            let id = vault.entry.id.clone();
            rows.push((
                SettingsPage::Vault(id.clone()),
                vault.entry.name.clone(),
                Icon::Wallet,
                1,
            ));
            if expanded_vaults.contains(&id) {
                for reference in FinanceReference::VAULT {
                    rows.push((
                        SettingsPage::Reference(id.clone(), reference),
                        reference.name().into(),
                        reference.icon(),
                        2,
                    ));
                }
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
    fn settings_page_ui(
        &mut self,
        state: &mut AppSettings,
        ui: &mut egui::Ui,
        next: &mut Option<SettingsPage>,
    ) -> (bool, bool) {
        let close = match state.page.clone() {
            SettingsPage::Tasks => {
                ui.heading("Tasks");
                design::hint(
                    ui,
                    "Choose a board in the list to configure its statuses and categories.",
                );
                design::dialog_footer(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        design::dialog_primary(ui, "Done").clicked()
                    })
                    .inner
                })
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
                egui::ScrollArea::vertical()
                    .id_salt("finance-settings-fields")
                    .max_height((design::dialog_body_height(ui.ctx()) - 70.).max(180.))
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        design::settings_group(ui, "Exchange Rates", |ui| {
                            design::settings_row(ui, "Refresh Every", |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut state.exchange_interval)
                                        .range(1..=86_400)
                                        .suffix(" s"),
                                );
                            });
                        });
                        design::settings_group(ui, "Vaults", |ui| {
                            for vault in &self.library.vaults {
                                design::settings_row(ui, &vault.entry.name, |ui| {
                                    let response = design::button(ui, "Manage…");
                                    ui.interact(
                                        response.rect,
                                        egui::Id::new(("vault-config-open", &vault.entry.id)),
                                        egui::Sense::hover(),
                                    );
                                    if response.clicked() {
                                        *next = Some(SettingsPage::Vault(vault.entry.id.clone()));
                                    }
                                });
                            }
                        });
                    });
                return preferences_footer(ui);
            }
            SettingsPage::FinanceReference(page) => {
                ui.heading(page.name());
                ui.add_space(12.);
                let tab = state
                    .references
                    .entry(state.page.clone())
                    .or_insert_with(|| FinanceTab::new(&state.shared_finance));
                tab.protect_references(state.shared_used.clone());
                tab.reference_in(page, &mut state.shared_finance, ui);
                if !state.shared_error.is_empty() {
                    ui.colored_label(colors(ui).red, &state.shared_error);
                }
                let done = design::dialog_footer(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        design::button(ui, "Done").clicked()
                    })
                    .inner
                });
                return (false, done);
            }
            SettingsPage::Reference(id, page) => {
                ui.heading(page.name());
                if !self.vault_unlocked || self.library.settings.vault != *id {
                    ui.label("Unlock this vault to edit its configuration.");
                    if design::button(ui, "Unlock Vault").clicked() {
                        self.prompt_locked_vault();
                    }
                } else {
                    if let Some(vault) = self.current_vault() {
                        design::hint(ui, &vault.entry.name);
                    }
                    ui.add_space(12.);
                    let tab = state
                        .references
                        .entry(state.page.clone())
                        .or_insert_with(|| FinanceTab::new(&self.workspace.finance));
                    tab.reference_in(page, &mut self.workspace.finance, ui);
                }
                let done = design::dialog_footer(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        design::button(ui, "Done").clicked()
                    })
                    .inner
                });
                return (false, done);
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
                        design::settings_group(ui, "Editor", |ui| {
                            design::settings_row(ui, "Default Editor Mode", |ui| {
                                ui.horizontal(|ui| {
                                    amigo_journal::mode_buttons_with_layout(
                                        ui,
                                        &mut state.journal.mode,
                                        "journal-default-mode",
                                        state.journal.preview_layout,
                                    )
                                });
                            });
                            design::settings_separator(ui);
                            design::settings_row(ui, "Font Size", |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut state.journal.font_size)
                                        .range(11.0..=28.0)
                                        .suffix(" px"),
                                );
                            });
                            design::settings_separator(ui);
                            design::checkbox(ui, &mut state.journal.line_numbers, "Line Numbers");
                            design::settings_separator(ui);
                            design::checkbox(ui, &mut state.journal.word_wrap, "Wrap Long Lines");
                        });
                        design::settings_group(ui, "Preview", |ui| {
                            let response = design::checkbox(
                                ui,
                                &mut state.journal.preview_editable,
                                "Enable Editing by Default",
                            );
                            ui.interact(
                                response.rect,
                                egui::Id::new("journal-preview-editable-default"),
                                egui::Sense::hover(),
                            );
                            design::settings_separator(ui);
                            design::settings_row(ui, "Font Size", |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut state.journal.preview_font_size)
                                        .range(11.0..=28.0)
                                        .suffix(" px"),
                                );
                            });
                            design::settings_separator(ui);
                            design::settings_row(ui, "Split Layout", |ui| {
                                ui.horizontal(|ui| {
                                    ui.radio_value(
                                        &mut state.journal.preview_layout,
                                        amigo_core::journal::PreviewLayout::Stacked,
                                        "Stacked",
                                    );
                                    ui.radio_value(
                                        &mut state.journal.preview_layout,
                                        amigo_core::journal::PreviewLayout::SideBySide,
                                        "Side by Side",
                                    );
                                });
                            });
                            design::settings_separator(ui);
                            design::checkbox(
                                ui,
                                &mut state.journal.sync_scroll,
                                "Synchronize Scrolling",
                            );
                        });
                    });
                return preferences_footer(ui);
            }
            SettingsPage::Updates => {
                ui.heading("Updates");
                design::settings_group(ui, "Software Updates", |ui| {
                    ui.label(format!("Installed version: {}", env!("CARGO_PKG_VERSION")));
                    design::settings_separator(ui);
                    design::checkbox(
                        ui,
                        &mut state.update_preferences.automatic_checks,
                        "Check for Updates Automatically",
                    );
                    design::settings_separator(ui);
                    self.updates_ui(ui);
                });
                let result = preferences_footer(ui);
                return result;
            }
        };
        (false, close)
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
