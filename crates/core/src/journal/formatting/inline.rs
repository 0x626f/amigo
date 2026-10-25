use super::*;
use syntax::{Syntax, encloses, intersects};

pub(super) fn active(
    text: &str,
    selection: &Range<usize>,
    spans: &[&syntax::Span],
    syntax: &Syntax,
) -> bool {
    if spans.iter().any(|s| encloses(&s.content, selection)) {
        return true;
    }
    if selection.is_empty() {
        return false;
    }
    let lines = blocks::lines(text, selection);
    let mut start = lines.start;
    let prefixes: Vec<_> = text[lines]
        .split_inclusive('\n')
        .map(|line| {
            let prefix = start..start + blocks::prefix(line).end;
            start += line.len();
            prefix
        })
        .collect();
    if !spans.iter().any(|s| intersects(&s.content, selection)) {
        return false;
    }
    let mut covered: Vec<_> = spans
        .iter()
        .map(|s| s.range.clone())
        .chain(prefixes)
        .chain(syntax.literal.iter().cloned())
        .chain(
            syntax
                .inline
                .iter()
                .flat_map(|s| [s.range.start..s.content.start, s.content.end..s.range.end]),
        )
        .filter(|r| intersects(r, selection))
        .collect();
    covered.sort_by_key(|r| r.start);
    let mut cursor = selection.start;
    for r in covered {
        if r.start > cursor && !text[cursor..r.start.min(selection.end)].trim().is_empty() {
            return false;
        }
        cursor = cursor.max(r.end).min(selection.end);
    }
    text[cursor..selection.end].trim().is_empty()
}

pub(super) fn apply(
    text: &mut String,
    selection: Range<usize>,
    kind: Format,
    syntax: &Syntax,
    remove: Option<bool>,
) -> Range<usize> {
    if syntax.literal_at(&selection) {
        return selection;
    }
    let spans: Vec<_> = syntax.inline.iter().filter(|s| s.kind == kind).collect();
    if kind == Format::Link {
        if let Some(span) = spans.iter().find(|s| intersects(&s.range, &selection)) {
            return span.destination.clone().unwrap_or(selection);
        }
        let label = if selection.is_empty() {
            "link text"
        } else {
            &text[selection.clone()]
        };
        let start = selection.start + label.len() + 3;
        let replacement = format!("[{label}](https://)");
        text.replace_range(selection, &replacement);
        return start..start + 8;
    }
    // Markdown emphasis is literal inside code spans.
    if kind != Format::Code
        && syntax
            .inline
            .iter()
            .any(|s| s.kind == Format::Code && encloses(&s.content, &selection))
    {
        return selection;
    }
    let mut range = selection.clone();
    let affected: Vec<_> = spans
        .iter()
        .filter(|s| intersects(&s.range, &selection))
        .copied()
        .collect();
    let remove = remove.unwrap_or_else(|| active(text, &selection, &spans, syntax));
    if remove && kind != Format::Code && !selection.is_empty() && affected.len() == 1 {
        let span = affected[0];
        // Split an ordinary styled run when only part of its text is selected. For nested
        // markup, use the complete enclosing run so other delimiters stay balanced.
        if encloses(&span.content, &selection)
            && selection != span.content
            && !syntax
                .inline
                .iter()
                .any(|s| s.kind != kind && intersects(&s.range, &span.content))
        {
            let marker = &text[span.range.start..span.content.start];
            let before = emphasis(&text[span.content.start..selection.start], marker);
            let after = emphasis(&text[selection.end..span.content.end], marker);
            let middle = &text[selection.clone()];
            let start = span.range.start + before.len();
            let end = start + middle.len();
            let replacement = format!("{before}{middle}{after}");
            text.replace_range(span.range.clone(), &replacement);
            return start..end;
        }
    }
    for span in &affected {
        range.start = range.start.min(span.range.start);
        range.end = range.end.max(span.range.end);
    }
    let mut body = text[range.clone()].to_owned();
    let mut edits: Vec<_> = affected
        .iter()
        .flat_map(|span| {
            [
                span.range.start..span.content.start,
                span.content.end..span.range.end,
            ]
        })
        .collect();
    edits.sort_by_key(|r| r.start);
    for edit in edits.into_iter().rev() {
        body.replace_range(edit.start - range.start..edit.end - range.start, "");
    }
    if remove {
        let end = range.start + body.len();
        text.replace_range(range.clone(), &body);
        return range.start..end;
    }
    if body.is_empty() {
        body.push_str("text");
    }
    let leading = body.len() - body.trim_start().len();
    let trailing = body.trim_end().len().max(leading);
    let content = &body[leading..trailing];
    if content.is_empty() && kind != Format::Code {
        return selection;
    }
    let marker = match kind {
        Format::Bold => "**".into(),
        Format::Italic => "*".into(),
        Format::Strike => "~~".into(),
        Format::Code => "`".repeat(longest_backticks(content) + 1),
        _ => unreachable!(),
    };
    let pad = if kind == Format::Code && (content.starts_with('`') || content.ends_with('`')) {
        " "
    } else {
        ""
    };
    // Inline code retains meaningful spaces instead of moving them outside the span.
    let (leading, trailing, content, pad) = if kind == Format::Code {
        let pad = if body.starts_with('`')
            || body.ends_with('`')
            || (body.starts_with(' ') && body.ends_with(' ') && !body.trim().is_empty())
        {
            " "
        } else {
            ""
        };
        (0, body.len(), body.as_str(), pad)
    } else {
        (leading, trailing, content, pad)
    };
    let replacement = format!(
        "{}{marker}{pad}{content}{pad}{marker}{}",
        &body[..leading],
        &body[trailing..]
    );
    let start = range.start + leading + marker.len() + pad.len();
    let end = start + content.len();
    text.replace_range(range, &replacement);
    start..end
}

fn emphasis(body: &str, marker: &str) -> String {
    let start = body.len() - body.trim_start().len();
    let end = body.trim_end().len();
    if start >= end {
        return body.to_owned();
    }
    format!(
        "{}{marker}{}{marker}{}",
        &body[..start],
        &body[start..end],
        &body[end..]
    )
}
