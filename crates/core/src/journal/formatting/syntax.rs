use super::*;
use pulldown_cmark::{Event, Options, Parser, Tag};

pub(super) struct Span {
    pub kind: Format,
    pub range: Range<usize>,
    pub content: Range<usize>,
    pub destination: Option<Range<usize>>,
}
pub(super) struct Syntax {
    pub inline: Vec<Span>,
    pub literal: Vec<Range<usize>>,
    length: usize,
}
impl Syntax {
    pub fn new(text: &str) -> Self {
        let mut result = Self {
            inline: vec![],
            literal: vec![],
            length: text.len(),
        };
        for (event, range) in Parser::new_ext(
            text,
            Options::ENABLE_STRIKETHROUGH
                | Options::ENABLE_TASKLISTS
                | Options::ENABLE_TABLES
                | Options::ENABLE_HEADING_ATTRIBUTES
                | Options::ENABLE_FOOTNOTES,
        )
        .into_offset_iter()
        {
            let (kind, width) = match event {
                Event::Start(Tag::Strong) => (Format::Bold, 2),
                Event::Start(Tag::Emphasis) => (Format::Italic, 1),
                Event::Start(Tag::Strikethrough) => (
                    Format::Strike,
                    text[range.clone()]
                        .bytes()
                        .take_while(|b| *b == b'~')
                        .count()
                        .min(2),
                ),
                Event::Code(_) => {
                    let source = &text[range.clone()];
                    let width = source.bytes().take_while(|b| *b == b'`').count();
                    let mut content = range.start + width..range.end - width;
                    let body = &text[content.clone()];
                    if body.starts_with(' ') && body.ends_with(' ') && !body.trim().is_empty() {
                        content.start += 1;
                        content.end -= 1;
                    }
                    result.inline.push(Span {
                        kind: Format::Code,
                        range,
                        content,
                        destination: None,
                    });
                    continue;
                }
                Event::Start(Tag::Link { dest_url, .. }) => {
                    let source = &text[range.clone()];
                    // Reference and automatic links remain intact; inline links edit their URL.
                    let destination = source.rfind("](").and_then(|start| {
                        source[start + 2..].find(dest_url.as_ref()).map(|offset| {
                            let a = range.start + start + 2 + offset;
                            a..a + dest_url.len()
                        })
                    });
                    result.inline.push(Span {
                        kind: Format::Link,
                        content: range.clone(),
                        range,
                        destination,
                    });
                    continue;
                }
                Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock) => {
                    result.literal.push(range);
                    continue;
                }
                _ => continue,
            };
            result.inline.push(Span {
                kind,
                content: range.start + width..range.end - width,
                range,
                destination: None,
            });
        }
        result
    }
    pub fn literal_at(&self, selection: &Range<usize>) -> bool {
        self.literal.iter().any(|r| {
            intersects(r, selection)
                || (selection.is_empty() && r.end == self.length && selection.start == r.end)
        })
    }
}
pub(super) fn intersects(a: &Range<usize>, b: &Range<usize>) -> bool {
    if b.is_empty() {
        a.start <= b.start && b.start < a.end
    } else {
        a.start < b.end && b.start < a.end
    }
}
pub(super) fn encloses(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start <= b.start && b.end <= a.end
}
