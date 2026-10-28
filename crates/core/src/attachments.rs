//! Board-owned file copies. Attachment metadata never grants arbitrary filesystem access.
use crate::model::{TaskAttachment, id};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

fn safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}
/// Copies remain provisional until their owning editor accepts them.
pub struct Batch {
    pub root: PathBuf,
    pub files: Vec<TaskAttachment>,
}
impl Batch {
    pub fn import(root: PathBuf, sources: &[PathBuf]) -> Result<Self, String> {
        let mut batch = Self {
            root,
            files: vec![],
        };
        for path in sources {
            batch.files.push(import(&batch.root, path)?);
        }
        Ok(batch)
    }
    pub fn keep(&mut self) {
        self.files.clear();
    }
}
impl Drop for Batch {
    fn drop(&mut self) {
        rollback(&self.root, &self.files);
    }
}
pub fn import(board: &Path, source: &Path) -> Result<TaskAttachment, String> {
    let mut input = fs::File::open(source).map_err(|e| format!("Cannot read attachment: {e}"))?;
    if !input.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Choose a regular file.".into());
    }
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| safe_name(s))
        .ok_or("The attachment needs a valid file name.")?
        .to_string();
    let attachment = TaskAttachment {
        id: id(),
        name,
        size: input.metadata().map_err(|e| e.to_string())?.len(),
    };
    let root = board.join("attachments");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    if !root
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(board.canonicalize().map_err(|e| e.to_string())?)
    {
        return Err("Attachment folder leaves its board folder.".into());
    }
    let folder = root.join(&attachment.id);
    fs::create_dir(&folder).map_err(|e| e.to_string())?;
    let temporary = folder.join(".copying");
    let result = (|| {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        fs::rename(&temporary, folder.join(&attachment.name)).map_err(|e| e.to_string())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&folder);
        return Err(error);
    }
    Ok(attachment)
}
pub fn resolve(board: &Path, attachment: &TaskAttachment) -> Result<PathBuf, String> {
    if attachment.id.is_empty()
        || !attachment
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || !safe_name(&attachment.name)
    {
        return Err("Invalid attachment path.".into());
    }
    let root = board
        .join("attachments")
        .canonicalize()
        .map_err(|e| format!("Attachment folder is unavailable: {e}"))?;
    if !root.starts_with(board.canonicalize().map_err(|e| e.to_string())?) {
        return Err("Attachment folder leaves its board folder.".into());
    }
    let file = root
        .join(&attachment.id)
        .join(&attachment.name)
        .canonicalize()
        .map_err(|e| format!("Attachment is unavailable: {e}"))?;
    if !file.starts_with(&root) || !file.is_file() {
        return Err("Attachment path leaves its board folder.".into());
    }
    Ok(file)
}

/// Roll back a failed multi-file import; callers only pass files just created by import.
pub fn rollback(board: &Path, attachments: &[TaskAttachment]) {
    for attachment in attachments {
        if let Ok(path) = resolve(board, attachment) {
            let _ = fs::remove_file(&path);
            if let Some(parent) = path.parent() {
                let _ = fs::remove_dir(parent);
            }
        }
    }
}

pub(crate) fn prune(board: &Path, tasks: &[crate::model::Task]) -> Result<(), String> {
    let root = board.join("attachments");
    if !root.exists() {
        return Ok(());
    }
    if !root
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(board.canonicalize().map_err(|e| e.to_string())?)
    {
        return Err("Attachment folder leaves its board folder.".into());
    }
    let live: std::collections::HashSet<_> = tasks
        .iter()
        .flat_map(|t| &t.attachments)
        .map(|a| a.id.as_str())
        .collect();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if uuid::Uuid::parse_str(&name).is_ok()
            && !live.contains(name.as_ref())
            && entry.file_type().map_err(|e| e.to_string())?.is_dir()
        {
            fs::remove_dir_all(entry.path()).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    #[test]
    fn failed_and_cancelled_batches_roll_back_only_their_new_files() {
        let root = std::env::temp_dir().join(format!("amigo-media-batch-{}", id()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.txt");
        fs::write(&source, "original").unwrap();
        let kept = import(&root, &source).unwrap();
        let batch = Batch::import(root.clone(), std::slice::from_ref(&source)).unwrap();
        let cancelled = batch.files[0].clone();
        drop(batch);
        assert!(resolve(&root, &cancelled).is_err());
        assert!(Batch::import(root.clone(), &[source.clone(), root.join("missing")]).is_err());
        assert_eq!(fs::read_dir(root.join("attachments")).unwrap().count(), 1);
        assert!(resolve(&root, &kept).unwrap().is_file());
        assert_eq!(fs::read_to_string(source).unwrap(), "original");
        fs::remove_dir_all(root).unwrap();
    }
}
