//! Paginated, selectable Unicode PDF output through Cairo / Pango. HTML is text,
//! never executed. Local images are resolved through Journal's media boundary.
use super::{MAX_PDF_BYTES, native::*};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::path::Path;
const WIDTH: f64 = 595.28;
const HEIGHT: f64 = 841.89;
const MARGIN: f64 = 42.;
#[derive(Debug)]
enum Block {
    Text(String, usize),
    Row(Vec<String>, bool),
    Image(String, String),
    Rule,
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
        .replace('\0', "�")
}
fn blocks(markdown: &str) -> Vec<Block> {
    let normalized = markdown.replace("\r\n", "\n");
    let markdown = normalized.as_str();
    let mut output = Vec::new();
    let mut text = String::new();
    let mut depth = Vec::new();
    let mut prefix = String::new();
    let mut row = Vec::new();
    let mut table_head = false;
    let mut in_cell = false;
    let mut image: Option<(String, String)> = None;
    fn flush(out: &mut Vec<Block>, text: &mut String, indent: usize) {
        if !text.is_empty() {
            out.push(Block::Text(std::mem::take(text), indent));
        }
    }
    // Front matter is metadata, matching the Journal preview.
    let markdown = if let Some(rest) = markdown.strip_prefix("---\n") {
        rest.find("\n---\n")
            .map_or(markdown, |end| &rest[end + 5..])
    } else {
        markdown
    };
    for event in Parser::new_ext(
        markdown,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    ) {
        if let Some((_, alt)) = &mut image {
            match event {
                Event::End(TagEnd::Image) => {
                    let (source, alt) = image.take().unwrap();
                    if text.is_empty() && !in_cell {
                        output.push(Block::Image(source, alt));
                    } else {
                        text.push_str(&escape(&format!("[Image: {alt}]")));
                    }
                }
                Event::Text(t) | Event::Code(t) => alt.push_str(&t),
                _ => (),
            }
            continue;
        }
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => {
                image = Some((dest_url.into_string(), String::new()));
            }
            Event::Start(Tag::Heading { level, .. }) => {
                flush(&mut output, &mut text, depth.len());
                text.push_str(&format!(
                    "<span size=\"{}pt\" weight=\"bold\">",
                    match level as u8 {
                        1 => 24,
                        2 => 20,
                        3 => 17,
                        _ => 14,
                    }
                ));
            }
            Event::End(TagEnd::Heading(_)) => {
                text.push_str("</span>");
                flush(&mut output, &mut text, depth.len());
            }
            Event::Start(Tag::Paragraph) => {
                if !prefix.is_empty() {
                    text.push_str(&std::mem::take(&mut prefix));
                }
            }
            Event::End(TagEnd::Paragraph) => {
                flush(&mut output, &mut text, depth.len());
            }
            Event::Start(Tag::Emphasis) => text.push_str("<i>"),
            Event::End(TagEnd::Emphasis) => text.push_str("</i>"),
            Event::Start(Tag::Strong) => text.push_str("<b>"),
            Event::End(TagEnd::Strong) => text.push_str("</b>"),
            Event::Start(Tag::Strikethrough) => text.push_str("<s>"),
            Event::End(TagEnd::Strikethrough) => text.push_str("</s>"),
            Event::Start(Tag::Link { .. }) => {
                text.push_str("<span foreground=\"#2859a8\" underline=\"single\">")
            }
            Event::End(TagEnd::Link) => text.push_str("</span>"),
            Event::Start(Tag::CodeBlock(_)) => {
                flush(&mut output, &mut text, depth.len());
                text.push_str(
                    "<span font_family=\"monospace\" size=\"10pt\" background=\"#f0f2f4\">",
                );
            }
            Event::End(TagEnd::CodeBlock) => {
                text.push_str("</span>");
                flush(&mut output, &mut text, depth.len());
            }
            Event::Code(t) => text.push_str(&format!(
                "<span font_family=\"monospace\" background=\"#f0f2f4\">{}</span>",
                escape(&t)
            )),
            Event::Start(Tag::List(start)) => {
                flush(&mut output, &mut text, depth.len());
                depth.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                flush(&mut output, &mut text, depth.len());
                depth.pop();
            }
            Event::Start(Tag::Item) => {
                flush(&mut output, &mut text, depth.len());
                prefix = match depth.last_mut() {
                    Some(Some(n)) => {
                        let p = format!("{n}. ");
                        *n += 1;
                        p
                    }
                    _ => "• ".into(),
                };
            }
            Event::End(TagEnd::Item) => {
                flush(&mut output, &mut text, depth.len());
                prefix.clear();
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush(&mut output, &mut text, depth.len());
                depth.push(None);
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush(&mut output, &mut text, depth.len());
                depth.pop();
            }
            Event::Start(Tag::Table(_)) => {
                flush(&mut output, &mut text, depth.len());
            }
            Event::Start(Tag::TableHead) => {
                table_head = true;
            }
            Event::Start(Tag::TableCell) => {
                in_cell = true;
            }
            Event::End(TagEnd::TableCell) => {
                row.push(std::mem::take(&mut text));
                in_cell = false;
            }
            Event::End(TagEnd::TableRow) | Event::End(TagEnd::TableHead) => {
                output.push(Block::Row(std::mem::take(&mut row), table_head));
                table_head = false;
            }
            Event::Text(t) | Event::Html(t) | Event::InlineHtml(t) => {
                if !prefix.is_empty() && !in_cell {
                    text.push_str(&std::mem::take(&mut prefix));
                }
                text.push_str(&escape(&t));
            }
            Event::SoftBreak => text.push(' '),
            Event::HardBreak => text.push('\n'),
            Event::TaskListMarker(done) => text.push_str(if done { "☑ " } else { "☐ " }),
            Event::Rule => {
                flush(&mut output, &mut text, depth.len());
                output.push(Block::Rule);
            }
            _ => (),
        }
    }
    flush(&mut output, &mut text, depth.len());
    output
}
unsafe extern "C" fn write_pdf(closure: Ptr, data: *const u8, len: u32) -> i32 {
    // SAFETY: Cairo invokes this synchronously while convert owns `bytes`.
    unsafe {
        let bytes = &mut *closure.cast::<Vec<u8>>();
        if bytes.len().saturating_add(len as usize) > MAX_PDF_BYTES as usize {
            return 11;
        }
        bytes.extend_from_slice(std::slice::from_raw_parts(data, len as usize));
    }
    0
}
struct Writer {
    api: &'static Api,
    cr: Context,
    y: f64,
    pages: usize,
}
impl Writer {
    unsafe fn layout(&self, markup: &str, width: f64) -> Result<Object, String> {
        let api = self.api;
        unsafe {
            let object = Object((api.pango_cairo_create_layout)(self.cr.0), api);
            let markup = cstring(&format!("<span font_desc=\"Sans 11\">{markup}</span>"))?;
            (api.pango_layout_set_width)(object.0, (width * 1024.) as i32);
            (api.pango_layout_set_wrap)(object.0, 2); // word / char fallback
            (api.pango_layout_set_markup)(object.0, markup.as_ptr(), -1);
            Ok(object)
        }
    }
    unsafe fn room(&mut self, height: f64) -> Result<(), String> {
        if self.y + height > HEIGHT - MARGIN {
            if self.pages >= 2000 {
                return Err("This note exceeds the 2,000 page conversion limit.".into());
            }
            unsafe { (self.api.cairo_show_page)(self.cr.0) };
            self.y = MARGIN;
            self.pages += 1;
        }
        Ok(())
    }
    unsafe fn line(&self, layout: Ptr, index: i32) -> (Ptr, PangoRectangle) {
        unsafe {
            let line = (self.api.pango_layout_get_line_readonly)(layout, index);
            let mut rect = PangoRectangle::default();
            (self.api.pango_layout_line_get_extents)(line, std::ptr::null_mut(), &mut rect);
            (line, rect)
        }
    }
    unsafe fn text(&mut self, markup: &str, indent: usize) -> Result<(), String> {
        let x = MARGIN + (indent.min(12) as f64 * 16.);
        unsafe {
            let layout = self.layout(markup, WIDTH - MARGIN - x)?;
            let count = (self.api.pango_layout_get_line_count)(layout.0);
            for i in 0..count {
                let (line, rect) = self.line(layout.0, i);
                let height = (rect.height as f64 / 1024.).max(14.);
                self.room(height)?;
                (self.api.cairo_move_to)(self.cr.0, x, self.y - rect.y as f64 / 1024.);
                (self.api.pango_cairo_show_layout_line)(self.cr.0, line);
                self.y += height;
            }
            self.y += 8.;
        }
        Ok(())
    }
    unsafe fn row(&mut self, cells: &[String], heading: bool) -> Result<(), String> {
        if cells.is_empty() {
            return Ok(());
        }
        let cell_width = (WIDTH - 2. * MARGIN) / cells.len() as f64;
        unsafe {
            let layouts: Vec<_> = cells
                .iter()
                .map(|s| {
                    self.layout(
                        &if heading {
                            format!("<b>{s}</b>")
                        } else {
                            s.clone()
                        },
                        (cell_width - 12.).max(6.),
                    )
                })
                .collect::<Result<_, _>>()?;
            let lines = layouts
                .iter()
                .map(|l| (self.api.pango_layout_get_line_count)(l.0))
                .max()
                .unwrap_or(0);
            for index in 0..lines {
                let line_data: Vec<_> = layouts
                    .iter()
                    .map(|l| {
                        if index < (self.api.pango_layout_get_line_count)(l.0) {
                            Some(self.line(l.0, index))
                        } else {
                            None
                        }
                    })
                    .collect();
                let height = line_data
                    .iter()
                    .flatten()
                    .map(|(_, r)| r.height as f64 / 1024.)
                    .fold(14., f64::max)
                    + 6.;
                self.room(height)?;
                (self.api.cairo_set_source_rgb)(
                    self.cr.0,
                    if heading { 0.91 } else { 0.97 },
                    if heading { 0.93 } else { 0.97 },
                    if heading { 0.95 } else { 0.97 },
                );
                (self.api.cairo_rectangle)(self.cr.0, MARGIN, self.y, WIDTH - 2. * MARGIN, height);
                (self.api.cairo_fill)(self.cr.0);
                (self.api.cairo_set_source_rgb)(self.cr.0, 0.08, 0.09, 0.12);
                for (col, data) in line_data.iter().enumerate() {
                    if let Some((line, rect)) = data {
                        (self.api.cairo_move_to)(
                            self.cr.0,
                            MARGIN + col as f64 * cell_width + 6.,
                            self.y + 3. - rect.y as f64 / 1024.,
                        );
                        (self.api.pango_cairo_show_layout_line)(self.cr.0, *line);
                    }
                }
                self.y += height;
            }
            self.y += 3.;
        }
        Ok(())
    }
    unsafe fn image(
        &mut self,
        target: &str,
        alt: &str,
        root: &Path,
        note: &str,
    ) -> Result<(), String> {
        let location = crate::media::resolve(
            root,
            &root.join(Path::new(note).parent().unwrap_or(Path::new(""))),
            target,
        );
        let path = match location {
            Ok(path) => path,
            Err(_) => {
                return unsafe { self.text(&escape(&format!("[Image: {alt}] ({target})")), 0) };
            }
        };
        let reader = image::ImageReader::open(&path)
            .map_err(|e| format!("Cannot export image {}: {e}", path.display()))?
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(128 * 1024 * 1024);
        let mut reader = reader;
        reader.limits(limits);
        let image = reader
            .decode()
            .map_err(|e| format!("Cannot export image: {e}"))?
            .to_rgba8();
        let (w, h) = image.dimensions();
        let factor = ((WIDTH - 2. * MARGIN) / w as f64)
            .min((HEIGHT - 2. * MARGIN) / h as f64)
            .min(1.);
        unsafe {
            self.room(h as f64 * factor)?;
            let surface = Surface(
                (self.api.cairo_image_surface_create)(0, w as i32, h as i32),
                self.api,
            );
            if (self.api.cairo_surface_status)(surface.0) != 0 {
                return Err("Could not allocate the Markdown image.".into());
            }
            (self.api.cairo_surface_flush)(surface.0);
            let data = (self.api.cairo_image_surface_get_data)(surface.0);
            let stride = (self.api.cairo_image_surface_get_stride)(surface.0) as usize;
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let pixel = image.get_pixel(x as u32, y as u32).0;
                    let a = pixel[3] as u32;
                    let value = (a << 24)
                        | ((pixel[0] as u32 * a / 255) << 16)
                        | ((pixel[1] as u32 * a / 255) << 8)
                        | (pixel[2] as u32 * a / 255);
                    std::ptr::copy_nonoverlapping(
                        value.to_ne_bytes().as_ptr(),
                        data.add(y * stride + x * 4),
                        4,
                    );
                }
            }
            (self.api.cairo_surface_mark_dirty)(surface.0);
            (self.api.cairo_save)(self.cr.0);
            (self.api.cairo_translate)(self.cr.0, MARGIN, self.y);
            (self.api.cairo_scale)(self.cr.0, factor, factor);
            (self.api.cairo_set_source_surface)(self.cr.0, surface.0, 0., 0.);
            (self.api.cairo_paint)(self.cr.0);
            (self.api.cairo_restore)(self.cr.0);
            self.y += h as f64 * factor + 8.;
        }
        Ok(())
    }
}
/// Render a Markdown snapshot as paginated A4 PDF with selectable text.
/// Remote / missing images are labeled; no network requests are made.
pub fn convert(markdown: &str, title: &str, root: &Path, note: &str) -> Result<Vec<u8>, String> {
    let _guard = lock();
    let api = api()?;
    let mut bytes = Vec::new();
    unsafe {
        let surface = Surface(
            (api.cairo_pdf_surface_create_for_stream)(
                write_pdf,
                (&mut bytes as *mut Vec<u8>).cast(),
                WIDTH,
                HEIGHT,
            ),
            api,
        );
        if (api.cairo_surface_status)(surface.0) != 0 {
            return Err("Could not start the PDF conversion.".into());
        }
        (api.cairo_pdf_surface_set_metadata)(surface.0, 0, cstring(title)?.as_ptr());
        (api.cairo_pdf_surface_set_metadata)(surface.0, 4, c"Amigo Journal".as_ptr());
        let cr = Context((api.cairo_create)(surface.0), api);
        (api.cairo_set_source_rgb)(cr.0, 0.08, 0.09, 0.12);
        let mut writer = Writer {
            api,
            cr,
            y: MARGIN,
            pages: 1,
        };
        for block in blocks(markdown) {
            match block {
                Block::Text(markup, indent) => writer.text(&markup, indent)?,
                Block::Row(cells, head) => writer.row(&cells, head)?,
                Block::Image(target, alt) => writer.image(&target, &alt, root, note)?,
                Block::Rule => {
                    writer.room(12.)?;
                    (api.cairo_rectangle)(writer.cr.0, MARGIN, writer.y, WIDTH - 2. * MARGIN, 1.);
                    (api.cairo_fill)(writer.cr.0);
                    writer.y += 12.;
                }
            }
        }
        if (api.cairo_status)(writer.cr.0) != 0 {
            return Err("PDF conversion could not render this note.".into());
        }
        drop(writer);
        (api.cairo_surface_finish)(surface.0);
        if (api.cairo_surface_status)(surface.0) != 0 {
            return Err("PDF conversion failed or exceeds the 64 MiB limit.".into());
        }
    }
    Ok(bytes)
}
