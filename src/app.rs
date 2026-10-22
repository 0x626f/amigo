use amigo_core::{
    library::{self, Library, Vault, VaultKey},
    model::*,
    storage,
};
#[cfg(test)]
use amigo_finance::{self as finance_ui, FinanceView};
use amigo_finance::{FinanceTab, VaultAction};
use amigo_tasks::{TasksAction, TasksTab};
use amigo_ui::design::{hint, panel, primary};
use amigo_ui::{
    design::{self, Icon, colors},
    forms::text_field,
};
#[cfg(test)]
use eframe::egui::Vec2;
use eframe::egui::{self, RichText};
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

mod autosave;
mod journal;
mod library_ui;
mod settings;
mod updates;
pub use settings::SettingsPage;
mod loading;
mod notifications;
mod organizer;
mod sync;
mod transfers;
use library_ui::{LibraryDialog, VaultJob};
use sync::SyncState;

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub enum Section {
    #[default]
    Tasks,
    Calendar,
    Finance,
    Journal,
}
pub struct Amigo {
    pub workspace: Workspace,
    pub tasks: TasksTab,
    pub finance: FinanceTab,
    pub calendar: amigo_calendar::CalendarTab,
    pub organizer: amigo_core::organizer::Organizer,
    productivity: organizer::Productivity,
    pub section: Section,
    pub journal: amigo_journal::JournalTab,
    app_settings: Option<settings::AppSettings>,
    updates: updates::Updates,
    journal_io: journal::JournalIo,
    library: Library,
    vault_key: Option<VaultKey>,
    vault_unlocked: bool,
    library_dialog: Option<LibraryDialog>,
    vault_job: Option<VaultJob>,
    data_path: PathBuf,
    legacy_path: Option<PathBuf>,
    last_saved: Workspace,
    save_job: Option<autosave::SaveJob>,
    last_save: Instant,
    save_blocked: bool,
    sync: SyncState,
    page_loader: loading::PageLoader,
    transfers: transfers::Transfers,
    notifications: notifications::Notifications,
    pub message: String,
    pub error: String,
}
impl Amigo {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::configure(&cc.egui_ctx);
        let mut app = Self::load_library(library::default_root(), Some(storage::default_path()));
        app.updates.enabled = std::env::var_os("AMIGO_DATA_FILE").is_none()
            && std::env::var_os("AMIGO_DISABLE_UPDATES").is_none();
        if let Ok(error) = std::env::var("AMIGO_UPDATE_ERROR") {
            app.error = error;
        }
        design::apply_theme(&cc.egui_ctx, app.workspace.dark_mode);
        app
    }
    #[cfg(test)]
    fn load(data_path: PathBuf) -> Self {
        Self::load_library(data_path, None)
    }
    fn load_library(data_path: PathBuf, legacy: Option<PathBuf>) -> Self {
        let (mut library, mut error, mut blocked) =
            match Library::open(data_path.clone(), legacy.as_deref()) {
                Ok(library) => (library, String::new(), false),
                Err(error) => (Library::unavailable(data_path.clone()), error, true),
            };
        if !library
            .boards
            .iter()
            .any(|b| b.id == library.settings.board)
        {
            library.settings.board = library
                .boards
                .first()
                .map(|b| b.id.clone())
                .unwrap_or_default();
        }
        if !library
            .vaults
            .iter()
            .any(|v| v.entry.id == library.settings.vault)
        {
            library.settings.vault = library
                .vaults
                .first()
                .map(|v| v.entry.id.clone())
                .unwrap_or_default();
        }
        let mut workspace = if library.settings.board.is_empty() {
            Workspace::default()
        } else {
            match library.load_board(&library.settings.board) {
                Ok(workspace) => workspace,
                Err(e) => {
                    error = e;
                    blocked = true;
                    Workspace::default()
                }
            }
        };
        workspace.dark_mode = library.settings.dark_mode;
        let mut vault_unlocked = false;
        if let Some(vault) = library
            .vaults
            .iter()
            .find(|v| v.entry.id == library.settings.vault)
            && !vault.encrypted
        {
            match library.load_plain_vault(vault) {
                Ok(plan) => {
                    workspace.finance = plan;
                    vault_unlocked = true;
                }
                Err(e) => error = e,
            }
        }
        let last_saved = workspace.clone();
        // Persist record migrations and account-property cleanup once, even offline.
        if vault_unlocked {
            workspace.finance.meta.record_metadata_version = RECORD_METADATA_VERSION;
        }
        let mut finance = FinanceTab::new(&workspace.finance);
        finance.exchange_interval_seconds = library.settings.sync.exchange_rate_interval_seconds;
        let (organizer, productivity, organizer_error) = organizer::Productivity::load(&data_path);
        if !organizer_error.is_empty() {
            error = organizer_error;
        }
        Self {
            last_saved,
            save_job: None,
            workspace,
            tasks: TasksTab::default(),
            calendar: Default::default(),
            organizer,
            productivity,
            finance,
            library,
            vault_key: None,
            vault_unlocked,
            library_dialog: None,
            vault_job: None,
            section: Section::Tasks,
            journal: Default::default(),
            app_settings: None,
            updates: Default::default(),
            journal_io: Default::default(),
            data_path,
            legacy_path: legacy,
            last_save: Instant::now(),
            save_blocked: blocked,
            sync: SyncState::default(),
            page_loader: loading::PageLoader::default(),
            transfers: transfers::Transfers::default(),
            notifications: notifications::Notifications::default(),
            message: String::new(),
            error,
        }
    }
    fn configure(ctx: &egui::Context) {
        design::install(ctx);
    }
    fn save_local(&mut self) -> bool {
        if self.save_blocked {
            return false;
        }
        if !self.flush_autosave() {
            return false;
        }
        if !self.save_organizer() {
            return false;
        }
        if self.workspace == self.last_saved {
            return true;
        }
        self.workspace.normalize();
        let result = (|| {
            let old = &self.last_saved;
            if self.workspace.tasks != old.tasks
                || self.workspace.task_statuses != old.task_statuses
                || self.workspace.task_categories != old.task_categories
                || self.workspace.next_task != old.next_task
            {
                self.library
                    .save_board(&self.library.settings.board, &self.workspace)?;
            }
            if self.workspace.finance != old.finance {
                if !self.vault_unlocked {
                    return Err("The vault is locked; finance has not been saved.".into());
                }
                let vault = self.current_vault().ok_or("No vault is open.")?;
                self.library
                    .save_vault(vault, &self.workspace.finance, self.vault_key.as_ref())?;
            }
            self.library.settings.dark_mode = self.workspace.dark_mode;
            self.library.save_settings()
        })();
        self.last_save = Instant::now();
        match result {
            Ok(()) => {
                self.last_saved = self.workspace.clone();
                true
            }
            Err(error) => {
                self.error = format!("Local save failed: {error}");
                false
            }
        }
    }
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("navigation")
            .exact_size(224.)
            .resizable(false)
            .frame(egui::Frame::new().fill(colors(ui).sidebar).inner_margin(18))
            .show(ui, |ui| {
                ui.add_space(14.);
                ui.horizontal(|ui| {
                    ui.add_space(26.);
                    crate::brand::logo(ui, 34.);
                    ui.label(RichText::new("Amigo").size(20.).strong());
                });
                ui.add_space(10.);
                ui.horizontal(|ui| {
                    ui.add_space(24.);
                    if design::icon_button(
                        ui,
                        egui::Id::new("app-settings-open"),
                        Icon::Settings,
                        "App Settings",
                        32.,
                    )
                    .clicked()
                    {
                        self.open_settings(SettingsPage::General);
                    }
                    for (name, icon, label, action) in [
                        (
                            "import",
                            Icon::Import,
                            "Import Amigo Setup",
                            design::DataAction::Import,
                        ),
                        (
                            "export",
                            Icon::Export,
                            "Export Amigo Setup",
                            design::DataAction::Export,
                        ),
                    ] {
                        if design::icon_button(
                            ui,
                            egui::Id::new(format!("app-settings-{name}")),
                            icon,
                            label,
                            32.,
                        )
                        .clicked()
                        {
                            self.start_setup_transfer(action, ui.ctx());
                        }
                    }
                });
                ui.add_space(12.);
                ui.separator();
                ui.add_space(12.);
                egui::ScrollArea::vertical()
                    .id_salt("sidebar-navigation-scroll")
                    .max_height((ui.available_height() - 52.).max(100.))
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        design::eyebrow(ui, "WORKSPACE");
                        ui.add_space(3.);
                        for (section, label, icon) in [
                            (Section::Tasks, "Tasks", Icon::Board),
                            (Section::Calendar, "Calendar", Icon::Calendar),
                            (Section::Finance, "Finance", Icon::Wallet),
                            (Section::Journal, "Journal", Icon::Book),
                        ] {
                            if design::nav_row(ui, label, icon, self.section == section, None)
                                .clicked()
                            {
                                self.section = section;
                                self.tasks.cancel_card_interaction();
                                if section == Section::Finance {
                                    self.prompt_locked_vault();
                                }
                            }
                        }
                        ui.add_space(24.);
                        match self.section {
                            Section::Tasks | Section::Finance => self.library_navigation(ui),
                            Section::Calendar => {
                                if let Some(action) = self.calendar.sidebar(&self.organizer, ui) {
                                    self.calendar_action(action);
                                }
                            }
                            Section::Journal => {
                                if let Some(action) = self.journal.sidebar(ui) {
                                    self.journal_action(action, ui.ctx());
                                }
                            }
                        }
                        ui.add_space(16.);
                        if self.section == Section::Finance && self.vault_unlocked {
                            self.finance.sidebar(ui);
                        }
                    });
                ui.add_space(8.);
                ui.horizontal(|ui| {
                    let response = ui
                        .scope(|ui| {
                            design::segmented(ui, &mut self.workspace.dark_mode, "Light", "Dark")
                        })
                        .response;
                    ui.interact(
                        response.rect,
                        egui::Id::new("app-theme-switcher"),
                        egui::Sense::hover(),
                    );
                });
            });
    }
    pub fn new_task(&mut self, status: &str) {
        self.tasks.new_task(&self.workspace, status);
    }
    pub fn manage_board(&mut self) {
        self.open_settings(SettingsPage::Board(self.library.settings.board.clone()));
    }
    fn render(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        design::apply_theme(&ctx, self.workspace.dark_mode);
        ui.set_style(ctx.global_style());
        self.poll_transfer();
        if self.transfers.busy() {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.spinner();
                        ui.label("Working with your Amigo file…");
                    });
                });
            });
            ctx.request_repaint_after(Duration::from_millis(50));
            self.show_notifications(&ctx);
            return;
        }
        self.tick_journal(&ctx);
        self.journal_close_request(&ctx);
        self.tick_updates(&ctx);
        self.poll_vault_job();
        self.poll_autosave();
        self.finance.poll_rates(&mut self.workspace.finance);
        self.tick_sync(&ctx, Instant::now());
        self.tick_organizer(&ctx);
        if self.save_blocked {
            self.page_loader.cancel();
        }
        self.sidebar(ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(colors(ui).canvas).inner_margin(28))
            .show(ui, |ui| {
                design::page_transition(
                    ui,
                    (
                        &self.section,
                        self.finance.view,
                        self.finance.expense_group,
                        &self.library.settings.board,
                        &self.library.settings.vault,
                    ),
                );
                if self.save_blocked {
                    hint(
                        ui,
                        "The library could not be opened. Existing files are preserved.",
                    );
                    if design::button(ui, "Retry Opening Library").clicked() {
                        self.retry_library();
                    }
                } else if self.prepare_page(ui) {
                    // The sidebar and dialogs stay interactive during preparation.
                } else if self.section == Section::Tasks {
                    let name = self.current_board_name().to_owned();
                    self.tasks.set_board_folder(
                        self.library.board_path(&self.library.settings.board).ok(),
                    );
                    self.tasks.show(&mut self.workspace, &name, ui);
                } else if self.section == Section::Calendar {
                    let result = ui
                        .add_enabled_ui(self.productivity.available(), |ui| {
                            self.calendar.show(&self.organizer, ui)
                        })
                        .inner;
                    if let Some(action) = result {
                        self.calendar_action(action);
                    }
                } else if self.section == Section::Journal {
                    let previous = self.library.settings.journal.clone();
                    if let Some(action) = self.journal.show(ui, &mut self.library.settings.journal)
                    {
                        self.journal_action(action, &ctx);
                    }
                    if self.library.settings.journal != previous
                        && let Err(error) = self.library.save_settings()
                    {
                        self.error = error;
                    }
                } else if self.vault_unlocked {
                    let vault = self.current_vault().cloned();
                    self.finance
                        .show(&mut self.workspace.finance, vault.as_ref(), ui);
                } else {
                    self.locked_vault_ui(ui);
                }
            });
        if self.section == Section::Tasks
            && let Some(action) = self.tasks.dialogs(&mut self.workspace, &ctx)
        {
            self.handle_tasks_action(action);
        }
        if self.section == Section::Finance && self.vault_unlocked {
            let vault = self.current_vault().cloned();
            if let Some(action) =
                self.finance
                    .dialogs(&mut self.workspace.finance, vault.as_ref(), &ctx)
            {
                self.handle_vault_action(action);
                ctx.request_repaint();
            }
        }
        if self.section == Section::Calendar
            && let Some(action) =
                self.calendar
                    .dialogs(&mut self.organizer, &ctx, self.productivity.busy())
        {
            self.calendar_action(action);
        }
        self.journal_dialogs(&ctx);
        self.app_settings_ui(&ctx);
        self.library_dialog_ui(&ctx);
        self.transfer_ui(&ctx);
        if self.workspace != self.last_saved || self.organizer != self.productivity.saved {
            if self.last_save.elapsed() >= Duration::from_millis(800) {
                self.start_autosave(&ctx);
            }
            ctx.request_repaint_after(Duration::from_millis(800));
        }
        if self.vault_job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.save_job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.finance.rates_loading() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        self.show_notifications(&ctx);
        self.update_notice(&ctx);
    }
}
impl eframe::App for Amigo {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render(ui);
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.finish_journal();
        self.finish_transfer();
        self.save_local();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn harness() -> (Amigo, egui::Context) {
        let app = Amigo::load(std::env::temp_dir().join(format!("amigo-ui-{}.json", id())));
        let ctx = egui::Context::default();
        Amigo::configure(&ctx);
        (app, ctx)
    }
    #[test]
    fn legacy_record_metadata_is_saved_once_without_exchange_network_access() {
        let (app, _) = harness();
        let vault = app.current_vault().unwrap().clone();
        let plan = amigo_core::model::plan_from_value(serde_json::json!({
            "meta":{"name":"Legacy","reportingCurrency":"EUR"},
            "income":[{"id":"legacy-income","date":"2026-09-10","currency":"USD","grossAmount":100}],
            "savings":[{"id":"legacy-saving","currency":"EUR","labels":["Reserve"]}]
        })).unwrap();
        app.library.save_vault(&vault, &plan, None).unwrap();
        let mut reopened = Amigo::load(app.data_path.clone());
        assert_eq!(
            reopened.workspace.finance.meta.record_metadata_version,
            RECORD_METADATA_VERSION
        );
        assert_eq!(reopened.last_saved.finance.meta.record_metadata_version, 0);
        assert!(reopened.workspace.finance.income[0].audit.inferred);
        assert_eq!(
            reopened.workspace.finance.income[0].audit.date(),
            "2026-09-10"
        );
        assert_eq!(reopened.workspace.finance.savings[0].category, "Reserve");
        assert!(!reopened.finance.rates_loading());
        assert!(reopened.save_local());
        let saved = reopened.library.load_plain_vault(&vault).unwrap();
        assert_eq!(saved, reopened.workspace.finance);
        let again = Amigo::load(app.data_path.clone());
        assert_eq!(again.workspace, again.last_saved);
        assert_eq!(again.workspace.finance, saved);
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    pub(super) fn render(
        app: &mut Amigo,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        render_sized(app, ctx, events, Vec2::new(1440.0, 900.0))
    }
    pub(super) fn render_sized(
        app: &mut Amigo,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        size: Vec2,
    ) -> egui::FullOutput {
        let mut output = render_frame_sized(app, ctx, events, size);
        let started = Instant::now();
        while app.is_page_loading() {
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "Page loading timed out"
            );
            std::thread::sleep(Duration::from_millis(1));
            output = render_frame_sized(app, ctx, vec![], size);
        }
        output
    }
    pub(super) fn render_frame(
        app: &mut Amigo,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        render_frame_sized(app, ctx, events, Vec2::new(1440.0, 900.0))
    }
    fn render_frame_sized(
        app: &mut Amigo,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        size: Vec2,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| app.render(ui),
        );
        output.textures_delta.clear();
        output
    }

    #[test]
    fn previous_account_properties_are_removed_once_without_changing_account_ids() {
        let (app, ctx) = harness();
        let vault = app.current_vault().unwrap().clone();
        let mut plan = app.workspace.finance.clone();
        plan.accounts.push(Account {
            id: "legacy-account".into(),
            label: "Legacy Bank".into(),
            audit: RecordAudit::new(),
        });
        let mut income = Income::new("EUR");
        income.account_id = "legacy-account".into();
        income.account = "Legacy Bank".into();
        plan.income.push(income);
        plan.normalize();
        plan.meta.record_metadata_version = 2;
        app.library.save_vault(&vault, &plan, None).unwrap();
        let folder = app
            .library
            .vault_path(&vault.entry.id)
            .unwrap()
            .join("records/accounts");
        let file = std::fs::read_dir(folder)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                std::fs::read_to_string(path)
                    .unwrap()
                    .contains("legacy-account")
            })
            .unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        let (metadata, body) = text
            .strip_prefix("---\n")
            .unwrap()
            .split_once("\n---\n")
            .unwrap();
        let mut value: serde_json::Value = serde_json::from_str(metadata).unwrap();
        value["country"] = "Ukraine".into();
        value["principal"] = "Old account holder".into();
        value["comment"] = "Old account comment".into();
        std::fs::write(
            &file,
            format!(
                "---\n{}\n---\n{body}",
                serde_json::to_string_pretty(&value).unwrap()
            ),
        )
        .unwrap();
        let mut reopened = Amigo::load(app.data_path.clone());
        assert_eq!(reopened.last_saved.finance.meta.record_metadata_version, 2);
        assert_eq!(
            reopened.workspace.finance.meta.record_metadata_version,
            RECORD_METADATA_VERSION
        );
        reopened.start_autosave(&ctx);
        assert!(reopened.flush_autosave());
        let text = std::fs::read_to_string(&file).unwrap();
        let value: serde_json::Value = serde_json::from_str(
            text.strip_prefix("---\n")
                .unwrap()
                .split_once("\n---\n")
                .unwrap()
                .0,
        )
        .unwrap();
        for removed in ["country", "principal", "comment"] {
            assert!(value.get(removed).is_none());
        }
        assert_eq!(value["id"], "legacy-account");
        assert_eq!(value["label"], "Legacy Bank");
        assert_eq!(
            reopened.workspace.finance.income[0].account_id,
            "legacy-account"
        );
        let again = Amigo::load(app.data_path.clone());
        assert_eq!(again.workspace, again.last_saved);
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    pub(super) fn text_rect_optional(output: &egui::FullOutput, text: &str) -> Option<egui::Rect> {
        fn find(shape: &egui::Shape, text: &str) -> Option<egui::Rect> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == text => {
                    Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, text)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .rev()
            .find_map(|s| find(&s.shape, text))
    }
    pub(super) fn text_rect(output: &egui::FullOutput, text: &str) -> egui::Rect {
        text_rect_optional(output, text).unwrap_or_else(|| panic!("Text not rendered: {text}"))
    }
    fn pointer_button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    pub(super) fn click(app: &mut Amigo, ctx: &egui::Context, pos: egui::Pos2) {
        let _ = render(
            app,
            ctx,
            vec![egui::Event::PointerMoved(pos), pointer_button(pos, true)],
        );
        let _ = render(app, ctx, vec![pointer_button(pos, false)]);
    }
    #[test]
    fn appearance_switch_persists_without_changing_finance() {
        let (mut app, ctx) = harness();
        let finance = app.workspace.finance.clone();
        let _ = render(&mut app, &ctx, vec![]);
        let output = render(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, text_rect(&output, "Dark").center());
        let _ = render(&mut app, &ctx, vec![]);
        assert!(app.workspace.dark_mode);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
        assert_eq!(app.workspace.finance, finance);
        app.save_local();
        let saved = Amigo::load(app.data_path.clone());
        assert!(saved.workspace.dark_mode);
        let output = render(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, text_rect(&output, "Light").center());
        let _ = render(&mut app, &ctx, vec![]);
        assert!(!app.workspace.dark_mode);
        assert_eq!(ctx.theme(), egui::Theme::Light);
        std::fs::remove_dir_all(&app.data_path).unwrap();
    }
    #[test]
    fn toolbar_rename_preserves_records_and_vault_security_opens_password_dialog() {
        let (mut app, ctx) = harness();
        let board = app.workspace.tasks.clone();
        let plan = app.workspace.finance.clone();
        for (section, button, new_name) in [
            (Section::Tasks, "board-rename", "Work Board"),
            (Section::Finance, "vault-rename", "Household"),
        ] {
            app.section = section;
            for _ in 0..4 {
                render(&mut app, &ctx, vec![]);
            }
            let rect = ctx.read_response(egui::Id::new(button)).unwrap().rect;
            click(&mut app, &ctx, rect.center());
            for _ in 0..4 {
                render(&mut app, &ctx, vec![]);
            }
            assert_eq!(
                ctx.memory(|m| m.focused()),
                Some(egui::Id::new("library-name"))
            );
            render(&mut app, &ctx, vec![egui::Event::Text(new_name.into())]);
            let output = render(&mut app, &ctx, vec![]);
            click(&mut app, &ctx, text_rect(&output, "Rename").center());
            assert!(app.library_dialog.is_none());
        }
        assert_eq!(app.current_board_name(), "Work Board");
        assert_eq!(app.current_vault().unwrap().entry.name, "Household");
        assert_eq!(app.workspace.tasks, board);
        assert_eq!(app.workspace.finance, plan);
        assert!(app.save_local());
        let reopened = Amigo::load(app.data_path.clone());
        assert_eq!(reopened.current_board_name(), "Work Board");
        assert_eq!(reopened.current_vault().unwrap().entry.name, "Household");
        for _ in 0..4 {
            render(&mut app, &ctx, vec![]);
        }
        let rect = ctx.read_response(egui::Id::new("vault-lock")).unwrap().rect;
        click(&mut app, &ctx, rect.center());
        let output = render(&mut app, &ctx, vec![]);
        assert!(text_rect_optional(&output, "Confirm Password").is_some());
        click(&mut app, &ctx, text_rect(&output, "Cancel").center());
        assert!(app.library_dialog.is_none());
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn every_finance_view_and_inline_record_renders() {
        let (mut app, ctx) = harness();
        app.section = Section::Finance;
        for view in FinanceView::ALL {
            app.finance.view = view;
            let _ = render(&mut app, &ctx, vec![]);
            let _ = render(&mut app, &ctx, vec![]);
        }
        for record in [
            finance_ui::Record::Income(Income::new("EUR")),
            finance_ui::Record::Expense(Expense::new("EUR")),
            finance_ui::Record::Saving(Saving::default()),
            finance_ui::Record::Budget(Budget::default()),
            finance_ui::Record::Chain(Chain::default()),
            finance_ui::Record::Stored(2, StoredValue::default()),
        ] {
            app.finance.edit_record(record, true);
            let _ = render(&mut app, &ctx, vec![]);
            let _ = render(&mut app, &ctx, vec![]);
            let key = app.finance.drafts.keys().next().unwrap();
            assert!(
                ctx.read_response(egui::Id::new(("finance-row-save", key)))
                    .is_some()
            );
            app.finance.drafts.clear();
        }
        app.finance
            .edit_record(finance_ui::Record::Currency(Currency::default()), true);
        let _ = render(&mut app, &ctx, vec![]);
        let output = render(&mut app, &ctx, vec![]);
        assert!(text_rect_optional(&output, "Save Currency").is_some());
    }
}
