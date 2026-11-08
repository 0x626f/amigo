//! Text fields with automatic spelling checks, corrections, and emoji insertion.
mod automatic;
use crate::design;
use egui::{self, Id, Ui};
use std::{
    ops::Range,
    sync::{Arc, Mutex},
};

pub struct TextEdit<'a> {
    inner: egui::TextEdit<'a>,
    id: Option<Id>,
    salt: Option<Id>,
    menu: bool,
    password: bool,
    editable: bool,
    spelling: bool,
    spelling_on_focus: bool,
}
macro_rules! forward {
    ($($name:ident($arg:ident: $ty:ty);)*) => {$(
        pub fn $name(mut self, $arg:$ty)->Self { self.inner=self.inner.$name($arg); self }
    )*};
}
impl<'a> TextEdit<'a> {
    pub fn singleline(text: &'a mut dyn egui::TextBuffer) -> Self {
        let editable = text.is_mutable();
        Self {
            inner: egui::TextEdit::singleline(text),
            id: None,
            salt: None,
            menu: true,
            password: false,
            editable,
            spelling: true,
            spelling_on_focus: false,
        }
    }
    pub fn multiline(text: &'a mut dyn egui::TextBuffer) -> Self {
        let editable = text.is_mutable();
        Self {
            inner: egui::TextEdit::multiline(text),
            id: None,
            salt: None,
            menu: true,
            password: false,
            editable,
            spelling: true,
            spelling_on_focus: false,
        }
    }
    forward! {
        desired_width(width:f32); desired_rows(rows:usize); char_limit(limit:usize);
        prefix(prefix:impl egui::IntoAtoms<'static>);
        suffix(suffix:impl egui::IntoAtoms<'static>);
        font(font:impl Into<egui::FontSelection>); hint_text(hint:impl egui::IntoAtoms<'static>);
        frame(frame:egui::Frame); margin(margin:impl Into<egui::Margin>);
        min_size(size:egui::Vec2); horizontal_align(align:egui::Align); vertical_align(align:egui::Align);
        lock_focus(lock:bool); cursor_at_end(end:bool); clip_text(clip:bool);
        text_color(color:egui::Color32); return_key(key:impl Into<Option<egui::KeyboardShortcut>>);
    }
    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }
    pub fn id_salt(mut self, salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.salt = Some(Id::new(salt));
        self
    }
    pub fn custom_menu(mut self) -> Self {
        self.menu = false;
        self
    }
    pub fn password(mut self, password: bool) -> Self {
        self.password = password;
        self.inner = self.inner.password(password);
        self
    }
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.editable &= interactive;
        self.inner = self.inner.interactive(interactive);
        self
    }
    pub fn spellcheck(mut self, enabled: bool) -> Self {
        self.spelling = enabled;
        self
    }
    /// Restrict automatic spelling marks and checks to the active editor.
    pub fn spellcheck_on_focus(mut self, enabled: bool) -> Self {
        self.spelling_on_focus = enabled;
        self
    }
    pub fn code_editor(mut self) -> Self {
        self.inner = self.inner.code_editor();
        self
    }
    pub fn layouter(
        mut self,
        layouter: &'a mut dyn FnMut(&Ui, &dyn egui::TextBuffer, f32) -> Arc<egui::Galley>,
    ) -> Self {
        self.inner = self.inner.layouter(layouter);
        self
    }
    pub fn show(self, ui: &mut Ui) -> egui::text_edit::TextEditOutput {
        let id = if let Some(id) = self.id {
            id
        } else if let Some(salt) = self.salt {
            ui.make_persistent_id(salt)
        } else {
            let id = ui.next_auto_id();
            ui.skip_ahead_auto_ids(1);
            id
        };
        if !self.password && self.editable {
            let secondary = ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary));
            if secondary
                && ui.ctx().read_response(id).is_some_and(|r| {
                    ui.input(|i| i.pointer.interact_pos().is_some_and(|p| r.rect.contains(p)))
                })
            {
                if let Some(range) =
                    egui::TextEdit::load_state(ui.ctx(), id).and_then(|s| s.cursor.char_range())
                {
                    let range = range.as_sorted_char_range();
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(id.with("text-menu-selection"), range.start.0..range.end.0)
                    });
                }
            } else if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
                && !egui::Popup::is_any_open(ui.ctx())
            {
                ui.ctx()
                    .data_mut(|d| d.remove::<Range<usize>>(id.with("text-menu-selection")));
            }
        }
        let pending = ui
            .ctx()
            .data_mut(|d| d.remove_temp::<Insertion>(id.with("text-insertion")));
        let mut injected = None;
        if let Some(pending) =
            pending.filter(|_| self.editable && ui.is_enabled() && !self.password)
        {
            let unchanged = ui
                .ctx()
                .data(|d| d.get_temp::<String>(id.with("text-content")))
                .as_ref()
                == Some(&pending.source);
            if unchanged {
                let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(pending.range.start),
                        egui::text::CCursor::new(pending.range.end),
                    )));
                state.store(ui.ctx(), id);
                ui.memory_mut(|m| m.request_focus(id));
                injected = Some(pending.text.clone());
                ui.input_mut(|i| i.events.push(egui::Event::Text(pending.text)));
            }
        }
        let mut output = self.inner.id(id).show(ui);
        if output.response.contains_pointer()
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary))
        {
            let pinned = ui
                .ctx()
                .data(|d| d.get_temp::<Range<usize>>(id.with("text-menu-selection")));
            let automatic = pinned
                .as_ref()
                .filter(|r| r.is_empty())
                .and_then(|_| ui.input(|i| i.pointer.interact_pos()))
                .and_then(|pos| {
                    automatic::word_at(
                        output.galley.text(),
                        output
                            .galley
                            .cursor_from_pos(pos - automatic::text_origin(&output))
                            .index
                            .0,
                    )
                })
                .map(|word| word.range);
            if let Some(range) = automatic.or(pinned) {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(id.with("text-menu-selection"), range.clone()));
                let range = egui::text::CCursorRange::two(
                    egui::text::CCursor::new(range.start),
                    egui::text::CCursor::new(range.end),
                );
                output.state.cursor.set_char_range(Some(range));
                output.state.clone().store(ui.ctx(), id);
                output.cursor_range = Some(range);
            }
        }
        if let Some(inserted) = injected {
            ui.input_mut(|i| {
                if let Some(index) = i
                    .events
                    .iter()
                    .rposition(|e| matches!(e,egui::Event::Text(text) if text==&inserted))
                {
                    i.events.remove(index);
                }
            });
        }
        if !self.password && self.editable && ui.is_enabled() {
            ui.ctx().data_mut(|d| {
                d.insert_temp(id.with("text-content"), output.galley.text().to_owned())
            });
            if self.spelling && (!self.spelling_on_focus || output.response.has_focus()) {
                automatic::show(ui, &output);
            }
            if self.menu {
                design::context_menu(&output.response).show(|ui| {
                    menu(ui, &output);
                });
            }
        }
        output
    }
}
impl egui::Widget for TextEdit<'_> {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        self.show(ui).response.response
    }
}
pub(crate) fn reset_spelling_cache(ctx: &egui::Context) {
    automatic::reset(ctx);
}
pub(crate) fn accept_spelling(ctx: &egui::Context, word: &str) {
    automatic::accept(ctx, word);
}

#[derive(Clone, Default)]
struct Insertion {
    source: String,
    range: Range<usize>,
    text: String,
}
type WordCheckResult = Arc<Mutex<Option<Result<Vec<String>, String>>>>;

#[derive(Clone)]
struct WordCheck {
    word: String,
    revision: u64,
    result: WordCheckResult,
}
fn queue(ui: &mut Ui, output: &egui::text_edit::TextEditOutput, range: Range<usize>, text: String) {
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            output.response.id.with("text-insertion"),
            Insertion {
                source: output.galley.text().into(),
                range,
                text,
            },
        )
    });
    ui.ctx().request_repaint();
    ui.close();
}
/// Add these actions to a field's existing context menu (e.g. journal formatting).
pub fn menu(ui: &mut Ui, output: &egui::text_edit::TextEditOutput) {
    let id = output.response.id;
    // Use the stored selection: popup focus must not collapse the selected word.
    let current_range = egui::TextEdit::load_state(ui.ctx(), id)
        .and_then(|s| s.cursor.char_range())
        .or(output.cursor_range)
        .map(|r| {
            let r = r.as_sorted_char_range();
            r.start.0..r.end.0
        })
        .unwrap_or(0..0);
    let range = ui
        .ctx()
        .data(|d| d.get_temp::<Range<usize>>(id.with("text-menu-selection")))
        .unwrap_or(current_range);
    ui.menu_button("Insert emoji", |ui| {
        egui::Grid::new(id.with("emoji-grid"))
            .num_columns(8)
            .show(ui, |ui| {
                for (i, (emoji, name)) in EMOJI.iter().enumerate() {
                    if ui.button(*emoji).on_hover_text(*name).clicked() {
                        queue(ui, output, range.clone(), emoji.to_string());
                    }
                    if i % 8 == 7 {
                        ui.end_row();
                    }
                }
            });
    });
    let selected = output
        .galley
        .text()
        .chars()
        .skip(range.start)
        .take(range.len())
        .collect::<String>();
    let word = selected.trim();
    let valid = !word.is_empty()
        && word.chars().any(char::is_alphabetic)
        && word
            .chars()
            .all(|c| c.is_alphabetic() || matches!(c, '\'' | '’' | '-'));
    ui.add_enabled_ui(valid, |ui| {
        ui.menu_button("Dictionary", |ui| {
            let accepted = crate::spelling::is_personal_word(ui.ctx(), word);
            let error_key = id.with("dictionary-error");
            if ui
                .add_enabled(!accepted, design::Button::new("Add to dictionary"))
                .clicked()
            {
                match crate::spelling::add_to_dictionary(ui.ctx(), word) {
                    Ok(()) => {
                        ui.ctx().data_mut(|d| d.remove::<String>(error_key));
                        ui.close();
                    }
                    Err(error) => {
                        ui.ctx().data_mut(|d| d.insert_temp(error_key, error));
                    }
                }
            }
            if let Some(error) = ui.ctx().data(|d| d.get_temp::<String>(error_key)) {
                ui.colored_label(design::colors(ui).red, error);
            }
            ui.add_enabled_ui(!accepted, |ui| {
                ui.menu_button("Fix to", |ui| {
                    let key = id.with("spelling-check");
                    let cached = automatic::suggestions(ui.ctx(), word);
                    let revision = crate::spelling::personal_revision(ui.ctx());
                    let check = ui.ctx().data_mut(|d| {
                        if let Some(check) = d
                            .get_temp::<WordCheck>(key)
                            .filter(|c| c.word == word && c.revision == revision)
                        {
                            return check;
                        }
                        let result = Arc::new(Mutex::new(cached));
                        let out = result.clone();
                        let value = word.to_owned();
                        let ctx = ui.ctx().clone();
                        if out.lock().unwrap().is_none() {
                            std::thread::spawn(move || {
                                let suggestions = crate::spelling::check_in_context(&value, &ctx)
                                    .map(|issues| {
                                        issues
                                            .into_iter()
                                            .find(|(w, _)| w == &value)
                                            .map(|(_, s)| s)
                                            .unwrap_or_default()
                                    });
                                *out.lock().unwrap() = Some(suggestions);
                                ctx.request_repaint();
                            });
                        }
                        let check = WordCheck {
                            word: word.into(),
                            revision,
                            result,
                        };
                        d.insert_temp(key, check.clone());
                        check
                    });
                    let result = check.result.lock().unwrap().clone();
                    match result {
                        None => {
                            ui.spinner();
                            ui.label("Checking spelling…");
                        }
                        Some(Err(error)) => {
                            ui.label(error);
                        }
                        Some(Ok(suggestions)) => {
                            if suggestions.is_empty() {
                                ui.label("No spelling suggestions.");
                            }
                            for suggestion in suggestions {
                                if ui.button(&suggestion).clicked() {
                                    queue(
                                        ui,
                                        output,
                                        range.clone(),
                                        selected.replacen(word, &suggestion, 1),
                                    );
                                }
                            }
                        }
                    }
                });
            });
        })
        .response
        .on_hover_text("Add this word to your dictionary or choose a correction");
    });
}

const EMOJI: &[(&str, &str)] = &[
    ("😀", "Smile"),
    ("😃", "Happy"),
    ("😄", "Laugh"),
    ("😁", "Grin"),
    ("😅", "Relief"),
    ("😂", "Joy"),
    ("🙂", "Slight smile"),
    ("😉", "Wink"),
    ("😊", "Blush"),
    ("😍", "Love"),
    ("🥰", "Affection"),
    ("😎", "Cool"),
    ("🤔", "Thinking"),
    ("😢", "Sad"),
    ("😭", "Crying"),
    ("😡", "Angry"),
    ("👍", "Thumbs up"),
    ("👎", "Thumbs down"),
    ("👏", "Applause"),
    ("🙌", "Celebrate"),
    ("🙏", "Thanks"),
    ("👋", "Hello"),
    ("💪", "Strength"),
    ("🤝", "Agreement"),
    ("❤️", "Heart"),
    ("💙", "Blue heart"),
    ("💚", "Green heart"),
    ("⭐", "Star"),
    ("🌟", "Shining star"),
    ("🔥", "Fire"),
    ("✨", "Sparkles"),
    ("🎉", "Party"),
    ("✅", "Done"),
    ("❌", "No"),
    ("⚠️", "Warning"),
    ("❓", "Question"),
    ("💡", "Idea"),
    ("📌", "Pin"),
    ("📅", "Calendar"),
    ("📝", "Note"),
    ("🚀", "Launch"),
    ("🎯", "Target"),
    ("🏆", "Achievement"),
    ("📚", "Books"),
    ("💻", "Computer"),
    ("☕", "Coffee"),
    ("🌍", "World"),
    ("🐙", "Octopus"),
];

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(ctx: &egui::Context, text: &mut String, events: Vec<egui::Event>) -> egui::FullOutput {
        frame_at(ctx, text, events, None)
    }
    fn frame_at(
        ctx: &egui::Context,
        text: &mut String,
        events: Vec<egui::Event>,
        time: Option<f64>,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(980., 640.),
                )),
                events,
                time,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.add(
                        TextEdit::multiline(text)
                            .id(Id::new("test-field"))
                            .desired_width(450.),
                    );
                });
            },
        );
        output.textures_delta.clear();
        output
    }
    fn label(output: &egui::FullOutput, value: &str) -> egui::Rect {
        fn find(shape: &egui::Shape, value: &str) -> Option<egui::Rect> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == value => {
                    Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, value)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .rev()
            .find_map(|s| find(&s.shape, value))
            .unwrap_or_else(|| panic!("Missing {value}"))
    }
    fn click(
        ctx: &egui::Context,
        text: &mut String,
        pos: egui::Pos2,
        button: egui::PointerButton,
    ) -> egui::FullOutput {
        let mut output = frame(ctx, text, vec![]);
        for pressed in [true, false] {
            output = frame(
                ctx,
                text,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        for _ in 0..3 {
            output = frame(ctx, text, vec![]);
        }
        output
    }
    fn open(ctx: &egui::Context, text: &mut String, range: Range<usize>) -> egui::FullOutput {
        design::install(ctx);
        for _ in 0..3 {
            frame(ctx, text, vec![]);
        }
        let id = Id::new("test-field");
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(range.start),
                egui::text::CCursor::new(range.end),
            )));
        state.store(ctx, id);
        ctx.memory_mut(|m| m.request_focus(id));
        frame(ctx, text, vec![]);
        let rect = ctx.read_response(id).unwrap().rect;
        click(ctx, text, rect.center(), egui::PointerButton::Secondary)
    }
    #[test]
    fn emoji_context_menu_inserts_at_unicode_selection_and_undo_restores_text() {
        let ctx = egui::Context::default();
        let mut text = "Hi 🌍 world".to_owned();
        let output = open(&ctx, &mut text, 5..10);
        let output = click(
            &ctx,
            &mut text,
            label(&output, "Insert emoji").center(),
            egui::PointerButton::Primary,
        );
        click(
            &ctx,
            &mut text,
            label(&output, "😀").center(),
            egui::PointerButton::Primary,
        );
        assert_eq!(text, "Hi 🌍 😀");
        let modifiers = egui::Modifiers::COMMAND;
        frame(
            &ctx,
            &mut text,
            vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::Key {
                    key: egui::Key::Z,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
            ],
        );
        assert_eq!(text, "Hi 🌍 world");
    }
    #[test]
    fn selected_word_spelling_menu_replaces_only_that_occurrence() {
        let ctx = egui::Context::default();
        let mut text = "teh 🌍 teh".to_owned();
        let output = open(&ctx, &mut text, 6..9);
        let revision = crate::spelling::personal_revision(&ctx);
        ctx.data_mut(|d| {
            d.insert_temp(
                Id::new("test-field").with("spelling-check"),
                WordCheck {
                    word: "teh".into(),
                    revision,
                    result: Arc::new(Mutex::new(Some(Ok(vec!["the".into()])))),
                },
            )
        });
        let output = click(
            &ctx,
            &mut text,
            label(&output, "Dictionary").center(),
            egui::PointerButton::Primary,
        );
        let output = click(
            &ctx,
            &mut text,
            label(&output, "Fix to").center(),
            egui::PointerButton::Primary,
        );
        click(
            &ctx,
            &mut text,
            label(&output, "the").center(),
            egui::PointerButton::Primary,
        );
        assert_eq!(text, "teh 🌍 the");
    }
    #[test]
    fn automatically_underlines_and_corrects_right_clicked_word_without_selection() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        automatic::seed(&ctx, "teh", vec!["the".into()]);
        let mut text = "🌍 teh again".to_owned();
        let output = frame(&ctx, &mut text, vec![]);
        let red = design::palette(&ctx).red;
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,egui::Shape::Path(path) if path.stroke.color==egui::epaint::ColorMode::Solid(red))));
        let id = Id::new("test-field");
        ctx.memory_mut(|m| m.request_focus(id));
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(0),
            )));
        state.store(&ctx, id);
        let text_shape = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == text => Some(t),
                _ => None,
            })
            .unwrap();
        let caret = text_shape
            .galley
            .pos_from_cursor(egui::text::CCursor::new(3));
        let pos = text_shape.pos + caret.center().to_vec2();
        let output = click(&ctx, &mut text, pos, egui::PointerButton::Secondary);
        let output = click(
            &ctx,
            &mut text,
            label(&output, "Dictionary").center(),
            egui::PointerButton::Primary,
        );
        let output = click(
            &ctx,
            &mut text,
            label(&output, "Fix to").center(),
            egui::PointerButton::Primary,
        );
        click(
            &ctx,
            &mut text,
            label(&output, "the").center(),
            egui::PointerButton::Primary,
        );
        assert_eq!(text, "🌍 the again");
        let output = frame(&ctx, &mut text, vec![]);
        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,egui::Shape::Path(path) if path.stroke.color==egui::epaint::ColorMode::Solid(red))));
    }
    #[test]
    fn focused_only_spelling_tracks_aligned_words_and_stops_after_blur() {
        for align in [egui::Align::Min, egui::Align::Center, egui::Align::Max] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            automatic::seed(&ctx, "teh", vec!["the".into()]);
            let id = Id::new("aligned-spelling-test");
            let mut text = "hello teh world".to_owned();
            let mut frame = |events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            ui.add(
                                TextEdit::singleline(&mut text)
                                    .id(id)
                                    .desired_width(300.0)
                                    .horizontal_align(align)
                                    .suffix(egui::Atom::custom(
                                        id.with("arrow"),
                                        egui::vec2(20.0, 20.0),
                                    ))
                                    .spellcheck_on_focus(true),
                            );
                        });
                    },
                );
                output.textures_delta.clear();
                output
            };
            let red = design::palette(&ctx).red;
            let underlines = |output: &egui::FullOutput| {
                output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Path(path)
                            if path.stroke.color == egui::epaint::ColorMode::Solid(red) =>
                        {
                            Some(path.points.clone())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            assert!(underlines(&frame(vec![])).is_empty());
            ctx.memory_mut(|memory| memory.request_focus(id));
            let output = frame(vec![]);
            let lines = underlines(&output);
            assert_eq!(lines.len(), 1);
            let shape = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "hello teh world" => {
                        Some(text)
                    }
                    _ => None,
                })
                .unwrap();
            let row = &shape.galley.rows[0];
            let from = shape.pos.x + row.pos.x + row.glyphs[6].pos.x;
            let through =
                shape.pos.x + row.pos.x + row.glyphs[8].pos.x + row.glyphs[8].advance_width;
            assert!(
                (lines[0].first().unwrap().x - from).abs() < 0.5,
                "{align:?}"
            );
            assert!(
                (lines[0].last().unwrap().x - through).abs() < 0.5,
                "{align:?}"
            );
            let pos = egui::pos2(
                (from + through) * 0.5,
                shape.pos.y + row.pos.y + row.size.y * 0.5,
            );
            for pressed in [true, false] {
                frame(vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
            let range = egui::TextEdit::load_state(&ctx, id)
                .unwrap()
                .cursor
                .char_range()
                .unwrap()
                .as_sorted_char_range();
            assert_eq!(range.start.0..range.end.0, 6..9);
            ctx.memory_mut(|memory| memory.surrender_focus(id));
            assert!(underlines(&frame(vec![])).is_empty());
        }
    }

    #[test]
    fn automatic_spelling_skips_password_and_readonly_fields() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        automatic::seed(&ctx, "teh", vec!["the".into()]);
        let mut text = "teh".to_owned();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.add(
                TextEdit::singleline(&mut text)
                    .id(Id::new("password-test"))
                    .password(true),
            );
            let mut readonly = "teh";
            ui.add(TextEdit::singleline(&mut readonly).id(Id::new("readonly-test")));
        });
        output.textures_delta.clear();
        for id in [Id::new("password-test"), Id::new("readonly-test")] {
            assert!(
                ctx.data(|d| d.get_temp::<String>(id.with("text-content")))
                    .is_none()
            );
        }
    }
    #[test]
    fn typing_automatically_checks_after_pause_and_shows_a_correction_hint() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        fn check(input: &str) -> Result<Vec<(String, Vec<String>)>, String> {
            CALLS.fetch_add(1, Ordering::SeqCst);
            Ok(input
                .lines()
                .filter(|word| *word == "teh")
                .map(|word| (word.into(), vec!["the".into()]))
                .collect())
        }
        let ctx = egui::Context::default();
        design::install(&ctx);
        automatic::checker_for_test(&ctx, check);
        let mut text = "te".to_owned();
        frame_at(&ctx, &mut text, vec![], Some(0.));
        let id = Id::new("test-field");
        ctx.memory_mut(|m| m.request_focus(id));
        frame_at(
            &ctx,
            &mut text,
            vec![egui::Event::Text("h".into())],
            Some(0.2),
        );
        assert_eq!(text, "teh");
        frame_at(&ctx, &mut text, vec![], Some(0.4));
        assert_eq!(CALLS.load(Ordering::SeqCst), 0);
        frame_at(&ctx, &mut text, vec![], Some(0.6));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while automatic::suggestions(&ctx, "teh").is_none() && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            automatic::suggestions(&ctx, "teh"),
            Some(Ok(vec!["the".into()]))
        );
        frame_at(&ctx, &mut text, vec![], Some(0.7));
        let output = frame_at(&ctx, &mut text, vec![], Some(0.8));
        assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text().contains("teh → the"))));
        assert!(!egui::Popup::is_any_open(&ctx));
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(3),
            )));
        state.store(&ctx, id);
        let output = frame_at(
            &ctx,
            &mut text,
            vec![egui::Event::Text("the".into())],
            Some(0.9),
        );
        assert_eq!(text, "the");
        let red = design::palette(&ctx).red;
        assert!(!output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Path(path) if path.stroke.color==egui::epaint::ColorMode::Solid(red))));
    }
    #[test]
    fn dictionary_menu_saves_words_and_immediately_removes_underlines() {
        let root =
            std::env::temp_dir().join(format!("amigo-dictionary-menu-{}", amigo_core::model::id()));
        let path = root.join("dictionary.txt");
        let ctx = egui::Context::default();
        design::install(&ctx);
        crate::spelling::set_personal_dictionary(&ctx, path.clone());
        automatic::seed(&ctx, "Squidyyy", vec!["Squidy".into()]);
        let mut text = "Squidyyy".to_owned();
        let output = open(&ctx, &mut text, 0..8);
        let output = click(
            &ctx,
            &mut text,
            label(&output, "Dictionary").center(),
            egui::PointerButton::Primary,
        );
        click(
            &ctx,
            &mut text,
            label(&output, "Add to dictionary").center(),
            egui::PointerButton::Primary,
        );
        assert_eq!(text, "Squidyyy");
        assert!(crate::spelling::is_personal_word(&ctx, "Squidyyy"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "squidyyy\n");
        let output = frame(&ctx, &mut text, vec![]);
        let red = design::palette(&ctx).red;
        assert!(!output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Path(path) if path.stroke.color==egui::epaint::ColorMode::Solid(red))));
        // A new context reloads the saved dictionary, even when a stale result arrives.
        let again = egui::Context::default();
        design::install(&again);
        crate::spelling::set_personal_dictionary(&again, path);
        automatic::seed(&again, "Squidyyy", vec!["Squidy".into()]);
        let output = frame(&again, &mut text, vec![]);
        let red = design::palette(&again).red;
        assert!(!output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Path(path) if path.stroke.color==egui::epaint::ColorMode::Solid(red))));
        std::fs::remove_dir_all(root).unwrap();
    }
}
