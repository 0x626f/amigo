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
    CodeBlock,
    Link,
    Table,
    Rule,
}
fn byte(text: &str, character: usize) -> usize {
    text.char_indices()
        .nth(character)
        .map_or(text.len(), |(i, _)| i)
}
pub fn apply(text: &mut String, selection: Range<usize>, format: Format) -> Range<usize> {
    let start = byte(text, selection.start.min(selection.end));
    let end = byte(text, selection.end.max(selection.start));
    let selected = &text[start..end];
    let (range, replacement, inset, selected_len) = match format {
        Format::Bold | Format::Italic | Format::Strike | Format::Code => {
            let marker = match format {
                Format::Bold => "**",
                Format::Italic => "_",
                Format::Strike => "~~",
                _ => "`",
            };
            if text[..start].ends_with(marker) && text[end..].starts_with(marker) {
                (
                    start - marker.len()..end + marker.len(),
                    selected.to_owned(),
                    0,
                    selected.chars().count(),
                )
            } else {
                let selected = if selected.is_empty() {
                    "text"
                } else {
                    selected
                };
                (
                    start..end,
                    format!("{marker}{selected}{marker}"),
                    marker.len(),
                    selected.chars().count(),
                )
            }
        }
        Format::Heading(_) | Format::Bullet | Format::Numbered | Format::Task | Format::Quote => {
            let a = text[..start].rfind('\n').map_or(0, |i| i + 1);
            let line_end = if end > a && text[..end].ends_with('\n') {
                end - 1
            } else {
                end
            };
            let b = text[line_end..]
                .find('\n')
                .map_or(text.len(), |i| line_end + i);
            let mut lines = vec![];
            for (index, line) in text[a..b].split('\n').enumerate() {
                let prefix = match format {
                    Format::Heading(0) => String::new(),
                    Format::Heading(n) => format!("{} ", "#".repeat(n.clamp(1, 6) as usize)),
                    Format::Bullet => "- ".into(),
                    Format::Numbered => format!("{}. ", index + 1),
                    Format::Task => "- [ ] ".into(),
                    _ => "> ".into(),
                };
                let line = if matches!(format, Format::Heading(_)) {
                    line.trim_start_matches('#').trim_start()
                } else {
                    line
                };
                lines.push(format!("{prefix}{line}"));
            }
            let replacement = lines.join("\n");
            let count = replacement.chars().count();
            (a..b, replacement, 0, count)
        }
        Format::CodeBlock => {
            let fence = if selected.contains("```") {
                "````"
            } else {
                "```"
            };
            let body = if selected.is_empty() {
                "code"
            } else {
                selected
            };
            (
                start..end,
                format!("{fence}\n{body}\n{fence}"),
                fence.len() + 1,
                body.chars().count(),
            )
        }
        Format::Link => {
            let label = if selected.is_empty() {
                "link text"
            } else {
                selected
            };
            (
                start..end,
                format!("[{label}](https://)"),
                label.chars().count() + 3,
                8,
            )
        }
        Format::Table => (
            start..end,
            "\n| Column 1 | Column 2 |\n| --- | --- |\n| Text | Text |\n".into(),
            3,
            8,
        ),
        Format::Rule => (start..end, "\n\n---\n\n".into(), 7, 0),
    };
    let prefix = text[..range.start].chars().count();
    text.replace_range(range, &replacement);
    prefix + inset..prefix + inset + selected_len
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_formatting_preserves_unicode_and_toggles_without_losing_text() {
        let mut text = "Hi 🌍 Україна!".to_owned();
        let selected = apply(&mut text, 3..12, Format::Bold);
        assert_eq!(text, "Hi **🌍 Україна**!");
        apply(&mut text, selected, Format::Bold);
        assert_eq!(text, "Hi 🌍 Україна!");
    }
    #[test]
    fn block_formatting_keeps_unselected_lines_and_links_select_the_url() {
        let mut text = "First\nSecond\nThird".to_owned();
        apply(&mut text, 6..13, Format::Task);
        assert_eq!(text, "First\n- [ ] Second\nThird");
        let mut text = "Journal".into();
        let range = apply(&mut text, 0..7, Format::Link);
        assert_eq!(text, "[Journal](https://)");
        assert_eq!(
            text.chars()
                .skip(range.start)
                .take(range.len())
                .collect::<String>(),
            "https://"
        );
    }
}
