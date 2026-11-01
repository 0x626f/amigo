//! Virtualized continuous PDF viewer. A bounded raster cache is retained; form
//! edits stay independent of rendering so typing remains responsive.
mod fields;
#[cfg(test)]
mod tests;
use amigo_core::journal::pdf::{self, Annotation, Edits, Page, PdfDocument};
use amigo_ui::{
    design::{self, Icon, colors},
    text_input,
};
use egui::{self, RichText};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Default, PartialEq)]
enum Tool {
    #[default]
    Read,
    Forms,
    Text,
    Note,
}
struct Rendered {
    page: Page,
    texture: egui::TextureHandle,
}
struct Loaded {
    page: Page,
    sizes: Option<Vec<[f64; 2]>>,
}
struct Job {
    receiver: mpsc::Receiver<Result<Loaded, String>>,
    revision: u64,
    scale: f32,
    cancel: Arc<AtomicBool>,
    searching: bool,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
#[derive(Clone)]
enum RemoveTarget {
    Existing { page: usize, index: usize },
    Added(String),
}
#[derive(Default)]
pub(super) struct PdfEditor {
    rendered: Option<Arc<Rendered>>,
    cache: BTreeMap<usize, Arc<Rendered>>,
    sizes: Vec<[f64; 2]>,
    visible_pages: Vec<usize>,
    scroll_target: Option<usize>,
    viewport_offset: Option<f32>,
    scroll_id: Option<egui::Id>,
    job: Option<Job>,
    retired: Vec<std::thread::JoinHandle<()>>,
    revision: u64,
    rendered_revision: u64,
    page: usize,
    zoom: f32,
    tool: Tool,
    error: String,
    unlock: String,
    selected: Option<i32>,
    reveal_field: Option<i32>,
    draft: Option<Annotation>,
    remove: Option<RemoveTarget>,
    annotation_focus: bool,
    annotation_target: Option<RemoveTarget>,
    annotation_original: String,
    annotation_original_color: Option<[u8; 3]>,
    annotation_read_only: bool,
    undo: Vec<Edits>,
    redo: Vec<Edits>,
    search_open: bool,
    search: String,
    search_requested: bool,
    drag: Option<egui::Pos2>,
    drag_page: Option<usize>,
    changed: Option<Instant>,
    history_applied: bool,
    history_wait: bool,
    fit_size: egui::Vec2,
}
impl Drop for PdfEditor {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            self.retire(job);
        }
        for worker in self.retired.drain(..) {
            let _ = worker.join();
        }
    }
}
impl PdfEditor {
    fn retire(&mut self, mut job: Job) {
        job.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = job.worker.take() {
            self.retired.push(worker);
        }
    }
    fn record(&mut self, before: Edits, document: &PdfDocument) {
        if before != document.edits {
            self.undo.push(before);
            if self.undo.len() > 128 {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.invalidate();
        }
    }
    fn invalidate(&mut self) {
        self.cache.clear();
        self.revision = self.revision.wrapping_add(1);
        self.changed = Some(Instant::now());
        self.error.clear();
    }
    fn worker(&mut self, ui: &egui::Ui, document: &PdfDocument) {
        self.retired.retain(|worker| !worker.is_finished());
        if self.cache.is_empty()
            && self.rendered_revision == self.revision
            && let Some(rendered) = &self.rendered
        {
            self.cache.insert(rendered.page.number, rendered.clone());
        }
        if let Some(job) = self.job.take() {
            match job.receiver.try_recv() {
                Ok(result)
                    if job.revision == self.revision && (job.scale - self.zoom).abs() < 0.001 =>
                {
                    let searching = job.searching;
                    self.retire(job);
                    match result {
                        Ok(Loaded { mut page, sizes }) => {
                            if let Some(sizes) = sizes {
                                self.sizes = sizes;
                            }
                            if searching {
                                self.page = page.number;
                                self.scroll_target = Some(page.number);
                            }
                            let image =
                                egui::ColorImage::from_rgba_unmultiplied(page.pixels, &page.rgba);
                            let texture = ui.ctx().load_texture(
                                format!("journal-pdf-page-{}", page.number),
                                image,
                                egui::TextureOptions::LINEAR,
                            );
                            page.rgba = Vec::new();
                            let number = page.number;
                            let rendered = Arc::new(Rendered { page, texture });
                            self.cache.insert(number, rendered.clone());
                            if number == self.page {
                                self.rendered = Some(rendered);
                                self.rendered_revision = self.revision;
                                self.history_wait = false;
                            }
                            // Keep visible pages and the nearest neighbors, bounded independently of document length.
                            while self.cache.len() > 1
                                && (self.cache.len() > 8
                                    || self
                                        .cache
                                        .values()
                                        .map(|r| r.page.pixels[0] * r.page.pixels[1])
                                        .sum::<usize>()
                                        > 32_000_000)
                            {
                                if let Some(farthest) = self
                                    .cache
                                    .keys()
                                    .max_by_key(|p| p.abs_diff(self.page))
                                    .copied()
                                {
                                    self.cache.remove(&farthest);
                                } else {
                                    break;
                                }
                            }
                            self.error.clear();
                        }
                        Err(error) => self.error = error,
                    }
                }
                Ok(_) => self.retire(job),
                Err(mpsc::TryRecvError::Empty) => {
                    if job.revision == self.revision && (job.scale - self.zoom).abs() < 0.001 {
                        self.job = Some(job);
                    } else {
                        self.retire(job);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.retire(job);
                    self.error = "The PDF worker stopped. Your draft is preserved.".into()
                }
            }
        }
        if let Some(rendered) = self.cache.get(&self.page) {
            self.rendered = Some(rendered.clone());
            self.rendered_revision = self.revision;
            self.history_wait = false;
        }
        let target = if self.search_requested || !self.cache.contains_key(&self.page) {
            Some(self.page)
        } else if !self.sizes.is_empty() {
            self.visible_pages
                .iter()
                .copied()
                .take(8)
                .find(|p| !self.cache.contains_key(p))
        } else {
            None
        };
        let ready = self
            .changed
            .is_none_or(|t| t.elapsed() > Duration::from_millis(250));
        if self.job.is_none()
            && self.error.is_empty()
            && ready
            && let Some(page) = target
        {
            let pdf = document.clone();
            let scale = self.zoom;
            let pixel_scale = ui.ctx().pixels_per_point();
            let ctx = ui.ctx().clone();
            let query = self.search_requested.then(|| self.search.clone());
            let searching = query.is_some();
            self.search_requested = false;
            let start = self
                .rendered
                .as_ref()
                .map_or(page, |r| (self.page + 1) % r.page.count);
            let need_sizes = self.sizes.is_empty();
            let (sender, receiver) = mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            let worker = std::thread::spawn(move || {
                let result = (|| {
                    let sizes = if need_sizes {
                        Some(pdf::page_sizes(&pdf, &worker_cancel)?)
                    } else {
                        None
                    };
                    let page = if let Some(query) = query {
                        pdf::find_with_cancel(&pdf, &query, start, &worker_cancel)?
                            .ok_or("No matching text found in this PDF.")?
                    } else {
                        page
                    };
                    if worker_cancel.load(Ordering::Relaxed) {
                        return Err("PDF rendering cancelled.".into());
                    }
                    Ok(Loaded {
                        page: pdf::render(&pdf, page, scale as f64 * pixel_scale as f64)?,
                        sizes,
                    })
                })();
                let _ = sender.send(result);
                ctx.request_repaint();
            });
            self.job = Some(Job {
                receiver,
                revision: self.revision,
                scale,
                cancel,
                searching,
                worker: Some(worker),
            });
        }
        if self.job.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(40));
        } else if target.is_some() && !ready {
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut PdfDocument,
        path: &str,
        width: f32,
        height: f32,
    ) -> bool {
        if self.scroll_id.is_none() {
            self.scroll_id = Some(egui::Id::new(("journal-pdf-scroll", path)));
        }
        if self.zoom <= 0. {
            self.zoom = 1.;
            self.rendered_revision = u64::MAX;
        }
        self.history_applied = false;
        let before = document.edits.clone();
        self.shortcuts(ui, document);
        self.worker(ui, document);
        ui.spacing_mut().item_spacing = egui::vec2(6., 6.);
        self.fit_size = egui::vec2(width - 24., height - 90.);
        self.toolbar(ui, document);
        if !self.error.is_empty() {
            ui.colored_label(colors(ui).red, &self.error);
            if self.error.to_lowercase().contains("encrypted")
                || self.error.to_lowercase().contains("password")
            {
                ui.horizontal(|ui| {
                    let input = text_input::TextEdit::singleline(&mut self.unlock)
                        .id_salt(("pdf-password", path))
                        .password(true)
                        .hint_text("PDF password")
                        .show(ui);
                    if design::button(ui, "Unlock").clicked()
                        || input.response.has_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        document.password = std::mem::take(&mut self.unlock);
                        self.invalidate();
                        self.changed = None;
                    }
                });
            } else if design::button(ui, "Retry PDF").clicked() {
                self.error.clear();
                self.invalidate();
                self.changed = None;
            }
        }
        if self.search_open {
            ui.horizontal(|ui| {
                ui.label("Find in PDF");
                let input = text_input::TextEdit::singleline(&mut self.search)
                    .id(egui::Id::new("journal-pdf-find"))
                    .spellcheck(false)
                    .desired_width(220.)
                    .show(ui);
                if (design::button(ui, "Find Next").clicked()
                    || input.response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    && !self.search.trim().is_empty()
                {
                    self.search_requested = true;
                    self.invalidate();
                    self.changed = None;
                }
                if design::button(ui, "Close").clicked() {
                    self.search_open = false;
                }
            });
        }
        let available = ui.available_height().min(height).max(60.);
        let panel = self.tool == Tool::Forms && self.page_has_forms();
        ui.horizontal_top(|ui| {
            let panel_width = if panel {
                (width * 0.32).clamp(200., 320.)
            } else {
                0.
            };
            let canvas_width = (width - panel_width - if panel { 8. } else { 0. }).max(80.);
            self.fit_size = egui::vec2(canvas_width - 32., available - 32.);
            ui.allocate_ui_with_layout(
                egui::vec2(canvas_width, available),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.canvas(ui, document, canvas_width, available),
            );
            if panel {
                ui.allocate_ui_with_layout(
                    egui::vec2(panel_width, available),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt(("pdf-fields", path))
                            .max_height(available)
                            .show(ui, |ui| {
                                if let Some(rendered) = self
                                    .rendered
                                    .as_ref()
                                    .filter(|r| r.page.number == self.page)
                                {
                                    ui.add_enabled_ui(!self.history_wait, |ui| {
                                        fields::show(
                                            ui,
                                            document,
                                            &rendered.page,
                                            &mut self.selected,
                                            &mut self.reveal_field,
                                        )
                                    });
                                }
                            });
                    },
                );
            }
        });
        if let Some(target) = self.remove.take() {
            match target {
                RemoveTarget::Existing { page, index } => {
                    document.edits.annotations.insert((page, index), None);
                }
                RemoveTarget::Added(id) => document.edits.added.retain(|a| a.id != id),
            }
        }
        self.annotation_dialog(ui.ctx(), document);
        let changed = before != document.edits;
        if changed && !self.history_applied {
            self.record(before, document);
        }
        changed
    }
    fn shortcuts(&mut self, ui: &mut egui::Ui, document: &mut PdfDocument) {
        let find = ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::F));
        if find {
            self.search_open = true;
            ui.ctx()
                .memory_mut(|m| m.request_focus(egui::Id::new("journal-pdf-find")));
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.tool = Tool::Read;
            self.selected = None;
            self.search_open = false;
            self.draft = None;
            self.annotation_target = None;
            self.annotation_read_only = false;
            self.drag = None;
            self.drag_page = None;
        }
        // TextEdit owns its own editing shortcuts; document Undo applies otherwise.
        if ui.ctx().memory(|m| m.focused().is_none()) {
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
                self.history(document, false);
            }
            if ui.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                ) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
            }) {
                self.history(document, true);
            }
        } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::A)) {
            // Delegate select-all to the focused native text widget.
            ui.input_mut(|i| {
                i.events.push(egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                })
            });
        }
    }
    fn history(&mut self, document: &mut PdfDocument, redo: bool) {
        let (source, target) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        if let Some(value) = source.pop() {
            target.push(std::mem::replace(&mut document.edits, value));
            self.history_applied = true;
            self.invalidate();
            self.history_wait = true;
            self.changed = None;
        }
    }
    fn page_has_forms(&self) -> bool {
        self.cache
            .get(&self.page)
            .or_else(|| {
                self.rendered
                    .as_ref()
                    .filter(|r| r.page.number == self.page)
            })
            .is_some_and(|r| !r.page.fields.is_empty())
    }
    fn toolbar(&mut self, ui: &mut egui::Ui, document: &mut PdfDocument) {
        let count = self.rendered.as_ref().map_or(1, |r| r.page.count);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.page > 0, design::Button::new("‹"))
                .on_hover_text("Previous Page")
                .clicked()
            {
                self.page -= 1;
                self.navigate();
            }
            let mut page = self.page + 1;
            if ui
                .add(
                    egui::DragValue::new(&mut page)
                        .range(1..=count)
                        .prefix("Page "),
                )
                .changed()
            {
                self.page = page - 1;
                self.navigate();
            }
            ui.label(format!("of {count}"));
            if ui
                .add_enabled(self.page + 1 < count, design::Button::new("›"))
                .on_hover_text("Next Page")
                .clicked()
            {
                self.page += 1;
                self.navigate();
            }
            ui.separator();
            if design::button(ui, "−").on_hover_text("Zoom Out").clicked() {
                self.set_zoom(self.zoom / 1.2);
            }
            ui.label(format!("{:.0}%", self.zoom * 100.));
            if design::button(ui, "+").on_hover_text("Zoom In").clicked() {
                self.set_zoom(self.zoom * 1.2);
            }
            if design::button(ui, "Fit").clicked() {
                let size = self.rendered.as_ref().map_or([595., 842.], |r| r.page.size);
                self.set_zoom(
                    ((self.fit_size.x.max(120.) / size[0] as f32)
                        .min(self.fit_size.y.max(120.) / size[1] as f32))
                    .clamp(0.2, 4.),
                );
            }
            ui.separator();
            for (tool, label) in [
                (Tool::Read, "Read"),
                (Tool::Forms, "Fill Forms"),
                (Tool::Text, "Add Text"),
                (Tool::Note, "Add Note"),
            ] {
                if tool == Tool::Forms && !self.page_has_forms() {
                    continue;
                }
                let enabled = match tool {
                    Tool::Text => self
                        .rendered
                        .as_ref()
                        .is_some_and(|r| r.page.can_annotate && r.page.can_add_text),
                    Tool::Note => self.rendered.as_ref().is_some_and(|r| r.page.can_annotate),
                    _ => true,
                };
                let response = ui.add_enabled(
                    enabled,
                    design::Button::new(label).selected(self.tool == tool),
                );
                if !enabled && tool==Tool::Text {response.clone().on_disabled_hover_text("Adding text requires annotation permission and a Poppler runtime with free-text support.");}
                ui.interact(
                    response.rect,
                    egui::Id::new(("pdf-tool", label)),
                    egui::Sense::hover(),
                );
                if response.clicked() {
                    self.tool = tool;
                }
            }
            if ui
                .add_enabled(!self.undo.is_empty(), design::Button::new("Undo"))
                .clicked()
            {
                self.history(document, false);
            }
            if ui
                .add_enabled(!self.redo.is_empty(), design::Button::new("Redo"))
                .clicked()
            {
                self.history(document, true);
            }
            if design::icon_button(
                ui,
                egui::Id::new("pdf-find"),
                Icon::Search,
                "Find in PDF",
                26.,
            )
            .clicked()
            {
                self.search_open = !self.search_open;
            }
            if self.job.is_some() {
                ui.spinner();
            }
        });
        if matches!(self.tool, Tool::Text | Tool::Note) {
            design::hint(
                ui,
                "Drag a rectangle on the page, then enter text. Escape returns to reading.",
            );
        }
    }
    fn navigate(&mut self) {
        self.selected = None;
        self.scroll_target = Some(self.page);
        self.changed = None;
    }
    fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(0.2, 5.);
        self.scroll_target = Some(self.page);
        self.invalidate();
        self.changed = None;
    }
    fn canvas(&mut self, ui: &mut egui::Ui, document: &PdfDocument, width: f32, height: f32) {
        let mut zoom_delta = 0.;
        let sizes = if self.sizes.is_empty() {
            self.rendered
                .as_ref()
                .map(|r| vec![r.page.size; r.page.count])
                .unwrap_or_default()
        } else {
            self.sizes.clone()
        };
        if sizes.is_empty() {
            ui.spinner();
            ui.label(if self.error.is_empty() {
                "Opening PDF…"
            } else {
                "PDF preview unavailable."
            });
            return;
        }
        let mut offsets = Vec::with_capacity(sizes.len() + 1);
        offsets.push(12.);
        for size in &sizes {
            offsets.push(offsets.last().copied().unwrap() + size[1] as f32 * self.zoom + 16.);
        }
        let total_width = sizes
            .iter()
            .map(|s| s[0] as f32 * self.zoom + 24.)
            .fold(width - 16., f32::max);
        let total_height = *offsets.last().unwrap();
        let backdrop = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(width, height));
        ui.painter().rect_filled(backdrop, 0, colors(ui).field);
        let target = self.scroll_target.take();
        ui.scope(|ui| {
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            let mut scroll = egui::ScrollArea::both()
                .id_salt(
                    self.scroll_id
                        .unwrap_or(egui::Id::new("journal-pdf-scroll")),
                )
                .max_width(width)
                .max_height(height)
                .auto_shrink([false, false])
                .animated(false);
            if let Some(page) = target {
                scroll = scroll
                    .vertical_scroll_offset((offsets[page.min(sizes.len() - 1)] - 12.).max(0.));
            }
            let scroll_output = scroll.show_viewport(ui, |ui, viewport| {
                ui.set_min_size(egui::vec2(total_width, total_height));
                let probe = viewport.center().y;
                let active = offsets
                    .partition_point(|y| *y <= probe)
                    .saturating_sub(1)
                    .min(sizes.len() - 1);
                let scrolled = self
                    .viewport_offset
                    .is_none_or(|top| (top - viewport.top()).abs() > 0.5);
                self.viewport_offset = Some(viewport.top());
                if self.drag.is_none() && (scrolled || self.selected.is_none()) {
                    self.page = target.unwrap_or(active);
                    if scrolled {
                        self.selected = None;
                    }
                }
                if let Some(rendered) = self.cache.get(&self.page) {
                    self.rendered = Some(rendered.clone());
                    self.rendered_revision = self.revision;
                }
                let first = offsets
                    .partition_point(|y| *y <= viewport.top())
                    .saturating_sub(1)
                    .min(sizes.len() - 1);
                self.visible_pages.clear();
                for number in first..sizes.len() {
                    if offsets[number] > viewport.bottom() {
                        break;
                    }
                    self.visible_pages.push(number);
                    let size =
                        egui::vec2(sizes[number][0] as f32, sizes[number][1] as f32) * self.zoom;
                    let rect = egui::Rect::from_min_size(
                        ui.min_rect().min
                            + egui::vec2((total_width - size.x) / 2., offsets[number]),
                        size,
                    );
                    let rendered = self.cache.get(&number).cloned().or_else(|| {
                        self.rendered
                            .as_ref()
                            .filter(|r| r.page.number == number)
                            .cloned()
                    });
                    ui.push_id(("pdf-page", number), |ui| {
                        let response = ui.allocate_rect(
                            rect,
                            if matches!(self.tool, Tool::Text | Tool::Note) {
                                egui::Sense::drag()
                            } else {
                                egui::Sense::click()
                            },
                        );
                        if let Some(rendered) = rendered {
                            self.paint_page(ui, rect, response, number, &rendered, &mut zoom_delta);
                            self.annotation_menus(ui, rect, number, &rendered.page, document);
                        } else {
                            ui.painter().rect_filled(rect, 0, egui::Color32::WHITE);
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                format!("Page {}", number + 1),
                                egui::FontId::proportional(14.),
                                egui::Color32::GRAY,
                            );
                        }
                    });
                }
                self.visible_pages.sort_by_key(|p| p.abs_diff(self.page));
                self.cache.retain(|number, _| {
                    self.visible_pages.iter().take(8).any(|p| p == number) || *number == self.page
                });
            });
            ui.interact(
                scroll_output.inner_rect,
                egui::Id::new("journal-pdf-viewport"),
                egui::Sense::hover(),
            );
        });
        if ui.input(|i| i.pointer.any_released()) {
            self.drag = None;
            self.drag_page = None;
        }
        if zoom_delta != 0. {
            self.set_zoom(self.zoom * (zoom_delta * 0.004).exp());
        }
    }
    fn paint_page(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        response: egui::Response,
        number: usize,
        rendered: &Rendered,
        zoom_delta: &mut f32,
    ) {
        ui.painter().image(
            rendered.texture.id(),
            rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            egui::Color32::WHITE,
        );
        if number == self.page {
            ui.interact(
                rect,
                egui::Id::new("journal-pdf-page"),
                egui::Sense::hover(),
            );
        }
        ui.interact(
            rect,
            egui::Id::new(("journal-pdf-page", number)),
            egui::Sense::hover(),
        );
        if response.hovered() {
            ui.input_mut(|i| {
                if i.modifiers.alt {
                    *zoom_delta = i.smooth_scroll_delta.y;
                    i.smooth_scroll_delta = egui::Vec2::ZERO;
                }
            });
        }
        let to_screen = |coords: [f64; 4]| {
            egui::Rect::from_min_max(
                rect.min + egui::vec2(coords[0] as f32, coords[1] as f32) * self.zoom,
                rect.min + egui::vec2(coords[2] as f32, coords[3] as f32) * self.zoom,
            )
        };
        if self.tool == Tool::Forms {
            for field in &rendered.page.fields {
                let field_rect = to_screen(field.rect);
                ui.painter().rect_stroke(
                    field_rect,
                    0,
                    egui::Stroke::new(
                        if self.selected == Some(field.id) {
                            2.
                        } else {
                            1.
                        },
                        colors(ui).accent,
                    ),
                    egui::StrokeKind::Inside,
                );
                let response = ui
                    .interact(
                        field_rect,
                        egui::Id::new(("pdf-field", field.id)),
                        egui::Sense::click(),
                    )
                    .on_hover_text(&field.name);
                if response.clicked() {
                    self.page = number;
                    self.selected = Some(field.id);
                    self.reveal_field = Some(field.id);
                }
            }
        }
        if matches!(self.tool, Tool::Text | Tool::Note) {
            if response.drag_started() {
                self.page = number;
                self.drag_page = Some(number);
                self.drag = ui
                    .input(|i| i.pointer.press_origin())
                    .or_else(|| response.interact_pointer_pos());
            }
            if let Some(start) = self.drag
                && self.drag_page == Some(number)
                && let Some(pos) = response.interact_pointer_pos()
            {
                let area = egui::Rect::from_two_pos(start, pos).intersect(rect);
                ui.painter().rect_stroke(
                    area,
                    0,
                    egui::Stroke::new(1.5, colors(ui).accent),
                    egui::StrokeKind::Inside,
                );
                if response.drag_stopped() {
                    let mut area = area;
                    if area.width() < 30. || area.height() < 20. {
                        area =
                            egui::Rect::from_min_size(start, egui::vec2(180., 60.)).intersect(rect);
                    }
                    if area.width() >= 10. && area.height() >= 10. {
                        let min = (area.min - rect.min) / self.zoom;
                        let max = (area.max - rect.min) / self.zoom;
                        self.annotation_focus = true;
                        self.annotation_target = None;
                        self.annotation_read_only = false;
                        self.draft = Some(Annotation {
                            id: amigo_core::model::id(),
                            page: number,
                            rect: [min.x as f64, min.y as f64, max.x as f64, max.y as f64],
                            text: String::new(),
                            note: self.tool == Tool::Note,
                            color: (self.tool == Tool::Note).then_some([255, 215, 70]),
                        });
                    }
                    self.drag = None;
                    self.drag_page = None;
                }
            }
        }
    }
    fn annotation_menus(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        number: usize,
        page: &Page,
        document: &PdfDocument,
    ) {
        let zoom = self.zoom;
        let to_screen = |r: [f64; 4]| {
            egui::Rect::from_min_max(
                rect.min + egui::vec2(r[0] as f32, r[1] as f32) * zoom,
                rect.min + egui::vec2(r[2] as f32, r[3] as f32) * zoom,
            )
        };
        for annotation in &page.annotations {
            if document
                .edits
                .annotations
                .get(&(number, annotation.index))
                .is_some_and(Option::is_none)
            {
                continue;
            }
            self.annotation_menu(
                ui,
                to_screen(annotation.rect),
                egui::Id::new(("pdf-existing-annotation", number, annotation.index)),
                RemoveTarget::Existing {
                    page: number,
                    index: annotation.index,
                },
                page.can_annotate && !annotation.read_only,
                Annotation {
                    id: String::new(),
                    page: number,
                    rect: annotation.rect,
                    text: document
                        .edits
                        .annotations
                        .get(&(number, annotation.index))
                        .and_then(Clone::clone)
                        .unwrap_or_else(|| annotation.text.clone()),
                    note: annotation.note,
                    color: document
                        .edits
                        .annotation_colors
                        .get(&(number, annotation.index))
                        .copied()
                        .or(annotation.color),
                },
            );
        }
        for annotation in document.edits.added.iter().filter(|a| a.page == number) {
            self.annotation_menu(
                ui,
                to_screen(annotation.rect),
                egui::Id::new(("pdf-added-annotation", &annotation.id)),
                RemoveTarget::Added(annotation.id.clone()),
                page.can_annotate,
                annotation.clone(),
            );
        }
    }
    fn annotation_menu(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        id: egui::Id,
        target: RemoveTarget,
        editable: bool,
        draft: Annotation,
    ) {
        let response = ui
            .interact(rect, id, egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_ui(|ui| {
                ui.set_max_width(360.);
                ui.label(&draft.text);
            });
        if response.double_clicked() {
            self.annotation_target = Some(target.clone());
            self.annotation_original = draft.text.clone();
            self.annotation_original_color = draft.color;
            self.annotation_read_only = !editable;
            self.annotation_focus = true;
            self.draft = Some(draft);
        }
        design::context_menu(&response).show(|ui| {
            let remove = ui.add_enabled(
                editable,
                design::Button::new(egui::RichText::new("Remove").color(colors(ui).red)),
            );
            ui.interact(remove.rect, id.with("remove"), egui::Sense::hover());
            if remove.clicked() {
                self.remove = Some(target);
                ui.close();
            }
        });
    }
    fn annotation_dialog(&mut self, ctx: &egui::Context, document: &mut PdfDocument) {
        let Some(mut draft) = self.draft.take() else {
            return;
        };
        let mut cancel = false;
        let mut save = false;
        let editing = self.annotation_target.is_some();
        let read_only = self.annotation_read_only;
        let modal = design::modal(ctx, egui::Id::new("pdf-annotation-dialog")).show(ctx, |ui| {
            ui.set_width(design::dialog_width(ctx, 480.));
            cancel = design::dialog_heading(
                ui,
                if editing {
                    "PDF Annotation"
                } else if draft.note {
                    "Add PDF Note"
                } else {
                    "Add PDF Text"
                },
            )
            .clicked();
            let mut readonly_text = draft.text.as_str();
            let input = if read_only {
                text_input::TextEdit::multiline(&mut readonly_text)
            } else {
                text_input::TextEdit::multiline(&mut draft.text)
            }
            .id(egui::Id::new("pdf-annotation-text"))
            .desired_width(f32::INFINITY)
            .desired_rows(5)
            .char_limit(16384)
            .show(ui);
            if self.annotation_focus && !ui.is_sizing_pass() {
                input.response.request_focus();
                self.annotation_focus = false;
            }
            if draft.note {
                ui.add_enabled_ui(!read_only, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Note color");
                        let mut color = draft.color.unwrap_or([255, 215, 70]);
                        if ui.color_edit_button_srgb(&mut color).changed() {
                            draft.color = Some(color);
                        }
                        for (name, rgb) in [
                            ("Yellow", [255, 215, 70]),
                            ("Red", [240, 90, 90]),
                            ("Green", [90, 200, 120]),
                            ("Blue", [80, 150, 245]),
                            ("Purple", [180, 120, 230]),
                        ] {
                            let selected = draft.color == Some(rgb);
                            let button = ui
                                .add(
                                    design::Button::new(
                                        RichText::new(if selected { "✓" } else { " " })
                                            .color(egui::Color32::BLACK),
                                    )
                                    .fill(egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]))
                                    .min_size(egui::vec2(26., 26.)),
                                )
                                .on_hover_text(name);
                            ui.interact(
                                button.rect,
                                egui::Id::new(("pdf-note-color", name)),
                                egui::Sense::hover(),
                            );
                            if button.clicked() {
                                draft.color = Some(rgb);
                            }
                        }
                    });
                });
            }
            if read_only {
                design::hint(ui, "This annotation is read-only.");
            }
            if !draft.note && !editing {
                design::hint(
                    ui,
                    "Text uses 12 pt. Choose a rectangle large enough to fit your text.",
                );
            }
            design::dialog_footer(ui, |ui| {
                cancel |= design::button(ui, if read_only { "Close" } else { "Cancel" }).clicked();
                if !read_only {
                    let confirm = ui.add_enabled(
                        !draft.text.trim().is_empty(),
                        design::Button::new(if editing { "Save" } else { "Add" }),
                    );
                    ui.interact(
                        confirm.rect,
                        egui::Id::new("pdf-annotation-save"),
                        egui::Sense::hover(),
                    );
                    save = confirm.clicked();
                }
            });
        });
        if cancel || design::dialog_should_close(&modal) {
            self.annotation_target = None;
            self.annotation_read_only = false;
            return;
        }
        if save {
            let target = self.annotation_target.take();
            if !editing
                || draft.text != self.annotation_original
                || draft.color != self.annotation_original_color
            {
                match target {
                    Some(RemoveTarget::Existing { page, index }) => {
                        if draft.text != self.annotation_original {
                            document
                                .edits
                                .annotations
                                .insert((page, index), Some(draft.text));
                        }
                        if draft.color != self.annotation_original_color
                            && let Some(color) = draft.color
                        {
                            document
                                .edits
                                .annotation_colors
                                .insert((page, index), color);
                        }
                    }
                    Some(RemoveTarget::Added(id)) => {
                        if let Some(annotation) =
                            document.edits.added.iter_mut().find(|a| a.id == id)
                        {
                            annotation.text = draft.text;
                            annotation.color = draft.color;
                        }
                    }
                    None => document.edits.added.push(draft),
                }
            }
            self.annotation_read_only = false;
        } else {
            self.draft = Some(draft);
        }
    }
}
