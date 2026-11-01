//! Native sequence diagrams with ordered messages and participant lifelines.
use crate::design;
use amigo_core::mermaid::{self, Line};
use egui::{FontId, Stroke, Vec2};
use std::sync::Arc;
#[derive(Clone)]
struct Cached {
    source: String,
    diagram: Arc<Result<mermaid::Sequence, String>>,
}
pub fn show(ui: &mut egui::Ui, source: &str) -> egui::Response {
    let id = ui.next_auto_id().with("sequence");
    let diagram = ui.ctx().data_mut(|d| {
        let cached = d.get_temp_mut_or_insert_with(id, || Cached {
            source: source.into(),
            diagram: Arc::new(mermaid::parse_sequence(source)),
        });
        if cached.source != source {
            cached.source = source.into();
            cached.diagram = Arc::new(mermaid::parse_sequence(source));
        }
        cached.diagram.clone()
    });
    let response = contents(ui, source, diagram.as_ref(), id, false);
    let mut enlarged = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(id.with("enlarged")))
        .unwrap_or(false);
    if enlarged && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        enlarged = false;
    }
    if enlarged {
        let bounds = ui.ctx().content_rect().shrink(16.);
        egui::Window::new("Sequence diagram — enlarged view")
            .id(id.with("window"))
            .order(egui::Order::Foreground)
            .open(&mut enlarged)
            .default_rect(bounds)
            .constrain_to(ui.ctx().content_rect())
            .min_size(egui::vec2(300., 200.).min(bounds.size()))
            .resizable(true)
            .collapsible(false)
            .show(ui.ctx(), |ui| {
                // Keep the enlarged view usable above a task/card description modal.
                ui.memory_mut(|m| m.set_modal_layer(ui.layer_id()));
                contents(ui, source, diagram.as_ref(), id, true);
            });
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(id.with("enlarged"), enlarged));
    response
}
fn contents(
    ui: &mut egui::Ui,
    source: &str,
    diagram: &Result<mermaid::Sequence, String>,
    id: egui::Id,
    enlarged: bool,
) -> egui::Response {
    let p = design::colors(ui);
    egui::Frame::new()
        .fill(p.column)
        .inner_margin(10)
        .corner_radius(6)
        .show(ui, |ui| {
            let diagram = match diagram {
                Ok(d) => d,
                Err(e) => {
                    ui.colored_label(p.red, format!("Sequence diagram: {e}"));
                    return;
                }
            };
            let mut zoom = ui
                .ctx()
                .data(|d| d.get_temp::<f32>(id.with("zoom")))
                .unwrap_or(1.);
            ui.horizontal(|ui| {
                ui.label("Sequence diagram");
                if ui.small_button("−").clicked() {
                    zoom = (zoom / 1.25).max(0.35);
                }
                if ui.small_button("+").clicked() {
                    zoom = (zoom * 1.25).min(2.);
                }
                if !enlarged
                    && ui
                        .small_button("Expand")
                        .on_hover_text("Open a large, resizable diagram window")
                        .clicked()
                {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(id.with("enlarged"), true));
                }
                if ui.small_button("Copy source").clicked() {
                    ui.ctx().copy_text(source.into());
                }
            });
            ui.ctx().data_mut(|d| d.insert_temp(id.with("zoom"), zoom));
            let font = FontId::proportional(14. * zoom);
            let labels: Vec<_> = diagram
                .participants
                .iter()
                .map(|p0| {
                    ui.painter()
                        .layout(p0.label.clone(), font.clone(), p.ink, 200. * zoom)
                })
                .collect();
            let messages: Vec<_> = diagram
                .messages
                .iter()
                .map(|m| {
                    ui.painter()
                        .layout(m.label.clone(), font.clone(), p.ink, 240. * zoom)
                })
                .collect();
            let step = labels
                .iter()
                .map(|g| g.size().x + 40. * zoom)
                .fold(280. * zoom, f32::max);
            let header = labels.iter().map(|g| g.size().y).fold(24. * zoom, f32::max) + 24. * zoom;
            let rows: Vec<_> = messages.iter().map(|g| g.size().y + 36. * zoom).collect();
            let height = header + rows.iter().sum::<f32>() + 30. * zoom;
            // Expanded diagrams use the window's full width and height, rather than
            // inheriting the description or notebook column's clipping rectangle.
            let viewport_height = if enlarged {
                ui.available_height().max(80.)
            } else {
                crate::flowchart::viewport_height(ui, height)
            };
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::both()
                .id_salt(id.with(if enlarged {
                    "enlarged-scroll"
                } else {
                    "inline-scroll"
                }))
                .auto_shrink(if enlarged {
                    [false, false]
                } else {
                    [false, true]
                })
                .min_scrolled_height(viewport_height)
                .max_height(viewport_height)
                .show(ui, |ui| {
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(
                            step * diagram.participants.len() as f32 + 100. * zoom,
                            height,
                        ),
                        egui::Sense::hover(),
                    );
                    let painter = ui.painter();
                    let x = |i: usize| rect.left() + step * (i as f32 + 0.5);
                    for (i, label) in labels.iter().enumerate() {
                        let center = egui::pos2(x(i), rect.top() + header / 2.);
                        let box_rect = egui::Rect::from_center_size(
                            center,
                            label.size() + Vec2::splat(20. * zoom),
                        );
                        painter.rect_filled(box_rect, 4, p.card);
                        painter.rect_stroke(
                            box_rect,
                            4,
                            Stroke::new(1., p.accent),
                            egui::StrokeKind::Inside,
                        );
                        painter.galley(center - label.size() / 2., label.clone(), p.ink);
                        painter.add(egui::Shape::dashed_line(
                            &[
                                egui::pos2(x(i), rect.top() + header),
                                egui::pos2(x(i), rect.bottom()),
                            ],
                            Stroke::new(1., p.muted),
                            5. * zoom,
                            4. * zoom,
                        ));
                    }
                    let mut y = rect.top() + header;
                    for ((m, label), row) in diagram.messages.iter().zip(&messages).zip(rows) {
                        let start = egui::pos2(x(m.from), y + row - 12. * zoom);
                        let end = egui::pos2(x(m.to), start.y);
                        let points = if m.from == m.to {
                            vec![
                                start,
                                start + egui::vec2(70., 0.) * zoom,
                                start + egui::vec2(70., 18.) * zoom,
                                end + egui::vec2(0., 18.) * zoom,
                            ]
                        } else {
                            vec![start, end]
                        };
                        let stroke = Stroke::new(1.5 * zoom, p.accent);
                        if m.line == Line::Dotted {
                            painter.add(egui::Shape::dashed_line(
                                &points,
                                stroke,
                                6. * zoom,
                                4. * zoom,
                            ));
                        } else {
                            painter.add(egui::Shape::line(points.clone(), stroke));
                        }
                        if m.end_arrow {
                            let tip = *points.last().unwrap();
                            let direction = (tip - points[points.len() - 2]).normalized();
                            let normal = egui::vec2(-direction.y, direction.x);
                            for sign in [-1., 1.] {
                                painter.line_segment(
                                    [tip, tip - direction * 9. * zoom + normal * sign * 4. * zoom],
                                    stroke,
                                );
                            }
                        }
                        let center = if m.from == m.to {
                            start.x + 90. * zoom + label.size().x / 2.
                        } else {
                            (start.x + end.x) / 2.
                        };
                        let pos = egui::pos2(center - label.size().x / 2., y + 4. * zoom);
                        painter.rect_filled(
                            egui::Rect::from_min_size(pos, label.size()),
                            2,
                            p.column,
                        );
                        painter.galley(pos, label.clone(), p.ink);
                        y += row;
                    }
                });
        })
        .response
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> String {
        let mut text = "sequenceDiagram\nparticipant A as Client\nparticipant B as Proxy\nparticipant C as Kernel\nparticipant D as Website\n".to_owned();
        for i in 0..24 {
            text.push_str(&format!("A->>D: Message {i}\n"));
        }
        text
    }
    fn frame(ctx: &egui::Context, modal: bool, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280., 1000.),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                let content = |ui: &mut egui::Ui| {
                    ui.set_width(340.);
                    egui::ScrollArea::vertical()
                        .max_width(340.)
                        .max_height(360.)
                        .show(ui, |ui| {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(
                                    egui::Id::new("test-sequence-id"),
                                    ui.next_auto_id().with("sequence"),
                                )
                            });
                            show(ui, &source());
                        });
                };
                if modal {
                    egui::Modal::new(egui::Id::new("description-modal")).show(ui.ctx(), content);
                } else {
                    egui::CentralPanel::default().show(ui, content);
                }
            },
        );
        output.textures_delta.clear();
        output
    }
    fn text_rect(output: &egui::FullOutput, text: &str) -> egui::Rect {
        output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == text && s.clip_rect.contains(t.pos) => {
                    Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("Missing text: {text}"))
    }
    fn click(ctx: &egui::Context, modal: bool, pos: egui::Pos2) {
        for pressed in [true, false] {
            frame(
                ctx,
                modal,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }
    fn viewport(output: &egui::FullOutput) -> egui::Rect {
        output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == "Client" => Some(s.clip_rect),
                _ => None,
            })
            .max_by(|a, b| a.area().total_cmp(&b.area()))
            .unwrap()
    }
    fn embedded_height(
        ctx: &egui::Context,
        source: &str,
        preceding_height: f32,
        panel_height: f32,
    ) -> f32 {
        let mut height = 0.;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 900.),
                )),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(panel_height)
                        .show(ui, |ui| {
                            ui.add_space(preceding_height);
                            height = crate::flowchart::show(ui, source).rect.height();
                        });
                });
            },
        );
        output.textures_delta.clear();
        height
    }
    #[test]
    fn embedded_height_tracks_content_and_panel_size_without_collapsing_after_text() {
        for (short, long) in [
            ("sequenceDiagram\nA->>B: Hello".to_owned(), source()),
            (
                "flowchart TD\nA[One] --> B[Two]".to_owned(),
                "flowchart TD\nA --> B --> C --> D --> E --> F --> G --> H".to_owned(),
            ),
        ] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            let short_height = embedded_height(&ctx, &short, 0., 600.);
            let long_height = embedded_height(&ctx, &long, 0., 600.);
            assert!(
                long_height > short_height + 30.,
                "{short_height} → {long_height}"
            );
            for prefix in [300., 900.] {
                let after_text = embedded_height(&ctx, &long, prefix, 600.);
                assert!(
                    (after_text - long_height).abs() <= 12.,
                    "Prefix {prefix}: {long_height} → {after_text}"
                );
            }
            let small_panel = embedded_height(&ctx, &long, 0., 360.);
            assert!(
                small_panel < long_height - 50. && small_panel > 240.,
                "{long_height} → {small_panel}"
            );
            let shrunk = embedded_height(&ctx, &short, 0., 600.);
            assert!(
                (shrunk - short_height).abs() <= 12.,
                "{short_height} → {shrunk}"
            );
        }
    }
    #[test]
    fn enlarged_window_escapes_narrow_parent_and_closes_without_changing_source_or_text_scale() {
        for modal in [false, true] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            let mut output = frame(&ctx, modal, vec![]);
            for _ in 0..4 {
                output = frame(&ctx, modal, vec![]);
            }
            let inline = viewport(&output);
            let initial = text_rect(&output, "Client").height();
            click(&ctx, modal, text_rect(&output, "Expand").center());
            for _ in 0..6 {
                output = frame(&ctx, modal, vec![]);
            }
            let expanded = viewport(&output);
            assert!(
                expanded.width() > inline.width() * 2.,
                "{inline:?} → {expanded:?}"
            );
            assert!(expanded.height() > 700., "{expanded:?}");
            assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "Website" && s.clip_rect.contains_rect(egui::Rect::from_min_size(t.pos, t.galley.size())))));
            for s in &output.shapes {
                if let egui::Shape::Text(t) = &s.shape
                    && t.galley.text() == "Client"
                {
                    assert!((t.galley.size().y - initial).abs() < 1.);
                }
            }
            // The enlarged window remains interactive above a description modal.
            let copy = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t)
                        if t.galley.text() == "Copy source" && s.clip_rect.width() > 700. =>
                    {
                        Some(t.pos + t.galley.size() / 2.)
                    }
                    _ => None,
                })
                .next()
                .unwrap();
            click(&ctx, modal, copy);
            let id = ctx
                .data(|d| d.get_temp::<egui::Id>(egui::Id::new("test-sequence-id")))
                .unwrap()
                .with("window");
            let before = egui::containers::AreaState::load(&ctx, id).unwrap().rect();
            let corner = before.right_bottom() - egui::vec2(2., 2.);
            frame(&ctx, modal, vec![egui::Event::PointerMoved(corner)]);
            frame(
                &ctx,
                modal,
                vec![egui::Event::PointerButton {
                    pos: corner,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            let smaller = corner - egui::vec2(200., 150.);
            frame(&ctx, modal, vec![egui::Event::PointerMoved(smaller)]);
            frame(
                &ctx,
                modal,
                vec![egui::Event::PointerButton {
                    pos: smaller,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            for _ in 0..5 {
                output = frame(&ctx, modal, vec![]);
            }
            let resized = viewport(&output);
            assert!(
                resized.width() < expanded.width() - 100.,
                "{expanded:?} → {resized:?}"
            );
            assert!(
                resized.height() < expanded.height() - 80.,
                "{expanded:?} → {resized:?}"
            );
            frame(
                &ctx,
                modal,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            for _ in 0..6 {
                output = frame(&ctx, modal, vec![]);
            }
            assert!(viewport(&output).width() < expanded.width() / 2.);
        }
    }
}
