//! Whole-directory trash moves and recovery, including the original parent path.
use super::*;

#[derive(Serialize, Deserialize)]
struct Origin {
    path: String,
}

impl Journal {
    pub fn trash(&self, name: &str) -> Result<()> {
        if is_in_trash(name) {
            return Err("This item is already in Trash.".into());
        }
        let source = self.path(name)?;
        let key = id();
        let target = self.path(&format!(
            "{TRASH_FOLDER}/{key}-{}",
            Path::new(name).file_name().unwrap().to_string_lossy()
        ))?;
        let record = self.path(&format!("{TRASH_FOLDER}/.{key}.origin.json"))?;
        // Persist the origin before the move so an interrupted operation remains recoverable.
        storage::atomic_write(
            &record,
            &serde_json::to_vec(&Origin { path: name.into() }).map_err(|e| e.to_string())?,
        )?;
        if let Err(error) = fs::rename(source, target) {
            let _ = fs::remove_file(record);
            return Err(error.to_string());
        }
        Ok(())
    }

    pub fn delete_permanently(&self, name: &str) -> Result<()> {
        self.require_trash_item(name)?;
        let path = self.path(name)?;
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        }
        .map_err(|e| e.to_string())?;
        self.clean_trash_origin(name);
        Ok(())
    }

    /// Restore a note or a whole folder tree, retaining both items on a name conflict.
    pub fn restore(&self, name: &str) -> Result<String> {
        self.require_trash_item(name)?;
        let source = self.path(name)?;
        let metadata = fs::symlink_metadata(&source).map_err(|e| e.to_string())?;
        if !metadata.is_dir() && !metadata.is_file() {
            return Err("Restore a regular note or folder from Trash.".into());
        }
        let original = self.original_path(name)?;
        if is_in_trash(&original) {
            return Err("The original location must be outside Trash.".into());
        }
        let destination = self.path(&original)?;
        let parent = destination
            .parent()
            .ok_or("Missing original parent folder.")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let occupied = fs::read_dir(parent)
            .map_err(|e| e.to_string())?
            .map(|item| item.map(|item| item.file_name().to_string_lossy().to_lowercase()))
            .collect::<std::io::Result<std::collections::BTreeSet<_>>>()
            .map_err(|e| e.to_string())?;
        let file = destination
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or("Invalid original name.")?;
        let (stem, extension) = if metadata.is_file() {
            file.rsplit_once('.')
                .map_or((file, String::new()), |(stem, ext)| {
                    (stem, format!(".{ext}"))
                })
        } else {
            (file, String::new())
        };
        for suffix in 0..10_000 {
            let file = if suffix == 0 {
                file.to_owned()
            } else {
                format!("{stem} (restored {suffix}){extension}")
            };
            if occupied.contains(&file.to_lowercase()) {
                continue;
            }
            let target = parent.join(file);
            fs::rename(&source, &target).map_err(|e| e.to_string())?;
            self.clean_trash_origin(name);
            return Ok(target
                .strip_prefix(&self.root)
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("Invalid restored name.")?
                .replace(std::path::MAIN_SEPARATOR, "/"));
        }
        Err("Too many restored items share the same name.".into())
    }

    fn require_trash_item(&self, name: &str) -> Result<()> {
        if !is_in_trash(name) || name.eq_ignore_ascii_case(TRASH_FOLDER) {
            return Err("Choose an item inside Trash, not the default Trash folder.".into());
        }
        Ok(())
    }

    fn trash_origin_record(&self, name: &str) -> Result<Option<PathBuf>> {
        let top = name.split('/').nth(1).ok_or("Missing Trash item.")?;
        let Some(prefix) = top.get(..37) else {
            return Ok(None);
        };
        if !prefix.ends_with('-') || uuid::Uuid::parse_str(&prefix[..36]).is_err() {
            return Ok(None);
        }
        self.path(&format!("{TRASH_FOLDER}/.{}.origin.json", &prefix[..36]))
            .map(Some)
    }

    fn original_path(&self, name: &str) -> Result<String> {
        let relative = name.split_once('/').ok_or("Missing Trash item.")?.1;
        let (top, suffix) = relative.split_once('/').unwrap_or((relative, ""));
        // Older Trash entries have no saved parent; restore their visible tree at Journal root.
        let mut original = entry_name(&format!("{TRASH_FOLDER}/{top}")).to_owned();
        if let Some(record) = self.trash_origin_record(name)? {
            match fs::read_to_string(record) {
                Ok(text) => {
                    original = serde_json::from_str::<Origin>(&text)
                        .map_err(|e| format!("Cannot read the original Trash location: {e}"))?
                        .path
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.to_string()),
            }
        }
        if !suffix.is_empty() {
            original.push('/');
            original.push_str(suffix);
        }
        Ok(original)
    }

    fn clean_trash_origin(&self, name: &str) {
        if name.matches('/').count() == 1
            && let Ok(Some(record)) = self.trash_origin_record(name)
        {
            // The move/deletion has already succeeded; a stale record must not report failure.
            let _ = fs::remove_file(record);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> (PathBuf, Journal) {
        let root = std::env::temp_dir().join(format!("amigo-journal-restore-{}", id()));
        let journal = Journal::open(&root).unwrap();
        (root, journal)
    }
    fn trashed(journal: &Journal, name: &str) -> String {
        journal
            .entries()
            .unwrap()
            .into_iter()
            .find(|entry| is_in_trash(&entry.path) && entry_name(&entry.path) == name)
            .unwrap()
            .path
    }

    #[test]
    fn removing_and_restoring_a_directory_preserves_its_complete_tree_and_original_parent() {
        let (root, journal) = library();
        for folder in [
            "Projects",
            "Projects/Work",
            "Projects/Work/Deep",
            "Projects/Work/Empty",
        ] {
            journal.create_folder(folder).unwrap();
        }
        journal
            .create("Projects/Work/Deep/Note", "Nested 🌍")
            .unwrap();
        fs::write(root.join("journal/Projects/Work/.hidden"), "Hidden file").unwrap();
        fs::write(
            root.join("journal/Projects/Work/Deep/extra.txt"),
            "Other file",
        )
        .unwrap();
        journal.create("Projects/Keep", "Sibling").unwrap();
        journal.trash("Projects/Work").unwrap();
        assert!(!root.join("journal/Projects/Work").exists());
        assert!(
            !journal
                .entries()
                .unwrap()
                .iter()
                .any(|entry| entry.path.starts_with("Projects/Work"))
        );
        let path = trashed(&journal, "Work");
        assert_eq!(
            journal.read(&format!("{path}/Deep/Note.md")).unwrap().text,
            "Nested 🌍"
        );
        let reopened = Journal::open(&root).unwrap();
        assert_eq!(reopened.restore(&path).unwrap(), "Projects/Work");
        assert!(!root.join("journal").join(&path).exists());
        assert_eq!(
            reopened.read("Projects/Work/Deep/Note.md").unwrap().text,
            "Nested 🌍"
        );
        assert!(root.join("journal/Projects/Work/Empty").is_dir());
        assert_eq!(
            fs::read_to_string(root.join("journal/Projects/Work/.hidden")).unwrap(),
            "Hidden file"
        );
        assert_eq!(
            fs::read_to_string(root.join("journal/Projects/Work/Deep/extra.txt")).unwrap(),
            "Other file"
        );
        assert_eq!(reopened.read("Projects/Keep.md").unwrap().text, "Sibling");
        assert_eq!(fs::read_dir(root.join("journal/Trash")).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn notes_and_individual_children_restore_with_missing_parents_recreated() {
        let (root, journal) = library();
        journal.create_folder("Work").unwrap();
        journal.create_folder("Work/Deep").unwrap();
        journal.create("Work/Deep/First", "First").unwrap();
        journal.create("Work/Deep/Second", "Second").unwrap();
        journal.trash("Work").unwrap();
        let folder = trashed(&journal, "Work");
        assert_eq!(
            journal.restore(&format!("{folder}/Deep/First.md")).unwrap(),
            "Work/Deep/First.md"
        );
        assert_eq!(journal.read("Work/Deep/First.md").unwrap().text, "First");
        assert!(journal.read(&format!("{folder}/Deep/Second.md")).is_ok());
        assert_eq!(journal.restore(&folder).unwrap(), "Work (restored 1)");
        journal.trash("Work/Deep/First.md").unwrap();
        fs::remove_dir_all(root.join("journal/Work")).unwrap();
        let note = trashed(&journal, "First.md");
        assert_eq!(
            Journal::open(&root).unwrap().restore(&note).unwrap(),
            "Work/Deep/First.md"
        );
        assert_eq!(
            journal
                .read("Work (restored 1)/Deep/Second.md")
                .unwrap()
                .text,
            "Second"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restore_keeps_existing_notes_and_folder_trees_on_case_insensitive_collisions() {
        let (root, journal) = library();
        journal.create("Note", "Old note").unwrap();
        journal.trash("Note.md").unwrap();
        journal.create("note", "Current note").unwrap();
        journal
            .create("Note (restored 1)", "Other restored note")
            .unwrap();
        let note = trashed(&journal, "Note.md");
        assert_eq!(journal.restore(&note).unwrap(), "Note (restored 2).md");
        assert_eq!(journal.read("note.md").unwrap().text, "Current note");
        assert_eq!(
            journal.read("Note (restored 2).md").unwrap().text,
            "Old note"
        );
        journal.create_folder("Work").unwrap();
        journal.create("Work/Old", "Old tree").unwrap();
        journal.trash("Work").unwrap();
        journal.create_folder("work").unwrap();
        journal.create("work/New", "Current tree").unwrap();
        assert_eq!(
            journal.restore(&trashed(&journal, "Work")).unwrap(),
            "Work (restored 1)"
        );
        assert_eq!(
            journal.read("Work (restored 1)/Old.md").unwrap().text,
            "Old tree"
        );
        assert_eq!(journal.read("work/New.md").unwrap().text, "Current tree");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_trash_restores_its_visible_tree_at_root_and_invalid_origins_preserve_the_source() {
        let (root, journal) = library();
        let folder = format!("Trash/{}-Legacy", id());
        journal.create_folder(&folder).unwrap();
        journal
            .create(&format!("{folder}/Note"), "Legacy note")
            .unwrap();
        assert_eq!(journal.restore(&folder).unwrap(), "Legacy");
        journal.trash("Legacy").unwrap();
        let path = trashed(&journal, "Legacy");
        let record = journal.trash_origin_record(&path).unwrap().unwrap();
        for origin in ["../outside", "Trash/Other", "/outside"] {
            fs::write(
                &record,
                serde_json::to_vec(&Origin {
                    path: origin.into(),
                })
                .unwrap(),
            )
            .unwrap();
            assert!(journal.restore(&path).is_err());
            assert_eq!(
                journal.read(&format!("{path}/Note.md")).unwrap().text,
                "Legacy note"
            );
        }
        for invalid in ["Trash", "trash", "Legacy", "Trash/../Legacy", "Trash/"] {
            assert!(journal.restore(invalid).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn restore_rejects_symlink_sources_and_destination_parents() {
        let (root, journal) = library();
        journal.create_folder("Work").unwrap();
        journal.create("Work/Note", "Keep safe").unwrap();
        journal.trash("Work/Note.md").unwrap();
        let path = trashed(&journal, "Note.md");
        fs::remove_dir(root.join("journal/Work")).unwrap();
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("journal/Work")).unwrap();
        assert!(journal.restore(&path).is_err());
        assert_eq!(journal.read(&path).unwrap().text, "Keep safe");
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        std::os::unix::fs::symlink(&outside, root.join("journal/Trash/Link")).unwrap();
        assert!(journal.restore("Trash/Link").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
