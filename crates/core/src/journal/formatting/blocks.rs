use super::*;

pub(super) struct Prefix {
    pub indent: usize,
    pub quote_end: usize,
    pub end: usize,
    pub kind: Option<Format>,
}
pub(super) fn prefix(line: &str) -> Prefix {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let mut end = indent;
    while line[end..].starts_with('>') {
        end += 1;
        if line[end..].starts_with(' ') {
            end += 1;
        }
    }
    let quote_end = end;
    let rest = &line[end..];
    let hashes = rest.bytes().take_while(|b| *b == b'#').count();
    let (kind, width) = if (1..=6).contains(&hashes)
        && (rest.len() == hashes || rest[hashes..].starts_with([' ', '\t']))
    {
        (Some(Format::Heading(hashes as u8)), hashes)
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let (kind, width) = if rest.starts_with(['-', '+', '*']) {
            (Some(Format::Bullet), 1)
        } else if (1..=9).contains(&digits) && rest[digits..].starts_with(['.', ')']) {
            (Some(Format::Numbered), digits + 1)
        } else {
            (None, 0)
        };
        if width > 0 && (rest.len() == width || rest[width..].starts_with([' ', '\t'])) {
            (kind, width)
        } else {
            (None, 0)
        }
    };
    end += width;
    if kind.is_some() {
        end += line[end..].len() - line[end..].trim_start_matches([' ', '\t']).len();
    }
    let mut kind = kind;
    if matches!(kind, Some(Format::Bullet | Format::Numbered)) {
        let rest = &line[end..];
        if ["[ ]", "[x]", "[X]"].iter().any(|p| rest.starts_with(p))
            && (rest.len() == 3 || rest[3..].starts_with([' ', '\t']))
        {
            kind = Some(Format::Task);
            end += 3;
            end += line[end..].len() - line[end..].trim_start_matches([' ', '\t']).len();
        }
    }
    Prefix {
        indent,
        quote_end,
        end,
        kind,
    }
}

pub(super) fn lines(text: &str, selection: &Range<usize>) -> Range<usize> {
    let start = text[..selection.start].rfind('\n').map_or(0, |i| i + 1);
    let end = if selection.end > start && text[..selection.end].ends_with('\n') {
        selection.end - 1
    } else {
        text[selection.end..]
            .find('\n')
            .map_or(text.len(), |i| selection.end + i)
    };
    start..end
}

pub(super) fn apply(text: &mut String, selection: Range<usize>, format: Format) -> Range<usize> {
    let range = lines(text, &selection);
    let syntax = syntax::Syntax::new(text);
    let mut offset = range.start;
    let rows: Vec<_> = text[range.clone()]
        .split_inclusive('\n')
        .chain(range.is_empty().then_some(""))
        .map(|line| {
            let start = offset;
            offset += line.len();
            let body = line.trim_end_matches(['\r', '\n']);
            let eligible = (!body.trim().is_empty() || selection.is_empty())
                && !syntax.literal_at(&(start..offset));
            (line, body, prefix(body), eligible)
        })
        .collect();
    if !rows.iter().any(|r| r.3) {
        return selection;
    }
    let toggle = !matches!(format, Format::Heading(_))
        && rows.iter().filter(|r| r.3).all(|r| {
            if format == Format::Quote {
                r.2.quote_end > r.2.indent
            } else {
                r.2.kind == Some(format)
            }
        });
    let mut replacement = String::new();
    let mut number = 0;
    for (line, body, p, eligible) in rows {
        if !eligible {
            replacement.push_str(line);
            continue;
        }
        number += 1;
        if format == Format::Quote {
            if toggle {
                let first = p.indent + 1 + usize::from(body[p.indent + 1..].starts_with(' '));
                replacement.push_str(&body[..p.indent]);
                replacement.push_str(&body[first..]);
            } else {
                replacement.push_str(&body[..p.indent]);
                if p.quote_end == p.indent {
                    replacement.push_str("> ");
                }
                replacement.push_str(&body[p.indent..]);
            }
        } else {
            replacement.push_str(&body[..p.quote_end]);
            let marker = match format {
                Format::Heading(0) => String::new(),
                Format::Heading(n) => format!("{} ", "#".repeat(n.clamp(1, 6) as usize)),
                _ if toggle => String::new(),
                Format::Bullet => "- ".into(),
                Format::Numbered => format!("{number}. "),
                Format::Task if p.kind == Some(Format::Task) => body[p.quote_end..p.end].into(),
                Format::Task => "- [ ] ".into(),
                _ => unreachable!(),
            };
            replacement.push_str(&marker);
            // Normal removes a heading only; list and quote structure stays intact.
            let content =
                if format == Format::Heading(0) && !matches!(p.kind, Some(Format::Heading(_))) {
                    p.quote_end
                } else {
                    p.end
                };
            let mut content = &body[content..];
            if matches!(p.kind, Some(Format::Heading(_))) {
                let trimmed = content.trim_end();
                let end = trimmed.trim_end_matches('#');
                if end.len() < trimmed.len() && end.ends_with([' ', '\t']) {
                    content = end.trim_end();
                }
            }
            replacement.push_str(content);
        }
        replacement.push_str(&line[body.len()..]);
    }
    let end = range.start + replacement.len();
    text.replace_range(range.clone(), &replacement);
    range.start..end
}
