//! Source highlighting keeps literal Markdown, including front matter and whitespace, untouched.
use amigo_ui::design::colors;
use egui::{
    self, FontId,
    text::{LayoutJob, TextFormat},
};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
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
            Event::Start(Tag::CodeBlock(_)) => code_block = true,
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
pub(super) fn code_job(ui: &egui::Ui, text: &str, size: f32) -> LayoutJob {
    let p = colors(ui);
    let mut job = LayoutJob::default();
    let mut cursor = 0;
    while cursor < text.len() {
        let rest = &text[cursor..];
        let first = rest.chars().next().unwrap();
        let (length, ink) = if rest.starts_with("//") || first == '#' {
            (rest.find('\n').unwrap_or(rest.len()), p.muted)
        } else if first == '\'' || first == '"' {
            let mut escaped = false;
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
            let keyword = [
                "let", "fn", "mut", "const", "class", "def", "return", "if", "else", "for",
                "while", "true", "false", "null", "None", "import", "use", "from", "pub", "struct",
                "enum", "impl", "Self", "match", "Some", "Ok", "async", "await", "try", "catch",
                "function", "var",
            ]
            .contains(&&rest[..length]);
            (length, if keyword { p.violet } else { p.ink })
        } else {
            (first.len_utf8(), p.ink)
        };
        job.append(
            &rest[..length],
            0.,
            TextFormat {
                font_id: FontId::monospace(size),
                color: ink,
                line_height: Some(size * 1.6),
                ..Default::default()
            },
        );
        cursor += length;
    }
    job.wrap.max_width = f32::INFINITY;
    job
}
#[cfg(test)]
mod tests {
    use super::*;
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
