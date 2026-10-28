//! Board/vault navigation and password prompts. Password derivation stays off the UI thread.
use super::*;
use zeroize::Zeroizing;

#[derive(Clone)]
enum Action {
    RenameBoard(String),
    RenameVault(Vault),
    RenameCalendar,
    NewBoard,
    NewVault,
    Unlock(Vault),
    Encrypt(Vault),
    ChangePassword(Vault),
}
pub(super) struct LibraryDialog {
    action: Action,
    name: String,
    encrypted: bool,
    password: Zeroizing<String>,
    current_password: Zeroizing<String>,
    confirmation: Zeroizing<String>,
    error: String,
    focus_name: bool,
}
impl LibraryDialog {
    fn new(action: Action, name: String) -> Self {
        Self {
            action,
            name,
            encrypted: false,
            password: Zeroizing::new(String::new()),
            current_password: Zeroizing::new(String::new()),
            confirmation: Zeroizing::new(String::new()),
            error: String::new(),
            focus_name: true,
        }
    }
}
struct PreparedVault {
    plan: Option<FinancePlan>,
    key: VaultKey,
    current_key: Option<VaultKey>,
}
type Prepared = Result<PreparedVault, String>;
pub(super) struct VaultJob {
    action: Action,
    name: String,
    receiver: mpsc::Receiver<Prepared>,
}

impl Amigo {
    pub(super) fn prompt_board_rename(&mut self) {
        self.library_dialog = Some(LibraryDialog::new(
            Action::RenameBoard(self.library.settings.board.clone()),
            self.current_board_name().into(),
        ));
    }
    pub(super) fn prompt_calendar_rename(&mut self) {
        self.library_dialog = Some(LibraryDialog::new(
            Action::RenameCalendar,
            self.organizer.name.clone(),
        ));
    }
    pub(super) fn current_vault(&self) -> Option<&Vault> {
        self.library
            .vaults
            .iter()
            .find(|v| v.entry.id == self.library.settings.vault)
    }
    pub(super) fn current_board_name(&self) -> &str {
        self.library
            .boards
            .iter()
            .find(|b| b.id == self.library.settings.board)
            .map(|b| b.name.as_str())
            .unwrap_or("Tasks")
    }
    pub(super) fn prompt_locked_vault(&mut self) {
        if !self.vault_unlocked
            && self.library_dialog.is_none()
            && let Some(vault) = self.current_vault().filter(|v| v.encrypted).cloned()
        {
            self.library_dialog = Some(LibraryDialog::new(
                Action::Unlock(vault.clone()),
                vault.entry.name,
            ));
        }
    }
    fn clear_finance(&mut self) {
        self.workspace.finance = FinancePlan::empty();
        self.last_saved.finance = FinancePlan::empty();
        self.vault_key = None;
        self.vault_unlocked = false;
        self.finance.reset();
        self.message.clear();
        self.error.clear();
    }
    fn opened_vault(&mut self, mut plan: FinancePlan, key: Option<VaultKey>) {
        self.clear_finance();
        if let Err(error) = self
            .library
            .share_finance_references(&self.library.settings.vault, &mut plan)
        {
            self.error = error;
        }
        self.last_saved.finance = plan.clone();
        plan.meta.record_metadata_version = RECORD_METADATA_VERSION;
        self.workspace.finance = plan;
        self.vault_key = key;
        self.vault_unlocked = true;
    }
    pub(super) fn switch_board(&mut self, id: &str) -> Result<(), String> {
        if !self.save_local() {
            return Err(self.error.clone());
        }
        let board = self.library.load_board(id)?;
        self.library.select_board(id)?;
        self.workspace.tasks = board.tasks;
        self.workspace.task_statuses = board.task_statuses;
        self.workspace.task_categories = board.task_categories;
        self.workspace.next_task = board.next_task;
        self.last_saved = self.workspace.clone();
        self.tasks.reset();
        self.message.clear();
        self.error.clear();
        Ok(())
    }
    pub fn switch_vault(&mut self, vault: Vault) -> Result<(), String> {
        if !self.save_local() {
            return Err(self.error.clone());
        }
        // Validate plain data before closing the currently opened vault.
        let plan = if vault.encrypted {
            None
        } else {
            Some(self.library.load_plain_vault(&vault)?)
        };
        self.library.select_vault(&vault.entry.id)?;
        self.clear_finance();
        if let Some(plan) = plan {
            self.opened_vault(plan, None);
        } else {
            self.library_dialog = Some(LibraryDialog::new(
                Action::Unlock(vault.clone()),
                vault.entry.name,
            ));
        }
        Ok(())
    }
    fn lock_vault(&mut self) {
        if !self.save_local() {
            return;
        }
        self.clear_finance();
    }
    pub(super) fn library_navigation(&mut self, ui: &mut egui::Ui) {
        let mut selected_board = None;
        let mut selected_vault = None;
        let mut security = None;
        if self.section == Section::Tasks {
            design::eyebrow(ui, "BOARDS");
            for board in self.library.boards.clone() {
                if design::nav_row(
                    ui,
                    &board.name,
                    Icon::Board,
                    self.library.settings.board == board.id,
                    None,
                )
                .clicked()
                {
                    selected_board = Some(board.id);
                }
            }
            if design::button(ui, "+ New Board").clicked() {
                self.library_dialog = Some(LibraryDialog::new(Action::NewBoard, String::new()));
            }
        } else {
            design::eyebrow(ui, "VAULTS");
            for vault in self.library.vaults.clone() {
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 30. - ui.spacing().item_spacing.x).max(80.);
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 32.),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            if design::nav_row(
                                ui,
                                &vault.entry.name,
                                Icon::Wallet,
                                self.library.settings.vault == vault.entry.id,
                                None,
                            )
                            .clicked()
                            {
                                selected_vault = Some(vault.clone());
                            }
                        },
                    );
                    let unlocked =
                        self.library.settings.vault == vault.entry.id && self.vault_unlocked;
                    let label = if !vault.encrypted {
                        "Set Vault Password"
                    } else if unlocked {
                        "Lock Vault"
                    } else {
                        "Unlock Vault"
                    };
                    if design::icon_button(
                        ui,
                        egui::Id::new(("vault-security", &vault.entry.id)),
                        if vault.encrypted && !unlocked {
                            Icon::Lock
                        } else {
                            Icon::Unlock
                        },
                        label,
                        28.,
                    )
                    .clicked()
                    {
                        security = Some(vault.clone());
                    }
                });
            }
            if design::button(ui, "+ New Vault").clicked() {
                self.library_dialog = Some(LibraryDialog::new(Action::NewVault, String::new()));
            }
        }
        if let Some(vault) = security {
            let same = self.library.settings.vault == vault.entry.id;
            if !same && let Err(error) = self.switch_vault(vault.clone()) {
                self.error = error;
                return;
            }
            if !vault.encrypted {
                self.handle_vault_action(VaultAction::Encrypt);
            } else if same && self.vault_unlocked {
                self.handle_vault_action(VaultAction::Lock);
            } else {
                self.prompt_locked_vault();
            }
        }
        if let Some(id) = selected_board
            && let Err(error) = self.switch_board(&id)
        {
            self.error = error;
        }
        if let Some(vault) = selected_vault
            && let Err(error) = self.switch_vault(vault)
        {
            self.error = error;
        }
    }
    pub(super) fn handle_vault_action(&mut self, action: VaultAction) {
        let Some(vault) = self.current_vault().cloned() else {
            return;
        };
        match action {
            VaultAction::RenameVaultRequested => {
                self.library_dialog = Some(LibraryDialog::new(
                    Action::RenameVault(vault.clone()),
                    vault.entry.name,
                ));
            }
            VaultAction::ImportVault | VaultAction::ExportVault | VaultAction::DeleteVault => {
                let operation = match action {
                    VaultAction::ImportVault => design::DataAction::Import,
                    VaultAction::ExportVault => design::DataAction::Export,
                    _ => design::DataAction::Delete,
                };
                self.open_data_dialog(amigo_core::backup::Scope::Vault(vault.entry.id), operation);
            }
            VaultAction::SaveSettings {
                vault_name,
                exchange_interval_seconds,
            } => {
                if let Some(name) = vault_name
                    && let Err(error) = self.library.rename_vault(&vault.entry.id, &name)
                {
                    self.error = error;
                    return;
                }
                if let Err(error) = self.save_exchange_interval(exchange_interval_seconds) {
                    self.error = error;
                }
            }
            VaultAction::Lock => self.lock_vault(),
            VaultAction::ChangePassword => {
                if vault.encrypted && self.vault_unlocked {
                    self.library_dialog = Some(LibraryDialog::new(
                        Action::ChangePassword(vault.clone()),
                        vault.entry.name,
                    ));
                }
            }
            VaultAction::Encrypt => {
                if self.save_local() {
                    self.finance.cancel_rates();
                    self.library_dialog = Some(LibraryDialog::new(
                        Action::Encrypt(vault.clone()),
                        vault.entry.name,
                    ));
                }
            }
        }
    }
    pub(super) fn locked_vault_ui(&mut self, ui: &mut egui::Ui) {
        let vault = self.current_vault().cloned();
        let mut action = None;
        let mut unlock = false;
        panel(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 215.).max(80.);
                ui.add_sized(
                    [width, 38.],
                    egui::Label::new(
                        RichText::new(
                            vault
                                .as_ref()
                                .map_or("Finance vaults", |v| v.entry.name.as_str()),
                        )
                        .size(30.)
                        .strong(),
                    )
                    .truncate(),
                );
                if vault.is_some() {
                    for (key, icon, label, candidate) in [
                        (
                            "import",
                            Icon::Import,
                            "Import Vault",
                            Some(VaultAction::ImportVault),
                        ),
                        (
                            "export",
                            Icon::Export,
                            "Export Vault",
                            Some(VaultAction::ExportVault),
                        ),
                        (
                            "delete",
                            Icon::Trash,
                            "Delete Vault",
                            Some(VaultAction::DeleteVault),
                        ),
                        (
                            "rename",
                            Icon::Edit,
                            "Rename Vault",
                            Some(VaultAction::RenameVaultRequested),
                        ),
                        ("security", Icon::Lock, "Unlock Vault", None),
                    ] {
                        if design::icon_button(
                            ui,
                            egui::Id::new(format!("vault-{key}")),
                            icon,
                            label,
                            32.,
                        )
                        .clicked()
                        {
                            unlock = candidate.is_none();
                            action = candidate;
                        }
                    }
                }
            });
            if let Some(vault) = vault {
                hint(
                    ui,
                    if vault.encrypted {
                        "This vault is locked. Enter its password to see your finances."
                    } else {
                        "This vault could not be opened. Its files have been preserved."
                    },
                );
                let requested = primary(
                    ui,
                    if vault.encrypted {
                        "Unlock Vault"
                    } else {
                        "Open vault"
                    },
                )
                .clicked()
                    || unlock;
                if requested && let Err(error) = self.switch_vault(vault) {
                    self.error = error;
                }
            } else {
                hint(ui, "Create a vault to start tracking your finances.");
            }
        });
        if let Some(action) = action {
            self.handle_vault_action(action);
        }
    }
    pub(super) fn retry_library(&mut self) {
        let updates = std::mem::take(&mut self.updates);
        let replacement = Self::load_library(self.data_path.clone(), self.legacy_path.clone());
        *self = replacement;
        self.updates = updates;
    }
    fn execute_dialog(
        &mut self,
        dialog: &mut LibraryDialog,
        ctx: &egui::Context,
    ) -> Result<bool, String> {
        let needs_key = matches!(
            dialog.action,
            Action::Unlock(_) | Action::Encrypt(_) | Action::ChangePassword(_)
        ) || matches!(dialog.action, Action::NewVault) && dialog.encrypted;
        if !matches!(dialog.action, Action::Unlock(_)) {
            dialog.name = library::name(&dialog.name)?;
        }
        if needs_key {
            let changing_password = matches!(dialog.action, Action::ChangePassword(_));
            if changing_password && dialog.current_password.is_empty() {
                return Err("Enter the current password.".into());
            }
            if dialog.password.is_empty() {
                return Err("Enter a password.".into());
            }
            if !matches!(dialog.action, Action::Unlock(_)) && dialog.password != dialog.confirmation
            {
                return Err("The passwords do not match.".into());
            }
            dialog.error.clear();
            let password = std::mem::take(&mut dialog.password);
            let current_password = std::mem::take(&mut dialog.current_password);
            dialog.confirmation = Zeroizing::new(String::new());
            let path = match &dialog.action {
                Action::Unlock(v) | Action::ChangePassword(v) => {
                    Some((self.library.vault_path(&v.entry.id)?, v.entry.id.clone()))
                }
                _ => None,
            };
            let (sender, receiver) = mpsc::channel();
            let ctx = ctx.clone();
            self.vault_job = Some(VaultJob {
                action: dialog.action.clone(),
                name: dialog.name.clone(),
                receiver,
            });
            std::thread::spawn(move || {
                let result = if changing_password {
                    (|| {
                        let (path, id) = path.ok_or("Missing vault path.")?;
                        let (_, current_key) = VaultKey::unlock(&path, &id, &current_password)?;
                        let key = VaultKey::new(&password)?;
                        Ok(PreparedVault {
                            plan: None,
                            key,
                            current_key: Some(current_key),
                        })
                    })()
                } else if let Some((path, id)) = path {
                    VaultKey::unlock(&path, &id, &password).map(|(plan, key)| PreparedVault {
                        plan: Some(plan),
                        key,
                        current_key: None,
                    })
                } else {
                    VaultKey::new(&password).map(|key| PreparedVault {
                        plan: None,
                        key,
                        current_key: None,
                    })
                };
                let _ = sender.send(result);
                ctx.request_repaint();
            });
            return Ok(false);
        }
        match &dialog.action {
            Action::RenameBoard(id) => self.library.rename_board(id, &dialog.name)?,
            Action::RenameVault(vault) => {
                self.library.rename_vault(&vault.entry.id, &dialog.name)?
            }
            Action::RenameCalendar => {
                let old = self.organizer.name.clone();
                self.organizer.name = dialog.name.clone();
                if !self.save_organizer() {
                    self.organizer.name = old;
                    return Err(self.error.clone());
                }
            }
            Action::NewBoard => {
                if !self.save_local() {
                    return Err(self.error.clone());
                }
                let board = self
                    .library
                    .create_board(&dialog.name, &Workspace::default())?;
                self.switch_board(&board.id)?;
            }
            Action::NewVault => {
                if !self.save_local() {
                    return Err(self.error.clone());
                }
                let mut plan = FinancePlan::empty();
                self.library.finance_settings()?.apply(&mut plan);
                plan.meta.name = dialog.name.clone();
                let vault = self.library.create_vault(&dialog.name, &plan, None)?;
                self.switch_vault(vault)?;
            }
            _ => unreachable!(),
        }
        Ok(true)
    }
    pub(super) fn poll_vault_job(&mut self) {
        let Some(job) = self.vault_job.take() else {
            return;
        };
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                self.vault_job = Some(job);
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Could not finish the vault operation. Try again.".into())
            }
        };
        let encrypting = matches!(&job.action, Action::Encrypt(_));
        let result = result.and_then(|prepared| {
            let PreparedVault {
                plan,
                key,
                current_key,
            } = prepared;
            match job.action {
                Action::Unlock(vault) => {
                    if self.library.settings.vault != vault.entry.id {
                        return Err("Vault selection changed. Please unlock it again.".into());
                    }
                    self.library.complete_encryption(&vault.entry.id)?;
                    self.opened_vault(plan.ok_or("Missing decrypted vault.")?, Some(key));
                }
                Action::NewVault => {
                    if !self.save_local() {
                        return Err(self.error.clone());
                    }
                    let mut plan = FinancePlan::empty();
                    self.library.finance_settings()?.apply(&mut plan);
                    plan.meta.name = job.name;
                    let vault = self
                        .library
                        .create_vault(&plan.meta.name, &plan, Some(&key))?;
                    self.library.select_vault(&vault.entry.id)?;
                    self.opened_vault(plan, Some(key));
                }
                Action::ChangePassword(vault) => {
                    if self.library.settings.vault != vault.entry.id || !self.vault_unlocked {
                        return Err("Vault selection changed. Please try again.".into());
                    }
                    // Finish every write using the old key before atomically
                    // storing the latest live records with the replacement key.
                    if !self.flush_autosave() {
                        return Err(self.error.clone());
                    }
                    self.library.change_vault_password(
                        &vault.entry.id,
                        &self.workspace.finance,
                        current_key
                            .as_ref()
                            .ok_or("Missing verified current password.")?,
                        &key,
                    )?;
                    self.vault_key = Some(key);
                    self.last_saved.finance = self.workspace.finance.clone();
                    self.message = "Vault password changed.".into();
                }
                Action::Encrypt(vault) => {
                    if self.library.settings.vault != vault.entry.id || !self.vault_unlocked {
                        return Err("Vault selection changed. Please try again.".into());
                    }
                    if !self.save_local() {
                        return Err(self.error.clone());
                    }
                    self.library
                        .encrypt_vault(&vault.entry.id, &self.workspace.finance, &key)?;
                    self.vault_key = Some(key);
                }
                _ => return Err("Invalid vault operation.".into()),
            }
            Ok(())
        });
        match result {
            Ok(()) => self.library_dialog = None,
            Err(error) => {
                if encrypting {
                    let _ = self.library.refresh();
                    if let Some(vault) = self.current_vault().filter(|v| v.encrypted).cloned() {
                        self.clear_finance();
                        if let Some(dialog) = &mut self.library_dialog {
                            dialog.action = Action::Unlock(vault);
                        }
                    }
                }
                if let Some(dialog) = &mut self.library_dialog {
                    dialog.error = error;
                } else {
                    self.error = error;
                }
            }
        }
    }
    pub(super) fn library_dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.library_dialog.take() else {
            return;
        };
        let busy = self.vault_job.is_some();
        let keys = design::ActionKeys::capture(ctx);
        let mut submit = false;
        let mut cancel = false;
        let title = match dialog.action {
            Action::RenameBoard(_) => "Rename Board",
            Action::RenameVault(_) => "Rename Vault",
            Action::RenameCalendar => "Rename Calendar",
            Action::NewBoard => "New Board",
            Action::NewVault => "New Vault",
            Action::Unlock(_) => "Unlock Vault",
            Action::Encrypt(_) => "Encrypt Vault",
            Action::ChangePassword(_) => "Change Password",
        };
        let modal=design::modal(ctx,egui::Id::new("library-dialog")).show(ctx,|ui| {
            ui.set_width(design::dialog_width(ctx, 460.0));
            cancel = design::dialog_heading(ui,title).clicked();
            design::dialog_body(ui, "library-dialog-body", |ui| {
            ui.add_enabled_ui(!busy,|ui| {
                if matches!(dialog.action,Action::Unlock(_)|Action::Encrypt(_)|Action::ChangePassword(_)) {ui.label(RichText::new(&dialog.name).strong());}
                else {
                    amigo_ui::forms::field_label(ui,"Name");
                    let response=ui.add(design::input(&mut dialog.name).id(egui::Id::new("library-name")).desired_width(f32::INFINITY));
                    if dialog.focus_name && !ui.is_sizing_pass(){response.request_focus();let mut state=egui::TextEdit::load_state(ctx,response.id).unwrap_or_default();state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0),egui::text::CCursor::new(dialog.name.chars().count()))));state.store(ctx,response.id);dialog.focus_name=false;}
                }
                if matches!(dialog.action,Action::NewVault) {design::checkbox(ui, &mut dialog.encrypted,"Encrypt This Vault");}
                if matches!(dialog.action,Action::Unlock(_)|Action::Encrypt(_)|Action::ChangePassword(_)) || matches!(dialog.action,Action::NewVault)&&dialog.encrypted {
                    if matches!(dialog.action, Action::ChangePassword(_)) {
                        amigo_ui::forms::field_label(ui,"Current Password");
                        let input = ui.add(design::input(&mut *dialog.current_password).id(egui::Id::new("vault-current-password")).password(true).desired_width(f32::INFINITY));
                        if dialog.focus_name && !ui.is_sizing_pass() { input.request_focus(); dialog.focus_name=false; }
                    }
                    amigo_ui::forms::field_label(ui, if matches!(dialog.action, Action::ChangePassword(_)) { "New Password" } else { "Password" });
                    let input=ui.add(design::input(&mut *dialog.password).id(egui::Id::new("vault-password")).password(true).desired_width(f32::INFINITY));
                    if dialog.focus_name && !ui.is_sizing_pass() { input.request_focus(); dialog.focus_name=false; }
                    if !matches!(dialog.action,Action::Unlock(_)) {
                        amigo_ui::forms::field_label(ui, if matches!(dialog.action, Action::ChangePassword(_)) { "Confirm New Password" } else { "Confirm Password" });ui.add(design::input(&mut *dialog.confirmation).password(true).desired_width(f32::INFINITY));
                        hint(ui,"Keep this password safe. It is required whenever you reopen this vault.");
                    }
                }
                if !dialog.error.is_empty() {ui.colored_label(colors(ui).red,&dialog.error);}
            });
            });
            if busy {ui.horizontal(|ui| {ui.spinner();hint(ui,"Processing vault…");});}
            design::dialog_footer(ui, |ui| {
              ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                submit |= ui.add_enabled_ui(!busy, |ui| design::dialog_primary(ui,match dialog.action {Action::Unlock(_)=>"Unlock",Action::Encrypt(_)=>"Encrypt",Action::ChangePassword(_)=>"Change Password",Action::NewBoard|Action::NewVault=>"Create",Action::RenameBoard(_)|Action::RenameVault(_)|Action::RenameCalendar=>"Rename"})).inner.clicked();
                cancel |= design::button(ui, "Cancel").clicked();
              });
            });
        });
        let shortcut = keys.dialog(&modal, !busy);
        submit |= shortcut.confirm;
        if cancel || shortcut.cancel {
            self.vault_job = None;
            return;
        }
        if submit {
            match self.execute_dialog(&mut dialog, ctx) {
                Ok(true) => return,
                Ok(false) => {}
                Err(error) => dialog.error = error,
            }
        }
        self.library_dialog = Some(dialog);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{click, harness, render, text_rect, text_rect_optional};

    fn finish_job(app: &mut Amigo) {
        let deadline = Instant::now() + Duration::from_secs(40);
        while app.vault_job.is_some() {
            app.poll_vault_job();
            assert!(Instant::now() < deadline, "vault worker timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn keyboard(app: &mut Amigo, ctx: &egui::Context, key: egui::Key) {
        for pressed in [true, false] {
            let _ = render(
                app,
                ctx,
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
    }
    #[test]
    fn library_keyboard_confirms_names_validates_passwords_and_discards_renames() {
        let (mut app, ctx) = harness();
        app.library_dialog = Some(LibraryDialog::new(
            Action::NewBoard,
            "Keyboard Board".into(),
        ));
        for _ in 0..3 {
            let _ = render(&mut app, &ctx, vec![]);
        }
        keyboard(&mut app, &ctx, egui::Key::Enter);
        assert!(app.library_dialog.is_none());
        assert_eq!(app.current_board_name(), "Keyboard Board");
        let count = app.library.vaults.len();
        let mut dialog = LibraryDialog::new(Action::NewVault, "Keyboard Vault".into());
        dialog.encrypted = true;
        *dialog.password = "keyboard-test-password".into();
        *dialog.confirmation = "mismatch".into();
        app.library_dialog = Some(dialog);
        for _ in 0..3 {
            let _ = render(&mut app, &ctx, vec![]);
        }
        keyboard(&mut app, &ctx, egui::Key::Enter);
        assert!(!app.library_dialog.as_ref().unwrap().error.is_empty());
        assert!(app.vault_job.is_none());
        assert_eq!(app.library.vaults.len(), count);
        *app.library_dialog.as_mut().unwrap().confirmation = "keyboard-test-password".into();
        keyboard(&mut app, &ctx, egui::Key::Enter);
        finish_job(&mut app);
        assert!(app.library_dialog.is_none());
        assert_eq!(app.current_vault().unwrap().entry.name, "Keyboard Vault");
        assert!(app.current_vault().unwrap().encrypted);
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    #[test]
    fn boards_and_plain_vaults_switch_independently_and_persist() {
        let (mut app, ctx) = harness();
        let personal = app.library.settings.board.clone();
        let mut task = Task::new(1, "todo");
        task.title = "Personal task".into();
        app.workspace.tasks.push(task);
        assert!(app.save_local());
        let _ = render(&mut app, &ctx, vec![]);
        let output = render(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, text_rect(&output, "+ New Board").center());
        app.library_dialog.as_mut().unwrap().name = "Work board".into();
        let _ = render(&mut app, &ctx, vec![]);
        let output = render(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, text_rect(&output, "Create").center());
        assert!(app.library_dialog.is_none());
        assert_eq!(app.current_board_name(), "Work board");
        assert!(app.workspace.tasks.is_empty());
        let work = app.library.settings.board.clone();
        let mut task = Task::new(1, "done");
        task.title = "Work task".into();
        app.workspace.tasks.push(task);
        app.switch_board(&personal).unwrap();
        assert_eq!(app.workspace.tasks[0].title, "Personal task");
        app.switch_board(&work).unwrap();
        assert_eq!(app.workspace.tasks[0].title, "Work task");
        let original_vault = app.current_vault().unwrap().clone();
        let mut income = Income::new("EUR");
        income.source = "Personal income".into();
        app.workspace.finance.income.push(income);
        assert!(app.save_local());
        let vault = app
            .library
            .create_vault("Household", &FinancePlan::empty(), None)
            .unwrap();
        app.switch_vault(vault.clone()).unwrap();
        assert!(app.vault_unlocked);
        assert!(app.library_dialog.is_none());
        assert!(app.workspace.finance.income.is_empty());
        assert_eq!(app.workspace.tasks[0].title, "Work task");
        app.switch_vault(original_vault).unwrap();
        assert_eq!(app.workspace.finance.income[0].source, "Personal income");
        app.switch_vault(vault).unwrap();
        let reopened = Amigo::load(app.data_path.clone());
        assert_eq!(reopened.current_board_name(), "Work board");
        assert!(reopened.vault_unlocked);
        assert!(reopened.workspace.finance.income.is_empty());
        std::fs::remove_dir_all(&app.data_path).unwrap();
    }
    #[test]
    fn changing_password_rejects_wrong_current_password_and_preserves_latest_autosaves() {
        let (mut app, ctx) = harness();
        let plan = app.workspace.finance.clone();
        let old_key = VaultKey::new("current password").unwrap();
        let vault = app
            .library
            .create_vault("Protected", &plan, Some(&old_key))
            .unwrap();
        app.switch_vault(vault.clone()).unwrap();
        app.opened_vault(plan, Some(old_key));
        app.section = Section::Finance;
        for _ in 0..3 {
            let _ = render(&mut app, &ctx, vec![]);
        }
        app.finance.configure_vault(&app.workspace.finance);
        app.handle_vault_action(VaultAction::ChangePassword);
        for _ in 0..3 {
            let _ = render(&mut app, &ctx, vec![]);
        }
        let output =
            crate::app::tests::render_sized(&mut app, &ctx, vec![], egui::vec2(980.0, 640.0));
        for label in [
            "Current Password",
            "New Password",
            "Confirm New Password",
            "Change Password",
        ] {
            assert!(text_rect(&output, label).bottom() < 640.0);
        }
        let mut dialog = app.library_dialog.take().unwrap();
        *dialog.password = "new password".into();
        *dialog.confirmation = "new password".into();
        assert!(
            app.execute_dialog(&mut dialog, &ctx)
                .unwrap_err()
                .contains("current password")
        );
        *dialog.current_password = "current password".into();
        *dialog.confirmation = "mismatch".into();
        assert!(
            app.execute_dialog(&mut dialog, &ctx)
                .unwrap_err()
                .contains("do not match")
        );
        assert!(app.vault_job.is_none());
        *dialog.current_password = "wrong password".into();
        *dialog.confirmation = "new password".into();
        let path = app.library.vault_path(&vault.entry.id).unwrap();
        let before = std::fs::read(path.join("vault.enc")).unwrap();
        assert!(!app.execute_dialog(&mut dialog, &ctx).unwrap());
        app.library_dialog = Some(dialog);
        finish_job(&mut app);
        assert!(
            app.library_dialog
                .as_ref()
                .unwrap()
                .error
                .contains("Incorrect password")
        );
        assert_eq!(std::fs::read(path.join("vault.enc")).unwrap(), before);
        assert!(app.vault_unlocked);
        let mut dialog = app.library_dialog.take().unwrap();
        *dialog.current_password = "current password".into();
        *dialog.password = "new password".into();
        *dialog.confirmation = "new password".into();
        assert!(!app.execute_dialog(&mut dialog, &ctx).unwrap());
        app.library_dialog = Some(dialog);
        // Changes saved while keys are being derived must survive the rotation,
        // and an old-key autosave must finish before the replacement is written.
        app.workspace.finance.meta.name = "Edited during password change".into();
        app.start_autosave(&ctx);
        assert!(app.save_job.is_some());
        finish_job(&mut app);
        assert!(app.library_dialog.is_none());
        assert!(app.save_job.is_none());
        assert_eq!(app.last_saved.finance, app.workspace.finance);
        assert!(VaultKey::unlock(&path, &vault.entry.id, "current password").is_err());
        assert_eq!(
            VaultKey::unlock(&path, &vault.entry.id, "new password")
                .unwrap()
                .0,
            app.workspace.finance
        );
        app.workspace.finance.meta.name = "Saved with new key".into();
        app.start_autosave(&ctx);
        assert!(app.flush_autosave());
        assert_eq!(
            VaultKey::unlock(&path, &vault.entry.id, "new password")
                .unwrap()
                .0,
            app.workspace.finance
        );
        let saved = app.workspace.finance.clone();
        app.lock_vault();
        app.prompt_locked_vault();
        let mut dialog = app.library_dialog.take().unwrap();
        *dialog.password = "new password".into();
        assert!(!app.execute_dialog(&mut dialog, &ctx).unwrap());
        app.library_dialog = Some(dialog);
        finish_job(&mut app);
        assert_eq!(app.workspace.finance, saved);
        assert!(app.vault_unlocked);
        assert!(!path.join("records").exists());
        std::fs::remove_dir_all(app.data_path).unwrap();
    }

    #[test]
    fn cancelling_password_change_drops_prepared_keys_without_changing_the_vault() {
        let (mut app, ctx) = harness();
        let plan = app.workspace.finance.clone();
        let old_key = VaultKey::new("current password").unwrap();
        let vault = app
            .library
            .create_vault("Protected", &plan, Some(&old_key))
            .unwrap();
        app.switch_vault(vault.clone()).unwrap();
        app.opened_vault(plan, Some(old_key.clone()));
        app.section = Section::Finance;
        for _ in 0..3 {
            let _ = render(&mut app, &ctx, vec![]);
        }
        app.handle_vault_action(VaultAction::ChangePassword);
        for _ in 0..3 {
            let _ = render(&mut app, &ctx, vec![]);
        }
        let path = app.library.vault_path(&vault.entry.id).unwrap();
        let before = std::fs::read(path.join("vault.enc")).unwrap();
        let (sender, receiver) = mpsc::channel();
        app.vault_job = Some(VaultJob {
            action: Action::ChangePassword(vault),
            name: "Protected".into(),
            receiver,
        });
        keyboard(&mut app, &ctx, egui::Key::Escape);
        assert!(app.vault_job.is_none());
        assert!(app.library_dialog.is_none());
        assert!(
            sender
                .send(Ok(PreparedVault {
                    plan: None,
                    key: old_key.clone(),
                    current_key: Some(old_key)
                }))
                .is_err()
        );
        app.poll_vault_job();
        assert_eq!(std::fs::read(path.join("vault.enc")).unwrap(), before);
        assert!(app.vault_unlocked);
        std::fs::remove_dir_all(app.data_path).unwrap();
    }

    #[test]
    fn encrypted_vault_prompt_worker_lock_and_wrong_password_preserve_data() {
        let (mut app, ctx) = harness();
        app.section = Section::Finance;
        let plain = app.current_vault().unwrap().clone();
        app.library_dialog = Some(LibraryDialog::new(Action::NewVault, "Private".into()));
        let dialog = app.library_dialog.as_mut().unwrap();
        dialog.encrypted = true;
        *dialog.password = "a test password".into();
        *dialog.confirmation = "a test password".into();
        let _ = render(&mut app, &ctx, vec![]);
        let output = render(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, text_rect(&output, "Create").center());
        assert!(app.vault_job.is_some());
        finish_job(&mut app);
        assert!(app.vault_unlocked);
        assert!(app.vault_key.is_some());
        assert!(app.library_dialog.is_none());
        let protected = app.current_vault().unwrap().clone();
        assert!(protected.encrypted);
        let mut income = Income::new("EUR");
        income.source = "Secret income".into();
        app.workspace.finance.income.push(income);
        app.lock_vault();
        assert!(!app.vault_unlocked);
        assert!(app.vault_key.is_none());
        assert!(app.workspace.finance.income.is_empty());
        let path = app
            .library
            .vault_path(&protected.entry.id)
            .unwrap()
            .join("vault.enc");
        let before = std::fs::read(&path).unwrap();
        app.workspace.dark_mode = !app.workspace.dark_mode;
        assert!(app.save_local());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let output = render(&mut app, &ctx, vec![]);
        assert!(text_rect_optional(&output, "Secret income").is_none());
        app.prompt_locked_vault();
        let mut dialog = app.library_dialog.take().unwrap();
        *dialog.password = "wrong".into();
        assert!(!app.execute_dialog(&mut dialog, &ctx).unwrap());
        app.library_dialog = Some(dialog);
        finish_job(&mut app);
        assert!(!app.vault_unlocked);
        assert!(!app.library_dialog.as_ref().unwrap().error.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let mut dialog = app.library_dialog.take().unwrap();
        *dialog.password = "a test password".into();
        assert!(!app.execute_dialog(&mut dialog, &ctx).unwrap());
        app.library_dialog = Some(dialog);
        finish_job(&mut app);
        assert!(app.vault_unlocked);
        assert_eq!(app.workspace.finance.income[0].source, "Secret income");
        app.switch_vault(plain).unwrap();
        assert!(app.vault_key.is_none());
        assert!(app.workspace.finance.income.is_empty());
        app.switch_vault(protected).unwrap();
        assert!(!app.vault_unlocked);
        assert!(app.library_dialog.is_some());
        let reopened = Amigo::load(app.data_path.clone());
        assert!(!reopened.vault_unlocked);
        assert!(reopened.vault_key.is_none());
        assert!(reopened.workspace.finance.income.is_empty());
        std::fs::remove_dir_all(&app.data_path).unwrap();
    }
}
