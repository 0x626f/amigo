//! Reusable Markdown preview for descriptions and research-card payloads.
use crate::{
    design,
    media::{self, Location},
};
use egui::{FontId, TextFormat, text::LayoutJob};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

fn flush(ui: &mut egui::Ui, job: &mut LayoutJob) {
    if !job.text.is_empty() {
        job.wrap.max_width = ui.available_width();
        ui.label(std::mem::take(job));
        ui.add_space(4.);
    }
}
pub fn show(ui: &mut egui::Ui, text: &str, location: Option<&Location>) {
    let mut parser = Parser::new_ext(
        text,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    )
    .peekable();
    let mut job = LayoutJob::default();
    let mut bold = false;
    let mut italic = false;
    let mut strike = false;
    let mut code = false;
    let mut level = 0;
    let mut lists = Vec::new();
    let mut linked = None::<String>;
    while let Some(event) = parser.next() {
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => {
                flush(ui, &mut job);
                let mut alt = String::new();
                for event in parser.by_ref() {
                    match event {
                        Event::End(TagEnd::Image) => break,
                        Event::Text(t) | Event::Code(t) => alt.push_str(&t),
                        _ => (),
                    }
                }
                if let Some(location) = location {
                    media::show(ui, location, &dest_url);
                } else {
                    design::hint(ui, &alt);
                }
                if !alt.is_empty() {
                    design::hint(ui, &alt);
                }
            }
            Event::Start(Tag::Heading { level: l, .. }) => {
                flush(ui, &mut job);
                level = l as u8;
            }
            Event::Start(Tag::Strong) => bold = true,
            Event::End(TagEnd::Strong) => bold = false,
            Event::Start(Tag::Emphasis) => italic = true,
            Event::End(TagEnd::Emphasis) => italic = false,
            Event::Start(Tag::Strikethrough) => strike = true,
            Event::End(TagEnd::Strikethrough) => strike = false,
            Event::Start(Tag::CodeBlock(_)) => {
                flush(ui, &mut job);
                code = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                flush(ui, &mut job);
                code = false;
            }
            Event::Start(Tag::Link { dest_url, .. }) => linked = Some(dest_url.into_string()),
            Event::End(TagEnd::Link) => linked = None,
            Event::Start(Tag::List(number)) => lists.push(number),
            Event::End(TagEnd::List(_)) => {
                flush(ui, &mut job);
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                flush(ui, &mut job);
                let marker = match lists.last_mut() {
                    Some(Some(n)) => {
                        let s = format!("{n}. ");
                        *n += 1;
                        s
                    }
                    _ => "• ".into(),
                };
                job.append(
                    &format!("{}{marker}", "  ".repeat(lists.len().saturating_sub(1))),
                    0.,
                    TextFormat::default(),
                );
            }
            Event::Text(ref t) | Event::Code(ref t) => {
                let inline_code = matches!(&event, Event::Code(_));
                let size = match level {
                    1 => 28.,
                    2 => 23.,
                    3 => 20.,
                    _ => 15.,
                };
                let p = design::colors(ui);
                let font = if code || inline_code {
                    FontId::monospace(14.)
                } else {
                    FontId::proportional(size)
                };
                job.append(
                    t,
                    0.,
                    TextFormat {
                        font_id: font,
                        color: if linked.is_some() { p.accent } else { p.ink },
                        italics: italic,
                        strikethrough: if strike {
                            egui::Stroke::new(1., p.ink)
                        } else {
                            egui::Stroke::NONE
                        },
                        coords: if bold || level > 0 {
                            egui::epaint::text::VariationCoords::new([(b"wght", 700.)])
                        } else {
                            Default::default()
                        },
                        background: if inline_code {
                            p.field
                        } else {
                            egui::Color32::TRANSPARENT
                        },
                        ..Default::default()
                    },
                );
            }
            Event::End(TagEnd::Heading(_)) => {
                flush(ui, &mut job);
                level = 0;
            }
            Event::End(TagEnd::Paragraph | TagEnd::Item | TagEnd::TableRow) => flush(ui, &mut job),
            Event::SoftBreak => job.append(" ", 0., TextFormat::default()),
            Event::HardBreak => job.append("\n", 0., TextFormat::default()),
            Event::TaskListMarker(done) => {
                job.append(if done { "☑ " } else { "☐ " }, 0., TextFormat::default())
            }
            Event::Rule => {
                flush(ui, &mut job);
                ui.separator();
            }
            _ => (),
        }
    }
    flush(ui, &mut job);
}
/// A Markdown payload that starts in preview, with configurable click activation.
#[derive(Default)]
pub struct InlineEditor {
    pub editing: bool,
    focus_requested: bool,
    single_click_placeholder: Option<&'static str>,
    original: Option<String>,
}
impl InlineEditor {
    pub fn single_click(mut self, placeholder: &'static str) -> Self {
        self.single_click_placeholder = Some(placeholder);
        self
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        id: egui::Id,
        text: &mut String,
        location: Option<&Location>,
        rows: usize,
    ) -> egui::Response {
        if self.editing {
            let cancel = self.single_click_placeholder.is_some()
                && ui.is_enabled()
                && !egui::Popup::is_any_open(ui.ctx())
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            let response = ui.add(
                egui::TextEdit::multiline(text)
                    .id(id)
                    .desired_rows(rows)
                    .desired_width(f32::INFINITY)
                    .margin(egui::Margin::same(12)),
            );
            if cancel {
                if let Some(original) = self.original.take() {
                    *text = original;
                }
                self.editing = false;
                response.surrender_focus();
                ui.ctx().request_repaint();
            } else if self.focus_requested {
                if !ui.is_sizing_pass() {
                    response.request_focus();
                    self.focus_requested = false;
                }
            } else if !response.has_focus() && !egui::Popup::is_any_open(ui.ctx()) {
                self.editing = false;
                self.original = None;
                ui.ctx().request_repaint();
            }
            response
        } else {
            let response = ui
                .scope_builder(
                    egui::UiBuilder::new()
                        .id_salt(id.with("preview-area"))
                        .sense(egui::Sense::click()),
                    |ui| {
                        ui.set_min_size(egui::vec2(ui.available_width(), 76.));
                        ui.style_mut().interaction.selectable_labels = false;
                        if text.trim().is_empty() {
                            design::hint(ui, self.single_click_placeholder.unwrap_or("Add text…"));
                        } else {
                            show(ui, text, location);
                        }
                    },
                )
                .response
                .on_hover_text(if self.single_click_placeholder.is_some() {
                    "Edit description"
                } else {
                    "Double-click to edit"
                });
            let activate = if self.single_click_placeholder.is_some() {
                response.clicked()
            } else {
                response.double_clicked()
            };
            if activate {
                self.editing = true;
                self.focus_requested = true;
                if self.single_click_placeholder.is_some() {
                    self.original = Some(text.clone());
                }
                ui.ctx().request_repaint();
            }
            response
        }
    }
}
