//! App-owned, Unicode personal words, separate from the system language dictionaries.
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
#[derive(Default)]
struct Personal {
    path: PathBuf,
    words: Arc<BTreeSet<String>>,
    loaded: bool,
    revision: u64,
}
fn state(ctx: &egui::Context) -> Arc<Mutex<Personal>> {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_insert_with(egui::Id::new("personal-spelling-dictionary"), || {
            Arc::new(Mutex::new(Personal {
                path: amigo_core::library::default_root().join("dictionary.txt"),
                ..Default::default()
            }))
        })
        .clone()
    })
}
fn normalized(word: &str) -> &str {
    word.trim().trim_matches(|c: char| !c.is_alphabetic())
}
fn valid(word: &str) -> bool {
    !word.is_empty()
        && word.chars().count() <= 64
        && word.chars().any(char::is_alphabetic)
        && word
            .chars()
            .all(|c| c.is_alphabetic() || matches!(c, '\'' | '’' | '-'))
}
fn read(path: &Path) -> Result<BTreeSet<String>, String> {
    let contents = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => return Err(format!("Cannot read personal dictionary: {error}")),
    };
    let mut words = BTreeSet::new();
    for (index, line) in contents.lines().enumerate() {
        let word = normalized(line);
        if word.is_empty() {
            continue;
        }
        if !valid(word) {
            return Err(format!(
                "Invalid personal dictionary word on line {}",
                index + 1
            ));
        }
        words.insert(word.to_lowercase());
    }
    Ok(words)
}
pub fn set_personal_dictionary(ctx: &egui::Context, path: PathBuf) {
    let state = state(ctx);
    let mut state = state.lock().unwrap();
    if state.path == path {
        return;
    }
    state.path = path;
    state.words = Arc::default();
    state.loaded = false;
    state.revision = state.revision.wrapping_add(1);
    drop(state);
    crate::text_input::reset_spelling_cache(ctx);
}
pub fn personal_revision(ctx: &egui::Context) -> u64 {
    state(ctx).lock().unwrap().revision
}
pub fn personal_words(ctx: &egui::Context) -> Arc<BTreeSet<String>> {
    let state = state(ctx);
    let mut state = state.lock().unwrap();
    if !state.loaded {
        if let Ok(words) = read(&state.path) {
            state.words = Arc::new(words);
            state.loaded = true;
        }
    }
    state.words.clone()
}
pub fn is_personal_word(ctx: &egui::Context, word: &str) -> bool {
    personal_words(ctx).contains(&normalized(word).to_lowercase())
}
pub fn add_to_dictionary(ctx: &egui::Context, word: &str) -> Result<(), String> {
    let word = normalized(word);
    if !valid(word) {
        return Err("Select one word to add to the dictionary.".into());
    }
    let state = state(ctx);
    let mut state = state.lock().unwrap();
    // Re-read before saving so previously saved or externally added words are preserved.
    let mut words = read(&state.path)?;
    words.insert(word.to_lowercase());
    let text = words.iter().cloned().collect::<Vec<_>>().join("\n") + "\n";
    amigo_core::storage::atomic_write(&state.path, text.as_bytes())?;
    state.words = Arc::new(words);
    state.loaded = true;
    state.revision = state.revision.wrapping_add(1);
    drop(state);
    crate::text_input::accept_spelling(ctx, word);
    ctx.request_repaint();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn personal_words_persist_across_contexts_and_preserve_unicode() {
        let root =
            std::env::temp_dir().join(format!("amigo-dictionary-{}", amigo_core::model::id()));
        let path = root.join("dictionary.txt");
        let first = egui::Context::default();
        set_personal_dictionary(&first, path.clone());
        for word in ["Squidy", "Españolísimo", "Сквидий"] {
            add_to_dictionary(&first, word).unwrap();
        }
        let second = egui::Context::default();
        set_personal_dictionary(&second, path.clone());
        assert!(is_personal_word(&second, "SQUIDY"));
        assert!(is_personal_word(&second, "españolísimo"));
        assert!(is_personal_word(&second, "СКВИДИЙ"));
        assert!(add_to_dictionary(&second, "two words").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 3);
        std::fs::remove_dir_all(root).unwrap();
    }
}
