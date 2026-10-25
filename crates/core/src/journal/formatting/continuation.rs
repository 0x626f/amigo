//! List-aware newline transactions shared by Markdown and rendered editing.
use super::*;
use pulldown_cmark::{Event, Options, Parser, Tag};

fn is_list(prefix: &blocks::Prefix) -> bool {
    matches!(
        prefix.kind,
        Some(Format::Bullet | Format::Numbered | Format::Task)
    )
}
fn marker(line: &str, p: &blocks::Prefix) -> String {
    let body = &line[p.quote_end..p.end];
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    let mut marker = if digits > 0 {
        let number = body[..digits].parse::<u32>().unwrap_or(1);
        let next = (number + 1).min(999_999_999);
        format!("{next}{}", &body[digits..])
    } else {
        body.to_owned()
    };
    if p.kind == Some(Format::Task) {
        marker = marker.replace("[x]", "[ ]").replace("[X]", "[ ]");
    }
    if !marker.ends_with([' ', '\t']) {
        marker.push(' ');
    }
    format!("{}{marker}", &line[..p.quote_end])
}

/// Continue a list, or leave an empty item. None lets the caller use its normal newline.
/// Selections use character indices, just like the editor and formatting commands.
pub fn list_newline(
    text: &mut String,
    selection: Range<usize>,
    soft: bool,
) -> Option<Range<usize>> {
    let selection = byte(text, selection.start.min(selection.end))
        ..byte(text, selection.start.max(selection.end));
    let lines = blocks::lines(text, &selection);
    let line_end = text[lines.start..]
        .find('\n')
        .map_or(text.len(), |i| lines.start + i);
    let line = text[lines.start..line_end].trim_end_matches('\r');
    let current = blocks::prefix(line);
    if syntax::Syntax::new(text).literal_at(&selection) {
        return None;
    }
    let mut items = vec![];
    for (event, span) in Parser::new_ext(
        text,
        Options::ENABLE_TASKLISTS | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS,
    )
    .into_offset_iter()
    {
        if matches!(event, Event::Start(Tag::MetadataBlock(_)))
            && span.start <= selection.start
            && selection.start <= span.end
        {
            return None;
        }
        if matches!(event, Event::Start(Tag::Item))
            && span.start <= selection.start
            && selection.start <= span.end
        {
            let start = text[..span.start].rfind('\n').map_or(0, |i| i + 1);
            let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
            let first = text[start..end].trim_end_matches('\r');
            let p = blocks::prefix(first);
            if is_list(&p) {
                items.push((start, first, p));
            }
        }
    }
    let (_, origin, prefix) = items.last()?;
    let prefix = if is_list(&current) { &current } else { prefix };
    let origin = if is_list(&current) { line } else { origin };
    if selection.start < lines.start + current.end || selection.end > line_end {
        return None;
    }
    if !is_list(&current) && line.trim().is_empty() {
        return None;
    }
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let (replace, insert) = if !soft && is_list(&current) && line[current.end..].trim().is_empty() {
        if let Some((_, parent, p)) = items
            .iter()
            .rev()
            .find(|(start, _, _)| *start < lines.start)
        {
            (lines.start..lines.start + line.len(), marker(parent, p))
        } else {
            let quote = &line[current.indent..current.quote_end];
            (
                lines.start..lines.start + line.len(),
                format!("{quote}{eol}{quote}"),
            )
        }
    } else if soft {
        let body = &origin[prefix.quote_end..prefix.end];
        let width = body.find('[').unwrap_or(body.len());
        (
            selection.clone(),
            format!(
                "  {eol}{}{}",
                &origin[..prefix.quote_end],
                " ".repeat(width)
            ),
        )
    } else {
        (
            selection.clone(),
            format!("{eol}{}", marker(origin, prefix)),
        )
    };
    let position = text[..replace.start].chars().count() + insert.chars().count();
    text.replace_range(replace, &insert);
    Some(position..position)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn continues_markers_numbers_unchecked_tasks_indentation_and_crlf() {
        for (source, expected) in [
            ("- One", "- One\n- "),
            ("* One", "* One\n* "),
            ("+ One", "+ One\n+ "),
            ("9) Nine", "9) Nine\n10) "),
            ("1. One", "1. One\n2. "),
            ("- [x] Done", "- [x] Done\n- [ ] "),
            ("2. [X] Done", "2. [X] Done\n3. [ ] "),
            ("- Parent\n  - Child", "- Parent\n  - Child\n  - "),
            ("> - One", "> - One\n> - "),
            ("- One\r\n- Two", "- One\r\n- Two\r\n- "),
        ] {
            let mut text = source.to_owned();
            let end = text.chars().count();
            let caret = list_newline(&mut text, end..end, false).unwrap();
            assert_eq!(text, expected);
            assert_eq!(caret, text.chars().count()..text.chars().count());
        }
    }
    #[test]
    fn second_enter_leaves_the_list_and_empty_nested_items_return_to_the_parent() {
        for (source, expected) in [
            ("- One\n- ", "- One\n\n"),
            ("1. One\n2. ", "1. One\n\n"),
            ("- [ ] ", "\n"),
            ("- Parent\n  - Child\n  - ", "- Parent\n  - Child\n- "),
        ] {
            let mut text = source.to_owned();
            let end = text.chars().count();
            list_newline(&mut text, end..end, false).unwrap();
            assert_eq!(text, expected);
        }
    }
    #[test]
    fn selection_splits_are_unicode_safe_and_code_metadata_and_rules_are_not_lists() {
        let mut text = "- Hello 🌍 world".to_owned();
        list_newline(&mut text, 9..10, false).unwrap();
        assert_eq!(text, "- Hello 🌍\n- world");
        for source in [
            "```md\n- literal",
            "    - code",
            "---\n- YAML\n---\n",
            "- - -",
            "ordinary text",
        ] {
            let mut text = source.to_owned();
            let end = if source.contains("YAML") {
                10
            } else {
                source.chars().count()
            };
            assert!(
                list_newline(&mut text, end..end, false).is_none(),
                "{source}"
            );
            assert_eq!(text, source);
        }
    }
}
