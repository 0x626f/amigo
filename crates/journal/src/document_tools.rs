//! Document-wide commands and action-sized history, shared by both editor views.
use super::*;
use std::ops::Range;
type Snapshot = (String, Range<usize>);
#[derive(Default)]
pub(super) struct DocumentTools {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    find: bool,
    focus_find: bool,
    source_focus: bool,
    query: String,
    next: usize,
    pub(super) all: bool,
    pub(super) reveal_source: bool,
    spelling: Option<Spelling>,
}
pub(super) fn record(editor: &mut Editor, before: Snapshot) {
    if before.0 != editor.document.text {
        editor.document_tools.undo.push(before);
        if editor.document_tools.undo.len() > 200 {
            editor.document_tools.undo.remove(0);
        }
        editor.document_tools.redo.clear();
    }
}
fn restore(editor: &mut Editor, snapshot: Snapshot, ui: &egui::Ui, prefs: &Preferences) {
    editor.document.text = snapshot.0;
    editor.selection = snapshot.1;
    editor.revision = editor.revision.wrapping_add(1);
    editor.changed_at = Instant::now();
    select(editor, ui, prefs);
}
fn select(editor: &mut Editor, ui: &egui::Ui, prefs: &Preferences) {
    let id = egui::Id::new(("journal-editor", &editor.document.path));
    if prefs.mode == Mode::Source
        || (prefs.mode == Mode::Split && editor.document_tools.source_focus)
    {
        preview_format::restore(ui, id, Some(editor.selection.clone()));
        editor.document_tools.reveal_source = true;
    } else {
        editor.preview = render::Preview::document(&editor.document.text);
        let point = |n| {
            editor
                .document
                .text
                .char_indices()
                .nth(n)
                .map_or(editor.document.text.len(), |(i, _)| i)
        };
        editor
            .preview
            .focus_source(point(editor.selection.start)..point(editor.selection.end));
        editor.preview_revision = editor.revision;
    }
    ui.ctx().request_repaint();
}
pub(super) fn begin(editor: &mut Editor, prefs: &Preferences, ui: &egui::Ui) {
    if !ui.is_enabled()
        || egui::Popup::is_any_open(ui.ctx())
        || ui.memory(|m| m.top_modal_layer().is_some())
    {
        return;
    }
    let source_id = egui::Id::new(("journal-editor", &editor.document.path));
    if ui.memory(|m| m.has_focus(source_id)) {
        editor.document_tools.source_focus = true;
    } else if editor.preview.has_focus(ui.ctx()) {
        editor.document_tools.source_focus = false;
    }
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::F)) {
        editor.document_tools.find = true;
        editor.document_tools.focus_find = true;
    }
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::A)) {
        editor.selection = 0..editor.document.text.chars().count();
        let source = egui::Id::new(("journal-editor", &editor.document.path));
        editor.document_tools.all =
            prefs.mode != Mode::Source && !ui.memory(|m| m.has_focus(source));
        select(editor, ui, prefs);
    }
    let source = egui::Id::new(("journal-editor", &editor.document.path));
    let focused = ui.memory(|m| m.has_focus(source)) || editor.preview.has_focus(ui.ctx());
    if !focused {
        return;
    }
    let redo = ui.input_mut(|i| {
        i.consume_key(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::Z,
        ) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
    });
    let undo = !redo && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
    if undo || redo {
        let current = (editor.document.text.clone(), editor.selection.clone());
        let next = if redo {
            let next = editor.document_tools.redo.pop();
            if next.is_some() {
                editor.document_tools.undo.push(current);
            }
            next
        } else {
            let next = editor.document_tools.undo.pop();
            if next.is_some() {
                editor.document_tools.redo.push(current);
            }
            next
        };
        if let Some(next) = next {
            restore(editor, next, ui, prefs);
        }
        editor.document_tools.all = false;
    }
    if editor.document_tools.all {
        let copy = ui.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::Copy | egui::Event::Cut))
        });
        if copy {
            ui.ctx().copy_text(editor.document.text.clone());
        }
        let replacement = ui.input_mut(|i| {
            let mut replacement = None;
            i.events.retain(|event| match event {
                egui::Event::Copy => false,
                egui::Event::Cut => {
                    replacement = Some(String::new());
                    false
                }
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    ..
                } => {
                    replacement = Some("\n".into());
                    false
                }
                egui::Event::Text(text) | egui::Event::Paste(text) => {
                    replacement = Some(text.clone());
                    false
                }
                egui::Event::Key {
                    key: egui::Key::Backspace | egui::Key::Delete,
                    pressed: true,
                    ..
                } => {
                    replacement = Some(String::new());
                    false
                }
                _ => true,
            });
            replacement
        });
        if ui.input(|i| i.key_pressed(egui::Key::C) && i.modifiers.command) {
            ui.ctx().copy_text(editor.document.text.clone());
        }
        let can_edit =
            prefs.mode == Mode::Source || editor.preview_editing.unwrap_or(prefs.preview_editable);
        if let Some(text) = replacement.filter(|_| can_edit) {
            let before = (editor.document.text.clone(), editor.selection.clone());
            let end = text.chars().count();
            restore(editor, (text, end..end), ui, prefs);
            record(editor, before);
            editor.document_tools.all = false;
        } else if ui.input(|i| {
            i.pointer.any_pressed()
                || [
                    egui::Key::Escape,
                    egui::Key::ArrowLeft,
                    egui::Key::ArrowRight,
                    egui::Key::ArrowUp,
                    egui::Key::ArrowDown,
                    egui::Key::Home,
                    egui::Key::End,
                ]
                .into_iter()
                .any(|key| i.key_pressed(key))
        }) {
            editor.document_tools.all = false;
        }
    }
}
pub(super) fn bar(editor: &mut Editor, prefs: &Preferences, ui: &mut egui::Ui) -> f32 {
    let top = ui.cursor().top();
    spelling_ui(editor, prefs, ui);
    if editor.document_tools.all {
        ui.label("Entire document selected · type to replace");
    }
    if editor.document_tools.find {
        let mut locate = false;
        ui.horizontal(|ui| {
            ui.label("Find:");
            let enter = ui.memory(|m| m.has_focus(egui::Id::new("journal-find")))
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            let response = ui.add(
                amigo_ui::text_input::TextEdit::singleline(&mut editor.document_tools.query)
                    .id(egui::Id::new("journal-find")),
            );
            if std::mem::take(&mut editor.document_tools.focus_find) {
                response.request_focus();
            }
            if response.changed() {
                editor.document_tools.next = 0;
            }
            locate = ui.button("Next").clicked() || enter;
            if ui.button("Close").clicked() {
                editor.document_tools.find = false;
            }
        });
        if locate && !editor.document_tools.query.is_empty() {
            let query = &editor.document_tools.query;
            let text = &editor.document.text;
            let mut start = editor.document_tools.next.min(text.len());
            while !text.is_char_boundary(start) {
                start -= 1;
            }
            if let Some(pos) = text[start..]
                .find(query)
                .map(|p| start + p)
                .or_else(|| text[..start].find(query))
            {
                let end = pos + query.len();
                editor.document_tools.next = end;
                editor.selection = text[..pos].chars().count()..text[..end].chars().count();
                select(editor, ui, prefs);
            } else {
                ui.label("No matches");
            }
        }
    }
    ui.cursor().top() - top
}

struct Spelling {
    source: String,
    receiver: std::sync::mpsc::Receiver<Result<Vec<(String, Vec<String>)>, String>>,
    result: Option<Result<Vec<(String, Vec<String>)>, String>>,
}
impl Spelling {
    fn start(source: String, ctx: &egui::Context) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        let text = source.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = amigo_ui::spelling::check_in_context(&text, &ctx);
            let _ = sender.send(result);
        });
        Self {
            source,
            receiver,
            result: None,
        }
    }
}

fn spelling_ui(editor: &mut Editor, prefs: &Preferences, ui: &egui::Ui) {
    let Some(mut spelling) = editor.document_tools.spelling.take() else {
        return;
    };
    if spelling.result.is_none() {
        spelling.result = spelling.receiver.try_recv().ok();
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    let mut open = true;
    let mut replacement = None;
    egui::Window::new("Spelling suggestions")
        .open(&mut open)
        .show(ui.ctx(), |ui| {
            if spelling.source != editor.document.text {
                ui.label("Document changed. Run Spelling again to refresh suggestions.");
                return;
            }
            match &spelling.result {
                None => {
                    ui.spinner();
                }
                Some(Err(error)) => {
                    ui.label(error);
                }
                Some(Ok(issues)) => {
                    let accepted = amigo_ui::spelling::personal_words(ui.ctx());
                    let issues = issues
                        .iter()
                        .filter(|(word, _)| !accepted.contains(&word.to_lowercase()))
                        .collect::<Vec<_>>();
                    if issues.is_empty() {
                        ui.label("No spelling suggestions.");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(360.)
                        .show(ui, |ui| {
                            for (word, suggestions) in issues {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(word);
                                    for suggestion in suggestions {
                                        if ui.button(suggestion).clicked() {
                                            replacement = Some((word.clone(), suggestion.clone()));
                                        }
                                    }
                                });
                            }
                        });
                }
            }
        });
    if let Some((word, suggestion)) = replacement {
        if let Some((start, _)) = editor
            .document
            .text
            .match_indices(&word)
            .find(|(start, _)| {
                !editor.document.text[..*start].ends_with(char::is_alphabetic)
                    && !editor.document.text[*start + word.len()..].starts_with(char::is_alphabetic)
            })
        {
            let before = (editor.document.text.clone(), editor.selection.clone());
            let mut text = editor.document.text.clone();
            text.replace_range(start..start + word.len(), &suggestion);
            let end = text[..start + suggestion.len()].chars().count();
            restore(editor, (text, end..end), ui, prefs);
            record(editor, before);
            spelling = Spelling::start(editor.document.text.clone(), ui.ctx());
        }
    }
    if open {
        editor.document_tools.spelling = Some(spelling);
    }
}

pub(super) fn start_spelling(editor: &mut Editor, ctx: &egui::Context) {
    editor.document_tools.spelling = Some(Spelling::start(editor.document.text.clone(), ctx));
}
