//! Debounced automatic checks. One worker batches words across the app's editable fields.
use crate::{design, spelling};
use egui::{Context, Id, Rect, Ui, text_edit::TextEditOutput};
use std::{
    collections::{BTreeMap, VecDeque},
    ops::Range,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
const DELAY: f64 = 0.35;
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Word {
    pub text: String,
    pub range: Range<usize>,
}
#[derive(Clone, Default)]
struct Field {
    text: String,
    words: Vec<Word>,
    changed_at: f64,
    submitted: bool,
    revision: u64,
}
#[derive(Clone)]
enum Entry {
    Pending,
    Correct,
    Wrong(Vec<String>),
}
type Checker = fn(&str) -> Result<Vec<(String, Vec<String>)>, String>;
struct Engine {
    checker: Checker,
    entries: BTreeMap<String, Entry>,
    queue: VecDeque<String>,
    running: bool,
    error: Option<(Instant, String)>,
}
impl Default for Engine {
    fn default() -> Self {
        Self {
            checker: spelling::check,
            entries: BTreeMap::new(),
            queue: VecDeque::new(),
            running: false,
            error: None,
        }
    }
}
fn engine(ctx: &Context) -> Arc<Mutex<Engine>> {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Arc<Mutex<Engine>>>(Id::new("automatic-spelling-engine"))
            .clone()
    })
}
fn request(ctx: &Context, words: impl Iterator<Item = String>) -> bool {
    let personal = spelling::personal_words(ctx);
    let engine = engine(ctx);
    let mut state = engine.lock().unwrap();
    for word in personal.iter() {
        state.entries.insert(word.clone(), Entry::Correct);
    }
    if state
        .error
        .as_ref()
        .is_some_and(|(at, _)| at.elapsed() < Duration::from_secs(30))
    {
        return false;
    }
    state.error = None;
    for word in words {
        if personal.contains(&word.to_lowercase()) {
            state.entries.insert(word, Entry::Correct);
            continue;
        }
        if !state.entries.contains_key(&word) {
            state.entries.insert(word.clone(), Entry::Pending);
            state.queue.push_back(word);
        }
    }
    if state.running || state.queue.is_empty() {
        return true;
    }
    state.running = true;
    drop(state);
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        loop {
            let words = {
                let mut state = engine.lock().unwrap();
                if state.queue.is_empty() {
                    state.running = false;
                    return;
                }
                let count = state.queue.len().min(256);
                state.queue.drain(..count).collect::<Vec<_>>()
            };
            let checker = engine.lock().unwrap().checker;
            let result = checker(&words.join("\n"));
            let mut state = engine.lock().unwrap();
            let personal = spelling::personal_words(&ctx);
            match result {
                Ok(issues) => {
                    let mut issues: BTreeMap<_, _> = issues.into_iter().collect();
                    for word in words {
                        let entry = if personal.contains(&word.to_lowercase()) {
                            Entry::Correct
                        } else {
                            issues.remove(&word).map_or(Entry::Correct, Entry::Wrong)
                        };
                        state.entries.insert(word, entry);
                    }
                }
                Err(error) => {
                    state
                        .entries
                        .retain(|_, entry| !matches!(entry, Entry::Pending));
                    state.queue.clear();
                    state.running = false;
                    state.error = Some((Instant::now(), error));
                    drop(state);
                    ctx.request_repaint();
                    return;
                }
            }
            drop(state);
            ctx.request_repaint();
        }
    });
    true
}
pub(super) fn suggestions(ctx: &Context, word: &str) -> Option<Result<Vec<String>, String>> {
    if spelling::is_personal_word(ctx, word) {
        return Some(Ok(Vec::new()));
    }
    let engine = engine(ctx);
    let state = engine.lock().unwrap();
    match state.entries.get(word) {
        Some(Entry::Wrong(suggestions)) => Some(Ok(suggestions.clone())),
        Some(Entry::Correct) => Some(Ok(Vec::new())),
        _ => state.error.as_ref().map(|(_, error)| Err(error.clone())),
    }
}
/// Preserve character offsets while excluding code, Markdown destinations, URLs and identifiers.
pub(super) fn words(text: &str) -> Vec<Word> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
    let mut code = false;
    let regions: Vec<_> = Parser::new_ext(
        text,
        Options::ENABLE_TABLES | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS,
    )
    .into_offset_iter()
    .filter_map(|(event, span)| match event {
        Event::Start(Tag::CodeBlock(_) | Tag::MetadataBlock(_)) => {
            code = true;
            None
        }
        Event::End(TagEnd::CodeBlock | TagEnd::MetadataBlock(_)) => {
            code = false;
            None
        }
        Event::Text(_) if !code => Some(span),
        _ => None,
    })
    .collect();
    let chars: Vec<_> = text.char_indices().collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].1.is_alphabetic() {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < chars.len()
            && (chars[i].1.is_alphabetic()
                || (matches!(chars[i].1, '\'' | '’' | '-')
                    && i + 1 < chars.len()
                    && chars[i + 1].1.is_alphabetic()))
        {
            i += 1;
        }
        let byte = chars[start].0;
        let end = chars.get(i).map_or(text.len(), |(b, _)| *b);
        let region = regions.partition_point(|r| r.end <= byte);
        if !regions
            .get(region)
            .is_some_and(|r| r.start <= byte && end <= r.end)
            || i - start > 64
        {
            continue;
        }
        let prose = &regions[region];
        let chunk_start = text[prose.start..byte]
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map_or(prose.start, |(p, c)| prose.start + p + c.len_utf8());
        let chunk_end = text[end..prose.end]
            .find(char::is_whitespace)
            .map_or(prose.end, |p| end + p);
        let chunk = &text[chunk_start..chunk_end];
        if chunk.contains("://")
            || chunk.contains('@')
            || chunk.contains('/')
            || chunk.contains('\\')
            || chunk.chars().any(|c| c.is_ascii_digit())
            || chunk
                .trim_matches(|c: char| matches!(c, '_' | '*' | '~' | '[' | ']' | '(' | ')'))
                .contains('_')
        {
            continue;
        }
        if byte > 0 && text[..byte].ends_with('&') && text[end..].starts_with(';') {
            continue;
        }
        result.push(Word {
            text: text[byte..end].into(),
            range: start..i,
        });
    }
    result
}
pub(super) fn word_at(text: &str, point: usize) -> Option<Word> {
    words(text)
        .into_iter()
        .find(|w| w.range.start <= point && point <= w.range.end)
}
// egui aligns the galley bounds before painting; centered/right-aligned rows
// can have a negative left edge within those bounds.
pub(super) fn text_origin(output: &TextEditOutput) -> egui::Pos2 {
    output.galley_pos - egui::vec2(output.galley.rect.left(), 0.0)
}

fn rectangles(output: &TextEditOutput, range: Range<usize>) -> Vec<Rect> {
    let mut offset = 0;
    let mut rects = Vec::new();
    for row in &output.galley.rows {
        let start = range.start.saturating_sub(offset).min(row.glyphs.len());
        let end = range.end.saturating_sub(offset).min(row.glyphs.len());
        if start < end {
            let left = row.pos.x + row.glyphs[start].pos.x;
            let right = row.pos.x + row.glyphs[end - 1].pos.x + row.glyphs[end - 1].advance_width;
            let rect = Rect::from_min_max(
                egui::pos2(left, row.pos.y),
                egui::pos2(right, row.pos.y + row.size.y),
            );
            rects.push(rect.translate(text_origin(output).to_vec2()));
        }
        offset += row.glyphs.len() + usize::from(row.ends_with_newline);
        if offset >= range.end {
            break;
        }
    }
    rects
}
pub(super) fn show(ui: &Ui, output: &TextEditOutput) {
    let key = output.response.id.with("automatic-spelling-field");
    let revision = spelling::personal_revision(ui.ctx());
    let now = ui.input(|i| i.time);
    let text = output.galley.text();
    let mut field = ui
        .ctx()
        .data_mut(|d| d.remove_temp::<Field>(key))
        .unwrap_or_default();
    if field.text != text {
        field = Field {
            text: text.into(),
            words: words(text),
            changed_at: now,
            submitted: false,
            revision,
        };
    }
    if field.revision != revision {
        field.revision = revision;
        field.submitted = false;
        field.changed_at = now;
    }
    if !field.words.is_empty() && !field.submitted {
        let remaining = DELAY - (now - field.changed_at);
        if remaining <= 0. {
            field.submitted = request(ui.ctx(), field.words.iter().map(|w| w.text.clone()));
            if !field.submitted {
                ui.ctx().request_repaint_after(Duration::from_secs(30));
            }
        } else {
            ui.ctx()
                .request_repaint_after(Duration::from_secs_f64(remaining));
        }
    }
    let personal = spelling::personal_words(ui.ctx());
    let errors = {
        let engine = engine(ui.ctx());
        let state = engine.lock().unwrap();
        field
            .words
            .iter()
            .filter(|word| !personal.contains(&word.text.to_lowercase()))
            .filter_map(|word| match state.entries.get(&word.text) {
                Some(Entry::Wrong(suggestions)) => Some((word.clone(), suggestions.clone())),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let clip = output.text_clip_rect.intersect(ui.clip_rect());
    let painter = ui.painter().with_clip_rect(clip);
    let caret = output
        .cursor_range
        .filter(|r| r.is_empty())
        .map(|r| r.primary.index.0);
    let mut hint = None;
    for (word, suggestions) in errors {
        let rects = rectangles(output, word.range.clone());
        for rect in &rects {
            if !rect.intersects(clip) {
                continue;
            }
            let mut points = Vec::new();
            let mut x = rect.left();
            let mut up = false;
            while x < rect.right() {
                points.push(egui::pos2(
                    x,
                    rect.bottom() - 1.5 + if up { -1. } else { 1. },
                ));
                up = !up;
                x += 2.;
            }
            points.push(egui::pos2(rect.right(), rect.bottom() - 1.5));
            painter.add(egui::Shape::line(
                points,
                egui::Stroke::new(1., design::colors(ui).red),
            ));
        }
        let hovering = output
            .response
            .hover_pos()
            .is_some_and(|p| rects.iter().any(|r| r.intersect(clip).contains(p)));
        if hovering {
            let message = if suggestions.is_empty() {
                format!(
                    "{}: spelling not recognized. Right-click to check.",
                    word.text
                )
            } else {
                format!(
                    "{} → {}\nRight-click → Dictionary → Fix to",
                    word.text,
                    suggestions.join(", ")
                )
            };
            output.response.response.clone().on_hover_text(message);
        }
        if output.response.has_focus()
            && !egui::Popup::is_any_open(ui.ctx())
            && now - field.changed_at >= DELAY
            && let Some(caret) = caret
        {
            let after_space = caret == word.range.end + 1
                && text
                    .chars()
                    .nth(word.range.end)
                    .is_some_and(char::is_whitespace);
            if ((word.range.start <= caret && caret <= word.range.end) || after_space)
                && let Some(last) = rects.last().filter(|r| r.intersects(clip))
            {
                hint = Some((last.left_bottom(), word.text, suggestions));
            }
        }
    }
    if let Some((anchor, word, suggestions)) = hint {
        let message = if suggestions.is_empty() {
            format!("{word}: spelling not recognized · Right-click to check")
        } else {
            format!(
                "{word} → {} · Right-click → Dictionary",
                suggestions
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        egui::Area::new(output.response.id.with("spelling-hint"))
            .order(egui::Order::Tooltip)
            .fixed_pos(anchor + egui::vec2(0., 4.))
            .interactable(false)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(360.);
                    ui.label(message);
                });
            });
    }
    ui.ctx().data_mut(|d| d.insert_temp(key, field));
}

pub(super) fn accept(ctx: &Context, word: &str) {
    let normalized = word.to_lowercase();
    let engine = engine(ctx);
    let mut state = engine.lock().unwrap();
    for (key, entry) in &mut state.entries {
        if key.to_lowercase() == normalized {
            *entry = Entry::Correct;
        }
    }
    state.entries.insert(word.into(), Entry::Correct);
}
pub(super) fn reset(ctx: &Context) {
    ctx.data_mut(|d| d.remove::<Arc<Mutex<Engine>>>(Id::new("automatic-spelling-engine")));
}

#[cfg(test)]
pub(super) fn seed(ctx: &Context, word: &str, suggestions: Vec<String>) {
    engine(ctx)
        .lock()
        .unwrap()
        .entries
        .insert(word.into(), Entry::Wrong(suggestions));
}

#[cfg(test)]
pub(super) fn checker_for_test(ctx: &Context, checker: Checker) {
    engine(ctx).lock().unwrap().checker = checker;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prose_words_keep_unicode_offsets_and_ignore_code_destinations_and_identifiers() {
        let text = "🌍 **teh** _wrng_ [labl](https://example.com)\n\n`speling`\n\n```rust\nfn qzx() {}\n```\n\nhello_world https://example.org user@example.com token123 naïve";
        let words = words(text);
        let values = words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>();
        assert_eq!(values, vec!["teh", "wrng", "labl", "naïve"]);
        for word in words {
            assert_eq!(
                text.chars()
                    .skip(word.range.start)
                    .take(word.range.len())
                    .collect::<String>(),
                word.text
            );
        }
    }
}
