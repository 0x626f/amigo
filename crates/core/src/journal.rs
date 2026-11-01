//! Portable Markdown notes and PDFs. Journal-relative paths and checked file writes.
use crate::{model::id, storage};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

#[cfg(test)]
mod file_tests;
pub mod formatting;
pub mod pdf;
mod trash;

type Result<T> = std::result::Result<T, String>;
pub const MAX_NOTE_BYTES: u64 = 8 * 1024 * 1024;
pub const TRASH_FOLDER: &str = "Trash";

pub fn is_pdf(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

pub fn is_in_trash(path: &str) -> bool {
    path.split('/')
        .next()
        .is_some_and(|folder| folder.eq_ignore_ascii_case(TRASH_FOLDER))
}
/// Trash storage IDs avoid collisions; people still see the original filename.
pub fn entry_name(path: &str) -> &str {
    let mut name = path.rsplit('/').next().unwrap_or(path);
    if is_in_trash(path) {
        while let Some(prefix) = name.get(..37) {
            if !prefix.ends_with('-') || uuid::Uuid::parse_str(&prefix[..36]).is_err() {
                break;
            }
            name = &name[37..];
        }
    }
    name
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Source,
    Preview,
    Split,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewLayout {
    #[default]
    SideBySide,
    Stacked,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub mode: Mode,
    pub font_size: f32,
    pub word_wrap: bool,
    pub preview_font_size: f32,
    pub preview_layout: PreviewLayout,
    pub preview_editable: bool,
    pub sync_scroll: bool,
    pub line_numbers: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            mode: Mode::Split,
            font_size: 16.,
            word_wrap: true,
            preview_font_size: 16.,
            preview_layout: PreviewLayout::SideBySide,
            preview_editable: false,
            sync_scroll: true,
            line_numbers: true,
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
    pub pdf: Option<pdf::PdfDocument>,
    /// Temporary drafts retain their source identity; core saves require an explicit import.
    pub external: Option<PathBuf>,
}
#[derive(Clone)]
pub struct Journal {
    root: PathBuf,
}
impl Journal {
    pub fn media_folder(&self) -> Result<PathBuf> {
        let path = self.root.join(".media");
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("The media directory must not be a symbolic link.".into());
        }
        fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        Ok(path)
    }
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
        let journal = Self {
            root: root.canonicalize().map_err(|e| e.to_string())?,
        };
        journal.prepare_trash()?;
        Ok(journal)
    }
    fn prepare_trash(&self) -> Result<()> {
        let trash = self.path(TRASH_FOLDER)?;
        if !trash.exists() {
            for entry in fs::read_dir(&self.root).map_err(|error| error.to_string())? {
                let entry = entry.map_err(|error| error.to_string())?;
                if entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(TRASH_FOLDER)
                {
                    let kind = entry.file_type().map_err(|error| error.to_string())?;
                    if !kind.is_dir() || kind.is_symlink() {
                        return Err("The Journal Trash must be a regular folder.".into());
                    }
                    fs::rename(entry.path(), &trash).map_err(|error| error.to_string())?;
                    break;
                }
            }
        }
        fs::create_dir_all(&trash).map_err(|error| error.to_string())?;
        let legacy = self.root.join(".trash");
        match fs::symlink_metadata(&legacy) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => (),
            Ok(_) => return Err("The old Journal trash must be a regular folder.".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.to_string()),
        }
        let mut names: std::collections::BTreeSet<_> = fs::read_dir(&trash)
            .map_err(|error| error.to_string())?
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().to_lowercase()))
            .collect::<std::io::Result<_>>()
            .map_err(|error| error.to_string())?;
        for entry in fs::read_dir(&legacy).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let mut name = entry.file_name();
            while !names.insert(name.to_string_lossy().to_lowercase()) {
                let mut unique = std::ffi::OsString::from(format!("{}-", id()));
                unique.push(name);
                name = unique;
            }
            fs::rename(entry.path(), trash.join(name)).map_err(|error| error.to_string())?;
        }
        fs::remove_dir(legacy).map_err(|error| error.to_string())
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
                    && path.extension().is_some_and(|e| {
                        e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("pdf")
                    })
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
        entries.sort_by_key(|e| (is_in_trash(&e.path), e.path.to_lowercase()));
        Ok(entries)
    }
    pub fn read(&self, name: &str) -> Result<Document> {
        let path = self.path(name)?;
        let is_pdf = is_pdf(name);
        if !is_pdf
            && !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            return Err("Choose a Markdown (.md) or PDF (.pdf) document.".into());
        }
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        use std::io::Read;
        let mut bytes = vec![];
        let limit = if is_pdf {
            pdf::MAX_PDF_BYTES
        } else {
            MAX_NOTE_BYTES
        };
        file.take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > limit {
            return Err(if is_pdf {
                "This PDF exceeds the 64 MiB document limit."
            } else {
                "This note exceeds the 8 MiB editor limit. Split it into smaller notes."
            }
            .into());
        }
        let fingerprint = hash(&bytes);
        let (text, pdf) = if is_pdf {
            pdf::validate_header(&bytes)?;
            (String::new(), Some(pdf::PdfDocument::new(bytes)))
        } else {
            (
                String::from_utf8(bytes).map_err(|_| "Notes must use UTF-8 encoding.")?,
                None,
            )
        };
        Ok(Document {
            path: name.into(),
            text,
            fingerprint,
            pdf,
            external: None,
        })
    }
    pub fn open_file(&self, source: &Path) -> Result<Document> {
        if fs::symlink_metadata(source)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Open a regular Markdown or PDF file.".into());
        }
        let canonical = source.canonicalize().map_err(|e| e.to_string())?;
        if let Ok(relative) = canonical.strip_prefix(&self.root) {
            return self.read(
                &relative
                    .to_str()
                    .ok_or("Invalid document path.")?
                    .replace(std::path::MAIN_SEPARATOR, "/"),
            );
        }
        Self::read_external(&canonical)
    }
    /// Open a bounded external file without importing or modifying it.
    pub fn read_external(source: &Path) -> Result<Document> {
        let metadata = fs::symlink_metadata(source).map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Err("Open a regular Markdown or PDF file.".into());
        }
        let filename = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Invalid document filename.")?;
        let pdf_file = is_pdf(filename);
        if !pdf_file
            && !source
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            return Err("Open a Markdown (.md) or PDF (.pdf) file.".into());
        }
        let limit = if pdf_file {
            pdf::MAX_PDF_BYTES
        } else {
            MAX_NOTE_BYTES
        };
        use std::io::Read;
        let mut bytes = Vec::new();
        fs::File::open(source)
            .map_err(|e| e.to_string())?
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > limit {
            return Err(
                "This document exceeds its size limit (Markdown: 8 MiB, PDF: 64 MiB).".into(),
            );
        }
        let fingerprint = hash(&bytes);
        let (text, pdf) = if pdf_file {
            pdf::validate_header(&bytes)?;
            (String::new(), Some(pdf::PdfDocument::new(bytes)))
        } else {
            (
                String::from_utf8(bytes).map_err(|_| "Notes must use UTF-8 encoding.")?,
                None,
            )
        };
        let source = source.canonicalize().map_err(|e| e.to_string())?;
        Ok(Document {
            path: source
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/"),
            text,
            pdf,
            fingerprint,
            external: Some(source),
        })
    }
    pub fn store_document(&self, document: &Document, folder: &str) -> Result<Document> {
        let stem = Path::new(&document.path)
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid document filename.")?;
        let bytes = if let Some(pdf) = &document.pdf {
            pdf::save(pdf)?
        } else {
            document.text.as_bytes().to_vec()
        };
        if document.pdf.is_none() && bytes.len() as u64 > MAX_NOTE_BYTES {
            return Err("This note exceeds the 8 MiB editor limit.".into());
        }
        self.unique_file(
            folder,
            stem,
            if document.pdf.is_some() { "pdf" } else { "md" },
            &bytes,
        )
    }
    pub fn move_file(&self, name: &str, folder: &str) -> Result<String> {
        if is_in_trash(name) || is_in_trash(folder) {
            return Err("Use Restore to move documents out of Trash.".into());
        }
        let source = self.path(name)?;
        if !source.is_file() {
            return Err("Choose a document to move.".into());
        }
        if !folder.is_empty() && !self.path(folder)?.is_dir() {
            return Err("Choose an existing Journal folder.".into());
        }
        let filename = source
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Invalid document filename.")?;
        self.rename(
            name,
            &if folder.is_empty() {
                filename.into()
            } else {
                format!("{folder}/{filename}")
            },
        )
    }
    pub fn save_document(&self, document: &Document) -> Result<String> {
        if document.external.is_some() {
            return Err(
                "Save this external draft to Journal first. The original file is unchanged.".into(),
            );
        }
        if let Some(pdf) = &document.pdf {
            let bytes = pdf::save(pdf)?;
            let current = self.read(&document.path)?;
            if current.fingerprint != document.fingerprint {
                return Err(
                    "This PDF changed outside Amigo. Reload it or save your edits as a copy."
                        .into(),
                );
            }
            storage::atomic_write(&self.path(&document.path)?, &bytes)?;
            Ok(hash(&bytes))
        } else {
            self.save(&document.path, &document.text, &document.fingerprint)
        }
    }
    pub fn save_copy(&self, document: &Document) -> Result<Document> {
        if document.external.is_some() {
            return self.store_document(document, "");
        }
        let stem = Path::new(&document.path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let parent = document.path.rsplit_once('/').map_or("", |(p, _)| p);
        let extension = if document.pdf.is_some() { "pdf" } else { "md" };
        let bytes = if let Some(pdf) = &document.pdf {
            pdf::save(pdf)?
        } else {
            document.text.as_bytes().to_vec()
        };
        self.unique_file(parent, &format!("{stem} (recovered)"), extension, &bytes)
    }
    /// Convert a saved note to a sibling PDF without replacing either file.
    pub fn convert_to_pdf(&self, name: &str) -> Result<Document> {
        let document = self.read(name)?;
        if document.pdf.is_some() {
            return Err("Choose a Markdown note to convert to PDF.".into());
        }
        if is_in_trash(name) {
            return Err("Restore this note before converting it.".into());
        }
        let stem = Path::new(name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let bytes = pdf::convert(&document.text, &stem, &self.root, name)?;
        self.unique_file(
            name.rsplit_once('/').map_or("", |(p, _)| p),
            &stem,
            "pdf",
            &bytes,
        )
    }
    fn unique_file(
        &self,
        folder: &str,
        stem: &str,
        extension: &str,
        bytes: &[u8],
    ) -> Result<Document> {
        if is_in_trash(folder) {
            return Err("Choose a destination outside Trash.".into());
        }
        for suffix in 0..10_000 {
            let filename = if suffix == 0 {
                format!("{stem}.{extension}")
            } else {
                format!("{stem} ({suffix}).{extension}")
            };
            let name = if folder.is_empty() {
                filename
            } else {
                format!("{folder}/{filename}")
            };
            let path = self.path(&name)?;
            let parent = path.parent().ok_or("Missing document folder.")?;
            if let Err(error) = Self::vacant(
                parent,
                path.file_name()
                    .and_then(|p| p.to_str())
                    .ok_or("Invalid document filename.")?,
                None,
            ) {
                if error.contains("already exists") {
                    continue;
                }
                return Err(error);
            }
            use std::io::Write;
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = match options.open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            };
            if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
                drop(file);
                let _ = fs::remove_file(&path);
                return Err(error.to_string());
            }
            return match self.read(&name) {
                Ok(doc) => Ok(doc),
                Err(error) => {
                    let _ = fs::remove_file(path);
                    Err(error)
                }
            };
        }
        Err("Too many documents share the same name.".into())
    }
    pub fn save(&self, name: &str, text: &str, expected: &str) -> Result<String> {
        if text.len() as u64 > MAX_NOTE_BYTES {
            return Err("This note exceeds the 8 MiB editor limit.".into());
        }
        let current = self.read(name)?;
        if current.pdf.is_some() {
            return Err("Save PDF documents through the PDF editor.".into());
        }
        if current.fingerprint != expected {
            return Err(
                "This note changed outside Amigo. Reload it or save your edits as a copy.".into(),
            );
        }
        storage::atomic_write(&self.path(name)?, text.as_bytes())?;
        Ok(hash(text.as_bytes()))
    }
    pub fn create(&self, name: &str, text: &str) -> Result<Document> {
        if is_pdf(name) {
            return Err(
                "Import a PDF or convert a Markdown note. New PDF creation is unavailable.".into(),
            );
        }
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
            pdf: None,
            external: None,
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
        if old.eq_ignore_ascii_case(TRASH_FOLDER) {
            return Err("The default Trash folder cannot be renamed or moved.".into());
        }
        let old_path = self.path(old)?;
        let folder = old_path.is_dir();
        let extension = if is_pdf(old) { "pdf" } else { "md" };
        let name = if folder
            || Path::new(name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(extension))
        {
            name.to_owned()
        } else {
            if Path::new(name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf") || e.eq_ignore_ascii_case("md"))
            {
                return Err("Renaming cannot change a document's file type.".into());
            }
            format!("{name}.{extension}")
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
    pub fn import(&self, sources: &[PathBuf], folder: &str) -> Result<Vec<Document>> {
        let mut documents = Vec::new();
        let result = (|| {
            for source in sources {
                let meta = fs::symlink_metadata(source).map_err(|e| e.to_string())?;
                let is_pdf = source
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
                let is_md = source
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("md"));
                let limit = if is_pdf {
                    pdf::MAX_PDF_BYTES
                } else {
                    MAX_NOTE_BYTES
                };
                if !meta.is_file() || (!is_pdf && !is_md) || meta.len() > limit {
                    return Err(
                        "Import regular UTF-8 .md files up to 8 MiB, or .pdf files up to 64 MiB."
                            .into(),
                    );
                }
                use std::io::Read;
                let mut bytes = Vec::new();
                fs::File::open(source)
                    .map_err(|e| e.to_string())?
                    .take(limit + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() as u64 > limit {
                    return Err("The imported document exceeds its size limit.".into());
                }
                if is_pdf {
                    pdf::validate(&bytes)?;
                } else {
                    std::str::from_utf8(&bytes).map_err(|_| "Notes must use UTF-8 encoding.")?;
                }
                let stem = source
                    .file_stem()
                    .and_then(|p| p.to_str())
                    .ok_or("Invalid document filename.")?;
                documents.push(self.unique_file(
                    folder,
                    stem,
                    if is_pdf { "pdf" } else { "md" },
                    &bytes,
                )?);
            }
            Ok(())
        })();
        if let Err(error) = result {
            for document in documents {
                let _ = fs::remove_file(self.path(&document.path)?);
            }
            return Err(error);
        }
        Ok(documents)
    }
    pub fn search(&self, query: &str) -> Result<Vec<Entry>> {
        let query = query.to_lowercase();
        Ok(self
            .entries()?
            .into_iter()
            .filter(|entry| {
                !entry.folder
                    && !is_in_trash(&entry.path)
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
        let target = if target.to_lowercase().ends_with(".md") || is_pdf(target) {
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
            .filter(|e| {
                !e.folder
                    && !is_in_trash(&e.path)
                    && e.path.rsplit('/').next() == Some(target.as_str())
            })
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
        let bytes = if let Some(pdf) = &doc.pdf {
            pdf.bytes.as_ref()
        } else {
            doc.text.as_bytes()
        };
        storage::atomic_write(destination, bytes)
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
    fn trash_is_a_default_browsable_folder_and_cannot_be_trashed_or_renamed() {
        let (root, journal) = library();
        assert_eq!(
            journal.entries().unwrap(),
            vec![Entry {
                path: TRASH_FOLDER.into(),
                folder: true
            }]
        );
        journal.create_folder("Work").unwrap();
        journal.create("Work/Note", "Recover this 🌍").unwrap();
        journal.trash("Work").unwrap();
        let entries = journal.entries().unwrap();
        let note = entries.iter().find(|entry| !entry.folder).unwrap();
        assert!(is_in_trash(&note.path));
        assert_eq!(entry_name(&note.path), "Note.md");
        assert_eq!(journal.read(&note.path).unwrap().text, "Recover this 🌍");
        assert!(journal.search("Recover").unwrap().is_empty());
        assert!(journal.follow_link("Other.md", "Note").is_err());
        assert!(journal.trash(TRASH_FOLDER).is_err());
        assert!(journal.trash(&note.path).is_err());
        assert!(journal.rename(TRASH_FOLDER, "Elsewhere").is_err());
        assert_eq!(Journal::open(&root).unwrap().entries().unwrap(), entries);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn permanent_deletion_removes_only_trash_items_and_survives_reopening() {
        let (root, journal) = library();
        journal.create("Keep", "Active note").unwrap();
        journal.create("Removed", "Delete this note").unwrap();
        journal.create_folder("Work").unwrap();
        journal.create_folder("Work/Deep").unwrap();
        journal
            .create("Work/Deep/Note", "Delete this folder")
            .unwrap();
        fs::write(root.join("journal/Work/Deep/extra.txt"), "Also remove").unwrap();
        journal.trash("Removed.md").unwrap();
        journal.trash("Work").unwrap();
        let entries = journal.entries().unwrap();
        let note = &entries
            .iter()
            .find(|e| entry_name(&e.path) == "Removed.md")
            .unwrap()
            .path;
        let folder = &entries
            .iter()
            .find(|e| entry_name(&e.path) == "Work")
            .unwrap()
            .path;
        for invalid in [
            "Keep.md",
            "Trash",
            "trash",
            "Trash/../Keep.md",
            "Trash/",
            "/Trash",
        ] {
            assert!(journal.delete_permanently(invalid).is_err(), "{invalid}");
        }
        journal.delete_permanently(note).unwrap();
        assert!(!root.join("journal").join(note).exists());
        assert!(journal.read(&format!("{folder}/Deep/Note.md")).is_ok());
        journal.delete_permanently(folder).unwrap();
        assert!(!root.join("journal").join(folder).exists());
        let reopened = Journal::open(&root).unwrap();
        assert_eq!(reopened.read("Keep.md").unwrap().text, "Active note");
        assert_eq!(reopened.entries().unwrap().len(), 2);
        assert!(root.join("journal/Trash").is_dir());
        assert_eq!(fs::read_dir(root.join("journal/Trash")).unwrap().count(), 0);
        assert!(reopened.delete_permanently(note).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn permanent_deletion_rejects_symlink_paths_and_never_follows_nested_links() {
        let (root, journal) = library();
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("Keep.md"), "Keep outside").unwrap();
        let link = root.join("journal/Trash/Link");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert!(journal.delete_permanently("Trash/Link").is_err());
        assert!(journal.delete_permanently("Trash/Link/Keep.md").is_err());
        journal.create_folder("Trash/Folder").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("journal/Trash/Folder/Nested")).unwrap();
        journal.delete_permanently("Trash/Folder").unwrap();
        assert!(!root.join("journal/Trash/Folder").exists());
        assert_eq!(
            fs::read_to_string(outside.join("Keep.md")).unwrap(),
            "Keep outside"
        );
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_hidden_trash_migrates_without_overwriting_existing_files_and_only_once() {
        let (root, _) = library();
        let visible = root.join("journal/Trash");
        let legacy = root.join("journal/.trash");
        fs::create_dir_all(legacy.join("Old folder")).unwrap();
        fs::write(visible.join("same.md"), "Existing trash").unwrap();
        fs::write(legacy.join("Same.md"), "Old trash 🌍").unwrap();
        fs::write(legacy.join("Old folder/Nested.md"), "Nested note").unwrap();
        let journal = Journal::open(&root).unwrap();
        assert!(!legacy.exists());
        assert_eq!(
            fs::read_to_string(visible.join("same.md")).unwrap(),
            "Existing trash"
        );
        let entries = journal.entries().unwrap();
        let contents: std::collections::BTreeSet<_> = entries
            .iter()
            .filter(|entry| !entry.folder)
            .map(|entry| journal.read(&entry.path).unwrap().text)
            .collect();
        assert_eq!(
            contents,
            [
                "Existing trash".into(),
                "Old trash 🌍".into(),
                "Nested note".into()
            ]
            .into()
        );
        assert!(entries.iter().all(|entry| is_in_trash(&entry.path)));
        assert_eq!(Journal::open(&root).unwrap().entries().unwrap(), entries);
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn trash_migration_rejects_symlink_directories_and_preserves_outside_files() {
        let (root, _) = library();
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("Keep.md"), "Outside").unwrap();
        let trash = root.join("journal/Trash");
        fs::remove_dir(&trash).unwrap();
        std::os::unix::fs::symlink(&outside, &trash).unwrap();
        assert!(Journal::open(&root).is_err());
        fs::remove_file(&trash).unwrap();
        fs::create_dir(&trash).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("journal/.trash")).unwrap();
        assert!(Journal::open(&root).is_err());
        assert_eq!(
            fs::read_to_string(outside.join("Keep.md")).unwrap(),
            "Outside"
        );
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
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
        let entries = j.entries().unwrap();
        assert!(entries.iter().all(|entry| is_in_trash(&entry.path)));
        assert!(
            entries
                .iter()
                .any(|entry| entry.path == TRASH_FOLDER && entry.folder)
        );
        assert!(
            entries
                .iter()
                .any(|entry| !entry.folder && entry_name(&entry.path) == "Note.md")
        );
        assert_eq!(
            j.entries()
                .unwrap()
                .iter()
                .filter(|entry| !entry.folder)
                .count(),
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
        assert_eq!(
            j.entries()
                .unwrap()
                .iter()
                .filter(|entry| !is_in_trash(&entry.path))
                .count(),
            2
        );
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
        assert_eq!(
            j.entries().unwrap(),
            vec![Entry {
                path: TRASH_FOLDER.into(),
                folder: true
            }]
        );
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

#[cfg(test)]
mod preference_tests {
    use super::*;
    #[test]
    fn old_notebook_preferences_get_compatible_editor_defaults() {
        let old: Preferences =
            serde_json::from_str(r#"{"mode":"split","font_size":18.0,"word_wrap":false}"#).unwrap();
        assert!(old.sync_scroll && old.line_numbers);
        assert_eq!(old.preview_layout, PreviewLayout::SideBySide);
        assert_eq!(old.preview_font_size, 16.);
        assert!(!old.word_wrap);
        assert!(!old.preview_editable);
        assert_eq!(
            serde_json::from_str::<Preferences>(&serde_json::to_string(&old).unwrap()).unwrap(),
            old
        );
    }
}

/// Resolve a local document link without letting an external note browse outside its folder.
pub fn external_link(source: &Path, target: &str) -> Result<PathBuf> {
    let decoded = percent_encoding::percent_decode_str(target)
        .decode_utf8()
        .map_err(|_| "Invalid document link.")?;
    let target = decoded.split('#').next().unwrap_or("");
    if target.contains([':', '\\']) || Path::new(target).is_absolute() {
        return Err("Use a relative document link.".into());
    }
    let target = if Path::new(target).extension().is_none() {
        format!("{target}.md")
    } else {
        target.into()
    };
    let parent = source.parent().ok_or("Missing external document folder.")?;
    let path = parent
        .join(target)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !path.starts_with(parent) {
        return Err("The linked document is outside this external folder.".into());
    }
    Ok(path)
}
