//! Resizable export preview and a shared renderer for the preview and PDF image.
use super::*;
use egui::{Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
pub(super) struct Capture {
    pub area: Rect,
    pub include_text: bool,
    view: Rect,
    drag: Option<(usize, Rect, Pos2)>,
}
pub(super) fn card_rects(board: &Board) -> BTreeMap<String, Rect> {
    let positions = board.positions();
    board
        .cards
        .iter()
        .map(|c| {
            let p = positions[&c.id];
            (
                c.id.clone(),
                Rect::from_min_size(egui::pos2(p[0], p[1]), egui::vec2(c.size[0], c.size[1])),
            )
        })
        .collect()
}
pub(super) fn captured_board(board: &Board, area: Rect) -> Board {
    let bounds = card_rects(board);
    let mut captured = board.clone();
    captured
        .cards
        .retain(|card| area.contains_rect(bounds[&card.id]));
    for card in &mut captured.cards {
        let p = bounds[&card.id].min;
        card.position = Some([p.x, p.y]);
    }
    captured.backgrounds.retain(|background| {
        Rect::from_min_size(
            egui::pos2(background.position[0], background.position[1]),
            egui::vec2(background.size[0], background.size[1]),
        )
        .intersects(area)
    });
    captured.links.retain(|link| {
        captured.cards.iter().any(|c| c.name == link.from)
            && captured.cards.iter().any(|c| c.name == link.to)
    });
    captured
}
impl Capture {
    pub fn new(board: &Board, selected: &BTreeSet<String>) -> Self {
        let rects = card_rects(board);
        let mut all = rects
            .values()
            .copied()
            .reduce(Rect::union)
            .unwrap_or(Rect::from_min_size(Pos2::ZERO, Vec2::splat(250.)));
        let mut area = rects
            .iter()
            .filter(|(id, _)| selected.is_empty() || selected.contains(*id))
            .map(|(_, rect)| *rect)
            .reduce(Rect::union)
            .unwrap_or(all)
            .expand(24.);
        for background in &board.backgrounds {
            let rect = Rect::from_min_size(
                egui::pos2(background.position[0], background.position[1]),
                egui::vec2(background.size[0], background.size[1]),
            );
            all = all.union(rect);
            if selected.is_empty()
                || rects
                    .iter()
                    .any(|(id, card)| selected.contains(id) && rect.contains_rect(*card))
            {
                area = area.union(rect.expand(16.));
            }
        }
        Self {
            area,
            include_text: false,
            view: all.union(area).expand(100.),
            drag: None,
        }
    }
}
fn transform(area: Rect, rect: Rect) -> (f32, Pos2) {
    let scale = (rect.width() / area.width()).min(rect.height() / area.height());
    let origin = rect.center() - area.center().to_vec2() * scale;
    (scale, origin)
}
impl SquidyTab {
    pub(super) fn capture_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut capture) = self.capture.take() else {
            return;
        };
        let Some(board) = &self.board else {
            return;
        };
        let mut close = false;
        let mut export = false;
        let response = design::modal(ctx, egui::Id::new("squidy-export-dialog")).show(ctx, |ui| {
            close = design::dialog_heading(ui, "Export PDF").clicked();
            design::hint(
                ui,
                "Move or resize the capture frame. Only whole cards inside it are included.",
            );
            let max =
                (ctx.content_rect().size() - egui::vec2(100., 290.)).max(egui::vec2(260., 140.));
            egui::Resize::default()
                .id(egui::Id::new("squidy-export-preview-size"))
                .default_size(egui::vec2(720., 420.).min(max))
                .min_size(egui::vec2(260., 140.))
                .max_size(max)
                .resizable(true)
                .with_stroke(true)
                .show(ui, |ui| {
                    let rect = ui.available_rect_before_wrap();
                    ui.allocate_rect(rect, Sense::hover());
                    ui.interact(rect, egui::Id::new("squidy-export-preview"), Sense::hover());
                    let (scale, origin) = transform(capture.view, rect.shrink(12.));
                    paint_board(ui.painter(), board, capture.view, rect.shrink(12.));
                    let area = Rect::from_min_max(
                        origin + capture.area.min.to_vec2() * scale,
                        origin + capture.area.max.to_vec2() * scale,
                    );
                    // Keep the live capture identical to export: omit cards crossing its edges.
                    let captured = captured_board(board, capture.area);
                    paint_board(
                        &ui.painter().with_clip_rect(area.intersect(rect)),
                        &captured,
                        capture.view,
                        rect.shrink(12.),
                    );
                    let ink = design::colors(ui).accent;
                    // Dim everything outside the capture frame.
                    for outside in [
                        Rect::from_min_max(rect.min, egui::pos2(rect.right(), area.top())),
                        Rect::from_min_max(egui::pos2(rect.left(), area.bottom()), rect.max),
                        Rect::from_min_max(egui::pos2(rect.left(), area.top()), area.left_bottom()),
                        Rect::from_min_max(
                            area.right_top(),
                            egui::pos2(rect.right(), area.bottom()),
                        ),
                    ] {
                        ui.painter().rect_filled(
                            outside.intersect(rect),
                            0,
                            Color32::from_black_alpha(70),
                        );
                    }
                    ui.painter()
                        .rect_stroke(area, 0, Stroke::new(2., ink), StrokeKind::Inside);
                    let moving = ui
                        .interact(
                            area.intersect(rect),
                            egui::Id::new("squidy-capture-move"),
                            Sense::drag(),
                        )
                        .on_hover_cursor(egui::CursorIcon::Move);
                    if moving.drag_started() {
                        capture.drag = Some((
                            8,
                            capture.area,
                            ui.input(|i| i.pointer.press_origin()).unwrap_or(area.min),
                        ));
                    }
                    let points = [
                        area.left_top(),
                        area.center_top(),
                        area.right_top(),
                        area.right_center(),
                        area.right_bottom(),
                        area.center_bottom(),
                        area.left_bottom(),
                        area.left_center(),
                    ];
                    let mut active = moving.dragged();
                    for (i, point) in points.into_iter().enumerate() {
                        let cursor = match i {
                            0 | 4 => egui::CursorIcon::ResizeNwSe,
                            2 | 6 => egui::CursorIcon::ResizeNeSw,
                            1 | 5 => egui::CursorIcon::ResizeVertical,
                            _ => egui::CursorIcon::ResizeHorizontal,
                        };
                        let handle = ui
                            .interact(
                                Rect::from_center_size(point, Vec2::splat(16.)).intersect(rect),
                                egui::Id::new(("squidy-capture-handle", i)),
                                Sense::drag(),
                            )
                            .on_hover_cursor(cursor);
                        ui.painter().rect_filled(
                            Rect::from_center_size(point, Vec2::splat(7.)),
                            1,
                            ink,
                        );
                        if handle.drag_started() {
                            capture.drag = Some((
                                i,
                                capture.area,
                                ui.input(|i| i.pointer.press_origin()).unwrap_or(point),
                            ));
                        }
                        active |= handle.dragged();
                    }
                    if active {
                        if let Some((handle, original, press)) = capture.drag {
                            if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                                let delta = (pointer - press) / scale;
                                let mut area = original;
                                if handle == 8 {
                                    area = original.translate(delta);
                                } else {
                                    if matches!(handle, 0 | 6 | 7) {
                                        area.min.x = (original.min.x + delta.x)
                                            .min(original.max.x - 24. / scale);
                                    }
                                    if matches!(handle, 2 | 3 | 4) {
                                        area.max.x = (original.max.x + delta.x)
                                            .max(original.min.x + 24. / scale);
                                    }
                                    if matches!(handle, 0 | 1 | 2) {
                                        area.min.y = (original.min.y + delta.y)
                                            .min(original.max.y - 24. / scale);
                                    }
                                    if matches!(handle, 4 | 5 | 6) {
                                        area.max.y = (original.max.y + delta.y)
                                            .max(original.min.y + 24. / scale);
                                    }
                                }
                                capture.area = area.intersect(capture.view);
                            }
                        }
                    }
                    if ui.input(|i| i.pointer.any_released()) {
                        capture.drag = None;
                    }
                });
            let count = card_rects(board)
                .values()
                .filter(|r| capture.area.contains_rect(**r))
                .count();
            ui.label(format!("{count} cards"));
            design::checkbox(ui, &mut capture.include_text, "Include card text").on_hover_text(
                "Append each captured card’s title, brief, and description after the image",
            );
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    close = true;
                }
                export = ui
                    .add_enabled(count > 0, design::Button::new("Export PDF"))
                    .clicked();
            });
        });
        close |= response.should_close()
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if export {
            self.action = Some(Action::Export {
                area: [
                    capture.area.min.x,
                    capture.area.min.y,
                    capture.area.max.x,
                    capture.area.max.y,
                ],
                include_text: capture.include_text,
            });
        } else if !close {
            self.capture = Some(capture);
        }
    }
}

/// Draw the same card diagram in the preview and offscreen image, independent of canvas pan/zoom.
pub(super) fn paint_board(painter: &egui::Painter, board: &Board, area: Rect, rect: Rect) {
    let painter = painter.with_clip_rect(rect);
    painter.rect_filled(rect, 0, Color32::WHITE);
    let (scale, origin) = transform(area, rect);
    let map = |p: Pos2| origin + p.to_vec2() * scale;
    for background in &board.backgrounds {
        super::backgrounds::paint(&painter, background, origin, scale, false, false);
    }
    let bounds = card_rects(board);
    let mut pairs = BTreeMap::new();
    for link in &board.links {
        let key = if link.from <= link.to {
            (&link.from, &link.to)
        } else {
            (&link.to, &link.from)
        };
        pairs.entry(key).or_insert((0usize, 0usize)).0 += 1;
    }
    let mut labels = Vec::new();
    for link in &board.links {
        let (Some(from), Some(to)) = (board.card(&link.from), board.card(&link.to)) else {
            continue;
        };
        let ar = bounds[&from.id];
        let br = bounds[&to.id];
        let key = if link.from <= link.to {
            (&link.from, &link.to)
        } else {
            (&link.to, &link.from)
        };
        let (count, rank) = pairs.get_mut(&key).unwrap();
        let offset = (*rank as f32 - (*count as f32 - 1.) * 0.5) * 52.;
        *rank += 1;
        let points = if from.id == to.id {
            [
                ar.right_center(),
                ar.right_top() + Vec2::new(130., -100. - offset),
                ar.left_top() + Vec2::new(-130., -100. - offset),
                ar.left_center(),
            ]
        } else if ar.center().x <= br.center().x {
            [
                ar.right_center(),
                ar.right_center() + Vec2::new(80., offset),
                br.left_center() + Vec2::new(-80., offset),
                br.left_center(),
            ]
        } else {
            [
                ar.left_center(),
                ar.left_center() + Vec2::new(-80., offset),
                br.right_center() + Vec2::new(80., offset),
                br.right_center(),
            ]
        };
        let [a, c1, c2, b] = points.map(map);
        let color = Color32::from_rgb(link.color[0], link.color[1], link.color[2]);
        painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
            [a, c1, c2, b],
            false,
            Color32::TRANSPARENT,
            Stroke::new((1.6 * scale).max(0.5), color),
        ));
        let arrow = |from: Pos2, tip: Pos2| {
            let direction = (tip - from).normalized();
            let normal = Vec2::new(-direction.y, direction.x);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    tip,
                    tip - direction * 10. * scale + normal * 4. * scale,
                    tip - direction * 10. * scale - normal * 4. * scale,
                ],
                color,
                Stroke::NONE,
            ));
        };
        arrow(c2, b);
        if !link.directed {
            arrow(c1, a);
        }
        let center = Pos2::ZERO
            + (a.to_vec2() + c1.to_vec2() * 3. + c2.to_vec2() * 3. + b.to_vec2()) / 8.
            + egui::vec2(0., -12. * scale);
        labels.push((center, &link.title));
    }
    for card in &board.cards {
        let world = bounds[&card.id];
        let r = Rect::from_min_max(map(world.min), map(world.max));
        if !r.intersects(rect) {
            continue;
        }
        let color = Color32::from_rgb(card.color[0], card.color[1], card.color[2]);
        painter.rect_filled(r, 4, Color32::WHITE.lerp_to_gamma(color, 0.1));
        painter.rect_stroke(r, 4, Stroke::new(scale.max(0.5), color), StrokeKind::Inside);
        let clipped = painter.with_clip_rect(r.shrink(10. * scale).intersect(rect));
        let mut title = egui::text::LayoutJob::simple(
            card.name.clone(),
            FontId::proportional((17. * scale).max(1.)),
            Color32::from_gray(25),
            (r.width() - 24. * scale).max(1.),
        );
        title.wrap.max_rows = 1;
        let title = clipped.layout_job(title);
        clipped.galley(
            r.min + egui::vec2(12., 10.) * scale,
            title,
            Color32::from_gray(25),
        );
        let mut brief = egui::text::LayoutJob::simple(
            card.brief.clone(),
            FontId::proportional((13. * scale).max(1.)),
            Color32::from_gray(75),
            (r.width() - 24. * scale).max(1.),
        );
        brief.wrap.max_rows = ((card.size[1] - 53.) / 16.).max(1.) as usize;
        let brief = clipped.layout_job(brief);
        clipped.galley(
            r.min + egui::vec2(12., 43.) * scale,
            brief,
            Color32::from_gray(75),
        );
    }
    for (center, label) in labels {
        let text = painter.layout_no_wrap(
            label.clone(),
            FontId::proportional((11. * scale).max(1.)),
            Color32::from_gray(60),
        );
        let rect = Rect::from_center_size(center, text.size() + egui::vec2(6., 4.) * scale);
        painter.rect_filled(rect, 2, Color32::WHITE);
        painter.galley(center - text.size() * 0.5, text, Color32::from_gray(60));
    }
}

pub(super) struct Raster {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}
pub(super) fn raster(board: &Board, area: Rect) -> Raster {
    let scale = (1800. / area.width()).min(1400. / area.height());
    let width = (area.width() * scale).round().max(1.) as usize;
    let height = (area.height() * scale).round().max(1.) as usize;
    let ctx = egui::Context::default();
    design::install(&ctx);
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(width as f32, height as f32));
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        },
        |ui| {
            paint_board(ui.painter(), board, area, rect);
        },
    );
    let primitives = ctx.tessellate(std::mem::take(&mut output.shapes), 1.);
    let mut atlas = None;
    if let Some(deltas) = output.textures_delta.set.get(&egui::TextureId::Managed(0)) {
        for delta in deltas {
            let egui::ImageData::Color(image) = &delta.image;
            if let Some([x, y]) = delta.pos {
                if let Some(base) = atlas.as_mut() {
                    let base: &mut egui::ColorImage = base;
                    for row in 0..image.size[1] {
                        let at = (y + row) * base.size[0] + x;
                        base.pixels[at..at + image.size[0]].copy_from_slice(
                            &image.pixels[row * image.size[0]..(row + 1) * image.size[0]],
                        );
                    }
                }
            } else {
                atlas = Some((**image).clone());
            }
        }
    }
    let mut rgb = vec![255; width * height * 3];
    if let Some(atlas) = atlas {
        for primitive in primitives {
            let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
                continue;
            };
            for triangle in mesh.indices.chunks_exact(3) {
                let vertices = triangle
                    .iter()
                    .map(|i| mesh.vertices[*i as usize])
                    .collect::<Vec<_>>();
                let a = vertices[0];
                let b = vertices[1];
                let c = vertices[2];
                let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
                let det = cross(b.pos - a.pos, c.pos - a.pos);
                if det.abs() < 0.00001 {
                    continue;
                }
                let bounds = Rect::from_points(&[a.pos, b.pos, c.pos])
                    .intersect(primitive.clip_rect)
                    .intersect(rect);
                for y in bounds.top().floor().max(0.) as usize
                    ..bounds.bottom().ceil().min(height as f32) as usize
                {
                    for x in bounds.left().floor().max(0.) as usize
                        ..bounds.right().ceil().min(width as f32) as usize
                    {
                        let point = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
                        if !primitive.clip_rect.contains(point) {
                            continue;
                        }
                        let wb = cross(point - a.pos, c.pos - a.pos) / det;
                        let wc = cross(b.pos - a.pos, point - a.pos) / det;
                        let wa = 1. - wb - wc;
                        if wa < 0. || wb < 0. || wc < 0. {
                            continue;
                        }
                        let uv = a.uv.to_vec2() * wa + b.uv.to_vec2() * wb + c.uv.to_vec2() * wc;
                        let tx = (uv.x * atlas.size[0] as f32)
                            .floor()
                            .clamp(0., (atlas.size[0] - 1) as f32)
                            as usize;
                        let ty = (uv.y * atlas.size[1] as f32)
                            .floor()
                            .clamp(0., (atlas.size[1] - 1) as f32)
                            as usize;
                        let texture = atlas.pixels[ty * atlas.size[0] + tx].to_array();
                        let ca = a.color.to_array();
                        let cb = b.color.to_array();
                        let cc = c.color.to_array();
                        let channel = |i: usize| {
                            (ca[i] as f32 * wa + cb[i] as f32 * wb + cc[i] as f32 * wc) / 255.
                                * texture[i] as f32
                                / 255.
                        };
                        let alpha = channel(3);
                        let at = (y * width + x) * 3;
                        for i in 0..3 {
                            rgb[at + i] = (channel(i) * 255. + rgb[at + i] as f32 * (1. - alpha))
                                .clamp(0., 255.)
                                .round() as u8;
                        }
                    }
                }
            }
        }
    }
    output.textures_delta.clear();
    Raster { width, height, rgb }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_keeps_whole_cards_at_original_positions() {
        let mut board = Board::new("Research");
        for name in ["Alpha", "Beta", "Gamma"] {
            board.cards.push(Card::new(name));
        }
        let bounds = card_rects(&board);
        let selected = BTreeSet::from([board.cards[1].id.clone()]);
        let capture = Capture::new(&board, &selected);
        let captured = captured_board(&board, capture.area);
        assert_eq!(captured.cards.len(), 1);
        assert_eq!(captured.cards[0].name, "Beta");
        assert_eq!(
            card_rects(&captured)[&captured.cards[0].id],
            bounds[&board.cards[1].id]
        );
        let mut partial = bounds[&board.cards[1].id];
        partial.max.x -= 1.;
        assert!(captured_board(&board, partial).cards.is_empty());
    }
}
