//! Cached CommonMark blocks, readable typography, and source-line locations for scroll sync.
use amigo_ui::design::{self, colors};
use egui::{
    self, FontId, RichText, Stroke,
    text::{LayoutJob, TextFormat},
};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

#[derive(Clone, Default, Debug)]
struct Atom {
    text: String,
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: Option<String>,
}
#[derive(Clone, Default, Debug)]
struct Inline {
    atoms: Vec<Atom>,
}
#[derive(Debug)]
enum Kind {
    Text(u8, Inline),
    Code(String, String),
    Table(Vec<Alignment>, Vec<Vec<Inline>>),
    Rule,
}
#[derive(Debug)]
struct Block {
    kind: Kind,
    lines: Range<usize>,
    indent: usize,
    marker: Option<String>,
    quote: usize,
    anchor: Option<String>,
}
#[derive(Default)]
pub(super) struct Preview {
    blocks: Vec<Block>,
    pub locations: Vec<(Range<usize>, f32, f32)>,
    heights: Vec<f32>,
    geometry: Option<(u32, u32, bool)>,
}
#[derive(Default)]
struct Builder {
    inline: Inline,
    style: Atom,
    start: usize,
    end: usize,
    heading: u8,
    quote: usize,
    lists: Vec<Option<u64>>,
    marker: Option<String>,
    anchor: Option<String>,
}
impl Builder {
    fn append(&mut self, value: &str, start: usize, end: usize) {
        if self.inline.atoms.is_empty() {
            self.start = start;
        }
        self.end = end;
        self.inline.atoms.push(Atom {
            text: value.into(),
            ..self.style.clone()
        });
    }
    fn flush(&mut self, blocks: &mut Vec<Block>, starts: &[usize], base: usize) {
        if self.inline.atoms.is_empty() {
            return;
        }
        let start = base
            + starts
                .partition_point(|n| *n <= self.start)
                .saturating_sub(1);
        let end = base + starts.partition_point(|n| *n <= self.end).saturating_sub(1) + 1;
        blocks.push(Block {
            kind: Kind::Text(self.heading, std::mem::take(&mut self.inline)),
            lines: start..end,
            indent: self.lists.len().saturating_sub(1),
            marker: self.marker.take(),
            quote: self.quote,
            anchor: self.anchor.take(),
        });
    }
}
impl Preview {
    pub fn parse(source: &str, base_line: usize) -> Self {
        let mut starts = vec![0];
        starts.extend(source.match_indices('\n').map(|(i, _)| i + 1));
        let line_at = |byte| starts.partition_point(|n| *n <= byte).saturating_sub(1);
        let mut result = Self::default();
        let mut b = Builder::default();
        let mut events = Parser::new_ext(
            source,
            Options::ENABLE_HEADING_ATTRIBUTES
                | Options::ENABLE_TABLES
                | Options::ENABLE_STRIKETHROUGH
                | Options::ENABLE_TASKLISTS
                | Options::ENABLE_FOOTNOTES,
        )
        .into_offset_iter()
        .peekable();
        while let Some((event, span)) = events.next() {
            match event {
                Event::Start(Tag::Heading { level, id, .. }) => {
                    b.flush(&mut result.blocks, &starts, base_line);
                    b.heading = level as u8;
                    b.anchor = id.map(|s| s.into_string());
                }
                Event::End(TagEnd::Heading(_)) => {
                    b.flush(&mut result.blocks, &starts, base_line);
                    b.heading = 0;
                }
                Event::Start(Tag::Paragraph) => b.flush(&mut result.blocks, &starts, base_line),
                Event::End(TagEnd::Paragraph) | Event::End(TagEnd::Item) => {
                    b.flush(&mut result.blocks, &starts, base_line)
                }
                Event::Start(Tag::Strong) => b.style.bold = true,
                Event::End(TagEnd::Strong) => b.style.bold = false,
                Event::Start(Tag::Emphasis) => b.style.italic = true,
                Event::End(TagEnd::Emphasis) => b.style.italic = false,
                Event::Start(Tag::Strikethrough) => b.style.strike = true,
                Event::End(TagEnd::Strikethrough) => b.style.strike = false,
                Event::Start(Tag::Link { dest_url, .. }) => {
                    b.style.link = Some(dest_url.into_string())
                }
                Event::End(TagEnd::Link) => b.style.link = None,
                Event::Start(Tag::Image { dest_url, .. }) => {
                    b.append(&format!("[Image: {dest_url}] "), span.start, span.end)
                }
                Event::Text(text) => b.append(&text, span.start, span.end),
                Event::Code(text) => {
                    b.style.code = true;
                    b.append(&text, span.start, span.end);
                    b.style.code = false;
                }
                Event::SoftBreak => b.append(" ", span.start, span.end),
                Event::HardBreak => b.append("\n", span.start, span.end),
                Event::Start(Tag::List(start)) => {
                    b.flush(&mut result.blocks, &starts, base_line);
                    b.lists.push(start);
                }
                Event::End(TagEnd::List(_)) => {
                    b.flush(&mut result.blocks, &starts, base_line);
                    b.lists.pop();
                }
                Event::Start(Tag::Item) => {
                    b.flush(&mut result.blocks, &starts, base_line);
                    b.marker = Some(match b.lists.last_mut() {
                        Some(Some(n)) => {
                            let s = format!("{n}.");
                            *n += 1;
                            s
                        }
                        _ => "•".into(),
                    });
                }
                Event::TaskListMarker(checked) => {
                    b.marker = Some(if checked { "☑" } else { "☐" }.into())
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    b.flush(&mut result.blocks, &starts, base_line);
                    b.quote += 1;
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    b.flush(&mut result.blocks, &starts, base_line);
                    b.quote = b.quote.saturating_sub(1);
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    b.flush(&mut result.blocks, &starts, base_line);
                    let language = match kind {
                        CodeBlockKind::Fenced(lang) => lang.into_string(),
                        _ => String::new(),
                    };
                    let mut text = String::new();
                    let mut end = span.end;
                    for (event, span) in events.by_ref() {
                        end = span.end;
                        match event {
                            Event::End(TagEnd::CodeBlock) => break,
                            Event::Text(s) => text.push_str(&s),
                            _ => (),
                        }
                    }
                    let start = base_line + line_at(span.start);
                    let end = base_line + line_at(end) + 1;
                    result.blocks.push(Block {
                        kind: Kind::Code(language, text),
                        lines: start..end,
                        indent: b.lists.len().saturating_sub(1),
                        marker: b.marker.take(),
                        quote: b.quote,
                        anchor: None,
                    });
                }
                Event::Start(Tag::Table(align)) => {
                    b.flush(&mut result.blocks, &starts, base_line);
                    let mut rows = vec![];
                    let mut row = vec![];
                    let mut cell = Inline::default();
                    let mut style = Atom::default();
                    let mut end = span.end;
                    for (event, range) in events.by_ref() {
                        end = range.end;
                        match event {
                            Event::End(TagEnd::Table) => break,
                            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                                rows.push(std::mem::take(&mut row))
                            }
                            Event::End(TagEnd::TableCell) => row.push(std::mem::take(&mut cell)),
                            Event::Text(s) => cell.atoms.push(Atom {
                                text: s.into_string(),
                                ..style.clone()
                            }),
                            Event::Code(s) => cell.atoms.push(Atom {
                                text: s.into_string(),
                                code: true,
                                ..style.clone()
                            }),
                            Event::Start(Tag::Strong) => style.bold = true,
                            Event::End(TagEnd::Strong) => style.bold = false,
                            Event::Start(Tag::Emphasis) => style.italic = true,
                            Event::End(TagEnd::Emphasis) => style.italic = false,
                            Event::Start(Tag::Link { dest_url, .. }) => {
                                style.link = Some(dest_url.into_string())
                            }
                            Event::End(TagEnd::Link) => style.link = None,
                            Event::SoftBreak | Event::HardBreak => cell.atoms.push(Atom {
                                text: " ".into(),
                                ..style.clone()
                            }),
                            _ => (),
                        }
                    }
                    let start = base_line + line_at(span.start);
                    let end = base_line + line_at(end) + 1;
                    result.blocks.push(Block {
                        kind: Kind::Table(align, rows),
                        lines: start..end,
                        indent: b.lists.len().saturating_sub(1),
                        marker: b.marker.take(),
                        quote: b.quote,
                        anchor: None,
                    });
                }
                Event::Rule => {
                    b.flush(&mut result.blocks, &starts, base_line);
                    let line = base_line + line_at(span.start);
                    result.blocks.push(Block {
                        kind: Kind::Rule,
                        lines: line..line + 1,
                        indent: 0,
                        marker: None,
                        quote: b.quote,
                        anchor: None,
                    });
                }
                Event::Html(text) | Event::InlineHtml(text) => {
                    b.append(&text, span.start, span.end)
                }
                Event::FootnoteReference(text) => {
                    b.append(&format!("[{text}]"), span.start, span.end)
                }
                _ => (),
            }
        }
        b.flush(&mut result.blocks, &starts, base_line);
        result.heights = vec![0.; result.blocks.len()];
        result
    }
    pub fn heading_y(&self, target: &str) -> Option<f32> {
        self.blocks.iter().enumerate().find_map(|(i, block)| {
            if let Kind::Text(level, inline) = &block.kind
                && *level > 0
            {
                let text: String = inline.atoms.iter().map(|a| a.text.as_str()).collect();
                let slug: String = text
                    .to_lowercase()
                    .chars()
                    .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '-' || *c == '_')
                    .map(|c| if c.is_whitespace() { '-' } else { c })
                    .collect();
                if slug == target || block.anchor.as_deref() == Some(target) {
                    return self.locations.get(i).map(|(_, y, _)| *y + 20.);
                }
            }
            None
        })
    }
    pub fn y_for_line(&self, line: f32) -> f32 {
        for (range, y, height) in &self.locations {
            if line <= range.end as f32 {
                let fraction =
                    ((line - range.start as f32) / range.len().max(1) as f32).clamp(0., 1.);
                return y + height * fraction;
            }
        }
        self.locations.last().map_or(0., |(_, y, h)| y + h)
    }
    pub fn line_for_y(&self, y: f32) -> f32 {
        for (range, top, height) in &self.locations {
            if y <= top + height {
                return range.start as f32
                    + ((y - top) / height.max(1.)).clamp(0., 1.) * range.len() as f32;
            }
        }
        self.locations
            .last()
            .map_or(0., |(range, _, _)| range.end as f32)
    }
    pub fn show(&mut self, ui: &mut egui::Ui, viewport: egui::Rect, size: f32) -> Option<String> {
        let width = ui.available_width();
        let geometry = (width.to_bits(), size.to_bits(), ui.visuals().dark_mode);
        if self.geometry != Some(geometry) {
            self.heights.fill(0.);
            self.geometry = Some(geometry);
        }
        let origin = ui.cursor().top();
        self.locations.clear();
        let mut link = None;
        for (i, block) in self.blocks.iter().enumerate() {
            let top = ui.cursor().top() - origin;
            if self.heights[i] == 0. {
                self.heights[i] = estimated_height(block, width, size);
            }
            let known = self.heights[i];
            if known > 0. && (top + known < viewport.top() - 120. || top > viewport.bottom() + 120.)
            {
                ui.allocate_space(egui::vec2(width, known));
            } else {
                let start = ui.cursor().top();
                ui.push_id(i, |ui| {
                    link = render_block(block, ui, width, size).or(link.take());
                });
                self.heights[i] = ui.cursor().top() - start;
            }
            self.locations
                .push((block.lines.clone(), top, self.heights[i]));
        }
        link
    }
}
fn estimated_height(block: &Block, width: f32, size: f32) -> f32 {
    let available = (width
        - block.indent as f32 * 20.
        - block.quote as f32 * 16.
        - if block.marker.is_some() { 28. } else { 0. })
    .max(24.);
    match &block.kind {
        Kind::Text(level, inline) => {
            let scale = if *level > 0 { 1.7 } else { 1. };
            let chars: usize = inline.atoms.iter().map(|a| a.text.chars().count()).sum();
            ((chars as f32 * size * scale * 0.5 / available)
                .ceil()
                .max(1.)
                * size
                * scale
                * 1.65
                + if *level > 0 { 24. } else { 12. })
            .max(1.)
        }
        Kind::Code(_, text) => text.lines().count().max(1) as f32 * size * 1.5 + 56.,
        Kind::Table(_, rows) => rows.len().max(1) as f32 * (size * 1.65 + 10.) + 16.,
        Kind::Rule => 32.,
    }
}
fn render_block(block: &Block, ui: &mut egui::Ui, width: f32, size: f32) -> Option<String> {
    let p = colors(ui);
    let inset = block.indent as f32 * 20. + block.quote as f32 * 16.;
    let start = ui.cursor().top();
    let mut link = None;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.;
        ui.add_space(inset);
        let marker_width = if block.marker.is_some() { 28. } else { 0. };
        if let Some(marker) = &block.marker {
            ui.add_sized(
                [marker_width, size * 1.65],
                egui::Label::new(RichText::new(marker).size(size)).halign(egui::Align::Min),
            );
        }
        let width = (width - inset - marker_width).max(24.);
        ui.allocate_ui_with_layout(
            egui::vec2(width, 0.),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(width);
                match &block.kind {
                    Kind::Text(level, inline) => {
                        if *level > 0 {
                            ui.add_space(if start > ui.max_rect().top() + 8. {
                                14.
                            } else {
                                4.
                            });
                        }
                        let font_size = match level {
                            1 => size * 2.,
                            2 => size * 1.65,
                            3 => size * 1.35,
                            4 => size * 1.15,
                            _ => size,
                        };
                        link =
                            inline_label(ui, inline, width, font_size, *level > 0, block.quote > 0);
                        ui.add_space(if *level > 0 { 10. } else { 12. });
                    }
                    Kind::Code(language, text) => {
                        egui::Frame::new()
                            .fill(p.column)
                            .stroke(Stroke::new(1., p.line))
                            .corner_radius(4)
                            .inner_margin(12)
                            .show(ui, |ui| {
                                ui.set_width((width - 26.).max(24.));
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(if language.is_empty() {
                                            "text"
                                        } else {
                                            language
                                        })
                                        .size(11.)
                                        .color(p.muted),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if design::small_button(ui, "Copy").clicked() {
                                                ui.ctx().copy_text(text.clone());
                                            }
                                        },
                                    );
                                });
                                let mut text = text.trim_end_matches('\n');
                                egui::ScrollArea::horizontal()
                                    .id_salt("code-scroll")
                                    .max_width((width - 26.).max(24.))
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                                    )
                                    .show(ui, |ui| {
                                        let mut layout =
                                            |ui: &egui::Ui, text: &dyn egui::TextBuffer, _: f32| {
                                                let job = super::highlight::code_job(
                                                    ui,
                                                    text.as_str(),
                                                    size * 0.9,
                                                );
                                                ui.fonts_mut(|f| f.layout_job(job))
                                            };
                                        ui.add(
                                            egui::TextEdit::multiline(&mut text)
                                                .font(FontId::monospace(size * 0.9))
                                                .frame(egui::Frame::NONE)
                                                .desired_width(f32::INFINITY)
                                                .desired_rows(1)
                                                .layouter(&mut layout),
                                        );
                                    });
                            });
                        ui.add_space(14.);
                    }
                    Kind::Table(align, rows) => {
                        let cols = align.len().max(1);
                        let cell_width =
                            ((width - (cols as f32 - 1.) * 16.) / cols as f32).max(80.);
                        egui::ScrollArea::horizontal()
                            .id_salt("table-scroll")
                            .max_width(width)
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                            )
                            .show(ui, |ui| {
                                egui::Grid::new("preview-table")
                                    .spacing([16., 10.])
                                    .min_col_width(cell_width)
                                    .max_col_width(cell_width)
                                    .striped(true)
                                    .show(ui, |ui| {
                                        for (index, row) in rows.iter().enumerate() {
                                            for cell in row {
                                                ui.allocate_ui_with_layout(
                                                    egui::vec2(cell_width, 0.),
                                                    egui::Layout::top_down(egui::Align::Min),
                                                    |ui| {
                                                        link = inline_label(
                                                            ui,
                                                            cell,
                                                            cell_width,
                                                            size,
                                                            index == 0,
                                                            false,
                                                        )
                                                        .or(link.take());
                                                    },
                                                );
                                            }
                                            ui.end_row();
                                        }
                                    });
                            });
                        ui.add_space(16.);
                    }
                    Kind::Rule => {
                        ui.add_space(8.);
                        ui.separator();
                        ui.add_space(16.);
                    }
                }
            },
        );
    });
    if block.quote > 0 {
        ui.painter().line_segment(
            [
                egui::pos2(ui.max_rect().left() + 3., start),
                egui::pos2(ui.max_rect().left() + 3., ui.cursor().top() - 10.),
            ],
            Stroke::new(3., p.line),
        );
    }
    link
}
fn inline_label(
    ui: &mut egui::Ui,
    inline: &Inline,
    width: f32,
    size: f32,
    bold: bool,
    quote: bool,
) -> Option<String> {
    let p = colors(ui);
    let mut job = LayoutJob::default();
    let mut links = vec![];
    for atom in &inline.atoms {
        let mut format = TextFormat {
            font_id: if atom.code {
                FontId::monospace(size * 0.9)
            } else {
                FontId::proportional(size)
            },
            color: if atom.link.is_some() {
                p.accent
            } else if quote {
                p.muted
            } else {
                p.ink
            },
            line_height: Some(size * 1.65),
            italics: atom.italic,
            ..Default::default()
        };
        if atom.bold || bold {
            format.coords = egui::epaint::text::VariationCoords::new([(b"wght", 700.)]);
        }
        if atom.code {
            format.background = p.column;
        }
        if atom.strike {
            format.strikethrough = Stroke::new(1., p.ink);
        }
        if atom.link.is_some() {
            format.underline = Stroke::new(0.7, p.accent);
        }
        let start = job.text.len();
        job.append(&atom.text, 0., format);
        if let Some(link) = &atom.link {
            links.push((start..job.text.len(), link));
        }
    }
    job.wrap.max_width = width;
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let response = ui.add(
        egui::Label::new(galley.clone())
            .selectable(true)
            .sense(egui::Sense::click()),
    );
    if let Some(pos) = response.hover_pos() {
        let char_index = galley.cursor_from_pos(pos - response.rect.min).index.0;
        let byte = galley
            .text()
            .char_indices()
            .nth(char_index)
            .map_or(galley.text().len(), |(i, _)| i);
        if let Some((_, link)) = links.iter().find(|(range, _)| range.contains(&byte)) {
            let clicked = response.clicked();
            response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(*link);
            if clicked {
                return Some((*link).clone());
            }
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commonmark_blocks_keep_lists_unicode_links_formatting_and_source_locations() {
        let source = "# Title\n\n**Bold** _italic_ ~~strike~~ `code` [note](Note.md) 🌍\n\n- [x] Done\n- [ ] Next\n\n> Quote\n\n```rust\nlet n = 1;\n```\n\n| A | B |\n| --- | --- |\n| one | two |\n";
        let preview = Preview::parse(source, 4);
        assert!(
            preview
                .blocks
                .iter()
                .any(|b| matches!(&b.kind, Kind::Text(1, _)))
        );
        assert!(
            preview
                .blocks
                .iter()
                .any(|b| b.marker.as_deref() == Some("☑"))
        );
        assert!(preview.blocks.iter().any(|b| b.quote > 0));
        assert!(preview.blocks.iter().any(|b| matches!(&b.kind, Kind::Code(lang, text) if lang == "rust" && text.contains("let n"))));
        assert!(
            preview
                .blocks
                .iter()
                .any(|b| matches!(&b.kind, Kind::Table(_, rows) if rows.len() == 2))
        );
        assert_eq!(preview.blocks[0].lines.start, 4);
        if let Kind::Text(_, inline) = &preview.blocks[1].kind {
            assert!(inline.atoms.iter().any(|a| a.bold));
            assert!(inline.atoms.iter().any(|a| a.italic));
            assert!(
                inline
                    .atoms
                    .iter()
                    .any(|a| a.link.as_deref() == Some("Note.md"))
            );
        } else {
            panic!("Missing paragraph");
        }
    }
}
