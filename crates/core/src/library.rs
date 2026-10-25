//! The user's folder library. Names are editable; stable IDs own disk paths.
use crate::{model::*, storage};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use pbkdf2::pbkdf2_hmac;
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

type Result<T> = std::result::Result<T, String>;
const ITERATIONS: u32 = 600_000;
const RECORDS: [&str; 6] = [
    "income", "expenses", "savings", "budgets", "fxRates", "accounts",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Vault {
    #[serde(flatten)]
    pub entry: Entry,
    pub encrypted: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    pub version: u32,
    pub dark_mode: bool,
    pub board: String,
    pub vault: String,
    #[serde(default)]
    pub sync: SyncSettings,
    #[serde(default)]
    pub journal: crate::journal::Preferences,
    #[serde(default)]
    pub updates: crate::updates::Preferences,
    #[serde(default)]
    migration: Option<Migration>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SyncSettings {
    pub exchange_rate_interval_seconds: u64,
}
impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            exchange_rate_interval_seconds: 60,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Migration {
    source: PathBuf,
    digest: String,
}
#[derive(Serialize, Deserialize)]
struct BoardDisk {
    version: u32,
    #[serde(flatten)]
    entry: Entry,
    statuses: Vec<TaskStatus>,
    categories: Vec<StoredValue>,
    next_task: u32,
    order: Vec<String>,
}
#[derive(Clone)]
pub struct Library {
    pub root: PathBuf,
    pub settings: Settings,
    pub boards: Vec<Entry>,
    pub vaults: Vec<Vault>,
}

pub fn default_root() -> PathBuf {
    root_from_environment(|key| std::env::var_os(key))
}

fn root_from_environment(get: impl Fn(&str) -> Option<std::ffi::OsString>) -> PathBuf {
    if let Some(root) = get("AMIGO_HOME") {
        return root.into();
    }
    if let Some(file) = get("AMIGO_DATA_FILE") {
        return PathBuf::from(file).with_extension("library");
    }
    // Legacy overrides still work, but explicit Amigo overrides take precedence.
    if let Some(root) = get("CLERK_HOME") {
        return root.into();
    }
    if let Some(file) = get("CLERK_DATA_FILE") {
        return PathBuf::from(file).with_extension("library");
    }
    storage::user_home(get)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".amigo")
}

fn move_previous_library(previous: &Path, root: &Path) -> Result<()> {
    if root.exists() || !previous.join("library.json").is_file() {
        return Ok(());
    }
    fs::rename(previous, root).map_err(|e| {
        format!(
            "Cannot move {} to {}: {e}. Existing data has been preserved.",
            previous.display(),
            root.display()
        )
    })
}
fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_slice(
        &fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?,
    )
    .map_err(|e| format!("Cannot read {}: {e}", path.display()))
}
fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    storage::atomic_write(
        path,
        &serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
}
fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn create_folder(path: &Path, populate: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    let parent = path.parent().ok_or("Missing library parent folder.")?;
    private_dir(parent)?;
    let stage = parent.join(format!(".new-{}", id()));
    private_dir(&stage)?;
    let result =
        populate(&stage).and_then(|()| fs::rename(&stage, path).map_err(|e| e.to_string()));
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}
fn component(id: &str) -> Result<&str> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Invalid library folder ID.".into());
    }
    Ok(id)
}
fn filename(id: &str) -> String {
    // Imported record IDs are data, never interpreted as relative paths.
    format!(
        "{}.md",
        id.as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}
pub fn name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err("Enter a name without line breaks.".into());
    }
    Ok(value.into())
}
fn markdown(metadata: &impl Serialize, body: &str) -> Result<Vec<u8>> {
    Ok(format!(
        "---\n{}\n---\n{}",
        serde_json::to_string_pretty(metadata).map_err(|e| e.to_string())?,
        body
    )
    .into_bytes())
}
fn parse_markdown(text: &str) -> Result<(Value, &str)> {
    let text = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
        .ok_or("Missing Markdown front matter.")?;
    let (metadata, body) = text
        .split_once("\n---\n")
        .or_else(|| text.split_once("\r\n---\r\n"))
        .ok_or("Unclosed Markdown front matter.")?;
    Ok((
        serde_json::from_str(metadata).map_err(|e| format!("Invalid front matter: {e}"))?,
        body,
    ))
}
fn md_files(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let mut paths = vec![];
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().extension().is_some_and(|e| e == "md") {
            if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                return Err("Markdown records must be regular files, not links.".into());
            }
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}
fn record_value(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_markdown(&text)
        .map(|(v, _)| v)
        .map_err(|e| format!("{}: {e}", path.display()))
}
fn remove_stale(path: &Path, keep: &HashSet<String>) -> Result<()> {
    for file in md_files(path)? {
        if !keep.contains(
            file.file_name()
                .unwrap()
                .to_str()
                .ok_or("Invalid record filename.")?,
        ) {
            fs::remove_file(&file).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

impl Library {
    pub fn open(root: PathBuf, legacy: Option<&Path>) -> Result<Self> {
        crate::backup::recover(&root)?;
        if let Some(home) = storage::user_home(|key| std::env::var_os(key))
            && root == home.join(".amigo")
        {
            move_previous_libraries(&home, &root)?;
        }
        if !root.join("library.json").exists() {
            if root.exists()
                && fs::read_dir(&root)
                    .map_err(|e| e.to_string())?
                    .next()
                    .is_some()
            {
                return Err(format!(
                    "{} has no valid library.json. Its files have been preserved.",
                    root.display()
                ));
            }
            let mut workspace = if let Some(path) = legacy.filter(|p| p.exists()) {
                storage::load_workspace(path)?
            } else {
                Workspace::default()
            };
            workspace.normalize();
            let parent = root.parent().unwrap_or(Path::new("."));
            let stage = parent.join(format!(".amigo-init-{}", id()));
            private_dir(&stage)?;
            let result: Result<()> = (|| {
                let mut lib = Self {
                    root: stage.clone(),
                    settings: Settings {
                        version: 1,
                        dark_mode: workspace.dark_mode,
                        ..Default::default()
                    },
                    boards: vec![],
                    vaults: vec![],
                };
                let board = lib.create_board("Personal", &workspace)?;
                let vault = lib.create_vault("Personal", &workspace.finance, None)?;
                lib.settings.board = board.id;
                lib.settings.vault = vault.entry.id;
                if let Some(path) = legacy.filter(|p| p.exists()) {
                    lib.settings.migration = Some(Migration {
                        source: path.into(),
                        digest: STANDARD
                            .encode(Sha256::digest(fs::read(path).map_err(|e| e.to_string())?)),
                    });
                }
                lib.save_settings()?;
                if root.exists() {
                    fs::remove_dir(&root).map_err(|e| e.to_string())?;
                }
                fs::rename(&stage, &root).map_err(|e| e.to_string())?;
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::remove_dir_all(&stage);
            }
            result?;
        }
        let settings: Settings = read(&root.join("library.json"))?;
        if settings.version != 1 {
            return Err("Unsupported library version. Existing files have been preserved.".into());
        }
        let mut lib = Self {
            root,
            settings,
            boards: vec![],
            vaults: vec![],
        };
        lib.refresh()?;
        lib.finish_migration()?;
        lib.initialize_finance_settings()?;
        Ok(lib)
    }
    // Used only for a failed-load error screen; never creates or overwrites files.
    pub fn unavailable(root: PathBuf) -> Self {
        Self {
            root,
            settings: Settings::default(),
            boards: vec![],
            vaults: vec![],
        }
    }
    fn finish_migration(&mut self) -> Result<()> {
        let Some(migration) = self.settings.migration.clone() else {
            return Ok(());
        };
        let bytes = fs::read(&migration.source).map_err(|e| e.to_string())?;
        let receipt = json!({"migratedTo":self.root,"format":"Amigo folder library"});
        let already_moved = serde_json::from_slice::<Value>(&bytes).ok().as_ref() == Some(&receipt);
        if !already_moved {
            if STANDARD.encode(Sha256::digest(&bytes)) != migration.digest {
                return Err("The legacy workspace changed during migration. Both copies have been preserved.".into());
            }
            let old = storage::workspace_from_value(
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
            )?;
            let migrated = self.load_board(&self.settings.board)?;
            let vault = self
                .vaults
                .iter()
                .find(|v| v.entry.id == self.settings.vault)
                .ok_or("Missing migrated vault.")?;
            let finance = self.load_plain_vault(vault)?;
            if old.tasks != migrated.tasks
                || old.task_statuses != migrated.task_statuses
                || old.task_categories != migrated.task_categories
                || old.finance != finance
            {
                return Err(
                    "Migration verification failed. The original workspace has been preserved."
                        .into(),
                );
            }
            // Replace the old plaintext with a receipt only after reading back all data.
            // This prevents a forgotten legacy file bypassing later vault encryption.
            write(&migration.source, &receipt)?;
        }
        self.settings.migration = None;
        self.save_settings()
    }
    pub fn save_settings(&self) -> Result<()> {
        write(&self.root.join("library.json"), &self.settings)
    }
    pub fn select_board(&mut self, id: &str) -> Result<()> {
        if !self.boards.iter().any(|b| b.id == id) {
            return Err("Board no longer exists.".into());
        }
        let previous = std::mem::replace(&mut self.settings.board, id.into());
        if let Err(error) = self.save_settings() {
            self.settings.board = previous;
            return Err(error);
        }
        Ok(())
    }
    pub fn select_vault(&mut self, id: &str) -> Result<()> {
        if !self.vaults.iter().any(|v| v.entry.id == id) {
            return Err("Vault no longer exists.".into());
        }
        let previous = std::mem::replace(&mut self.settings.vault, id.into());
        if let Err(error) = self.save_settings() {
            self.settings.vault = previous;
            return Err(error);
        }
        Ok(())
    }
    pub fn refresh(&mut self) -> Result<()> {
        fn folders(path: &Path) -> Result<Vec<PathBuf>> {
            if !path.exists() {
                return Ok(vec![]);
            }
            let mut entries = vec![];
            for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                    return Err("Library entries must be regular folders.".into());
                }
                entries.push(entry.path());
            }
            Ok(entries)
        }
        let mut boards = vec![];
        let mut vaults = vec![];
        for path in folders(&self.root.join("boards"))? {
            let board: BoardDisk = read(&path.join("board.json"))?;
            if board.version != 1
                || path.file_name().and_then(|s| s.to_str()) != Some(component(&board.entry.id)?)
            {
                return Err("Invalid board metadata.".into());
            }
            boards.push(board.entry);
        }
        for path in folders(&self.root.join("vaults"))? {
            let mut vault: Vault = read(&path.join("vault.json"))?;
            // An interrupted conversion is recovered by unlocking the committed payload.
            if path.join("vault.enc").exists() {
                vault.encrypted = true;
            }
            if path.file_name().and_then(|s| s.to_str()) != Some(component(&vault.entry.id)?) {
                return Err("Invalid vault metadata.".into());
            }
            vaults.push(vault);
        }
        boards.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        vaults.sort_by(|a, b| {
            a.entry
                .name
                .cmp(&b.entry.name)
                .then(a.entry.id.cmp(&b.entry.id))
        });
        self.boards = boards;
        self.vaults = vaults;
        Ok(())
    }
    pub fn board_path(&self, id: &str) -> Result<PathBuf> {
        Ok(self.root.join("boards").join(component(id)?))
    }
    pub fn vault_path(&self, id: &str) -> Result<PathBuf> {
        Ok(self.root.join("vaults").join(component(id)?))
    }
    pub fn create_board(&mut self, title: &str, workspace: &Workspace) -> Result<Entry> {
        let entry = Entry {
            id: id(),
            name: name(title)?,
        };
        create_folder(&self.board_path(&entry.id)?, |path| {
            self.write_board_at(path, &entry, workspace)
        })?;
        self.boards.push(entry.clone());
        Ok(entry)
    }
    pub fn save_board(&self, id: &str, workspace: &Workspace) -> Result<()> {
        let entry = self
            .boards
            .iter()
            .find(|b| b.id == id)
            .ok_or("Board no longer exists.")?;
        self.write_board(entry, workspace)
    }
    fn write_board(&self, entry: &Entry, workspace: &Workspace) -> Result<()> {
        self.write_board_at(&self.board_path(&entry.id)?, entry, workspace)
    }
    fn write_board_at(&self, path: &Path, entry: &Entry, workspace: &Workspace) -> Result<()> {
        private_dir(path)?;
        let mut keep = HashSet::new();
        for task in &workspace.tasks {
            if task.id.is_empty() || !keep.insert(filename(&task.id)) {
                return Err("Missing or duplicate task ID.".into());
            }
            let mut metadata = serde_json::to_value(task).map_err(|e| e.to_string())?;
            metadata.as_object_mut().unwrap().remove("description");
            metadata.as_object_mut().unwrap().remove("title");
            let body = format!(
                "# {}\n\n{}",
                task.title.replace(['\n', '\r'], " "),
                task.description
            );
            let file = path.join(filename(&task.id));
            let bytes = markdown(&metadata, &body)?;
            if fs::read(&file).ok().as_deref() != Some(bytes.as_slice()) {
                storage::atomic_write(&file, &bytes)?;
            }
        }
        write(
            &path.join("board.json"),
            &BoardDisk {
                version: 1,
                entry: entry.clone(),
                statuses: workspace.task_statuses.clone(),
                categories: workspace.task_categories.clone(),
                next_task: workspace.next_task,
                order: workspace.tasks.iter().map(|t| t.id.clone()).collect(),
            },
        )?;
        remove_stale(path, &keep)?;
        crate::attachments::prune(path, &workspace.tasks)
    }
    pub fn load_board(&self, id: &str) -> Result<Workspace> {
        let path = self.board_path(id)?;
        let disk: BoardDisk = read(&path.join("board.json"))?;
        if disk.version != 1 || disk.entry.id != id {
            return Err("Invalid board metadata.".into());
        }
        let mut tasks = vec![];
        let mut ids = HashSet::new();
        for file in md_files(&path)? {
            let text = fs::read_to_string(&file)
                .map_err(|e| e.to_string())?
                .replace("\r\n", "\n");
            let (metadata, body) =
                parse_markdown(&text).map_err(|e| format!("{}: {e}", file.display()))?;
            let mut task: Task = serde_json::from_value(metadata).map_err(|e| e.to_string())?;
            let (heading, description) = body
                .split_once("\n\n")
                .ok_or_else(|| format!("{} needs a title and a blank line.", file.display()))?;
            task.title = heading
                .strip_prefix("# ")
                .ok_or("Task title must start with #.")?
                .into();
            task.description = description.into();
            if task.id.is_empty() || !ids.insert(task.id.clone()) {
                return Err("Missing or duplicate task ID in Markdown files.".into());
            }
            tasks.push(task);
        }
        tasks.sort_by_key(|t| {
            (
                disk.order
                    .iter()
                    .position(|id| id == &t.id)
                    .unwrap_or(usize::MAX),
                t.number,
                t.id.clone(),
            )
        });
        let mut workspace = Workspace {
            tasks,
            task_statuses: disk.statuses,
            task_categories: disk.categories,
            next_task: disk.next_task,
            ..Workspace::default()
        };
        workspace.normalize();
        Ok(workspace)
    }
    pub fn rename_board(&mut self, id: &str, title: &str) -> Result<()> {
        let path = self.board_path(id)?;
        let mut disk: BoardDisk = read(&path.join("board.json"))?;
        disk.entry.name = name(title)?;
        write(&path.join("board.json"), &disk)?;
        self.refresh()
    }
    pub fn rename_vault(&mut self, id: &str, title: &str) -> Result<()> {
        let path = self.vault_path(id)?;
        let mut vault: Vault = read(&path.join("vault.json"))?;
        vault.entry.name = name(title)?;
        write(&path.join("vault.json"), &vault)?;
        self.refresh()
    }
    pub fn create_vault(
        &mut self,
        title: &str,
        plan: &FinancePlan,
        key: Option<&VaultKey>,
    ) -> Result<Vault> {
        let vault = Vault {
            entry: Entry {
                id: id(),
                name: name(title)?,
            },
            encrypted: key.is_some(),
        };
        let path = self.vault_path(&vault.entry.id)?;
        create_folder(&path, |path| {
            self.write_vault_data(path, &vault, plan, key)?;
            write(&path.join("vault.json"), &vault)
        })?;
        self.vaults.push(vault.clone());
        Ok(vault)
    }
    pub fn load_plain_vault(&self, vault: &Vault) -> Result<FinancePlan> {
        if vault.encrypted {
            return Err("Unlock this vault with its password.".into());
        }
        read_plain(&self.vault_path(&vault.entry.id)?.join("records"))
    }
    pub fn save_vault(
        &self,
        vault: &Vault,
        plan: &FinancePlan,
        key: Option<&VaultKey>,
    ) -> Result<()> {
        self.write_vault_data(&self.vault_path(&vault.entry.id)?, vault, plan, key)?;
        self.share_finance_references(&vault.entry.id, &mut plan.clone())
    }
    fn write_vault_data(
        &self,
        path: &Path,
        vault: &Vault,
        plan: &FinancePlan,
        key: Option<&VaultKey>,
    ) -> Result<()> {
        if vault.encrypted {
            let key = key.ok_or("This vault is locked.")?;
            storage::atomic_write(&path.join("vault.enc"), &key.seal(&vault.entry.id, plan)?)
        } else {
            write_plain(&path.join("records"), plan)
        }
    }
    /// Re-encrypt an unlocked vault atomically after authenticating its current
    /// on-disk key. Callers must finish any outstanding saves before rotating it.
    pub fn change_vault_password(
        &self,
        id: &str,
        plan: &FinancePlan,
        current_key: &VaultKey,
        new_key: &VaultKey,
    ) -> Result<()> {
        let vault = self
            .vaults
            .iter()
            .find(|vault| vault.entry.id == id)
            .ok_or("Vault no longer exists.")?;
        if !vault.encrypted {
            return Err("Encrypt this vault before changing its password.".into());
        }
        let path = self.vault_path(id)?;
        current_key.load(&path, id)?;
        self.write_vault_data(&path, vault, plan, Some(new_key))
    }

    pub fn encrypt_vault(&mut self, id: &str, plan: &FinancePlan, key: &VaultKey) -> Result<Vault> {
        let old = self.vault_path(id)?;
        let mut vault = self
            .vaults
            .iter()
            .find(|v| v.entry.id == id)
            .cloned()
            .ok_or("Vault no longer exists.")?;
        if vault.encrypted {
            return Err("Vault is already encrypted.".into());
        }
        let stage = self
            .root
            .join("vaults")
            .join(format!(".encrypt-{}", crate::model::id()));
        private_dir(&stage)?;
        vault.encrypted = true;
        self.write_vault_data(&stage, &vault, plan, Some(key))?;
        write(&stage.join("vault.json"), &vault)?;
        // The payload is committed before the encryption marker and before removing
        // plaintext. Startup detects it even if conversion is interrupted here.
        fs::rename(stage.join("vault.enc"), old.join("vault.enc")).map_err(|e| e.to_string())?;
        fs::rename(stage.join("vault.json"), old.join("vault.json")).map_err(|e| e.to_string())?;
        self.refresh()?;
        self.complete_encryption(id)?;
        fs::remove_dir(&stage).map_err(|e| e.to_string())?;
        self.refresh()?;
        Ok(vault)
    }
    pub fn complete_encryption(&mut self, id: &str) -> Result<()> {
        let path = self.vault_path(id)?;
        if !path.join("vault.enc").exists() {
            return Ok(());
        }
        let mut vault: Vault = read(&path.join("vault.json"))?;
        vault.encrypted = true;
        write(&path.join("vault.json"), &vault)?;
        if path.join("records").exists() {
            fs::remove_dir_all(path.join("records")).map_err(|e| {
                format!("Could not finish removing plaintext records: {e}. Unlock to retry.")
            })?;
        }
        self.refresh()
    }
}

fn move_previous_libraries(home: &Path, root: &Path) -> Result<()> {
    for previous in [home.join(".clerk"), home.join("Clerk")] {
        move_previous_library(&previous, root)?;
    }
    Ok(())
}
fn write_plain(path: &Path, plan: &FinancePlan) -> Result<()> {
    private_dir(path)?;
    let mut value = serde_json::to_value(plan).map_err(|e| e.to_string())?;
    let mut order = serde_json::Map::new();
    for kind in RECORDS {
        let rows = value
            .as_object_mut()
            .unwrap()
            .remove(kind)
            .unwrap_or(json!([]));
        order.insert(
            kind.into(),
            Value::Array(
                rows.as_array()
                    .ok_or("Invalid finance records.")?
                    .iter()
                    .map(|r| r["id"].clone())
                    .collect(),
            ),
        );
        let dir = path.join(kind);
        private_dir(&dir)?;
        let mut keep = HashSet::new();
        for row in rows.as_array().ok_or("Invalid finance records.")? {
            let id = row["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("Missing finance record ID.")?;
            let file = filename(id);
            if !keep.insert(file.clone()) {
                return Err("Duplicate finance record ID.".into());
            }
            storage::atomic_write(
                &dir.join(file),
                &markdown(row, &format!("# {kind} record\n"))?,
            )?;
        }
        remove_stale(&dir, &keep)?;
    }
    value
        .as_object_mut()
        .unwrap()
        .insert("_recordOrder".into(), Value::Object(order));
    write(&path.join("references.json"), &value)
}
fn read_plain(path: &Path) -> Result<FinancePlan> {
    let mut value: Value = read(&path.join("references.json"))?;
    let object = value
        .as_object_mut()
        .ok_or("Invalid finance reference data.")?;
    let order = object.remove("_recordOrder").unwrap_or(json!({}));
    for kind in RECORDS {
        let mut rows = md_files(&path.join(kind))?
            .iter()
            .map(|path| record_value(path))
            .collect::<Result<Vec<_>>>()?;
        let mut ids = HashSet::new();
        for row in &rows {
            if !ids.insert(
                row["id"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("Missing finance record ID.")?,
            ) {
                return Err("Duplicate finance record ID.".into());
            }
        }
        if let Some(order) = order[kind].as_array() {
            rows.sort_by_key(|r| {
                order
                    .iter()
                    .position(|id| id == &r["id"])
                    .unwrap_or(usize::MAX)
            });
        }
        object.insert(kind.into(), Value::Array(rows));
    }
    plan_from_value(value)
}

/// The derived key is kept only while this vault is unlocked, never on disk.
#[derive(Clone)]
pub struct VaultKey {
    bytes: Zeroizing<[u8; 32]>,
    salt: [u8; 16],
}
#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u32,
    algorithm: String,
    kdf: String,
    iterations: u32,
    salt: String,
    nonce: String,
    data: String,
}
pub(crate) fn validate_encrypted_backup(path: &Path) -> Result<()> {
    let envelope: Envelope = read(&path.join("vault.enc"))?;
    VaultKey::validate(&envelope)?;
    VaultKey::salt(&envelope)?;
    if STANDARD
        .decode(&envelope.nonce)
        .map_err(|e| e.to_string())?
        .len()
        != 12
        || STANDARD
            .decode(&envelope.data)
            .map_err(|e| e.to_string())?
            .len()
            < 16
    {
        return Err("Damaged encrypted vault backup.".into());
    }
    Ok(())
}
impl VaultKey {
    pub fn new(password: &str) -> Result<Self> {
        if password.is_empty() {
            return Err("Enter a password for the encrypted vault.".into());
        }
        let mut salt = [0; 16];
        OsRng.fill_bytes(&mut salt);
        Ok(Self::derive(password, salt))
    }
    fn derive(password: &str, salt: [u8; 16]) -> Self {
        let mut bytes = Zeroizing::new([0; 32]);
        pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, ITERATIONS, bytes.as_mut());
        Self { bytes, salt }
    }
    fn seal(&self, id: &str, plan: &FinancePlan) -> Result<Vec<u8>> {
        let mut nonce = [0; 12];
        OsRng.fill_bytes(&mut nonce);
        let cipher = Aes256Gcm::new_from_slice(self.bytes.as_ref()).map_err(|e| e.to_string())?;
        let plaintext = Zeroizing::new(serde_json::to_vec(plan).map_err(|e| e.to_string())?);
        let encrypted = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &plaintext,
                    aad: id.as_bytes(),
                },
            )
            .map_err(|e| e.to_string())?;
        serde_json::to_vec_pretty(&Envelope {
            version: 1,
            algorithm: "AES-256-GCM".into(),
            kdf: "PBKDF2-SHA-256".into(),
            iterations: ITERATIONS,
            salt: STANDARD.encode(self.salt),
            nonce: STANDARD.encode(nonce),
            data: STANDARD.encode(encrypted),
        })
        .map_err(|e| e.to_string())
    }
    pub fn unlock(path: &Path, id: &str, password: &str) -> Result<(FinancePlan, Self)> {
        let envelope: Envelope = read(&path.join("vault.enc"))?;
        Self::validate(&envelope)?;
        let salt = Self::salt(&envelope)?;
        let key = Self::derive(password, salt);
        Ok((key.open(envelope, id)?, key))
    }
    /// Reload an unlocked vault without deriving its key or prompting again.
    pub fn load(&self, path: &Path, id: &str) -> Result<FinancePlan> {
        self.open(read(&path.join("vault.enc"))?, id)
    }
    fn validate(envelope: &Envelope) -> Result<()> {
        if envelope.version != 1
            || envelope.algorithm != "AES-256-GCM"
            || envelope.kdf != "PBKDF2-SHA-256"
            || envelope.iterations != ITERATIONS
        {
            return Err("Unsupported vault encryption format.".into());
        }
        Ok(())
    }
    fn salt(envelope: &Envelope) -> Result<[u8; 16]> {
        STANDARD
            .decode(&envelope.salt)
            .map_err(|e| e.to_string())?
            .try_into()
            .map_err(|_| "Invalid vault salt.".into())
    }
    fn open(&self, envelope: Envelope, id: &str) -> Result<FinancePlan> {
        Self::validate(&envelope)?;
        if Self::salt(&envelope)? != self.salt {
            return Err("The vault password has changed. Lock and reopen the vault.".into());
        }
        let nonce: [u8; 12] = STANDARD
            .decode(envelope.nonce)
            .map_err(|e| e.to_string())?
            .try_into()
            .map_err(|_| "Invalid vault nonce.")?;
        let cipher = Aes256Gcm::new_from_slice(self.bytes.as_ref()).map_err(|e| e.to_string())?;
        let data = STANDARD.decode(envelope.data).map_err(|e| e.to_string())?;
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(
                    Nonce::from_slice(&nonce),
                    Payload {
                        msg: &data,
                        aad: id.as_bytes(),
                    },
                )
                .map_err(|_| "Incorrect password or damaged vault.".to_string())?,
        );
        let value: Value = serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;
        plan_from_value(value)
    }
}

#[cfg(test)]
#[path = "library_tests.rs"]
mod tests;
