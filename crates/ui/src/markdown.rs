//! Reusable Markdown preview for descriptions and research-card payloads.
use crate::math::RichJob;
use crate::{
    design,
    media::{self, Location},
};
use egui::{FontId, TextFormat};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

fn flush(ui: &mut egui::Ui, job: &mut RichJob) {
    if !job.is_empty() {
        job.wrap.max_width = ui.available_width();
        std::mem::take(job).show(ui);
        ui.add_space(4.);
    }
}
pub fn show(ui: &mut egui::Ui, text: &str, location: Option<&Location>) {
    let mut parser = Parser::new_ext(
        text,
        Options::ENABLE_MATH
            | Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS,
    )
    .peekable();
    let mut job = RichJob::default();
    let mut bold = false;
    let mut italic = false;
    let mut strike = false;
    let mut code = false;
    let mut level = 0;
    let mut lists = Vec::new();
    let mut linked = None::<String>;
    while let Some(event) = parser.next() {
        match event {
            Event::DisplayMath(t) => {
                flush(ui, &mut job);
                crate::math::show(ui, &t);
            }
            Event::InlineMath(t) => job.append_formula(
                ui,
                &t,
                TextFormat {
                    font_id: FontId::proportional(15.),
                    color: design::colors(ui).ink,
                    ..Default::default()
                },
                false,
            ),
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
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(language)))
                if amigo_core::mermaid::is_mermaid(&language) =>
            {
                flush(ui, &mut job);
                let mut source = String::new();
                for event in parser.by_ref() {
                    match event {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(t) => source.push_str(&t),
                        _ => (),
                    }
                }
                crate::flowchart::show(ui, &source);
            }
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
                crate::text_input::TextEdit::multiline(text)
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

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        ctx: &egui::Context,
        editor: &mut InlineEditor,
        text: &mut String,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 800.),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    editor.show(ui, egui::Id::new("flowchart-description"), text, None, 8);
                });
            },
        );
        output.textures_delta.clear();
        output
    }
    #[test]
    fn formulas_and_sequences_render_as_preview_shapes() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut editor = InlineEditor::default();
        let mut text = r"$$P_{wall}=1400/0.92\approx1522\text{ Вт}$$

```mermaid
sequenceDiagram
participant C as curl
participant K as Ядро Linux
C->>K: connect(127.0.0.1:1080)
K-->>C: TCP установлен
C->>C: self
```
"
        .to_owned();
        let original = text.clone();
        let mut output = frame(&ctx, &mut editor, &mut text, vec![]);
        for _ in 0..3 {
            output = frame(&ctx, &mut editor, &mut text, vec![]);
        }
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect();
        assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.fill_texture_id() != egui::TextureId::default())), "Formula should be typeset into a texture");
        assert!(!texts.iter().any(|t| t.contains("P_{wall}")), "{texts:?}");
        assert!(texts.iter().any(|t| t == "Ядро Linux"));
        assert!(texts.iter().any(|t| t == "TCP установлен"));
        assert!(!texts.iter().any(|t| t.contains("Only Mermaid")));
        assert_eq!(text, original);
    }
    #[test]
    fn descriptions_render_mermaid_and_keep_double_click_source_editing() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut editor = InlineEditor::default();
        let source = "```mermaid\nflowchart LR\nA[Начало] --> B[Конец]\n```";
        let mut text = source.to_owned();
        let mut output = frame(&ctx, &mut editor, &mut text, vec![]);
        for _ in 0..3 {
            output = frame(&ctx, &mut editor, &mut text, vec![]);
        }
        let node = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t)
                    if t.galley.text() == "Начало" && shape.clip_rect.contains(t.pos) =>
                {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(text, source);
        assert!(!editor.editing);
        for _ in 0..2 {
            for pressed in [true, false] {
                frame(
                    &ctx,
                    &mut editor,
                    &mut text,
                    vec![
                        egui::Event::PointerMoved(node),
                        egui::Event::PointerButton {
                            pos: node,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
        }
        assert!(editor.editing);
        assert_eq!(text, source);
    }
}
