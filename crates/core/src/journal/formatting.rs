//! Formatting transforms character-index selections, matching egui and Unicode text editing.
use std::ops::Range;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Bold,
    Italic,
    Strike,
    Code,
    Heading(u8),
    Bullet,
    Numbered,
    Task,
    Quote,
    CodeBlock(&'static str),
    Link,
    Table,
    Rule,
}
fn byte(text: &str, character: usize) -> usize {
    text.char_indices()
        .nth(character)
        .map_or(text.len(), |(i, _)| i)
}
mod blocks;
mod continuation;
mod inline;
mod syntax;
pub use continuation::list_newline;

/// The applied styles of the selection, derived from the same Markdown grammar as preview.
#[derive(Clone, Debug, Default)]
pub struct SelectionState {
    active: Vec<Format>,
    pub heading: Option<u8>,
    pub literal: bool,
    pub language: Option<String>,
}
impl SelectionState {
    pub fn new(text: &str, selection: Range<usize>) -> Self {
        let selection = byte(text, selection.start.min(selection.end))
            ..byte(text, selection.start.max(selection.end));
        let syntax = syntax::Syntax::new(text);
        let mut state = Self {
            literal: syntax
                .literal
                .iter()
                .any(|r| syntax::encloses(r, &selection)),
            language: enclosing_fence(text, selection.clone()).map(|f| f.language.to_owned()),
            ..Self::default()
        };
        for kind in [
            Format::Bold,
            Format::Italic,
            Format::Strike,
            Format::Code,
            Format::Link,
        ] {
            let spans = syntax
                .inline
                .iter()
                .filter(|s| s.kind == kind)
                .collect::<Vec<_>>();
            if inline::active(text, &selection, &spans, &syntax) {
                state.active.push(kind);
            }
        }
        let mut offset = blocks::lines(text, &selection).start;
        let rows: Vec<_> = text[blocks::lines(text, &selection)]
            .split_inclusive('\n')
            .filter_map(|line| {
                let start = offset;
                offset += line.len();
                (!line.trim().is_empty() && !syntax.literal_at(&(start..offset)))
                    .then(|| blocks::prefix(line.trim_end_matches(['\r', '\n'])))
            })
            .collect();
        if let Some(first) = rows.first() {
            let heading = match first.kind {
                Some(Format::Heading(n)) => n,
                _ => 0,
            };
            if rows.iter().all(|p| match p.kind {
                Some(Format::Heading(n)) => n == heading,
                _ => heading == 0,
            }) {
                state.heading = Some(heading);
                state.active.push(Format::Heading(heading));
            }
            for kind in [Format::Bullet, Format::Numbered, Format::Task] {
                if rows.iter().all(|p| p.kind == Some(kind)) {
                    state.active.push(kind);
                }
            }
            if rows.iter().all(|p| p.quote_end > p.indent) {
                state.active.push(Format::Quote);
            }
        }
        state
    }
    pub fn is_active(&self, kind: Format) -> bool {
        match kind {
            Format::CodeBlock(language) => self.language.as_deref() == Some(language),
            _ => self.active.contains(&kind),
        }
    }
    pub fn can_apply(&self, kind: Format) -> bool {
        !self.literal || matches!(kind, Format::CodeBlock(_))
    }
}

pub fn apply(text: &mut String, selection: Range<usize>, format: Format) -> Range<usize> {
    let selection = byte(text, selection.start.min(selection.end))
        ..byte(text, selection.end.max(selection.start));
    if let Format::CodeBlock(language) = format {
        return apply_code_block(text, selection, language);
    }
    let range = match format {
        Format::Bold | Format::Italic | Format::Strike | Format::Code | Format::Link => {
            apply_inline(text, selection, format)
        }
        Format::Heading(_) | Format::Bullet | Format::Numbered | Format::Task | Format::Quote => {
            blocks::apply(text, selection, format)
        }
        Format::Table | Format::Rule => {
            if syntax::Syntax::new(text).literal_at(&selection) {
                selection
            } else {
                let (replacement, inset, count) = if format == Format::Table {
                    (
                        "\n| Column 1 | Column 2 |\n| --- | --- |\n| Text | Text |\n",
                        3,
                        8,
                    )
                } else {
                    ("\n\n---\n\n", 7, 0)
                };
                let start = selection.start + inset;
                text.replace_range(selection, replacement);
                start..start + count
            }
        }
        Format::CodeBlock(_) => unreachable!(),
    };
    text[..range.start].chars().count()..text[..range.end].chars().count()
}

fn apply_inline(text: &mut String, selection: Range<usize>, format: Format) -> Range<usize> {
    let syntax = syntax::Syntax::new(text);
    let spans = syntax
        .inline
        .iter()
        .filter(|s| s.kind == format)
        .collect::<Vec<_>>();
    let remove = inline::active(text, &selection, &spans, &syntax);
    let line_start = blocks::lines(text, &selection).start;
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |n| line_start + n);
    let multiline_span = syntax.inline.iter().any(|s| {
        s.kind == format
            && syntax::intersects(&s.range, &selection)
            && text[s.range.clone()].contains('\n')
    });
    if !matches!(format, Format::Code | Format::Link)
        && !multiline_span
        && (text[selection.clone()].contains('\n')
            || selection.start < line_start + blocks::prefix(&text[line_start..line_end]).end)
    {
        // Keep inline markers inside each block, after its heading/list/quote prefix.
        let mut offset = blocks::lines(text, &selection).start;
        let ranges: Vec<_> = text[blocks::lines(text, &selection)]
            .split_inclusive('\n')
            .filter_map(|line| {
                let start = offset;
                offset += line.len();
                let line = line.trim_end_matches(['\r', '\n']);
                let a = (start + blocks::prefix(line).end).max(selection.start);
                let b = (start + line.len()).min(selection.end);
                (a < b && !syntax.literal_at(&(start..offset))).then_some(a..b)
            })
            .collect();
        let mut end = selection.end;
        let mut start = selection.start;
        for range in ranges.into_iter().rev() {
            let before = text.len();
            let result = inline::apply(text, range, format, &syntax, Some(remove));
            end = end.saturating_add_signed(text.len() as isize - before as isize);
            start = start.min(result.start);
        }
        start..end
    } else {
        inline::apply(text, selection, format, &syntax, None)
    }
}

struct Fence<'a> {
    opening: Range<usize>,
    body: Range<usize>,
    block: Range<usize>,
    language: &'a str,
}

// Only fenced lines count: a closing fence must match the opener and be at least as long.
fn enclosing_fence(text: &str, selection: Range<usize>) -> Option<Fence<'_>> {
    let mut opening = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        let trimmed = content.trim_start_matches(' ');
        let indent = content.len() - trimmed.len();
        if indent <= 3 {
            let marker = trimmed.as_bytes().first().copied().unwrap_or_default();
            let length = trimmed.bytes().take_while(|b| *b == marker).count();
            if let Some((start, body_start, kind, count, language)) = opening {
                if marker == kind && length >= count && trimmed[length..].trim().is_empty() {
                    let block_end = offset + line.len();
                    if selection.start >= start
                        && selection.end <= block_end
                        && (selection.start < block_end || block_end == text.len())
                    {
                        return Some(Fence {
                            opening: start..body_start,
                            body: body_start..offset,
                            block: start..block_end,
                            language,
                        });
                    }
                    opening = None;
                }
            } else if matches!(marker, b'`' | b'~') && length >= 3 {
                let info = trimmed[length..].trim();
                if marker != b'`' || !info.contains('`') {
                    opening = Some((
                        offset,
                        offset + line.len(),
                        marker,
                        length,
                        info.split_whitespace().next().unwrap_or(""),
                    ));
                }
            }
        }
        offset += line.len();
    }
    if let Some((start, body_start, _, _, language)) = opening
        && selection.start >= start
        && selection.end <= text.len()
    {
        return Some(Fence {
            opening: start..body_start,
            body: body_start..text.len(),
            block: start..text.len(),
            language,
        });
    }
    None
}

pub fn code_block_language(text: &str, selection: Range<usize>) -> Option<&str> {
    enclosing_fence(
        text,
        byte(text, selection.start.min(selection.end))
            ..byte(text, selection.start.max(selection.end)),
    )
    .map(|fence| fence.language)
}

fn longest_backticks(text: &str) -> usize {
    text.split(|c| c != '`').map(str::len).max().unwrap_or(0)
}

fn apply_code_block(text: &mut String, selection: Range<usize>, language: &str) -> Range<usize> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    if let Some(existing) = enclosing_fence(text, selection.clone()) {
        let opening = &text[existing.opening.clone()];
        let indent = opening.len() - opening.trim_start_matches(' ').len();
        let marker = opening.as_bytes()[indent] as char;
        let length = opening[indent..]
            .chars()
            .take_while(|c| *c == marker)
            .count();
        let heading = format!("{}{language}{newline}", marker.to_string().repeat(length));
        let heading = format!("{}{heading}", " ".repeat(indent));
        let body = &text[existing.body.clone()];
        let body_len = body.trim_end_matches(['\r', '\n']).chars().count();
        let prefix = text[..existing.block.start].chars().count();
        // An unfinished fence is completed when its language is chosen.
        if existing.body.end == existing.block.end {
            let separator = if body.ends_with('\n') { "" } else { newline };
            let replacement = format!(
                "{heading}{body}{separator}{}",
                marker.to_string().repeat(length)
            );
            text.replace_range(existing.block, &replacement);
        } else {
            text.replace_range(existing.opening, &heading);
        }
        let start = prefix + heading.chars().count();
        return start..start + body_len;
    }
    let body = &text[selection.clone()];
    let body = if body.is_empty() { "code" } else { body };
    let fence = "`".repeat((longest_backticks(body) + 1).max(3));
    let before = if selection.start > 0 && !text[..selection.start].ends_with('\n') {
        newline
    } else {
        ""
    };
    let after = if selection.end < text.len() && !text[selection.end..].starts_with('\n') {
        newline
    } else {
        ""
    };
    let separator = if body.ends_with('\n') { "" } else { newline };
    let heading = format!("{before}{fence}{language}{newline}");
    let replacement = format!("{heading}{body}{separator}{fence}{after}");
    let start = text[..selection.start].chars().count() + heading.chars().count();
    let length = body.chars().count();
    text.replace_range(selection, &replacement);
    start..start + length
}
#[cfg(test)]
mod tests;
