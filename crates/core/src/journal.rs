//! Portable Markdown notes. Paths are relative to the journal; writes detect external edits.
use crate::{model::id, storage};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub mod formatting;

type Result<T> = std::result::Result<T, String>;
pub const MAX_NOTE_BYTES: u64 = 8 * 1024 * 1024;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Source,
    Preview,
    Split,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub mode: Mode,
    pub font_size: f32,
    pub word_wrap: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            mode: Mode::Split,
            font_size: 16.,
            word_wrap: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub folder: bool,
}
#[derive(Clone, Debug)]
pub struct Document {
    pub path: String,
    pub text: String,
    pub fingerprint: String,
}
#[derive(Clone)]
pub struct Journal {
    root: PathBuf,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn relative(value: &str) -> Result<&Path> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains(['\\', ':', '\0'])
        || value.split('/').any(|s| {
            s.is_empty()
                || s == "."
                || s == ".."
                || s == ".trash"
                || s.ends_with(['.', ' '])
                || s.chars().any(|c| c.is_control() || "<>\"|?*".contains(c))
                || reserved(s)
        })
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(
            "Use a relative note path without reserved names or special path characters.".into(),
        );
    }
    Ok(path)
}
fn reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}
impl Journal {
    pub fn open(library: &Path) -> Result<Self> {
        let root = library.join("journal");
        if root.exists()
            && fs::symlink_metadata(&root)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
        {
            return Err("The Journal folder must not be a symbolic link.".into());
        }
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        Ok(Self {
            root: root.canonicalize().map_err(|e| e.to_string())?,
        })
    }
    fn path(&self, value: &str) -> Result<PathBuf> {
        let relative = relative(value)?;
        let mut path = self.root.clone();
        for part in relative.components() {
            path.push(part);
            match fs::symlink_metadata(&path) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err("Journal paths must not contain symbolic links.".into());
                }
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
                _ => (),
            }
        }
        Ok(path)
    }
    pub fn entries(&self) -> Result<Vec<Entry>> {
        fn visit(base: &Path, dir: &Path, out: &mut Vec<Entry>, depth: usize) -> Result<()> {
            if depth > 48 {
                return Err("The notebook contains too many nested folders.".into());
            }
            for item in fs::read_dir(dir).map_err(|e| e.to_string())? {
                let item = item.map_err(|e| e.to_string())?;
                let kind = item.file_type().map_err(|e| e.to_string())?;
                let path = item.path();
                let Some(name) = item.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if name.starts_with('.') || kind.is_symlink() {
                    continue;
                }
                let key = path
                    .strip_prefix(base)
                    .map_err(|e| e.to_string())?
                    .to_str()
                    .ok_or("A note name is not UTF-8.")?
                    .replace(std::path::MAIN_SEPARATOR, "/");
                if kind.is_dir() {
                    out.push(Entry {
                        path: key,
                        folder: true,
                    });
                    visit(base, &path, out, depth + 1)?;
                } else if kind.is_file()
                    && path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
                {
                    out.push(Entry {
                        path: key,
                        folder: false,
                    });
                }
            }
            Ok(())
        }
        let mut entries = vec![];
        visit(&self.root, &self.root, &mut entries, 0)?;
        entries.sort_by_key(|e| e.path.to_lowercase());
        Ok(entries)
    }
    pub fn read(&self, name: &str) -> Result<Document> {
        let path = self.path(name)?;
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            return Err("Choose a Markdown (.md) note.".into());
        }
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        use std::io::Read;
        let mut bytes = vec![];
        file.take(MAX_NOTE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_NOTE_BYTES {
            return Err(
                "This note exceeds the 8 MiB editor limit. Split it into smaller notes.".into(),
            );
        }
        let fingerprint = hash(&bytes);
        let text = String::from_utf8(bytes).map_err(|_| "Notes must use UTF-8 encoding.")?;
        Ok(Document {
            path: name.into(),
            text,
            fingerprint,
        })
    }
    pub fn save(&self, name: &str, text: &str, expected: &str) -> Result<String> {
        if text.len() as u64 > MAX_NOTE_BYTES {
            return Err("This note exceeds the 8 MiB editor limit.".into());
        }
        let current = self.read(name)?;
        if current.fingerprint != expected {
            return Err(
                "This note changed outside Amigo. Reload it or save your edits as a copy.".into(),
            );
        }
        storage::atomic_write(&self.path(name)?, text.as_bytes())?;
        Ok(hash(text.as_bytes()))
    }
    pub fn create(&self, name: &str, text: &str) -> Result<Document> {
        let name = if name.to_ascii_lowercase().ends_with(".md") {
            name.to_owned()
        } else {
            format!("{name}.md")
        };
        let path = self.path(&name)?;
        if text.len() as u64 > MAX_NOTE_BYTES {
            return Err("This note exceeds the 8 MiB editor limit.".into());
        }
        let parent = path.parent().ok_or("Missing note folder.")?;
        if !parent.is_dir() {
            return Err("Create the note's folder first.".into());
        }
        Self::vacant(
            parent,
            path.file_name()
                .and_then(|p| p.to_str())
                .ok_or("Invalid note name.")?,
            None,
        )?;
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        if let Err(e) = file
            .write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
        {
            drop(file);
            let _ = fs::remove_file(path);
            return Err(e.to_string());
        }
        Ok(Document {
            path: name,
            text: text.into(),
            fingerprint: hash(text.as_bytes()),
        })
    }
    fn vacant(parent: &Path, name: &str, except: Option<&Path>) -> Result<()> {
        for item in fs::read_dir(parent).map_err(|e| e.to_string())? {
            let item = item.map_err(|e| e.to_string())?;
            if item.file_name().to_string_lossy().to_lowercase() == name.to_lowercase()
                && except != Some(item.path().as_path())
            {
                return Err("A note or folder with this name already exists.".into());
            }
        }
        Ok(())
    }
    pub fn create_folder(&self, name: &str) -> Result<()> {
        let path = self.path(name)?;
        let parent = path.parent().ok_or("Missing parent folder.")?;
        Self::vacant(
            parent,
            path.file_name()
                .and_then(|p| p.to_str())
                .ok_or("Invalid folder name.")?,
            None,
        )?;
        fs::create_dir(path).map_err(|e| e.to_string())
    }
    pub fn rename(&self, old: &str, name: &str) -> Result<String> {
        let old_path = self.path(old)?;
        let folder = old_path.is_dir();
        let name = if folder || name.to_ascii_lowercase().ends_with(".md") {
            name.to_owned()
        } else {
            format!("{name}.md")
        };
        let new_path = self.path(&name)?;
        if new_path == old_path {
            return Ok(name);
        }
        let parent = new_path.parent().ok_or("Missing note folder.")?;
        Self::vacant(
            parent,
            new_path
                .file_name()
                .and_then(|p| p.to_str())
                .ok_or("Invalid note name.")?,
            Some(&old_path),
        )?;
        fs::rename(old_path, new_path).map_err(|e| e.to_string())?;
        Ok(name)
    }
    pub fn trash(&self, name: &str) -> Result<()> {
        let path = self.path(name)?;
        let trash = self.root.join(".trash");
        if trash.exists()
            && fs::symlink_metadata(&trash)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
        {
            return Err("The Journal trash folder must not be a symbolic link.".into());
        }
        fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
        fs::rename(
            path,
            trash.join(format!(
                "{}-{}",
                id(),
                Path::new(name).file_name().unwrap().to_string_lossy()
            )),
        )
        .map_err(|e| e.to_string())
    }
    pub fn import(&self, sources: &[PathBuf], folder: &str) -> Result<Vec<Document>> {
        let mut documents = vec![];
        let mut created = vec![];
        let result = (|| {
            for source in sources {
                let meta = fs::symlink_metadata(source).map_err(|e| e.to_string())?;
                if !meta.is_file()
                    || meta.len() > MAX_NOTE_BYTES
                    || !source
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
                {
                    return Err("Import regular UTF-8 .md files up to 8 MiB each.".into());
                }
                let text = fs::read_to_string(source).map_err(|e| e.to_string())?;
                let name = source
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or("Invalid note filename.")?;
                let stem = source
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or("Invalid note filename.")?;
                let before = documents.len();
                for suffix in 0..10_000 {
                    let file = if suffix == 0 {
                        name.to_owned()
                    } else {
                        format!("{stem} ({suffix}).md")
                    };
                    let file = if folder.is_empty() {
                        file
                    } else {
                        format!("{folder}/{file}")
                    };
                    match self.create(&file, &text) {
                        Ok(doc) => {
                            created.push(self.path(&doc.path)?);
                            documents.push(doc);
                            break;
                        }
                        Err(error)
                            if error.contains("already exists") || self.path(&file)?.exists() =>
                        {
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
                if documents.len() == before {
                    return Err("Too many imported notes share the same name.".into());
                }
            }
            Ok(documents)
        })();
        if result.is_err() {
            for path in created {
                let _ = fs::remove_file(path);
            }
        }
        result
    }
    pub fn search(&self, query: &str) -> Result<Vec<Entry>> {
        let query = query.to_lowercase();
        Ok(self
            .entries()?
            .into_iter()
            .filter(|entry| {
                !entry.folder
                    && (entry.path.to_lowercase().contains(&query)
                        || self
                            .read(&entry.path)
                            .is_ok_and(|d| d.text.to_lowercase().contains(&query)))
            })
            .collect())
    }
    pub fn follow_link(&self, from: &str, target: &str) -> Result<Document> {
        let target = target.strip_prefix("amigo-journal://").unwrap_or(target);
        let target = percent_encoding::percent_decode_str(target)
            .decode_utf8()
            .map_err(|_| "The note link is not valid UTF-8.")?;
        let target = target.split('#').next().unwrap_or("");
        if target.contains(':') || target.starts_with('/') || target.contains('\\') {
            return Err("Use a relative link to a Journal note.".into());
        }
        let target = if target.to_lowercase().ends_with(".md") {
            target.into()
        } else {
            format!("{target}.md")
        };
        let entries = self.entries()?;
        let parent = from.rsplit_once('/').map_or("", |(p, _)| p);
        let relative = if parent.is_empty() {
            target.clone()
        } else {
            format!("{parent}/{target}")
        };
        for candidate in [relative, target.clone()] {
            let mut parts = vec![];
            let mut valid = true;
            for part in candidate.split('/') {
                match part {
                    ".." => {
                        if parts.pop().is_none() {
                            valid = false;
                            break;
                        }
                    }
                    "." | "" => (),
                    _ => parts.push(part),
                }
            }
            if valid {
                let path = parts.join("/");
                if entries.iter().any(|e| !e.folder && e.path == path) {
                    return self.read(&path);
                }
            }
        }
        let matches: Vec<_> = entries
            .iter()
            .filter(|e| !e.folder && e.path.rsplit('/').next() == Some(target.as_str()))
            .collect();
        if matches.len() == 1 {
            return self.read(&matches[0].path);
        }
        Err(if matches.is_empty() {
            "This linked note does not exist in Journal."
        } else {
            "This link matches multiple notes. Include its folder in the link."
        }
        .into())
    }
    pub fn export(&self, name: &str, destination: &Path) -> Result<()> {
        let doc = self.read(name)?;
        let parent = destination
            .parent()
            .ok_or("Choose an export destination.")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if parent.starts_with(&self.root) {
            return Err("Export outside the Journal folder to preserve your working notes.".into());
        }
        storage::atomic_write(destination, doc.text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn library() -> (PathBuf, Journal) {
        let root = std::env::temp_dir().join(format!("amigo-journal-{}", id()));
        let journal = Journal::open(&root).unwrap();
        (root, journal)
    }
    #[test]
    fn markdown_is_plain_unicode_roundtrips_and_preserves_front_matter() {
        let (root, j) = library();
        j.create_folder("Work").unwrap();
        let text = "---\ntags: [work]\n---\n# Щоденник\n\n**Hello** 🌍\n- [ ] item\n[[Other]]\n";
        let doc = j.create("Work/Щоденник", text).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("journal/Work/Щоденник.md")).unwrap(),
            text
        );
        let edited = format!("{text}\nMore");
        j.save(&doc.path, &edited, &doc.fingerprint).unwrap();
        assert_eq!(j.read(&doc.path).unwrap().text, edited);
        assert_eq!(j.search("Hello").unwrap().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn outside_edits_and_deletions_are_never_silently_overwritten() {
        let (root, j) = library();
        let doc = j.create("Note", "first").unwrap();
        fs::write(root.join("journal/Note.md"), "external").unwrap();
        assert!(
            j.save(&doc.path, "draft", &doc.fingerprint)
                .unwrap_err()
                .contains("outside")
        );
        assert_eq!(j.read(&doc.path).unwrap().text, "external");
        j.trash(&doc.path).unwrap();
        assert!(j.save(&doc.path, "draft", &doc.fingerprint).is_err());
        assert!(j.entries().unwrap().is_empty());
        assert_eq!(
            fs::read_dir(root.join("journal/.trash")).unwrap().count(),
            1
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn import_copies_without_overwriting_and_rolls_back_a_failed_batch() {
        let (root, j) = library();
        let source = root.join("Note.md");
        fs::write(&source, "import").unwrap();
        j.create("Note", "original").unwrap();
        let notes = j.import(std::slice::from_ref(&source), "").unwrap();
        assert_eq!(notes[0].path, "Note (1).md");
        assert_eq!(fs::read_to_string(&source).unwrap(), "import");
        let bad = root.join("bad.md");
        fs::write(&bad, [255]).unwrap();
        assert!(j.import(&[source, bad], "").is_err());
        assert_eq!(j.entries().unwrap().len(), 2);
        assert_eq!(j.read("Note.md").unwrap().text, "original");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn paths_names_and_case_collisions_are_portable_and_safe() {
        let (root, j) = library();
        for name in [
            "../outside",
            "/absolute",
            "a/../b",
            "CON",
            "COM1.md",
            "a\\b",
            "C:/note",
            ".trash/a",
            "bad?",
        ] {
            assert!(j.create(name, "text").is_err(), "{name}");
        }
        j.create("Note", "text").unwrap();
        assert!(j.create("note", "other").is_err());
        j.rename("Note.md", "Renamed").unwrap();
        assert_eq!(j.read("Renamed.md").unwrap().text, "text");
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_escape_the_library() {
        let (root, j) = library();
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("journal/link")).unwrap();
        assert!(j.create("link/escape", "secret").is_err());
        assert!(j.entries().unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod link_tests {
    use super::*;
    #[test]
    fn links_resolve_relative_paths_unicode_and_ambiguous_filenames_safely() {
        let root = std::env::temp_dir().join(format!("amigo-journal-links-{}", id()));
        let journal = Journal::open(&root).unwrap();
        journal.create_folder("Work").unwrap();
        journal.create_folder("Other").unwrap();
        journal.create("Work/Note", "One").unwrap();
        journal.create("Other/Note", "Two").unwrap();
        journal.create("Щоденник", "Unicode").unwrap();
        assert_eq!(
            journal
                .follow_link("Work/Current.md", "Note.md")
                .unwrap()
                .text,
            "One"
        );
        assert_eq!(
            journal
                .follow_link("Other/Current.md", "../Work/Note.md")
                .unwrap()
                .text,
            "One"
        );
        assert!(journal.follow_link("Root.md", "Note").is_err());
        assert_eq!(
            journal
                .follow_link(
                    "Root.md",
                    "amigo-journal://%D0%A9%D0%BE%D0%B4%D0%B5%D0%BD%D0%BD%D0%B8%D0%BA"
                )
                .unwrap()
                .text,
            "Unicode"
        );
        assert!(journal.follow_link("Root.md", "../../outside.md").is_err());
        assert!(
            journal
                .follow_link("Root.md", "file:///outside.md")
                .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
