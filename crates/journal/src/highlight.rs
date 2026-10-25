//! Source highlighting keeps literal Markdown, including front matter and whitespace, untouched.
use amigo_ui::design::colors;
use egui::{
    self, FontId,
    text::{LayoutJob, TextFormat},
};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::sync::Arc;
#[derive(Default)]
pub(super) struct SourceCache {
    galley: Option<Arc<egui::Galley>>,
    key: Option<SourceKey>,
}
#[derive(Clone, Copy, PartialEq)]
struct SourceKey {
    width: u32,
    size: u32,
    scale: u32,
    colors: [egui::Color32; 4],
}
impl SourceCache {
    pub fn layout(
        &mut self,
        ui: &egui::Ui,
        text: &str,
        width: f32,
        size: f32,
    ) -> Arc<egui::Galley> {
        let palette = colors(ui);
        let key = SourceKey {
            width: width.to_bits(),
            size: size.to_bits(),
            scale: ui.ctx().pixels_per_point().to_bits(),
            colors: [palette.ink, palette.accent, palette.violet, palette.green],
        };
        if self.key == Some(key)
            && let Some(galley) = &self.galley
            && galley.text() == text
        {
            return galley.clone();
        }
        let job = markdown_job(ui, text, width, size);
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        self.key = Some(key);
        self.galley = Some(galley.clone());
        galley
    }
}
pub(super) fn markdown_job(ui: &egui::Ui, text: &str, width: f32, size: f32) -> LayoutJob {
    let p = colors(ui);
    let base = TextFormat {
        font_id: FontId::monospace(size),
        color: p.accent,
        line_height: Some(size * 1.55),
        ..Default::default()
    };
    let mut job = LayoutJob::default();
    let mut heading = false;
    let mut bold = false;
    let mut italic = false;
    let mut code_block = false;
    let mut language = String::new();
    let mut link = false;
    let mut strike = false;
    let mut cursor = 0;
    for (event, range) in Parser::new_ext(text, Options::all()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { .. }) => heading = true,
            Event::End(TagEnd::Heading(_)) => heading = false,
            Event::Start(Tag::Strong) => bold = true,
            Event::End(TagEnd::Strong) => bold = false,
            Event::Start(Tag::Emphasis) => italic = true,
            Event::End(TagEnd::Emphasis) => italic = false,
            Event::Start(Tag::CodeBlock(kind)) => {
                code_block = true;
                language = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().unwrap_or("").into()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
            }
            Event::End(TagEnd::CodeBlock) => code_block = false,
            Event::Start(Tag::Link { .. }) => link = true,
            Event::End(TagEnd::Link) => link = false,
            Event::Start(Tag::Strikethrough) => strike = true,
            Event::End(TagEnd::Strikethrough) => strike = false,
            Event::Text(_) | Event::Code(_) | Event::Html(_) | Event::InlineHtml(_) => {
                if range.start < cursor {
                    continue;
                }
                job.append(&text[cursor..range.start], 0., base.clone());
                if code_block {
                    let code = code_job(ui, &text[range.clone()], size, &language);
                    for section in code.sections {
                        job.append(
                            &code.text[section.byte_range.start.0..section.byte_range.end.0],
                            0.,
                            TextFormat {
                                line_height: base.line_height,
                                ..section.format
                            },
                        );
                    }
                    cursor = range.end;
                    continue;
                }
                let code = code_block || matches!(event, Event::Code(_));
                let mut format = TextFormat {
                    color: if code {
                        p.green
                    } else if heading {
                        p.violet
                    } else if link {
                        p.accent
                    } else {
                        p.ink
                    },
                    italics: italic || heading,
                    ..base.clone()
                };
                if bold {
                    format.coords = egui::epaint::text::VariationCoords::new([(b"wght", 700.)]);
                }
                if strike {
                    format.strikethrough = egui::Stroke::new(1., p.ink);
                }
                job.append(&text[range.clone()], 0., format);
                cursor = range.end;
            }
            _ => (),
        }
    }
    job.append(&text[cursor..], 0., base);
    job.wrap.max_width = width;
    job
}
pub(super) fn code_job(ui: &egui::Ui, text: &str, size: f32, language: &str) -> LayoutJob {
    let p = colors(ui);
    let language = language
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let (comments, block_comment, keywords): (&[&str], Option<(&str, &str)>, &str) = match language
        .as_str()
    {
        "rust" | "rs" => (
            &["//"],
            Some(("/*", "*/")),
            "as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while Some None Ok Err",
        ),
        "python" | "py" => (
            &["#"],
            None,
            "and as assert async await break class continue def del elif else except False finally for from global if import in is lambda None nonlocal not or pass raise return True try while with yield",
        ),
        "javascript" | "js" | "typescript" | "ts" | "jsx" | "tsx" => (
            &["//"],
            Some(("/*", "*/")),
            "async await break case catch class const continue debugger default delete do else enum export extends false finally for from function if implements import in instanceof interface let new null of private protected public return static super switch this throw true try type typeof undefined var void while yield",
        ),
        "c" | "cpp" | "c++" | "java" | "go" | "golang" => (
            &["//"],
            Some(("/*", "*/")),
            "auto bool boolean break byte case char class const continue default defer do double else enum extends extern false final float for func goto if implements import int interface long map namespace new nil null package private protected public range return select short signed sizeof static string struct super switch template this throw throws true try typedef union unsigned using var virtual void volatile while",
        ),
        "bash" | "sh" | "shell" | "zsh" => (
            &["#"],
            None,
            "case do done elif else esac export fi for function if in local readonly return then unset until while",
        ),
        "sql" => (
            &["--"],
            Some(("/*", "*/")),
            "SELECT FROM WHERE INSERT INTO VALUES UPDATE SET DELETE CREATE ALTER DROP TABLE JOIN LEFT RIGHT INNER OUTER ON AS AND OR NOT NULL TRUE FALSE GROUP BY ORDER HAVING LIMIT OFFSET DISTINCT UNION WITH CASE WHEN THEN ELSE END IN IS ASC DESC",
        ),
        "json" => (&[], None, "true false null"),
        "yaml" | "yml" => (&["#"], None, "true false null yes no on off"),
        "html" | "xml" => (&[], Some(("<!--", "-->")), ""),
        "css" => (
            &[],
            Some(("/*", "*/")),
            "important inherit initial unset none auto solid flex grid block inline relative absolute fixed",
        ),
        _ => (&[], None, ""),
    };
    let supported = !keywords.is_empty() || block_comment.is_some();
    let mut job = LayoutJob::default();
    let mut cursor = 0;
    while cursor < text.len() {
        let rest = &text[cursor..];
        let first = rest.chars().next().unwrap();
        let (length, ink) = if !supported {
            (rest.len(), p.ink)
        } else if comments.iter().any(|prefix| rest.starts_with(prefix)) {
            (rest.find('\n').unwrap_or(rest.len()), p.muted)
        } else if let Some((open, close)) = block_comment.filter(|(open, _)| rest.starts_with(open))
        {
            (
                rest[open.len()..]
                    .find(close)
                    .map_or(rest.len(), |end| open.len() + end + close.len()),
                p.muted,
            )
        } else if first == '\''
            || first == '"'
            || (matches!(
                language.as_str(),
                "javascript" | "js" | "typescript" | "ts" | "bash" | "sh"
            ) && first == '`')
        {
            let mut escaped = false;
            let triple = matches!(language.as_str(), "python" | "py")
                && rest.starts_with(&first.to_string().repeat(3));
            if triple {
                let delimiter = first.to_string().repeat(3);
                let end = rest[3..].find(&delimiter).map_or(rest.len(), |i| i + 6);
                job.append(&rest[..end], 0., code_format(size, p.green));
                cursor += end;
                continue;
            }
            let mut end = rest.len();
            for (i, c) in rest.char_indices().skip(1) {
                if c == first && !escaped {
                    end = i + c.len_utf8();
                    break;
                }
                escaped = c == '\\' && !escaped;
            }
            (end, p.green)
        } else if first.is_ascii_digit() {
            (
                rest.char_indices()
                    .find_map(|(i, c)| (!c.is_ascii_alphanumeric() && c != '.').then_some(i))
                    .unwrap_or(rest.len()),
                p.amber,
            )
        } else if first.is_alphabetic() || first == '_' {
            let length = rest
                .char_indices()
                .find_map(|(i, c)| (!c.is_alphanumeric() && c != '_').then_some(i))
                .unwrap_or(rest.len());
            let keyword = keywords.split_whitespace().any(|word| {
                if language == "sql" {
                    word.eq_ignore_ascii_case(&rest[..length])
                } else {
                    word == &rest[..length]
                }
            });
            (length, if keyword { p.violet } else { p.ink })
        } else {
            (first.len_utf8(), p.ink)
        };
        job.append(&rest[..length], 0., code_format(size, ink));
        cursor += length;
    }
    job.wrap.max_width = f32::INFINITY;
    job
}
fn code_format(size: f32, color: egui::Color32) -> TextFormat {
    TextFormat {
        font_id: FontId::monospace(size),
        color,
        line_height: Some(size * 1.6),
        ..Default::default()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn color_at(job: &LayoutJob, byte: usize) -> egui::Color32 {
        job.sections
            .iter()
            .find(|s| s.byte_range.start.0 <= byte && s.byte_range.end.0 > byte)
            .unwrap()
            .format
            .color
    }
    #[test]
    fn code_highlighting_uses_fence_languages_and_keeps_markdown_inside_code_literal() {
        let ctx = egui::Context::default();
        amigo_ui::design::install(&ctx);
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let palette = colors(ui);
            let code = "def f():\n    # 🌍 **literal**\n    return 1\n";
            let python = code_job(ui, code, 14., "python");
            let rust = code_job(ui, code, 14., "rust");
            assert_eq!(python.text, code);
            assert_eq!(color_at(&python, 0), palette.violet);
            assert_eq!(color_at(&rust, 0), palette.ink);
            assert_eq!(color_at(&python, code.find('#').unwrap()), palette.muted);
            assert_eq!(color_at(&rust, code.find('#').unwrap()), palette.ink);
            for language in ["", "text", "unknown"] {
                let plain = code_job(ui, code, 14., language);
                assert_eq!(plain.text, code);
                assert!(plain.sections.iter().all(|s| s.format.color == palette.ink));
            }
            let source = format!("```python\n{code}```\n# Outside\n");
            let job = markdown_job(ui, &source, 600., 14.);
            assert_eq!(job.text, source);
            assert_eq!(color_at(&job, source.find("def").unwrap()), palette.violet);
            assert_eq!(
                color_at(&job, source.find("**literal**").unwrap()),
                palette.muted
            );
            assert_eq!(
                color_at(&job, source.find("Outside").unwrap()),
                palette.violet
            );
            let sql = code_job(ui, "select * from notes -- comment", 14., "sql");
            assert_eq!(color_at(&sql, 0), palette.violet);
            assert_eq!(color_at(&sql, sql.text.find("--").unwrap()), palette.muted);
        });
        output.textures_delta.clear();
    }
    #[test]
    fn highlighting_retains_unicode_crlf_front_matter_and_markdown_markers() {
        let text = "---\r\ntitle: 🌍\r\n---\r\n# Heading\n\n**bold** and `code` [note](Note.md)\n```rust\nlet x = 1;\n```\n";
        let ctx = egui::Context::default();
        amigo_ui::design::install(&ctx);
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let job = markdown_job(ui, text, 600., 14.);
            assert_eq!(job.text, text);
            let colors: std::collections::HashSet<_> =
                job.sections.iter().map(|s| s.format.color).collect();
            assert!(colors.len() >= 4);
        });
        output.textures_delta.clear();
    }
}
