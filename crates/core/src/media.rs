//! Portable Markdown media references, resolved strictly inside their owning library.
use crate::model::TaskAttachment;
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image,
    Video,
}

pub fn kind(name: &str) -> Option<Kind> {
    match Path::new(name)
        .extension()?
        .to_str()?
        .to_ascii_lowercase()
        .as_str()
    {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" => Some(Kind::Image),
        "mp4" | "m4v" | "mov" | "webm" | "mkv" | "avi" | "ogv" | "mpeg" | "mpg" => {
            Some(Kind::Video)
        }
        _ => None,
    }
}

pub fn target(file: &TaskAttachment, parent_levels: usize) -> String {
    format!(
        "{}attachments/{}/{}",
        "../".repeat(parent_levels),
        file.id,
        utf8_percent_encode(&file.name, NON_ALPHANUMERIC)
    )
}
pub fn filename_url(name: &str) -> String {
    utf8_percent_encode(name, NON_ALPHANUMERIC).to_string()
}

pub fn embed(label: &str, target: &str) -> String {
    let label = label
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace(['\n', '\r'], " ");
    format!("![{label}]({target})")
}
pub fn remove_embeds(text: &mut String, target: &str) {
    use pulldown_cmark::{Event, Parser, Tag};
    let mut ranges = vec![];
    for (event, span) in Parser::new(text).into_offset_iter() {
        if let Event::Start(Tag::Image { dest_url, .. }) = event
            && dest_url.as_ref() == target
        {
            ranges.push(span);
        }
    }
    for range in ranges.into_iter().rev() {
        text.replace_range(range, "");
    }
}

pub fn resolve(root: &Path, base: &Path, target: &str) -> Result<PathBuf, String> {
    let decoded = percent_decode_str(target)
        .decode_utf8()
        .map_err(|_| "Invalid attachment URL.")?;
    if decoded.contains(['\0', ':', '\\']) || Path::new(decoded.as_ref()).is_absolute() {
        return Err("Media must reference a file inside this library.".into());
    }
    let root = root
        .canonicalize()
        .map_err(|e| format!("Attachment folder is unavailable: {e}"))?;
    let mut path = base.canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(&root) {
        return Err("Attachment location leaves its library.".into());
    }
    // Journal's shared media survives moving a note/folder into or out of Trash.
    let parts: Vec<_> = decoded.split('/').collect();
    let shared = parts
        .windows(2)
        .position(|pair| pair == [".media", "attachments"])
        .filter(|index| {
            parts[..*index]
                .iter()
                .all(|part| matches!(*part, ".." | "." | ""))
        });
    let reference = if let Some(index) = shared {
        path = root.clone();
        parts[index..].join("/")
    } else {
        decoded.into_owned()
    };
    for component in Path::new(&reference).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => (),
            Component::ParentDir if path != root => {
                path.pop();
            }
            _ => return Err("Attachment path leaves its library.".into()),
        }
    }
    let path = path
        .canonicalize()
        .map_err(|e| format!("Attachment is unavailable: {e}"))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err("Attachment path leaves its library.".into());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removing_an_attachment_only_removes_its_embeds() {
        let mut text = "Before ![one](attachments/a.png) and ![two](attachments/a.png).\n![keep](attachments/b.png)\n`![example](attachments/a.png)`".to_owned();
        remove_embeds(&mut text, "attachments/a.png");
        assert_eq!(
            text,
            "Before  and .\n![keep](attachments/b.png)\n`![example](attachments/a.png)`"
        );
    }

    #[test]
    fn journal_attachments_survive_folder_trash_restore_and_setup_backup() {
        use crate::{
            backup::{Archive, Scope},
            journal::Journal,
            library::Library,
        };
        let root = std::env::temp_dir().join(format!("amigo-journal-media-{}", crate::model::id()));
        let library = Library::open(root.clone(), None).unwrap();
        let journal = Journal::open(&root).unwrap();
        let notebook = root.join("journal");
        journal.create_folder("Research").unwrap();
        journal.create_folder("Research/Images").unwrap();
        let source = root.join("my photo.png");
        std::fs::write(&source, b"image bytes").unwrap();
        let file = crate::attachments::import(&journal.media_folder().unwrap(), &source).unwrap();
        let target = format!("../../.media/{}", target(&file, 0));
        let document = journal
            .create("Research/Images/Observation", &embed("Photo", &target))
            .unwrap();
        let expected =
            crate::attachments::resolve(&journal.media_folder().unwrap(), &file).unwrap();
        assert_eq!(
            resolve(&notebook, &notebook.join("Research/Images"), &target).unwrap(),
            expected
        );
        journal.trash("Research").unwrap();
        let trashed = journal
            .entries()
            .unwrap()
            .into_iter()
            .find(|entry| !entry.folder)
            .unwrap();
        let doc = journal.read(&trashed.path).unwrap();
        assert_eq!(doc.text, document.text);
        assert_eq!(
            resolve(
                &notebook,
                notebook.join(&trashed.path).parent().unwrap(),
                &target
            )
            .unwrap(),
            expected
        );
        let restored = journal.restore(&trashed.path).unwrap();
        assert_eq!(
            resolve(
                &notebook,
                notebook.join(restored).parent().unwrap(),
                &target
            )
            .unwrap(),
            expected
        );
        let archive = Archive::capture(&library, Scope::Setup).unwrap();
        assert!(archive.review(&library).is_ok());
        // The media and source Markdown are both included, independent of the current view.
        let serialized = serde_json::to_string(&archive).unwrap();
        assert!(serialized.contains(".media/attachments/"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn references_preserve_unicode_and_spaces_and_reject_library_escape() {
        let root = std::env::temp_dir().join(format!("amigo-media-{}", crate::model::id()));
        std::fs::create_dir_all(root.join("tasks")).unwrap();
        let source = root.join("photo ✓ (1).png");
        std::fs::write(&source, b"image").unwrap();
        let file = crate::attachments::import(&root, &source).unwrap();
        let target = target(&file, 1);
        assert_eq!(
            resolve(&root, &root.join("tasks"), &target).unwrap(),
            crate::attachments::resolve(&root, &file).unwrap()
        );
        for bad in [
            "../../secret",
            "https://example.com/a.png",
            "file:///etc/passwd",
            "%2e%2e/%2e%2e/etc/passwd",
        ] {
            assert!(resolve(&root, &root.join("tasks"), bad).is_err());
        }
        assert_eq!(
            embed("[hello]", &target),
            format!("![\\[hello\\]]({target})")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
