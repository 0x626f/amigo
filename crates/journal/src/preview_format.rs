//! Preview selection mapping and formatting share the source editor's Markdown transforms.
use super::*;
use pulldown_cmark::{Event, Options, Parser};
use std::ops::Range;

pub(super) struct Input {
    pub output: egui::text_edit::TextEditOutput,
    pub span: Range<usize>,
    pub text: String,
    pub whole_lines: bool,
    pub inline_only: bool,
    pub accepts_enter: bool,
    pub list: bool,
}
#[derive(Clone)]
struct Context {
    id: egui::Id,
    local: Range<usize>,
    revision: u64,
    document: String,
    fragment: Range<usize>,
    selection: Range<usize>,
    state: formatting::SelectionState,
    inline_only: bool,
    accepts_enter: bool,
    rect: egui::Rect,
}
struct LinkDraft {
    document: String,
    target: Range<usize>,
    restore: Range<usize>,
    url: String,
    focus: bool,
}
#[derive(Default)]
pub(super) struct Tools {
    context: Option<Context>,
    menus: Vec<egui::Id>,
    pinned: Option<(egui::Id, Range<usize>)>,
    link: Option<LinkDraft>,
}
impl Tools {
    pub fn begin(&mut self, ctx: &egui::Context) {
        if let Some(c) = &self.context
            && !c.local.is_empty()
            && ctx.input(|i| {
                i.pointer.button_pressed(egui::PointerButton::Secondary)
                    && i.pointer.interact_pos().is_some_and(|p| c.rect.contains(p))
            })
        {
            self.pinned = Some((c.id, c.local.clone()));
        }
    }
    pub fn bar(&mut self, ui: &mut egui::Ui, width: f32) -> Option<Format> {
        let state = self
            .context
            .as_ref()
            .map(|c| c.state.clone())
            .unwrap_or_default();
        let inline_only = self.context.as_ref().is_some_and(|c| c.inline_only);
        let mut action = None;
        ui.add_space(6.);
        egui::ScrollArea::horizontal()
            .id_salt("preview-format-controls")
            .max_width(width)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(2., 0.);
                ui.horizontal(|ui| {
                    action = toolbar::controls(ui, &state, &mut self.menus, inline_only, true);
                });
            });
        ui.add_space(6.);
        action
    }
    pub fn update(
        &mut self,
        preview: &mut render::Preview,
        revision: u64,
        ui: &mut egui::Ui,
    ) -> Option<Format> {
        if self.link.is_some() {
            return None;
        }
        let index = preview
            .inputs
            .iter()
            .position(|i| i.output.response.has_focus())
            .or_else(|| {
                self.context.as_ref().and_then(|c| {
                    preview
                        .inputs
                        .iter()
                        .position(|i| i.output.response.id == c.id)
                })
            })
            .or_else(|| preview.inputs.len().checked_sub(1))?;
        let input = &preview.inputs[index];
        let id = input.output.response.id;
        let range = self
            .pinned
            .as_ref()
            .filter(|(owner, _)| *owner == id)
            .map(|(_, r)| r.clone())
            .or_else(|| {
                input.output.cursor_range.map(|r| {
                    let r = r.as_sorted_char_range();
                    r.start.0..r.end.0
                })
            })
            .unwrap_or_else(|| {
                self.context
                    .as_ref()
                    .filter(|c| c.id == id)
                    .map_or(0..0, |c| c.local.clone())
            });
        if self
            .context
            .as_ref()
            .is_none_or(|c| c.id != id || c.local != range || c.revision != revision)
        {
            let (document, fragment) = preview.project_input(input);
            if let Some(selection) =
                source_range(&document[fragment.clone()], &input.text, range.clone())
            {
                let text = &document[fragment.clone()];
                let selection =
                    text[..selection.start].chars().count()..text[..selection.end].chars().count();
                self.context = Some(Context {
                    id,
                    local: range.clone(),
                    revision,
                    state: formatting::SelectionState::new(text, selection.clone()),
                    document,
                    fragment,
                    selection,
                    inline_only: input.inline_only,
                    accepts_enter: input.accepts_enter,
                    rect: input.output.response.rect,
                });
            } else {
                self.context = None;
            }
        }
        let c = self.context.as_mut()?;
        c.rect = input.output.response.rect;
        if self.pinned.is_some() {
            set_cursor(ui.ctx(), id, range);
        }
        let mut action = None;
        input.output.response.context_menu(|ui| {
            action = toolbar::format_menu(ui, &c.state, c.inline_only);
            ui.separator();
            if ui
                .add_enabled(!c.local.is_empty(), design::Button::new("Copy"))
                .clicked()
            {
                ui.ctx().copy_text(
                    input
                        .text
                        .chars()
                        .skip(c.local.start)
                        .take(c.local.len())
                        .collect(),
                );
                ui.close();
            }
        });
        if !input.output.response.context_menu_opened()
            && !ui.input(|i| i.pointer.button_down(egui::PointerButton::Secondary))
        {
            self.pinned = None;
        }
        action
    }
    pub fn shortcut(&self, ctx: &egui::Context) -> Option<Format> {
        if ctx.memory(|m| m.top_modal_layer().is_some()) || egui::Popup::is_any_open(ctx) {
            return None;
        }
        let shifted = egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        };
        for (key, modifiers, kind) in [
            (egui::Key::B, egui::Modifiers::COMMAND, Format::Bold),
            (egui::Key::I, egui::Modifiers::COMMAND, Format::Italic),
            (egui::Key::S, shifted, Format::Strike),
            (egui::Key::C, shifted, Format::Code),
            (egui::Key::U, shifted, Format::Link),
        ] {
            if ctx.input_mut(|i| i.consume_key(modifiers, key)) {
                return Some(kind);
            }
        }
        None
    }
}
pub(super) fn restore(ui: &egui::Ui, id: egui::Id, range: Option<Range<usize>>) {
    if let Some(range) = range {
        set_cursor(ui.ctx(), id, range);
        ui.memory_mut(|m| m.request_focus(id));
    }
}
fn set_cursor(ctx: &egui::Context, id: egui::Id, range: Range<usize>) {
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(range.start),
            egui::text::CCursor::new(range.end),
        )));
    state.store(ctx, id);
}

/// Every rendered character retains its original byte span, including entities and escapes.
fn glyphs(prepared: &editing::Prepared, span: Range<usize>) -> Vec<(char, Range<usize>)> {
    let mut result = vec![];
    for (event, mut range) in Parser::new_ext(&prepared.text, Options::all()).into_offset_iter() {
        if range.start < span.start || range.end > span.end {
            continue;
        }
        let inline_code = matches!(event, Event::Code(_));
        let value = match event {
            Event::Text(t) | Event::Code(t) => t.into_string(),
            Event::SoftBreak => " ".into(),
            Event::HardBreak => "\n".into(),
            _ => continue,
        };
        if inline_code {
            let raw = &prepared.text[range.clone()];
            let fence = raw.bytes().take_while(|b| *b == b'`').count();
            range.start += fence;
            range.end -= fence;
            let content = &prepared.text[range.clone()];
            if content.starts_with(' ') && content.ends_with(' ') && !content.trim().is_empty() {
                range.start += 1;
                range.end -= 1;
            }
        }
        let raw = &prepared.text[range.clone()];
        let mut offset = range.start;
        let count = value.chars().count();
        for (index, ch) in value.chars().enumerate() {
            let start = if count == 1 {
                range.start
            } else {
                prepared.text[offset..range.end]
                    .find(ch)
                    .map_or(offset, |i| offset + i)
            };
            let end = if index + 1 == count {
                range.end
            } else {
                (start + ch.len_utf8()).min(range.end)
            };
            let mut original = prepared.range(start..end);
            if raw == value
                && index == 0
                && prepared.source()[..original.start]
                    .bytes()
                    .rev()
                    .take_while(|b| *b == b'\\')
                    .count()
                    % 2
                    == 1
            {
                original.start -= 1;
            }
            result.push((ch, original));
            offset = end;
        }
    }
    result
}
fn source_range(markdown: &str, value: &str, selection: Range<usize>) -> Option<Range<usize>> {
    let prepared = editing::Prepared::new(markdown);
    let glyphs = glyphs(&prepared, 0..prepared.text.len());
    let mut positions = vec![];
    let mut next = 0;
    for ch in value.chars() {
        while glyphs
            .get(next)
            .is_some_and(|(g, _)| g.is_whitespace() && !ch.is_whitespace())
        {
            next += 1;
        }
        if let Some((g, range)) = glyphs.get(next) {
            if *g == ch || (g.is_whitespace() && ch.is_whitespace()) {
                positions.push(range.clone());
                next += 1;
            } else if ch.is_whitespace() {
                positions.push(range.start..range.start);
            } else {
                return None;
            }
        } else if ch.is_whitespace() {
            positions.push(markdown.len()..markdown.len());
        } else {
            return None;
        }
    }
    let start = positions
        .get(selection.start)
        .map(|r| r.start)
        .or_else(|| positions.last().map(|r| r.end))
        .unwrap_or(markdown.len());
    let end = if selection.is_empty() {
        start
    } else {
        positions
            .get(selection.end.saturating_sub(1))
            .map_or(start, |r| r.end)
    };
    Some(start..end)
}
pub(super) fn local_range(
    prepared: &editing::Prepared,
    span: Range<usize>,
    source: &Range<usize>,
) -> Range<usize> {
    let glyphs = glyphs(prepared, span);
    let start = glyphs
        .iter()
        .take_while(|(_, r)| r.end <= source.start)
        .count();
    let end = if source.is_empty() {
        start
    } else {
        glyphs
            .iter()
            .take_while(|(_, r)| r.start < source.end)
            .count()
    };
    start..end
}
pub(super) fn apply(editor: &mut Editor, kind: Format, ui: &egui::Ui) {
    let Some(c) = editor.preview_tools.context.clone() else {
        return;
    };
    if !c.state.can_apply(kind)
        || (c.inline_only
            && !matches!(
                kind,
                Format::Bold | Format::Italic | Format::Strike | Format::Code | Format::Link
            ))
    {
        return;
    }
    let mut fragment = c.document[c.fragment.clone()].to_owned();
    if kind == Format::Link {
        let selected = byte(&fragment, c.selection.start)..byte(&fragment, c.selection.end);
        if let Some(start) = fragment[..selected.start].rfind("[[")
            && let Some(end) = fragment[start + 2..].find("]]").map(|i| start + 2 + i)
            && selected.end <= end
        {
            let content = &fragment[start + 2..end];
            let target_end = start + 2 + content.find('|').unwrap_or(content.len());
            let target = c.fragment.start + start + 2..c.fragment.start + target_end;
            let url = c.document[target.clone()].to_owned();
            editor.preview_tools.link = Some(LinkDraft {
                document: c.document,
                target,
                restore: c.fragment.start + selected.start..c.fragment.start + selected.end,
                url,
                focus: true,
            });
            return;
        }
    }
    let selection = formatting::apply(&mut fragment, c.selection, kind);
    let range = byte(&fragment, selection.start)..byte(&fragment, selection.end);
    let mut document = c.document;
    document.replace_range(c.fragment.clone(), &fragment);
    let range = c.fragment.start + range.start..c.fragment.start + range.end;
    if kind == Format::Link {
        let url = document[range.clone()].to_owned();
        let prepared = editing::Prepared::new(&document);
        let span = Parser::new(&prepared.text)
            .into_offset_iter()
            .find_map(|(event, span)| {
                (matches!(event, Event::Start(pulldown_cmark::Tag::Link { .. }))
                    && prepared.range(span.clone()).contains(&range.start))
                .then_some(span)
            });
        let label = span.map(|span| glyphs(&prepared, span)).unwrap_or_default();
        let restore = label
            .first()
            .zip(label.last())
            .map_or(range.clone(), |((_, first), (_, last))| {
                first.start..last.end
            });
        editor.preview_tools.link = Some(LinkDraft {
            document,
            target: range,
            restore,
            url,
            focus: true,
        });
    } else {
        commit(editor, document, range, ui);
    }
}
fn byte(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(i, _)| i)
}
pub(super) fn enter_key(ui: &egui::Ui) -> Option<bool> {
    if !ui.is_enabled()
        || egui::Popup::is_any_open(ui.ctx())
        || ui.ctx().memory(|m| m.top_modal_layer().is_some())
    {
        return None;
    }
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter)) {
        Some(true)
    } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
        Some(false)
    } else {
        None
    }
}
pub(super) fn enter(editor: &mut Editor, ui: &egui::Ui) {
    let Some(c) = editor
        .preview_tools
        .context
        .clone()
        .filter(|c| c.accepts_enter && ui.memory(|m| m.has_focus(c.id)))
    else {
        return;
    };
    let Some(soft) = enter_key(ui) else {
        return;
    };
    let mut document = c.document;
    let mut selection = c.fragment.start + byte(&document[c.fragment.clone()], c.selection.start)
        ..c.fragment.start + byte(&document[c.fragment.clone()], c.selection.end);
    if c.local.is_empty()
        && editor.preview.inputs.iter().any(|input| {
            input.output.response.id == c.id
                && input.list
                && c.local.start == input.text.chars().count()
        })
    {
        // A rendered end-of-item caret is after hidden closing styles and link destinations.
        let end = document[c.fragment.clone()]
            .trim_end_matches(['\r', '\n'])
            .len()
            + c.fragment.start;
        selection = end..end;
    }
    let range =
        document[..selection.start].chars().count()..document[..selection.end].chars().count();
    editor.selection = range.clone();
    if let Some(range) = formatting::list_newline(&mut document, range, soft) {
        let selection = byte(&document, range.start)..byte(&document, range.end);
        commit(editor, document, selection, ui);
    } else {
        let caret = c.local.start + 1;
        editor.preview.insert_break(c.id, c.local);
        restore(ui, c.id, Some(caret..caret));
    }
}
pub(super) fn auto_list(editor: &mut Editor, ui: &egui::Ui) {
    let Some(input) =
        editor.preview.inputs.iter().find(|i| {
            i.output.response.has_focus() && i.output.response.changed() && i.accepts_enter
        })
    else {
        return;
    };
    let Some(local) = input.output.cursor_range.map(|r| r.as_sorted_char_range()) else {
        return;
    };
    let (mut document, range) = editor.preview.project_input(input);
    let raw = &document[range.clone()];
    let state = formatting::SelectionState::new(raw, 0..raw.chars().count());
    if local.start != local.end
        || local.end.0 != input.text.chars().count()
        || state.literal
        || state.heading.is_some_and(|level| level != 0)
        || [
            Format::Bold,
            Format::Italic,
            Format::Strike,
            Format::Code,
            Format::Link,
        ]
        .into_iter()
        .any(|kind| state.is_active(kind))
    {
        return;
    }
    let value = input.text.as_str();
    let mut fragment = None;
    if input.list {
        if matches!(value, "[ ] " | "[x] " | "[X] ")
            && let Some(start) = raw.find("\\[")
        {
            fragment = Some(format!("{}{value}", &raw[..start]));
        }
    } else {
        let trimmed = value.trim_end_matches(' ');
        let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
        if value.ends_with(' ')
            && (matches!(trimmed, "-" | "+" | "*" | "- [ ]" | "- [x]" | "- [X]")
                || ((1..=9).contains(&digits) && matches!(&trimmed[digits..], "." | ")")))
        {
            fragment = Some(value.to_owned());
        }
    }
    if let Some(fragment) = fragment {
        document.replace_range(range.clone(), &fragment);
        let point = range.start + fragment.len();
        commit(editor, document, point..point, ui);
    }
}
fn commit(editor: &mut Editor, document: String, selection: Range<usize>, ui: &egui::Ui) {
    let id = egui::Id::new(("journal-editor", &editor.document.path));
    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    let mut history = state.undoer();
    if editor.document.text != document {
        history.add_undo(&(
            egui::text::CCursorRange::one(egui::text::CCursor::new(editor.selection.start)),
            editor.document.text.clone(),
        ));
        editor.document.text = document;
        editor.revision = editor.revision.wrapping_add(1);
        editor.changed_at = Instant::now();
    }
    state.set_undoer(history);
    editor.selection = editor.document.text[..selection.start].chars().count()
        ..editor.document.text[..selection.end].chars().count();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(editor.selection.start),
            egui::text::CCursor::new(editor.selection.end),
        )));
    state.store(ui.ctx(), id);
    editor.preview = render::Preview::document(&editor.document.text);
    editor.preview.focus_source(selection);
    editor.preview_revision = editor.revision;
    editor.preview_tools.context = None;
    editor.preview_tools.pinned = None;
    ui.ctx().request_repaint();
}
pub(super) fn link_dialog(editor: &mut Editor, ui: &egui::Ui) {
    let Some(draft) = editor.preview_tools.link.as_mut() else {
        return;
    };
    let keys = design::ActionKeys::capture(ui.ctx());
    let mut cancel = false;
    let mut save = false;
    let modal =
        design::modal(ui.ctx(), egui::Id::new("journal-preview-link")).show(ui.ctx(), |ui| {
            ui.set_width(design::dialog_width(ui.ctx(), 460.));
            cancel = design::dialog_heading(ui, "Link").clicked();
            amigo_ui::forms::field_label(ui, "Destination");
            let input = ui.add(
                design::input(&mut draft.url)
                    .id(egui::Id::new("journal-preview-link-url"))
                    .desired_width(f32::INFINITY),
            );
            if draft.focus {
                input.request_focus();
                draft.focus = false;
            }
            let valid = !draft.url.trim().is_empty() && !draft.url.contains(['\n', '\r']);
            design::dialog_footer(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    save = ui
                        .add_enabled_ui(valid, |ui| design::primary(ui, "Apply Link"))
                        .inner
                        .clicked();
                    cancel |= design::button(ui, "Cancel").clicked();
                });
            });
            let action = keys.in_ui(ui, valid);
            save |= action.confirm;
            cancel |= action.cancel;
        });
    cancel |= modal.should_close();
    if save {
        let mut draft = editor.preview_tools.link.take().unwrap();
        draft.document.replace_range(
            draft.target,
            &draft.url.trim().replace(' ', "%20").replace(')', "%29"),
        );
        commit(editor, draft.document, draft.restore, ui);
    } else if cancel {
        editor.preview_tools.link = None;
        if let Some(c) = &editor.preview_tools.context {
            restore(ui, c.id, Some(c.local.clone()));
        }
    }
}
