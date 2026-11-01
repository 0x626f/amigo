//! Local rendering of basic Mermaid diagrams in Markdown previews.
use crate::design;
use amigo_core::mermaid::{self, Direction, Flowchart, Line, Shape};
use egui::{Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use std::sync::Arc;
#[derive(Clone)]
struct Cached {
    source: String,
    graph: Arc<Result<Flowchart, String>>,
}
#[derive(Clone, Copy)]
struct Viewport {
    rect: Rect,
    offset: Vec2,
    zoom: f32,
}
struct Layout {
    nodes: Vec<Rect>,
    labels: Vec<Arc<egui::Galley>>,
    size: Vec2,
}
fn layout(ui: &egui::Ui, graph: &Flowchart, zoom: f32) -> Layout {
    let labels = graph
        .nodes
        .iter()
        .map(|node| {
            ui.painter().layout(
                node.label.clone(),
                FontId::proportional(14. * zoom),
                design::colors(ui).ink,
                220. * zoom,
            )
        })
        .collect::<Vec<_>>();
    let sizes = labels
        .iter()
        .zip(&graph.nodes)
        .map(|(label, node)| {
            let mut size = label.size() + egui::vec2(30., 24.) * zoom;
            size = size.max(egui::vec2(100., 50.) * zoom);
            if node.shape == Shape::Circle {
                size = Vec2::splat(size.max_elem());
            }
            if node.shape == Shape::Diamond {
                size = label.size() * 1.6 + egui::vec2(40., 36.) * zoom;
            }
            size
        })
        .collect::<Vec<_>>();
    let cell = sizes.iter().copied().fold(Vec2::ZERO, Vec2::max);
    let ranks = graph.ranks();
    let layers = ranks.iter().map(|r| r.0).max().unwrap_or(0) + 1;
    let mut counts = vec![0; layers];
    for (rank, _) in &ranks {
        counts[*rank] += 1;
    }
    let max_slots = *counts.iter().max().unwrap_or(&1);
    let vertical = matches!(graph.direction, Direction::Down | Direction::Up);
    let step = cell + egui::vec2(36., 70.) * zoom;
    let size = if vertical {
        egui::vec2(max_slots as f32 * step.x, layers as f32 * step.y)
    } else {
        egui::vec2(layers as f32 * step.x, max_slots as f32 * step.y)
    } + Vec2::splat(48. * zoom);
    let nodes = ranks
        .iter()
        .zip(sizes)
        .map(|((rank, slot), node_size)| {
            let rank = if matches!(graph.direction, Direction::Up | Direction::Left) {
                layers - 1 - rank
            } else {
                *rank
            };
            (rank, *slot, node_size)
        })
        .collect::<Vec<_>>();
    let nodes = nodes
        .into_iter()
        .enumerate()
        .map(|(i, (rank, slot, node_size))| {
            let slots = counts[ranks[i].0];
            let across = slot as f32 + (max_slots - slots) as f32 / 2. + 0.5;
            let along = rank as f32 + 0.5;
            let center = if vertical {
                egui::vec2(across * step.x, along * step.y)
            } else {
                egui::vec2(along * step.x, across * step.y)
            } + Vec2::splat(24. * zoom);
            Rect::from_center_size(Pos2::ZERO + center, node_size)
        })
        .collect();
    Layout {
        nodes,
        labels,
        size,
    }
}
/// Reserve a content-sized viewport even below other blocks in a scrolling parent.
/// `available_height` is only the space left below the cursor, so it can collapse
/// nested scroll areas to a thin strip. Use the full visible panel as the cap.
pub(crate) fn viewport_height(ui: &egui::Ui, content_height: f32) -> f32 {
    let panel_height = ui
        .clip_rect()
        .height()
        .min(ui.ctx().content_rect().height());
    let cap = (panel_height - 64.).max(180.);
    content_height.min(cap).max(1.)
}

/// Returns a response so description previews retain their normal double-click editing.
pub fn show(ui: &mut egui::Ui, source: &str) -> egui::Response {
    if mermaid::is_sequence(source) {
        return crate::sequence::show(ui, source);
    }
    let id = ui.next_auto_id().with("mermaid-flowchart");
    let cached = ui.ctx().data_mut(|d| {
        let cached = d.get_temp_mut_or_insert_with(id, || Cached {
            source: source.into(),
            graph: Arc::new(mermaid::parse(source)),
        });
        if cached.source != source {
            cached.source = source.into();
            cached.graph = Arc::new(mermaid::parse(source));
        }
        cached.graph.clone()
    });
    egui::Frame::new()
        .fill(design::colors(ui).column)
        .stroke(Stroke::new(1., design::colors(ui).line))
        .corner_radius(6)
        .inner_margin(10)
        .show(ui, |ui| {
            let graph = match cached.as_ref() {
                Ok(graph) => graph,
                Err(error) => {
                    ui.colored_label(design::colors(ui).red, format!("Flowchart: {error}"));
                    ui.label("Edit the Mermaid source to correct the diagram.");
                    return;
                }
            };
            let natural = layout(ui, graph, 1.);
            let fit = (ui.available_width() / natural.size.x).clamp(0.5, 1.);
            let mut zoom = ui
                .ctx()
                .data(|d| d.get_temp::<f32>(id.with("zoom")))
                .unwrap_or(fit);
            let previous = ui
                .ctx()
                .data(|d| d.get_temp::<Viewport>(id.with("viewport")));
            let mut gesture = ui
                .ctx()
                .data(|d| d.get_temp::<bool>(id.with("wheel-zoom")))
                .unwrap_or(false);
            let mut wheel_anchor = None;
            if let Some(view) = previous {
                let hovered = ui.is_enabled()
                    && !egui::Popup::is_any_open(ui.ctx())
                    && ui.input(|i| {
                        i.pointer
                            .hover_pos()
                            .is_some_and(|p| view.rect.intersect(ui.clip_rect()).contains(p))
                    });
                if hovered {
                    let delta = ui.input_mut(|i| {
                        if let Some(alt) = i.events.iter().rev().find_map(|event| match event {
                            egui::Event::MouseWheel { modifiers, .. } => Some(modifiers.alt),
                            _ => None,
                        }) {
                            gesture = alt;
                        }
                        if gesture {
                            let delta = if i.smooth_scroll_delta.y != 0. {
                                i.smooth_scroll_delta.y
                            } else {
                                i.smooth_scroll_delta.x
                            };
                            i.smooth_scroll_delta = Vec2::ZERO;
                            if !i.is_scrolling() && delta == 0. {
                                gesture = false;
                            }
                            delta
                        } else {
                            0.
                        }
                    });
                    if delta != 0. {
                        zoom = (zoom * (delta * 0.003).exp()).clamp(0.35, 2.);
                        wheel_anchor = ui.input(|i| i.pointer.hover_pos());
                    }
                } else {
                    gesture = false;
                }
            }
            ui.ctx()
                .data_mut(|d| d.insert_temp(id.with("wheel-zoom"), gesture));
            let mut reset_scroll = false;
            ui.horizontal(|ui| {
                ui.label("Flowchart");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Copy source").clicked() {
                        ui.ctx().copy_text(source.to_owned());
                    }
                    if ui.small_button("Fit").clicked() {
                        zoom = fit;
                        reset_scroll = true;
                    }
                    if ui
                        .small_button("+")
                        .on_hover_text("Zoom in (Alt + scroll up)")
                        .clicked()
                    {
                        zoom = (zoom * 1.25).min(2.);
                    }
                    if ui
                        .small_button("−")
                        .on_hover_text("Zoom out (Alt + scroll down)")
                        .clicked()
                    {
                        zoom = (zoom / 1.25).max(0.35);
                    }
                });
            });
            ui.ctx().data_mut(|d| d.insert_temp(id.with("zoom"), zoom));
            let target_offset = previous
                .filter(|view| wheel_anchor.is_some() && (view.zoom - zoom).abs() > f32::EPSILON)
                .map(|view| {
                    let anchor = wheel_anchor.unwrap_or(view.rect.center()) - view.rect.min;
                    ((view.offset + anchor) * (zoom / view.zoom) - anchor).max(Vec2::ZERO)
                });
            let layout = layout(ui, graph, zoom);
            ui.scope(|ui| {
                // Diagrams need visible navigation controls even though ordinary app lists hide bars.
                ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                let height = viewport_height(ui, layout.size.y);
                let mut scrolling = egui::ScrollArea::both()
                    .id_salt(id)
                    .scroll_source(egui::scroll_area::ScrollSource::ALL)
                    .on_hover_cursor(egui::CursorIcon::Grab)
                    .on_drag_cursor(egui::CursorIcon::Grabbing)
                    .max_height(height)
                    .min_scrolled_height(height)
                    .auto_shrink([false, true]);
                if reset_scroll || target_offset.is_some() {
                    egui::scroll_area::State::default()
                        .store(ui.ctx(), ui.make_persistent_id(egui::IdSalt::new(id)));
                    scrolling = scrolling.scroll_offset(if reset_scroll {
                        Vec2::ZERO
                    } else {
                        target_offset.unwrap()
                    });
                }
                let output = scrolling.show(ui, |ui| {
                    let (rect, _) = ui.allocate_exact_size(layout.size, Sense::hover());
                    paint(
                        ui.painter(),
                        graph,
                        &layout,
                        rect.min,
                        zoom,
                        design::colors(ui),
                    );
                });
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        id.with("viewport"),
                        Viewport {
                            rect: output.inner_rect,
                            offset: output.state.offset,
                            zoom,
                        },
                    )
                });
            });
        })
        .response
}
fn point(points: [Pos2; 4], t: f32) -> Pos2 {
    let u = 1. - t;
    Pos2::ZERO
        + points[0].to_vec2() * u * u * u
        + points[1].to_vec2() * 3. * u * u * t
        + points[2].to_vec2() * 3. * u * t * t
        + points[3].to_vec2() * t * t * t
}
fn paint(
    painter: &egui::Painter,
    graph: &Flowchart,
    layout: &Layout,
    origin: Pos2,
    zoom: f32,
    p: design::Palette,
) {
    let rects = layout
        .nodes
        .iter()
        .map(|r| r.translate(origin.to_vec2()))
        .collect::<Vec<_>>();
    for group in &graph.groups {
        if let Some(rect) = group.nodes.iter().map(|i| rects[*i]).reduce(Rect::union) {
            let mut rect = rect.expand(14. * zoom);
            rect.min.y -= 26. * zoom;
            painter.rect_filled(rect, 6, p.blue_tint.gamma_multiply(0.4));
            painter.rect_stroke(rect, 6, Stroke::new(1., p.line), StrokeKind::Inside);
            painter.text(
                rect.min + egui::vec2(8., 6.) * zoom,
                egui::Align2::LEFT_TOP,
                &group.title,
                FontId::proportional(12. * zoom),
                p.muted,
            );
        }
    }
    let mut labels = Vec::new();
    for edge in &graph.edges {
        let a = rects[edge.from];
        let b = rects[edge.to];
        let vertical = matches!(graph.direction, Direction::Down | Direction::Up);
        let points = if edge.from == edge.to {
            [
                a.right_center(),
                a.right_center() + egui::vec2(70., 0.) * zoom,
                a.center_top() + egui::vec2(0., -70.) * zoom,
                a.center_top(),
            ]
        } else if vertical {
            let (start, end, sign) = if a.center().y < b.center().y {
                (a.center_bottom(), b.center_top(), 1.)
            } else {
                (a.center_top(), b.center_bottom(), -1.)
            };
            let reach = ((start.y - end.y).abs() * 0.5).max(30. * zoom);
            [
                start,
                start + egui::vec2(0., sign * reach),
                end - egui::vec2(0., sign * reach),
                end,
            ]
        } else {
            let (start, end, sign) = if a.center().x < b.center().x {
                (a.right_center(), b.left_center(), 1.)
            } else {
                (a.left_center(), b.right_center(), -1.)
            };
            let reach = ((start.x - end.x).abs() * 0.5).max(30. * zoom);
            [
                start,
                start + egui::vec2(sign * reach, 0.),
                end - egui::vec2(sign * reach, 0.),
                end,
            ]
        };
        let stroke = Stroke::new(
            if edge.line == Line::Thick {
                3. * zoom
            } else {
                1.5 * zoom
            },
            p.muted,
        );
        if edge.line == Line::Dotted {
            for segment in 0..36 {
                if segment % 3 != 2 {
                    painter.line_segment(
                        [
                            point(points, segment as f32 / 36.),
                            point(points, (segment + 1) as f32 / 36.),
                        ],
                        stroke,
                    );
                }
            }
        } else {
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                points,
                false,
                Color32::TRANSPARENT,
                stroke,
            ));
        }
        let arrow = |from: Pos2, tip: Pos2| {
            let dir = (tip - from).normalized();
            let normal = egui::vec2(-dir.y, dir.x);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    tip,
                    tip - dir * 9. * zoom + normal * 4. * zoom,
                    tip - dir * 9. * zoom - normal * 4. * zoom,
                ],
                p.muted,
                Stroke::NONE,
            ));
        };
        if edge.start_arrow {
            arrow(points[1], points[0]);
        }
        if edge.end_arrow {
            arrow(points[2], points[3]);
        }
        if !edge.label.is_empty() {
            labels.push((point(points, 0.5), &edge.label));
        }
    }
    for ((node, rect), label) in graph.nodes.iter().zip(&rects).zip(&layout.labels) {
        if !rect.intersects(painter.clip_rect()) {
            continue;
        }
        let stroke = Stroke::new(1.4 * zoom, p.accent);
        match node.shape {
            Shape::Circle => {
                painter.circle(rect.center(), rect.width() / 2., p.card, stroke);
            }
            Shape::Diamond => {
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        rect.center_top(),
                        rect.right_center(),
                        rect.center_bottom(),
                        rect.left_center(),
                    ],
                    p.card,
                    stroke,
                ));
            }
            _ => {
                painter.rect_filled(
                    *rect,
                    if node.shape == Shape::Rectangle {
                        2
                    } else if node.shape == Shape::Stadium {
                        255
                    } else {
                        10
                    },
                    p.card,
                );
                painter.rect_stroke(
                    *rect,
                    if node.shape == Shape::Rectangle {
                        2
                    } else if node.shape == Shape::Stadium {
                        255
                    } else {
                        10
                    },
                    stroke,
                    StrokeKind::Inside,
                );
                if node.shape == Shape::Database {
                    let center = rect.center_top() + egui::vec2(0., 8. * zoom);
                    let radius = egui::vec2(rect.width() / 2., 8. * zoom);
                    painter.add(egui::epaint::EllipseShape::filled(center, radius, p.card));
                    painter.add(egui::epaint::EllipseShape::stroke(center, radius, stroke));
                }
            }
        }
        painter.galley(rect.center() - label.size() / 2., label.clone(), p.ink);
    }
    for (center, text) in labels {
        let label = painter.layout_no_wrap(text.clone(), FontId::proportional(12. * zoom), p.ink);
        painter.rect_filled(
            Rect::from_center_size(center, label.size() + egui::vec2(8., 4.) * zoom),
            3,
            p.column,
        );
        painter.galley(center - label.size() / 2., label, p.ink);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(ctx: &egui::Context, source: &str) -> egui::FullOutput {
        frame_events(ctx, source, Vec::new())
    }
    fn frame_events(
        ctx: &egui::Context,
        source: &str,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000., 800.))),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    show(ui, source);
                });
            },
        );
        output.textures_delta.clear();
        output
    }
    fn has_text(output: &egui::FullOutput, value: &str) -> bool {
        output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==value && s.clip_rect.contains_rect(Rect::from_min_size(t.pos,t.galley.size()))))
    }
    #[test]
    fn renders_unicode_flowcharts_and_replaces_cached_source() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let source = "flowchart TD\nOS[Приложения / ОС] --> CPU[CPU: ядра]\nCPU <-- PCIe --> GPU[GPU]\nPLATFORM[Платформа] -.-> CPU";
        let mut output = frame(&ctx, source);
        for _ in 0..2 {
            output = frame(&ctx, source);
        }
        assert!(has_text(&output, "Приложения / ОС"));
        assert!(has_text(&output, "PCIe"));
        assert!(has_text(&output, "Платформа"));
        let output = frame(&ctx, "flowchart LR\nA[Changed] --> B[Next]");
        assert!(has_text(&output, "Changed"));
        assert!(!has_text(&output, "Платформа"));
        let output = frame(&ctx, "flowchart TD\nA[broken");
        assert!(output.shapes.iter().any(
            |s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().starts_with("Flowchart:"))
        ));
    }
    #[test]
    fn respects_directions_and_keeps_node_boxes_separate() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for direction in ["TD", "BT", "LR", "RL"] {
                let graph = mermaid::parse(&format!(
                    "flowchart {direction}\nA[Start] --> B[Next]\nB --> C[End]\nA --> D[Another]"
                ))
                .unwrap();
                let layout = layout(ui, &graph, 1.);
                for (i, a) in layout.nodes.iter().enumerate() {
                    for b in &layout.nodes[i + 1..] {
                        assert!(!a.intersects(*b));
                    }
                }
                let a = layout.nodes[0].center();
                let b = layout.nodes[1].center();
                match direction {
                    "TD" => assert!(a.y < b.y),
                    "BT" => assert!(a.y > b.y),
                    "LR" => assert!(a.x < b.x),
                    _ => assert!(a.x > b.x),
                }
            }
        });
        output.textures_delta.clear();
    }
    fn text_rect(output: &egui::FullOutput, value: &str) -> Rect {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == value => {
                    Some(Rect::from_min_size(t.pos, t.galley.size()))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("Missing {value}"))
    }
    fn pointer(ctx: &egui::Context, source: &str, pos: Pos2, pressed: bool) -> egui::FullOutput {
        frame_events(
            ctx,
            source,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )
    }
    #[test]
    fn zoomed_diagrams_pan_in_both_directions_and_fit_resets_navigation() {
        for direction in ["LR", "TD"] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            let source = format!(
                "flowchart {direction}\nA[Start] --> B[Second] --> C[Third] --> D[Fourth] --> E[Fifth] --> F[Sixth] --> G[Seventh] --> H[End]"
            );
            let mut output = frame(&ctx, &source);
            for _ in 0..3 {
                output = frame(&ctx, &source);
            }
            for _ in 0..4 {
                let plus = text_rect(&output, "+").center();
                pointer(&ctx, &source, plus, true);
                pointer(&ctx, &source, plus, false);
                output = frame(&ctx, &source);
            }
            let before_wheel = text_rect(&output, "Second");
            let wheel = if direction == "LR" {
                egui::vec2(-50., 0.)
            } else {
                egui::vec2(0., -50.)
            };
            frame_events(
                &ctx,
                &source,
                vec![
                    egui::Event::PointerMoved(before_wheel.center()),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        phase: egui::TouchPhase::Move,
                        delta: wheel,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            let mut output = frame(&ctx, &source);
            for _ in 0..3 {
                output = frame(&ctx, &source);
            }
            let after_wheel = text_rect(&output, "Second");
            if direction == "LR" {
                assert!(after_wheel.left() < before_wheel.left() - 1.);
            } else {
                assert!(after_wheel.top() < before_wheel.top() - 1.);
            }
            let before = text_rect(&output, "Second");
            let delta = if direction == "LR" {
                egui::vec2(-120., 0.)
            } else {
                egui::vec2(0., -120.)
            };
            pointer(&ctx, &source, before.center(), true);
            frame_events(
                &ctx,
                &source,
                vec![egui::Event::PointerMoved(before.center() + delta)],
            );
            pointer(&ctx, &source, before.center() + delta, false);
            let output = frame(&ctx, &source);
            let after = text_rect(&output, "Second");
            if direction == "LR" {
                assert!(after.left() < before.left() - 50., "{before:?} → {after:?}");
            } else {
                assert!(after.top() < before.top() - 50., "{before:?} → {after:?}");
            }
            let fit = text_rect(&output, "Fit").center();
            pointer(&ctx, &source, fit, true);
            pointer(&ctx, &source, fit, false);
            let mut output = frame(&ctx, &source);
            for _ in 0..8 {
                output = frame(&ctx, &source);
            }
            assert!(has_text(&output, "Start"), "{direction}");
        }
    }
    #[test]
    fn alt_scroll_zooms_around_pointer_and_is_limited_to_the_diagram() {
        for unit in [egui::MouseWheelUnit::Point, egui::MouseWheelUnit::Line] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            let source = "flowchart LR\nA[Start] --> B[Second] --> C[Third] --> D[Fourth] --> E[Fifth] --> F[Sixth] --> G[Seventh] --> H[End]";
            let mut output = frame(&ctx, source);
            for _ in 0..3 {
                output = frame(&ctx, source);
            }
            let before = text_rect(&output, "Second");
            let anchor = before.center();
            let delta = if unit == egui::MouseWheelUnit::Point {
                80.
            } else {
                2.
            };
            frame_events(
                &ctx,
                source,
                vec![
                    egui::Event::ModifiersChanged(egui::Modifiers::ALT),
                    egui::Event::PointerMoved(anchor),
                    egui::Event::MouseWheel {
                        unit,
                        phase: egui::TouchPhase::Move,
                        delta: egui::vec2(0., delta),
                        modifiers: egui::Modifiers::ALT,
                    },
                ],
            );
            for _ in 0..8 {
                output = frame(&ctx, source);
            }
            let larger = text_rect(&output, "Second");
            assert!(
                larger.width() > before.width() + 1.,
                "{before:?} → {larger:?}"
            );
            assert!(
                (larger.center().x - anchor.x).abs() < 5.,
                "anchor moved: {anchor:?} → {:?}",
                larger.center()
            );
            frame_events(
                &ctx,
                source,
                vec![
                    egui::Event::PointerMoved(anchor),
                    egui::Event::MouseWheel {
                        unit,
                        phase: egui::TouchPhase::Move,
                        delta: egui::vec2(0., -delta),
                        modifiers: egui::Modifiers::ALT,
                    },
                ],
            );
            for _ in 0..8 {
                output = frame(&ctx, source);
            }
            let smaller = text_rect(&output, "Second");
            assert!(smaller.width() < larger.width() - 1.);
            // Wheel events over the toolbar must not change the diagram's zoom.
            let outside = text_rect(&output, "Flowchart").center();
            frame_events(
                &ctx,
                source,
                vec![
                    egui::Event::PointerMoved(outside),
                    egui::Event::MouseWheel {
                        unit,
                        phase: egui::TouchPhase::Move,
                        delta: egui::vec2(0., delta),
                        modifiers: egui::Modifiers::ALT,
                    },
                ],
            );
            for _ in 0..8 {
                output = frame(&ctx, source);
            }
            assert!((text_rect(&output, "Second").width() - smaller.width()).abs() < 0.1);
        }
    }
}
