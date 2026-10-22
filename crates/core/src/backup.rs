//! Versioned, portable Amigo files and staged library replacement.
use crate::{
    attachments,
    credentials::Credentials,
    library::{Library, Settings},
    model::{Workspace, id},
    organizer::Organizer,
    storage::atomic_write,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum Scope {
    Board(String),
    Vault(String),
    Calendar,
    Setup,
}
impl Scope {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Board(_) => "Board",
            Self::Vault(_) => "Vault",
            Self::Calendar => "Calendar",
            Self::Setup => "Amigo Setup",
        }
    }
    pub fn same_kind(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct File {
    data: String,
    sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Archive {
    format: String,
    version: u32,
    pub scope: Scope,
    files: BTreeMap<String, File>,
}

fn safe_path(value: &str) -> Result<&Path> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains(['\\', ':', '\0'])
        || value
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("The Amigo file contains an invalid path.".into());
    }
    Ok(path)
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
struct Stage(PathBuf);
impl Stage {
    fn new(root: &Path) -> Result<Self> {
        let path = root
            .parent()
            .ok_or("Missing library parent folder.")?
            .join(format!(".amigo-stage-{}", id()));
        private_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn visit(root: &Path, path: &Path, files: &mut BTreeMap<String, File>) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            visit(root, &entry.map_err(|e| e.to_string())?.path(), files)?;
        }
    } else if meta.is_file() {
        let relative = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_str()
            .ok_or("File name is not valid UTF-8.")?
            .replace(std::path::MAIN_SEPARATOR, "/");
        safe_path(&relative)?;
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        files.insert(
            relative,
            File {
                sha256: STANDARD.encode(Sha256::digest(&bytes)),
                data: STANDARD.encode(bytes),
            },
        );
    } else {
        return Err("Amigo backups require regular files and folders, not symbolic links.".into());
    }
    Ok(())
}
impl Archive {
    pub fn capture(library: &Library, scope: Scope) -> Result<Self> {
        let mut files = BTreeMap::new();
        match &scope {
            Scope::Board(id) => {
                visit(&library.root, &library.board_path(id)?, &mut files)?;
            }
            Scope::Vault(id) => {
                visit(&library.root, &library.vault_path(id)?, &mut files)?;
            }
            Scope::Calendar => {
                let stage = Stage::new(&library.root)?;
                Organizer::load(&library.root)?.save(&stage.0)?;
                let credentials = Credentials {
                    calendar_urls: Credentials::load(&library.root)?.calendar_urls,
                    ..Default::default()
                };
                credentials.save(&stage.0)?;
                visit(&stage.0, &stage.0, &mut files)?;
            }
            Scope::Setup => {
                visit(&library.root, &library.root, &mut files)?;
                // A migration receipt is local to the original machine.
                let mut settings =
                    serde_json::to_value(&library.settings).map_err(|e| e.to_string())?;
                settings.as_object_mut().unwrap().remove("migration");
                let bytes = serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?;
                files.insert(
                    "library.json".into(),
                    File {
                        sha256: STANDARD.encode(Sha256::digest(&bytes)),
                        data: STANDARD.encode(bytes),
                    },
                );
            }
        }
        let archive = Self {
            format: "amigo".into(),
            version: 1,
            scope,
            files,
        };
        archive.validate_files()?;
        Ok(archive)
    }
    pub fn read(path: &Path) -> Result<Self> {
        let archive: Self = serde_json::from_slice(
            &fs::read(path).map_err(|e| format!("Cannot read Amigo file: {e}"))?,
        )
        .map_err(|e| format!("Invalid Amigo file: {e}"))?;
        archive.validate_files()?;
        Ok(archive)
    }
    pub fn write(&self, path: &Path, library_root: &Path) -> Result<()> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = parent
            .canonicalize()
            .map_err(|e| format!("Choose an existing destination folder: {e}"))?;
        if parent.starts_with(library_root.canonicalize().map_err(|e| e.to_string())?) {
            return Err("Choose a backup destination outside the Amigo library.".into());
        }
        atomic_write(path, &serde_json::to_vec(self).map_err(|e| e.to_string())?)
    }
    fn validate_files(&self) -> Result<()> {
        if self.format != "amigo" || self.version != 1 || self.files.is_empty() {
            return Err("This is not a supported Amigo backup file.".into());
        }
        let prefix = match &self.scope {
            Scope::Board(id) => Some(format!("boards/{}/", valid_id(id)?)),
            Scope::Vault(id) => Some(format!("vaults/{}/", valid_id(id)?)),
            _ => None,
        };
        for (path, file) in &self.files {
            safe_path(path)?;
            if prefix
                .as_ref()
                .is_some_and(|prefix| !path.starts_with(prefix))
                || (self.scope == Scope::Calendar
                    && !["organizer.json", "credentials.enc", "credentials.key"]
                        .contains(&path.as_str()))
            {
                return Err("The Amigo file contains data outside its declared scope.".into());
            }
            let bytes = STANDARD
                .decode(&file.data)
                .map_err(|_| "Damaged backup file data.")?;
            if STANDARD.encode(Sha256::digest(&bytes)) != file.sha256 {
                return Err("The Amigo file is damaged (checksum mismatch).".into());
            }
        }
        let required = match &self.scope {
            Scope::Board(id) => format!("boards/{id}/board.json"),
            Scope::Vault(id) => format!("vaults/{id}/vault.json"),
            Scope::Calendar => "organizer.json".into(),
            Scope::Setup => "library.json".into(),
        };
        if !self.files.contains_key(&required) {
            return Err("The Amigo file is missing required data.".into());
        }
        if self.files.contains_key("credentials.enc") != self.files.contains_key("credentials.key")
        {
            return Err("The backup is missing its connection credentials or key.".into());
        }
        Ok(())
    }
    fn unpack(&self, root: &Path) -> Result<()> {
        for (path, file) in &self.files {
            let target = root.join(safe_path(path)?);
            private_dir(target.parent().ok_or("Missing backup folder.")?)?;
            atomic_write(
                &target,
                &STANDARD.decode(&file.data).map_err(|e| e.to_string())?,
            )?;
        }
        Ok(())
    }
    /// Build and validate the exact proposed replacement without changing live files.
    pub fn review(&self, library: &Library) -> Result<String> {
        let stage = self.prepare(library)?;
        let staged = validate_library(&stage.0)?;
        Ok(match &self.scope {
            Scope::Board(id) => {
                let entry = staged
                    .boards
                    .iter()
                    .find(|b| b.id == *id)
                    .ok_or("Missing imported board.")?;
                format!(
                    "{} · {} tasks",
                    entry.name,
                    staged.load_board(id)?.tasks.len()
                )
            }
            Scope::Vault(id) => {
                let vault = staged
                    .vaults
                    .iter()
                    .find(|v| v.entry.id == *id)
                    .ok_or("Missing imported vault.")?;
                format!(
                    "{} · {}",
                    vault.entry.name,
                    if vault.encrypted {
                        "Password protected"
                    } else {
                        "Not encrypted"
                    }
                )
            }
            Scope::Calendar => format!(
                "{} events · {} subscriptions",
                Organizer::load(&stage.0)?.events.len(),
                Organizer::load(&stage.0)?.subscriptions.len()
            ),
            Scope::Setup => format!(
                "{} boards · {} vaults · {} calendar events",
                staged.boards.len(),
                staged.vaults.len(),
                Organizer::load(&stage.0)?.events.len()
            ),
        })
    }
    fn prepare(&self, library: &Library) -> Result<Stage> {
        self.validate_files()?;
        let stage = Stage::new(&library.root)?;
        if self.scope != Scope::Setup {
            copy_library(library, &stage.0)?;
            match &self.scope {
                Scope::Board(id) => remove_if_exists(&stage.0.join("boards").join(id))?,
                Scope::Vault(id) => remove_if_exists(&stage.0.join("vaults").join(id))?,
                _ => (),
            }
        }
        if self.scope == Scope::Calendar {
            let imported = Stage::new(&library.root)?;
            self.unpack(&imported.0)?;
            let mut credentials = Credentials::load(&stage.0)?;
            let incoming = Credentials::load(&imported.0)?;
            // Calendar import replaces calendar data while preserving Telegram and other connections.
            credentials.calendar_urls = incoming.calendar_urls;
            credentials.save(&stage.0)?;
            Organizer::load(&imported.0)?.save(&stage.0)?;
        } else {
            self.unpack(&stage.0)?;
        }
        // Never run a foreign machine's legacy migration during restore.
        let settings_path = stage.0.join("library.json");
        let mut settings: serde_json::Value =
            serde_json::from_slice(&fs::read(&settings_path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        settings
            .as_object_mut()
            .ok_or("Invalid backup settings.")?
            .remove("migration");
        match &self.scope {
            Scope::Board(id) => settings["board"] = id.clone().into(),
            Scope::Vault(id) => settings["vault"] = id.clone().into(),
            _ => (),
        }
        atomic_write(
            &settings_path,
            &serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?,
        )?;
        validate_library(&stage.0)?;
        Ok(stage)
    }
    pub fn import(&self, library: &Library) -> Result<()> {
        let stage = self.prepare(library)?;
        commit(&library.root, &stage.0)
    }
}
fn valid_id(id: &str) -> Result<&str> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("Invalid backup item ID.".into());
    }
    Ok(id)
}
fn remove_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn copy_library(library: &Library, to: &Path) -> Result<()> {
    let mut files = BTreeMap::new();
    visit(&library.root, &library.root, &mut files)?;
    Archive {
        format: "amigo".into(),
        version: 1,
        scope: Scope::Setup,
        files,
    }
    .unpack(to)
}
fn validate_library(root: &Path) -> Result<Library> {
    // Validation must never initialize a library or run a legacy migration.
    let settings: Settings = serde_json::from_slice(
        &fs::read(root.join("library.json"))
            .map_err(|_| "The backup is missing library settings.")?,
    )
    .map_err(|e| e.to_string())?;
    if settings.version != 1 {
        return Err("Unsupported library version in Amigo file.".into());
    }
    let mut library = Library {
        root: root.into(),
        settings: settings.clone(),
        boards: vec![],
        vaults: vec![],
    };
    library.refresh()?;
    if library.boards.is_empty()
        || library.vaults.is_empty()
        || !library.boards.iter().any(|b| b.id == settings.board)
        || !library.vaults.iter().any(|v| v.entry.id == settings.vault)
    {
        return Err("The backup is missing its selected board or vault.".into());
    }
    for board in &library.boards {
        let workspace = library.load_board(&board.id)?;
        for task in &workspace.tasks {
            for file in &task.attachments {
                let path = attachments::resolve(&library.board_path(&board.id)?, file)?;
                if fs::metadata(path).map_err(|e| e.to_string())?.len() != file.size {
                    return Err("A task attachment is incomplete.".into());
                }
            }
        }
    }
    for vault in &library.vaults {
        if vault.encrypted {
            crate::library::validate_encrypted_backup(&library.vault_path(&vault.entry.id)?)?;
        } else {
            library.load_plain_vault(vault)?;
        }
    }
    Organizer::load(root)?;
    Credentials::load(root)?;
    Ok(library)
}

pub fn delete(library: &Library, scope: &Scope) -> Result<()> {
    let stage = Stage::new(&library.root)?;
    copy_library(library, &stage.0)?;
    let mut staged = Library::open(stage.0.clone(), None)?;
    match scope {
        Scope::Board(id) => {
            if !staged.boards.iter().any(|b| b.id == *id) {
                return Err("Board no longer exists.".into());
            }
            remove_if_exists(&staged.board_path(id)?)?;
            staged.refresh()?;
            if staged.boards.is_empty() {
                staged.create_board("Personal", &Workspace::default())?;
            }
            if staged.settings.board == *id {
                staged.settings.board = staged.boards[0].id.clone();
            }
        }
        Scope::Vault(id) => {
            if !staged.vaults.iter().any(|v| v.entry.id == *id) {
                return Err("Vault no longer exists.".into());
            }
            remove_if_exists(&staged.vault_path(id)?)?;
            staged.refresh()?;
            if staged.vaults.is_empty() {
                staged.create_vault("Personal", &crate::model::FinancePlan::empty(), None)?;
            }
            if staged.settings.vault == *id {
                staged.settings.vault = staged.vaults[0].entry.id.clone();
            }
        }
        _ => return Err("Only boards and vaults can be deleted here.".into()),
    }
    staged.save_settings()?;
    validate_library(&stage.0)?;
    commit(&library.root, &stage.0)
}

fn recovery_path(root: &Path) -> Result<PathBuf> {
    Ok(root
        .parent()
        .ok_or("Missing library parent folder.")?
        .join(format!(
            ".{}.amigo-recovery",
            root.file_name()
                .and_then(|s| s.to_str())
                .ok_or("Invalid library folder.")?
        )))
}
pub(crate) fn recover(root: &Path) -> Result<()> {
    let marker = recovery_path(root)?;
    if !marker.exists() {
        return Ok(());
    }
    let old = fs::read_to_string(&marker).map_err(|e| e.to_string())?;
    valid_id(&old)?;
    if !old.starts_with("amigo-rollback-") {
        return Err("Invalid restore recovery marker.".into());
    }
    let old = root.parent().unwrap().join(old);
    if !root.exists() && old.exists() {
        fs::rename(&old, root).map_err(|e| format!("Cannot recover library: {e}"))?;
    }
    if validate_library(root).is_err() {
        return Err(
            "Interrupted restore requires recovery; existing files have been preserved.".into(),
        );
    }
    remove_if_exists(&old)?;
    fs::remove_file(marker).map_err(|e| e.to_string())
}
fn commit(root: &Path, stage: &Path) -> Result<()> {
    recover(root)?;
    let rollback_name = format!("amigo-rollback-{}", id());
    let old = root.parent().unwrap().join(&rollback_name);
    let marker = recovery_path(root)?;
    atomic_write(&marker, rollback_name.as_bytes())?;
    if let Err(e) = fs::rename(root, &old) {
        let _ = fs::remove_file(marker);
        return Err(format!("Cannot replace library: {e}"));
    }
    if let Err(e) = fs::rename(stage, root) {
        recover(root)?;
        return Err(format!(
            "Cannot install imported library: {e}. Previous data was restored."
        ));
    }
    // A committed replacement remains usable even if old-folder cleanup must retry on startup.
    let _ = recover(root);
    Ok(())
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
