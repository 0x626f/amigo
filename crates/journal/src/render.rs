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
    original: String,
    span: Range<usize>,
    wrappers: Vec<Range<usize>>,
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: Option<String>,
}
#[derive(Clone, Default, Debug)]
struct Inline {
    atoms: Vec<Atom>,
    value: String,
    anchor: Range<usize>,
    restore: Option<Range<usize>>,
    needs_space: bool,
}
struct InlineStyle {
    size: f32,
    bold: bool,
    quote: bool,
    multiline: bool,
    list: bool,
}
#[derive(Debug)]
enum Kind {
    Text(u8, Inline),
    Code(String, String),
    Table(Vec<Alignment>, Vec<Vec<Inline>>),
    Rule,
    Media(String, Inline),
}
#[derive(Debug)]
struct Block {
    extra_inset: f32,
    span: Range<usize>,
    code_original: String,
    task: Option<(Range<usize>, bool, bool)>,
    removed: bool,
    edit_ids: Vec<egui::Id>,
    restore: Option<Range<usize>>,
    kind: Kind,
    lines: Range<usize>,
    indent: usize,
    marker: Option<String>,
    quote: usize,
    anchor: Option<String>,
}
#[derive(Default)]
pub(super) struct Preview {
    prepared: super::editing::Prepared,
    changed: bool,
    append: String,
    focused: Vec<egui::Id>,
    blocks: Vec<Block>,
    pub locations: Vec<(Range<usize>, f32, f32)>,
    heights: Vec<f32>,
    geometry: Option<(u32, u32, bool)>,
    pub inputs: Vec<super::preview_format::Input>,
    append_restore: Option<Range<usize>>,
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
    task: Option<(Range<usize>, bool, bool)>,
    empty_task: bool,
}
impl Builder {
    fn append(&mut self, value: &str, start: usize, end: usize) {
        if self.inline.atoms.is_empty() {
            self.start = start;
        }
        self.end = end;
        self.inline.atoms.push(Atom {
            text: value.into(),
            span: start..end,
            ..self.style.clone()
        });
    }
    fn flush(&mut self, blocks: &mut Vec<Block>, starts: &[usize], base: usize) {
        if self.inline.atoms.is_empty() && self.marker.is_none() {
            return;
        }
        if self.inline.atoms.is_empty() {
            self.inline.anchor = self.end..self.end;
        }
        let start = base
            + starts
                .partition_point(|n| *n <= self.start)
                .saturating_sub(1);
        let end = base + starts.partition_point(|n| *n <= self.end).saturating_sub(1) + 1;
        self.inline.value = self.inline.atoms.iter().map(|a| a.text.as_str()).collect();
        blocks.push(Block {
            extra_inset: 0.,
            span: self.start..self.end,
            code_original: String::new(),
            task: self.task.take(),
            removed: false,
            edit_ids: vec![],
            restore: None,
            kind: Kind::Text(self.heading, std::mem::take(&mut self.inline)),
            lines: start..end,
            indent: self.lists.len().saturating_sub(1),
            marker: self.marker.take(),
            quote: self.quote,
            anchor: self.anchor.take(),
        });
        self.empty_task = false;
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
                    if !b.inline.atoms.is_empty() {
                        b.flush(&mut result.blocks, &starts, base_line);
                    }
                    b.heading = level as u8;
                    b.anchor = id.map(|s| s.into_string());
                }
                Event::End(TagEnd::Heading(_)) => {
                    b.flush(&mut result.blocks, &starts, base_line);
                    b.heading = 0;
                }
                Event::Start(Tag::Paragraph) => {
                    if !b.inline.atoms.is_empty() {
                        b.flush(&mut result.blocks, &starts, base_line);
                    }
                }
                Event::End(TagEnd::Paragraph) | Event::End(TagEnd::Item) => {
                    b.flush(&mut result.blocks, &starts, base_line)
                }
                Event::Start(Tag::Strong) => {
                    b.style.bold = true;
                    b.style.wrappers.push(span.clone());
                }
                Event::End(TagEnd::Strong) => {
                    b.style.bold = false;
                    b.style.wrappers.pop();
                }
                Event::Start(Tag::Emphasis) => {
                    b.style.italic = true;
                    b.style.wrappers.push(span.clone());
                }
                Event::End(TagEnd::Emphasis) => {
                    b.style.italic = false;
                    b.style.wrappers.pop();
                }
                Event::Start(Tag::Strikethrough) => {
                    b.style.strike = true;
                    b.style.wrappers.push(span.clone());
                }
                Event::End(TagEnd::Strikethrough) => {
                    b.style.strike = false;
                    b.style.wrappers.pop();
                }
                Event::Start(Tag::Link { dest_url, .. }) => {
                    b.style.link = Some(dest_url.into_string());
                    b.style.wrappers.push(span.clone());
                }
                Event::End(TagEnd::Link) => {
                    b.style.link = None;
                    b.style.wrappers.pop();
                }
                Event::Start(Tag::Image { dest_url, .. }) => {
                    if !b.inline.atoms.is_empty() {
                        b.flush(&mut result.blocks, &starts, base_line);
                    }
                    let mut alt = Inline::default();
                    let mut end = span.end;
                    for (event, range) in events.by_ref() {
                        end = range.end;
                        match event {
                            Event::End(TagEnd::Image) => break,
                            Event::Text(text) | Event::Code(text) => {
                                alt.value.push_str(&text);
                                alt.atoms.push(Atom {
                                    text: text.into_string(),
                                    span: range,
                                    wrappers: vec![span.clone()],
                                    ..Default::default()
                                });
                            }
                            _ => (),
                        }
                    }
                    result.blocks.push(Block {
                        extra_inset: 0.,
                        span: span.start..end,
                        code_original: String::new(),
                        task: b.task.take(),
                        removed: false,
                        edit_ids: vec![],
                        restore: None,
                        kind: Kind::Media(dest_url.into_string(), alt),
                        lines: base_line + line_at(span.start)..base_line + line_at(end) + 1,
                        indent: b.lists.len().saturating_sub(1),
                        marker: b.marker.take(),
                        quote: b.quote,
                        anchor: None,
                    });
                }
                Event::End(TagEnd::Image) => {
                    b.style.link = None;
                    b.style.wrappers.pop();
                }
                Event::Text(text) if !b.empty_task => b.append(&text, span.start, span.end),
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
                    b.start = span.start;
                    b.end = source[span.start..span.end]
                        .find('\n')
                        .map_or(span.end, |i| span.start + i);
                    if source[..b.end].ends_with('\r') {
                        b.end -= 1;
                    }
                    b.marker = Some(match b.lists.last_mut() {
                        Some(Some(n)) => {
                            let s = format!("{n}.");
                            *n += 1;
                            s
                        }
                        _ => "•".into(),
                    });
                    let line = &source[b.start..b.end];
                    b.inline.needs_space = !line.ends_with([' ', '\t']);
                    if let Some(start) = line.find('[') {
                        let task = line[start..].trim();
                        if matches!(task, "[ ]" | "[x]" | "[X]") {
                            let checked = task != "[ ]";
                            b.task = Some((b.start + start..b.start + start + 3, checked, checked));
                            b.marker = Some(if checked { "☑" } else { "☐" }.into());
                            b.empty_task = true;
                        }
                    }
                }
                Event::TaskListMarker(checked) => {
                    b.task = Some((span.clone(), checked, checked));
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
                    if !b.inline.atoms.is_empty() {
                        b.flush(&mut result.blocks, &starts, base_line);
                    }
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
                    let byte_end = end;
                    let start = base_line + line_at(span.start);
                    let end = base_line + line_at(end) + 1;
                    let original = text.trim_end_matches(['\r', '\n']).to_owned();
                    result.blocks.push(Block {
                        extra_inset: 0.,
                        span: span.start..byte_end,
                        code_original: original.clone(),
                        task: b.task.take(),
                        removed: false,
                        edit_ids: vec![],
                        restore: None,
                        kind: Kind::Code(language, original),
                        lines: start..end,
                        indent: b.lists.len().saturating_sub(1),
                        marker: b.marker.take(),
                        quote: b.quote,
                        anchor: None,
                    });
                }
                Event::Start(Tag::Table(align)) => {
                    if !b.inline.atoms.is_empty() {
                        b.flush(&mut result.blocks, &starts, base_line);
                    }
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
                            Event::Start(Tag::TableCell) => cell.anchor = range.start..range.start,
                            Event::End(TagEnd::TableCell) => {
                                cell.value = cell.atoms.iter().map(|a| a.text.as_str()).collect();
                                row.push(std::mem::take(&mut cell));
                            }
                            Event::Text(s) => cell.atoms.push(Atom {
                                text: s.into_string(),
                                span: range.clone(),
                                ..style.clone()
                            }),
                            Event::Code(s) => cell.atoms.push(Atom {
                                text: s.into_string(),
                                span: range.clone(),
                                code: true,
                                ..style.clone()
                            }),
                            Event::Start(Tag::Strong) => {
                                style.bold = true;
                                style.wrappers.push(range.clone());
                            }
                            Event::End(TagEnd::Strong) => {
                                style.bold = false;
                                style.wrappers.pop();
                            }
                            Event::Start(Tag::Emphasis) => {
                                style.italic = true;
                                style.wrappers.push(range.clone());
                            }
                            Event::End(TagEnd::Emphasis) => {
                                style.italic = false;
                                style.wrappers.pop();
                            }
                            Event::Start(Tag::Link { dest_url, .. }) => {
                                style.link = Some(dest_url.into_string());
                                style.wrappers.push(range.clone());
                            }
                            Event::End(TagEnd::Link) => {
                                style.link = None;
                                style.wrappers.pop();
                            }
                            Event::SoftBreak | Event::HardBreak => cell.atoms.push(Atom {
                                text: " ".into(),
                                span: range.clone(),
                                ..style.clone()
                            }),
                            _ => (),
                        }
                    }
                    let start = base_line + line_at(span.start);
                    let end = base_line + line_at(end) + 1;
                    result.blocks.push(Block {
                        extra_inset: 0.,
                        span: span.clone(),
                        code_original: String::new(),
                        task: b.task.take(),
                        removed: false,
                        edit_ids: vec![],
                        restore: None,
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
                        extra_inset: 0.,
                        span: span.clone(),
                        code_original: String::new(),
                        task: None,
                        removed: false,
                        edit_ids: vec![],
                        restore: None,
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
        for block in &mut result.blocks {
            match &mut block.kind {
                Kind::Text(_, inline) | Kind::Media(_, inline) => {
                    for atom in &mut inline.atoms {
                        atom.original = atom.text.clone();
                    }
                }
                Kind::Table(_, rows) => {
                    for row in rows {
                        for inline in row {
                            for atom in &mut inline.atoms {
                                atom.original = atom.text.clone();
                            }
                        }
                    }
                }
                _ => (),
            }
        }
        result.heights = vec![0.; result.blocks.len()];
        result
    }
    pub fn document(source: &str) -> Self {
        let prepared = super::editing::Prepared::new(source);
        let mut preview = Self::parse(&prepared.text, prepared.base_line);
        preview.prepared = prepared;
        preview
    }
    pub fn has_focus(&self, ctx: &egui::Context) -> bool {
        self.focused
            .iter()
            .any(|id| ctx.memory(|m| m.has_focus(*id)))
    }
    pub fn edited_document(&self) -> Option<String> {
        if !self.changed {
            return None;
        }
        Some(self.prepared.apply(self.patches()))
    }
    fn patches(&self) -> Vec<super::editing::Patch> {
        let mut patches = vec![];
        for block in &self.blocks {
            if block.removed {
                patches.push(super::editing::Patch {
                    range: self.prepared.range(block.span.clone()),
                    text: String::new(),
                });
                continue;
            }
            if let Some((span, original, checked)) = &block.task
                && original != checked
            {
                patches.push(super::editing::Patch {
                    range: self.prepared.range(span.clone()),
                    text: if *checked { "[x]" } else { "[ ]" }.into(),
                });
            }
            match &block.kind {
                Kind::Text(_, inline) | Kind::Media(_, inline) => {
                    patches.extend(self.inline_patches(inline))
                }
                Kind::Table(_, rows) => {
                    for row in rows {
                        for inline in row {
                            patches.extend(self.inline_patches(inline));
                        }
                    }
                }
                Kind::Code(_, text) if text != &block.code_original => patches.push(
                    super::editing::code_patch(&self.prepared, block.span.clone(), text),
                ),
                _ => (),
            }
        }
        if !self.append.is_empty() {
            patches.push(self.prepared.append_patch(&self.append));
        }
        patches
    }
    pub fn project_input(&self, input: &super::preview_format::Input) -> (String, Range<usize>) {
        self.prepared
            .project(self.patches(), input.span.clone(), input.whole_lines)
    }
    pub fn focus_source(&mut self, source: Range<usize>) {
        let body = self.prepared.range(0..0).start;
        let source = source.start.max(body)..source.end.max(body);
        for block in &mut self.blocks {
            match &mut block.kind {
                Kind::Text(_, inline) | Kind::Media(_, inline) => {
                    if let Some(range) = restore_inline(&self.prepared, inline, &source) {
                        inline.restore = Some(range);
                        return;
                    }
                }
                Kind::Table(_, rows) => {
                    for inline in rows.iter_mut().flatten() {
                        if let Some(range) = restore_inline(&self.prepared, inline, &source) {
                            inline.restore = Some(range);
                            return;
                        }
                    }
                }
                Kind::Code(_, _) => {
                    let range = self.prepared.range(block.span.clone());
                    if range.start <= source.start && source.start <= range.end {
                        block.restore = Some(super::preview_format::local_range(
                            &self.prepared,
                            block.span.clone(),
                            &source,
                        ));
                        return;
                    }
                }
                _ => (),
            }
        }
        let point = self.prepared.generated_point(source.start);
        let line = self.prepared.source()[..source.start]
            .bytes()
            .filter(|b| *b == b'\n')
            .count();
        let index = self.blocks.partition_point(|b| b.lines.start < line);
        let mut extra_inset = 0.;
        let mut indent = 0;
        let mut quote = 0;
        if let Some(previous) = index.checked_sub(1).and_then(|i| self.blocks.get(i)) {
            let end = self.prepared.range(previous.span.clone()).end;
            let gap = &self.prepared.source()[end.min(source.start)..source.start];
            if previous.marker.is_some()
                && gap.bytes().filter(|b| *b == b'\n').count() == 1
                && gap.trim().is_empty()
            {
                extra_inset = if previous.task.is_some() { 36. } else { 28. };
                indent = previous.indent;
                quote = previous.quote;
            }
        }
        self.blocks.insert(
            index,
            Block {
                extra_inset,
                span: point..point,
                code_original: String::new(),
                task: None,
                removed: false,
                edit_ids: vec![],
                restore: None,
                kind: Kind::Text(
                    0,
                    Inline {
                        anchor: point..point,
                        restore: Some(0..0),
                        ..Default::default()
                    },
                ),
                lines: line..line + 1,
                indent,
                marker: None,
                quote,
                anchor: None,
            },
        );
        self.heights.insert(index, 0.);
    }
    pub fn insert_break(&mut self, id: egui::Id, range: Range<usize>) {
        let Some(index) = self
            .inputs
            .iter()
            .position(|input| input.output.response.id == id)
        else {
            return;
        };
        if index + 1 == self.inputs.len() {
            replace_chars(&mut self.append, range);
        } else {
            let span = self.inputs[index].span.clone();
            for block in &mut self.blocks {
                if let Kind::Text(_, inline) = &mut block.kind
                    && inline_span(inline) == span
                {
                    replace_chars(&mut inline.value, range);
                    update_atoms(inline);
                    break;
                }
            }
        }
        self.changed = true;
    }
    fn inline_patches(&self, inline: &Inline) -> Vec<super::editing::Patch> {
        let units: Vec<_> = inline
            .atoms
            .iter()
            .map(|a| super::editing::Unit {
                text: a.original.clone(),
                span: a.span.clone(),
                code: a.code,
                wrappers: a.wrappers.clone(),
            })
            .collect();
        if units.is_empty() && !inline.value.is_empty() {
            let value = if inline.needs_space {
                format!(" {}", inline.value)
            } else {
                inline.value.clone()
            };
            return vec![self.prepared.insert_patch(inline.anchor.clone(), &value)];
        }
        self.prepared.inline(
            &units,
            &inline
                .atoms
                .iter()
                .map(|a| a.text.clone())
                .collect::<Vec<_>>(),
        )
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
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        viewport: egui::Rect,
        size: f32,
        editable: bool,
    ) -> Option<String> {
        let width = ui.available_width();
        let geometry = (width.to_bits(), size.to_bits(), ui.visuals().dark_mode);
        if self.geometry != Some(geometry) {
            self.heights.fill(0.);
            self.geometry = Some(geometry);
        }
        let origin = ui.cursor().top();
        self.locations.clear();
        let mut link = None;
        self.focused.clear();
        self.changed = false;
        self.inputs.clear();
        for (i, block) in self.blocks.iter_mut().enumerate() {
            let top = ui.cursor().top() - origin;
            if self.heights[i] == 0. {
                self.heights[i] = estimated_height(block, width, size);
            }
            let known = self.heights[i];
            if known > 0.
                && !block
                    .edit_ids
                    .iter()
                    .any(|id| ui.memory(|m| m.has_focus(*id)))
                && (top + known < viewport.top() - 120. || top > viewport.bottom() + 120.)
            {
                ui.allocate_space(egui::vec2(width, known));
            } else {
                let start = ui.cursor().top();
                ui.push_id(i, |ui| {
                    let (target, changed, focused) =
                        render_block(block, ui, width, size, editable, &mut self.inputs);
                    link = target.or(link.take());
                    self.changed |= changed;
                    block.edit_ids = focused.clone();
                    self.focused.extend(focused);
                });
                self.heights[i] = ui.cursor().top() - start;
            }
            self.locations
                .push((block.lines.clone(), top, self.heights[i]));
        }
        let id = ui.make_persistent_id("preview-append");
        if editable {
            super::preview_format::restore(ui, id, self.append_restore.take());
            let output = egui::TextEdit::multiline(&mut self.append)
                .id(id)
                .frame(egui::Frame::NONE)
                .desired_width(width)
                .desired_rows(1)
                .hint_text(if self.blocks.is_empty() {
                    "Write a note…"
                } else {
                    ""
                })
                .interactive(ui.is_enabled())
                .return_key(None)
                .show(ui);
            self.changed |= output.response.changed();
            self.focused.push(id);
            self.inputs.push(super::preview_format::Input {
                output,
                span: self.prepared.text.len()..self.prepared.text.len(),
                text: self.append.clone(),
                whole_lines: false,
                inline_only: false,
                accepts_enter: true,
                list: false,
            });
        }
        link
    }
}
fn inline_span(inline: &Inline) -> Range<usize> {
    inline
        .atoms
        .iter()
        .flat_map(|a| std::iter::once(a.span.clone()).chain(a.wrappers.iter().cloned()))
        .fold(inline.anchor.clone(), |r, a| {
            if r.is_empty() {
                a
            } else {
                r.start.min(a.start)..r.end.max(a.end)
            }
        })
}
fn restore_inline(
    prepared: &super::editing::Prepared,
    inline: &Inline,
    source: &Range<usize>,
) -> Option<Range<usize>> {
    let span = inline_span(inline);
    let range = prepared.range(span.clone());
    let line_start = prepared.source()[..range.start]
        .rfind('\n')
        .map_or(0, |i| i + 1);
    (if source.is_empty() {
        line_start <= source.start && source.start <= range.end
    } else {
        source.start <= range.end && range.start <= source.end
    })
    .then(|| super::preview_format::local_range(prepared, span, source))
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
        Kind::Media(_, _) => 280.,
    }
}
fn render_block(
    block: &mut Block,
    ui: &mut egui::Ui,
    width: f32,
    size: f32,
    editable: bool,
    inputs: &mut Vec<super::preview_format::Input>,
) -> (Option<String>, bool, Vec<egui::Id>) {
    let p = colors(ui);
    let inset = block.indent as f32 * 20. + block.quote as f32 * 16. + block.extra_inset;
    let start = ui.cursor().top();
    let mut link = None;
    let mut changed = false;
    let mut focused = vec![];
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.;
        ui.add_space(inset);
        let marker_width = if block.task.is_some() {
            36.
        } else if block.marker.is_some() {
            28.
        } else {
            0.
        };
        if let Some((_, _, checked)) = &mut block.task {
            let response = ui
                .scope(|ui| {
                    ui.spacing_mut().interact_size.y = 24.;
                    ui.add_enabled_ui(editable, |ui| design::checkbox(ui, checked, ""))
                        .inner
                })
                .inner;
            changed |= response.changed();
            if response.clicked() {
                response.request_focus();
            }
            focused.push(response.id);
            ui.add_space(12.);
        } else if let Some(marker) = &block.marker {
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
                match &mut block.kind {
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
                        let result = inline_label(
                            ui,
                            inline,
                            width,
                            InlineStyle {
                                size: font_size,
                                bold: *level > 0,
                                quote: block.quote > 0,
                                multiline: true,
                                list: block.marker.is_some(),
                            },
                            editable,
                            inputs,
                        );
                        link = result.0;
                        changed |= result.1;
                        focused.push(result.2);
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
                                            language.as_str()
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
                                                    language,
                                                );
                                                ui.fonts_mut(|f| f.layout_job(job))
                                            };
                                        let id = ui.make_persistent_id("preview-code");
                                        focused.push(id);
                                        super::preview_format::restore(
                                            ui,
                                            id,
                                            block.restore.take(),
                                        );
                                        let mut readonly = text.as_str();
                                        let buffer: &mut dyn egui::TextBuffer =
                                            if editable { text } else { &mut readonly };
                                        let output = egui::TextEdit::multiline(buffer)
                                            .id(id)
                                            .interactive(ui.is_enabled())
                                            .font(FontId::monospace(size * 0.9))
                                            .frame(egui::Frame::NONE)
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(1)
                                            .layouter(&mut layout)
                                            .show(ui);
                                        changed |= output.response.changed();
                                        inputs.push(super::preview_format::Input {
                                            output,
                                            span: block.span.clone(),
                                            text: text.clone(),
                                            whole_lines: true,
                                            inline_only: false,
                                            accepts_enter: false,
                                            list: false,
                                        });
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
                                ui.spacing_mut().item_spacing = egui::vec2(16., 10.);
                                for (index, row) in rows.iter_mut().enumerate() {
                                    egui::Frame::new()
                                        .fill(if index % 2 == 1 {
                                            p.column
                                        } else {
                                            egui::Color32::TRANSPARENT
                                        })
                                        .show(ui, |ui| {
                                            ui.horizontal_top(|ui| {
                                                for (column, cell) in row.iter_mut().enumerate() {
                                                    ui.push_id((index, column), |ui| {
                                                        ui.allocate_ui_with_layout(
                                                            egui::vec2(cell_width, 0.),
                                                            egui::Layout::top_down(
                                                                egui::Align::Min,
                                                            ),
                                                            |ui| {
                                                                let result = inline_label(
                                                                    ui,
                                                                    cell,
                                                                    cell_width,
                                                                    InlineStyle {
                                                                        size,
                                                                        bold: index == 0,
                                                                        quote: false,
                                                                        multiline: false,
                                                                        list: false,
                                                                    },
                                                                    editable,
                                                                    inputs,
                                                                );
                                                                link = result.0.or(link.take());
                                                                changed |= result.1;
                                                                focused.push(result.2);
                                                            },
                                                        );
                                                    });
                                                }
                                            });
                                        });
                                }
                            });
                        ui.add_space(16.);
                    }
                    Kind::Media(target, alt) => {
                        amigo_ui::media::show_current(ui, target);
                        if !alt.value.is_empty() || editable {
                            let result = inline_label(
                                ui,
                                alt,
                                width,
                                InlineStyle {
                                    size: (size - 2.).max(11.),
                                    bold: false,
                                    quote: false,
                                    multiline: false,
                                    list: false,
                                },
                                editable,
                                inputs,
                            );
                            changed |= result.1;
                            focused.push(result.2);
                        }
                        ui.add_space(10.);
                    }
                    Kind::Rule => {
                        ui.add_space(8.);
                        let rule = ui.separator();
                        design::context_menu(&rule).show(|ui| {
                            if ui
                                .add_enabled(editable, design::Button::new("Remove Divider"))
                                .clicked()
                            {
                                block.removed = true;
                                changed = true;
                                ui.close();
                            }
                        });
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
    (link, changed, focused)
}
fn inline_label(
    ui: &mut egui::Ui,
    inline: &mut Inline,
    width: f32,
    style: InlineStyle,
    editable: bool,
    inputs: &mut Vec<super::preview_format::Input>,
) -> (Option<String>, bool, egui::Id) {
    let InlineStyle {
        size,
        bold,
        quote,
        multiline,
        list,
    } = style;
    let id = ui.make_persistent_id("preview-text");
    super::preview_format::restore(ui, id, inline.restore.take());
    let atoms = &inline.atoms;
    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap: f32| {
        let job = inline_job(ui, atoms, text.as_str(), wrap, size, bold, quote);
        ui.fonts_mut(|f| f.layout_job(job))
    };
    let enabled = ui.is_enabled();
    let mut readonly = inline.value.as_str();
    let buffer: &mut dyn egui::TextBuffer = if editable {
        &mut inline.value
    } else {
        &mut readonly
    };
    let edit = if multiline {
        egui::TextEdit::multiline(buffer)
    } else {
        egui::TextEdit::singleline(buffer)
    };
    let output = edit
        .id(id)
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::ZERO)
        .desired_width(width)
        .desired_rows(1)
        .font(FontId::proportional(size))
        .interactive(enabled)
        .return_key(if multiline {
            None
        } else {
            Some(egui::KeyboardShortcut::new(
                egui::Modifiers::NONE,
                egui::Key::Enter,
            ))
        })
        .layouter(&mut layouter)
        .show(ui);
    let mut link = None;
    if let Some(pos) = output.response.hover_pos() {
        let index = output
            .galley
            .cursor_from_pos(pos - output.galley_pos)
            .index
            .0;
        let mut offset = 0;
        for atom in atoms {
            let end = offset + atom.text.chars().count();
            if offset <= index
                && index < end
                && let Some(target) = &atom.link
            {
                let follow =
                    (!editable || ui.input(|i| i.modifiers.command)) && output.response.clicked();
                output.response.response.clone().on_hover_text(format!(
                    "{} to open {target}",
                    if editable {
                        "Ctrl/Command-click"
                    } else {
                        "Click"
                    }
                ));
                if follow {
                    link = Some(target.clone());
                }
                break;
            }
            offset = end;
        }
    }
    let changed = output.response.changed();
    if changed {
        update_atoms(inline);
    }
    let span = inline_span(inline);
    inputs.push(super::preview_format::Input {
        output,
        span,
        text: inline.value.clone(),
        whole_lines: multiline,
        inline_only: !multiline,
        accepts_enter: multiline,
        list,
    });
    (link, changed, id)
}
fn inline_job(
    ui: &egui::Ui,
    atoms: &[Atom],
    value: &str,
    width: f32,
    size: f32,
    bold: bool,
    quote: bool,
) -> LayoutJob {
    let p = colors(ui);
    let mut job = LayoutJob::default();
    let units: Vec<_> = atoms
        .iter()
        .map(|a| super::editing::Unit {
            text: a.text.clone(),
            span: a.span.clone(),
            code: a.code,
            wrappers: a.wrappers.clone(),
        })
        .collect();
    let values = super::editing::values(&units, value);
    if atoms.is_empty() {
        job.append(
            value,
            0.,
            TextFormat {
                font_id: FontId::proportional(size),
                color: p.ink,
                line_height: Some(size * 1.65),
                ..Default::default()
            },
        );
    }
    for (atom, text) in atoms.iter().zip(values) {
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
        job.append(&text, 0., format);
    }
    job.wrap.max_width = width;
    job
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

#[cfg(test)]
#[path = "render_editing_tests.rs"]
mod editing_tests;

fn update_atoms(inline: &mut Inline) {
    let units: Vec<_> = inline
        .atoms
        .iter()
        .map(|a| super::editing::Unit {
            text: a.text.clone(),
            span: a.span.clone(),
            code: a.code,
            wrappers: a.wrappers.clone(),
        })
        .collect();
    for (atom, text) in inline
        .atoms
        .iter_mut()
        .zip(super::editing::values(&units, &inline.value))
    {
        atom.text = text;
    }
}

fn replace_chars(text: &mut String, range: Range<usize>) {
    let point = |n| text.char_indices().nth(n).map_or(text.len(), |(i, _)| i);
    text.replace_range(point(range.start)..point(range.end), "\n");
}
