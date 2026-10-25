//! Project rendered text edits back into their original Markdown spans.
use pulldown_cmark::{Event, Parser, Tag};
use std::ops::Range;

#[derive(Clone, Debug)]
pub(super) struct Patch {
    pub range: Range<usize>,
    pub text: String,
}
#[derive(Clone, Debug)]
pub(super) struct Unit {
    pub text: String,
    pub span: Range<usize>,
    pub code: bool,
    pub wrappers: Vec<Range<usize>>,
}
#[derive(Default)]
pub(super) struct Prepared {
    pub text: String,
    pub base_line: usize,
    source: String,
    segments: Vec<(Range<usize>, Range<usize>)>,
    wiki: Vec<(Range<usize>, Range<usize>, String)>,
}
impl Prepared {
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn generated_point(&self, point: usize) -> usize {
        self.segments
            .iter()
            .find(|(_, raw)| raw.start <= point && point <= raw.end)
            .map_or(self.text.len(), |(generated, raw)| {
                if generated.len() == raw.len() {
                    generated.start + point.saturating_sub(raw.start)
                } else if point == raw.end {
                    generated.end
                } else {
                    generated.start
                }
            })
    }
    pub fn new(source: &str) -> Self {
        let mut out = Self {
            source: source.into(),
            ..Self::default()
        };
        let text = source.strip_prefix('\u{feff}').unwrap_or(source);
        let body = if let Some(rest) = text.strip_prefix("---\r\n") {
            rest.split_once("\r\n---\r\n")
                .map_or(text, |(_, body)| body)
        } else if let Some(rest) = text.strip_prefix("---\n") {
            rest.split_once("\n---\n").map_or(text, |(_, body)| body)
        } else {
            text
        };
        let base = source.len() - body.len();
        out.base_line = source[..base].bytes().filter(|b| *b == b'\n').count();
        let mut protected: Vec<_> = Parser::new(body)
            .into_offset_iter()
            .filter_map(|(event, range)| {
                matches!(event, Event::Code(_) | Event::Start(Tag::CodeBlock(_))).then_some(range)
            })
            .collect();
        protected.sort_by_key(|r| r.start);
        let mut cursor = 0;
        for range in protected {
            if range.start < cursor {
                continue;
            }
            out.wiki_text(&body[cursor..range.start], base + cursor);
            out.push(&body[range.clone()], base + range.start..base + range.end);
            cursor = range.end;
        }
        out.wiki_text(&body[cursor..], base + cursor);
        out
    }
    fn push(&mut self, text: &str, original: Range<usize>) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        self.segments.push((start..self.text.len(), original));
    }
    fn wiki_text(&mut self, mut rest: &str, mut base: usize) {
        while let Some(start) = rest.find("[[") {
            self.push(&rest[..start], base..base + start);
            let Some(end) = rest[start + 2..].find("]]").map(|n| start + 2 + n) else {
                self.push(&rest[start..], base + start..base + rest.len());
                return;
            };
            let content = &rest[start + 2..end];
            let (target, label) = content.split_once('|').unwrap_or((content, content));
            let label_start =
                base + start + 2 + content.split_once('|').map_or(0, |(t, _)| t.len() + 1);
            let label_range = label_start..label_start + label.len();
            self.push("[", base + start..base + start + 2);
            for (i, c) in label.char_indices() {
                let escaped = if matches!(c, '[' | ']') {
                    format!("\\{c}")
                } else {
                    c.to_string()
                };
                self.push(&escaped, label_start + i..label_start + i + c.len_utf8());
            }
            self.push(
                &format!("](amigo-journal://{})", super::encode_target(target)),
                base + end..base + end + 2,
            );
            if !content.contains('|') {
                self.wiki
                    .push((base + start..base + end + 2, label_range, target.into()));
            }
            base += end + 2;
            rest = &rest[end + 2..];
        }
        self.push(rest, base..base + rest.len());
    }
    pub fn range(&self, range: Range<usize>) -> Range<usize> {
        let point = |byte: usize, end: bool| {
            self.segments
                .iter()
                .find(|(generated, _)| {
                    if end {
                        generated.start < byte && byte <= generated.end
                    } else {
                        generated.contains(&byte)
                    }
                })
                .map_or(self.source.len(), |(generated, original)| {
                    if generated.len() == original.len() {
                        original.start + byte.saturating_sub(generated.start)
                    } else if end {
                        original.end
                    } else {
                        original.start
                    }
                })
        };
        point(range.start, false)..point(range.end, true)
    }
    pub fn apply(&self, mut patches: Vec<Patch>) -> String {
        patches = self.normalized(patches);
        replace(&self.source, &patches, 0..self.source.len())
    }
    fn normalized(&self, mut patches: Vec<Patch>) -> Vec<Patch> {
        // Editing a wiki link's display text must retain its navigation target.
        for (whole, label, target) in &self.wiki {
            let inner: Vec<_> = patches
                .iter()
                .filter(|p| label.start <= p.range.start && p.range.end <= label.end)
                .cloned()
                .collect();
            if !inner.is_empty()
                && !patches
                    .iter()
                    .any(|p| p.range.start <= whole.start && whole.end <= p.range.end)
            {
                let text = replace(&self.source, &inner, label.clone());
                patches.retain(|p| !(label.start <= p.range.start && p.range.end <= label.end));
                patches.push(Patch {
                    range: whole.clone(),
                    text: format!("[[{target}|{text}]]"),
                });
            }
        }
        patches.sort_by(|a, b| {
            a.range
                .start
                .cmp(&b.range.start)
                .then(b.range.end.cmp(&a.range.end))
        });
        let mut end = 0;
        patches.retain(|p| {
            if p.range.start < end {
                false
            } else {
                end = p.range.end;
                true
            }
        });
        patches
    }
    pub fn project(
        &self,
        patches: Vec<Patch>,
        range: Range<usize>,
        whole_lines: bool,
    ) -> (String, Range<usize>) {
        let mut range = self.range(range);
        if whole_lines {
            range.start = self.source[..range.start].rfind('\n').map_or(0, |i| i + 1);
            range.end = self.source[range.end..]
                .find('\n')
                .map_or(self.source.len(), |i| range.end + i);
        }
        let patches = self.normalized(patches);
        let point = |offset: usize, end: bool| {
            let mut delta = 0isize;
            for p in &patches {
                if offset < p.range.start {
                    break;
                }
                if offset < p.range.end || (offset == p.range.start && (!end || p.range.is_empty()))
                {
                    return p.range.start.saturating_add_signed(delta)
                        + if end { p.text.len() } else { 0 };
                }
                delta += p.text.len() as isize - p.range.len() as isize;
            }
            offset.saturating_add_signed(delta)
        };
        let projected = point(range.start, false)..point(range.end, true);
        (
            replace(&self.source, &patches, 0..self.source.len()),
            projected,
        )
    }
    pub fn insert_patch(&self, span: Range<usize>, value: &str) -> Patch {
        Patch {
            range: self.range(span),
            text: escape(value, &self.source),
        }
    }
    pub fn append_patch(&self, value: &str) -> Patch {
        let eol = if self.source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let prefix = if self.text.is_empty() || self.source.ends_with(&format!("{eol}{eol}")) {
            String::new()
        } else if self.source.ends_with(eol) {
            eol.into()
        } else {
            format!("{eol}{eol}")
        };
        Patch {
            range: self.source.len()..self.source.len(),
            text: format!("{prefix}{}", escape(value, &self.source)),
        }
    }
    fn unit_range(&self, unit: &Unit) -> Range<usize> {
        let mut range = self.range(unit.span.clone());
        if !unit.code
            && self.source[..range.start]
                .bytes()
                .rev()
                .take_while(|b| *b == b'\\')
                .count()
                % 2
                == 1
        {
            range.start -= 1;
        }
        range
    }
    pub fn inline(&self, units: &[Unit], changed: &[String]) -> Vec<Patch> {
        if units.iter().zip(changed).all(|(u, v)| &u.text == v) {
            return vec![];
        }
        let mut patches = vec![];
        for (unit, next) in units.iter().zip(changed) {
            if next != &unit.text {
                patches.push(Patch {
                    range: self.unit_range(unit),
                    text: if unit.code {
                        inline_code(next)
                    } else {
                        escape(next, &self.source)
                    },
                });
            }
        }
        let mut wrappers: Vec<_> = units
            .iter()
            .flat_map(|r| r.wrappers.iter().cloned())
            .collect();
        wrappers.sort_by(|a, b| {
            a.len()
                .cmp(&b.len())
                .then(a.start.cmp(&b.start))
                .then(a.end.cmp(&b.end))
        });
        wrappers.dedup();
        for wrapper in wrappers {
            let range = self.range(wrapper.clone());
            let members: Vec<_> = units
                .iter()
                .zip(changed)
                .filter(|(u, _)| wrapper.start <= u.span.start && u.span.end <= wrapper.end)
                .collect();
            if members.is_empty() {
                continue;
            }
            if members.iter().all(|(_, text)| text.is_empty()) {
                patches.retain(|p| !(range.start <= p.range.start && p.range.end <= range.end));
                patches.push(Patch {
                    range,
                    text: String::new(),
                });
                continue;
            }
            let raw = &self.source[range.clone()];
            if let Some(marker) = ["**", "__", "~~", "*", "_"]
                .into_iter()
                .find(|m| raw.starts_with(m) && raw.ends_with(m) && raw.len() >= m.len() * 2)
            {
                let rewritten = replace(&self.source, &patches, range.clone());
                let inner = &rewritten[marker.len()..rewritten.len() - marker.len()];
                let trimmed = inner.trim();
                let leading = inner.len() - inner.trim_start().len();
                let trailing = inner.trim_end().len();
                let text = if trimmed.is_empty() {
                    inner.into()
                } else {
                    let blank = if self.source.contains("\r\n") {
                        "\r\n\r\n"
                    } else {
                        "\n\n"
                    };
                    let paragraphs = trimmed
                        .split(blank)
                        .map(|part| {
                            let content = part.trim();
                            if content.is_empty() {
                                part.to_owned()
                            } else {
                                format!(
                                    "{}{marker}{content}{marker}{}",
                                    &part[..part.len() - part.trim_start().len()],
                                    &part[part.trim_end().len()..]
                                )
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(blank);
                    format!("{}{paragraphs}{}", &inner[..leading], &inner[trailing..])
                };
                if text != raw {
                    patches.retain(|p| !(range.start <= p.range.start && p.range.end <= range.end));
                    patches.push(Patch { range, text });
                }
            }
        }
        patches
    }
}
pub(super) fn values(units: &[Unit], value: &str) -> Vec<String> {
    let old: String = units.iter().map(|r| r.text.as_str()).collect();
    if old == value {
        return units.iter().map(|r| r.text.clone()).collect();
    }
    let before: Vec<_> = old.chars().collect();
    let after: Vec<_> = value.chars().collect();
    let prefix = before
        .iter()
        .zip(&after)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = before[prefix..]
        .iter()
        .rev()
        .zip(after[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let removed = prefix..before.len() - suffix;
    let inserted: String = after[prefix..after.len() - suffix].iter().collect();
    let mut offset = 0;
    let mut insertion_done = false;
    units
        .iter()
        .map(|unit| {
            let chars: Vec<_> = unit.text.chars().collect();
            let end = offset + chars.len();
            let start_remove = removed.start.saturating_sub(offset).min(chars.len());
            let end_remove = removed.end.saturating_sub(offset).min(chars.len());
            let insert = !insertion_done
                && offset <= prefix
                && (prefix < end || (removed.is_empty() && prefix == end));
            let mut next: String = chars[..start_remove].iter().collect();
            if insert {
                next.push_str(&inserted);
                insertion_done = true;
            }
            next.extend(chars[end_remove..].iter());
            offset = end;
            next
        })
        .collect()
}
fn replace(source: &str, patches: &[Patch], range: Range<usize>) -> String {
    let mut included: Vec<_> = patches
        .iter()
        .filter(|p| range.start <= p.range.start && p.range.end <= range.end)
        .collect();
    included.sort_by(|a, b| {
        a.range
            .start
            .cmp(&b.range.start)
            .then(b.range.end.cmp(&a.range.end))
    });
    let mut out = String::new();
    let mut cursor = range.start;
    for p in included {
        if p.range.start < cursor {
            continue;
        }
        out.push_str(&source[cursor..p.range.start]);
        out.push_str(&p.text);
        cursor = p.range.end;
    }
    out.push_str(&source[cursor..range.end]);
    out
}
fn escape(value: &str, source: &str) -> String {
    let mut out = String::new();
    for c in value.replace("\r\n", "\n").chars() {
        if "\\*_[]`~<>|#!&".contains(c) {
            out.push('\\');
        }
        if c == '\n' && source.contains("\r\n") {
            out.push('\r');
        }
        out.push(c);
    }
    out
}
fn inline_code(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let count = value.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1;
    let fence = "`".repeat(count);
    let pad = value.starts_with('`')
        || value.ends_with('`')
        || (value.starts_with(' ') && value.ends_with(' ') && !value.trim().is_empty());
    format!(
        "{fence}{}{value}{}{fence}",
        if pad { " " } else { "" },
        if pad { " " } else { "" }
    )
}
pub(super) fn code_patch(prepared: &Prepared, span: Range<usize>, text: &str) -> Patch {
    let range = prepared.range(span);
    let raw = &prepared.source[range.clone()];
    let eol = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines = raw.split_inclusive('\n');
    let first = lines.next().unwrap_or("");
    let trimmed = first.trim_start();
    let line_start = prepared.source[..range.start]
        .rfind('\n')
        .map_or(0, |i| i + 1);
    // The parser excludes the first line's container/indent prefix from its span.
    // Keep it outside the patch and repeat its continuation on subsequent lines.
    let continuation: String = prepared.source[line_start..range.start]
        .chars()
        .map(|c| {
            if c.is_whitespace() || c == '>' {
                c
            } else {
                ' '
            }
        })
        .collect();
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        let ch = trimmed.chars().next().unwrap();
        let count = trimmed.chars().take_while(|c| *c == ch).count();
        let longest = text.split(|c| c != ch).map(str::len).max().unwrap_or(0);
        let fence = ch.to_string().repeat(count.max(longest + 1));
        let language = trimmed[count..].trim_end_matches(['\r', '\n']);
        let indent = &first[..first.len() - trimmed.len()];
        let content = text
            .replace("\r\n", "\n")
            .split('\n')
            .map(|line| format!("{continuation}{indent}{line}"))
            .collect::<Vec<_>>()
            .join(eol);
        let ending = if raw.ends_with('\n') { eol } else { "" };
        Patch {
            range,
            text: format!(
                "{indent}{fence}{language}{eol}{content}{}{continuation}{indent}{fence}{ending}",
                if content.ends_with(eol) { "" } else { eol }
            ),
        }
    } else {
        let body = text
            .replace("\r\n", "\n")
            .split('\n')
            .enumerate()
            .map(|(i, s)| {
                if i == 0 {
                    s.to_owned()
                } else {
                    format!("{continuation}{s}")
                }
            })
            .collect::<Vec<_>>()
            .join(eol);
        Patch {
            range,
            text: format!("{body}{eol}"),
        }
    }
}
