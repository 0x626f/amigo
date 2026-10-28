use super::*;
use egui::{Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
impl SquidyTab {
    pub(super) fn canvas(&mut self, ui: &mut egui::Ui) {
        let (board_rect, _) = ui.allocate_exact_size(
            ui.available_size().max(egui::vec2(1., 174.)),
            Sense::hover(),
        );
        let rect = Rect::from_min_max(board_rect.min + Vec2::splat(54.), board_rect.max);
        self.viewport = rect;
        let response = ui.interact(
            rect,
            egui::Id::new("squidy-canvas"),
            Sense::click_and_drag(),
        );
        let p = design::colors(ui);
        ui.painter().rect_filled(board_rect, 8, p.field);
        let header_rect = Rect::from_min_size(
            board_rect.min + Vec2::splat(10.),
            Vec2::new(board_rect.width() - 20., 32.),
        );
        let mut header = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("squidy-header")
                .max_rect(header_rect),
        );
        header.set_clip_rect(header_rect.intersect(ui.clip_rect()));
        self.board_header(&mut header);
        let toolbar_rect = Rect::from_min_max(
            egui::pos2(board_rect.left() + 10., rect.top() + 10.),
            egui::pos2(board_rect.left() + 42., board_rect.bottom() - 10.),
        );
        let mut toolbar = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("squidy-toolbar")
                .max_rect(toolbar_rect),
        );
        toolbar.set_clip_rect(toolbar_rect.intersect(ui.clip_rect()));
        self.toolbar(&mut toolbar);
        let hovered = response.hovered()
            || ui.input(|i| i.pointer.hover_pos().is_some_and(|p| rect.contains(p)));
        if ui.is_enabled() && hovered && !egui::Popup::is_any_open(ui.ctx()) {
            let (zoom, scroll, delta, middle, anchor) = ui.input(|i| {
                (
                    i.zoom_delta(),
                    i.smooth_scroll_delta,
                    i.pointer.delta(),
                    i.pointer.button_down(egui::PointerButton::Middle)
                        && !i.pointer.button_pressed(egui::PointerButton::Middle),
                    i.pointer.hover_pos().unwrap_or(rect.center()),
                )
            });
            if zoom != 1. {
                self.zoom(zoom, anchor);
            }
            let pan = if middle { delta } else { scroll };
            if pan != Vec2::ZERO {
                let b = self.board.as_mut().unwrap();
                b.pan[0] = (b.pan[0] + pan.x).clamp(-10_000_000., 10_000_000.);
                b.pan[1] = (b.pan[1] + pan.y).clamp(-10_000_000., 10_000_000.);
                self.changed();
            }
        }
        if response.dragged_by(egui::PointerButton::Primary) {
            let d = response.drag_delta();
            let b = self.board.as_mut().unwrap();
            b.pan[0] += d.x;
            b.pan[1] += d.y;
            self.changed();
        }
        let board = self.board.as_ref().unwrap();
        let zoom = board.zoom;
        let origin = rect.min + Vec2::new(board.pan[0], board.pan[1]);
        let painter = ui.painter().with_clip_rect(rect);
        let spacing = 40. * zoom;
        if spacing > 10. {
            let x = (origin.x - rect.left()).rem_euclid(spacing) + rect.left();
            let y = (origin.y - rect.top()).rem_euclid(spacing) + rect.top();
            let mut cx = x;
            while cx < rect.right() {
                let mut cy = y;
                while cy < rect.bottom() {
                    painter.circle_filled(egui::pos2(cx, cy), 0.8, p.line);
                    cy += spacing;
                }
                cx += spacing;
            }
        }
        let positions: &BTreeMap<_, _> = &self.positions;
        let cards: BTreeMap<_, _> = board
            .cards
            .iter()
            .map(|card| (card.name.as_str(), card))
            .collect();
        let mut pairs = BTreeMap::<_, (usize, usize)>::new();
        for edge in &board.links {
            let key = if edge.from <= edge.to {
                (&edge.from, &edge.to)
            } else {
                (&edge.to, &edge.from)
            };
            pairs.entry(key).or_default().0 += 1;
        }
        let mut edit_link = None;
        let mut delete_link = None;
        for edge in &board.links {
            let (Some(from), Some(to)) =
                (cards.get(edge.from.as_str()), cards.get(edge.to.as_str()))
            else {
                continue;
            };
            let (Some(a), Some(b)) = (positions.get(&from.id), positions.get(&to.id)) else {
                continue;
            };
            let key = if edge.from <= edge.to {
                (&edge.from, &edge.to)
            } else {
                (&edge.to, &edge.from)
            };
            let (count, rank) = pairs.get_mut(&key).unwrap();
            let offset = (*rank as f32 - (*count as f32 - 1.) * 0.5) * 52.;
            *rank += 1;
            let ar = Rect::from_min_size(
                origin + Vec2::new(a[0], a[1]) * zoom,
                Vec2::new(250., 125.) * zoom,
            );
            let br = Rect::from_min_size(
                origin + Vec2::new(b[0], b[1]) * zoom,
                Vec2::new(250., 125.) * zoom,
            );
            let points = if from.id == to.id {
                [
                    ar.right_center(),
                    ar.right_top() + Vec2::new(130., -100. - offset) * zoom,
                    ar.left_top() + Vec2::new(-130., -100. - offset) * zoom,
                    ar.left_center(),
                ]
            } else if ar.center().x <= br.center().x {
                [
                    ar.right_center(),
                    ar.right_center() + Vec2::new(80., offset) * zoom,
                    br.left_center() + Vec2::new(-80., offset) * zoom,
                    br.left_center(),
                ]
            } else {
                [
                    ar.left_center(),
                    ar.left_center() + Vec2::new(-80., offset) * zoom,
                    br.right_center() + Vec2::new(80., offset) * zoom,
                    br.right_center(),
                ]
            };
            if !Rect::from_points(&points).expand(140.).intersects(rect) {
                continue;
            }
            let active = self
                .path
                .as_ref()
                .is_none_or(|path| path.links.contains(&edge.id));
            let color = Color32::from_rgb(edge.color[0], edge.color[1], edge.color[2])
                .gamma_multiply(if active { 1. } else { 0.25 });
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                points,
                false,
                Color32::TRANSPARENT,
                Stroke::new(
                    if self.path.is_some() && active {
                        3.
                    } else {
                        1.6
                    },
                    color,
                ),
            ));
            arrowhead(&painter, points[2], points[3], zoom, color);
            if !edge.directed {
                arrowhead(&painter, points[1], points[0], zoom, color);
            }
            if zoom >= 0.4 {
                let center = bezier(points, 0.5);
                let label: String = edge.title.chars().take(38).collect();
                let galley = painter.layout_no_wrap(
                    label,
                    FontId::proportional((12. * zoom).max(9.)),
                    p.ink,
                );
                let label_rect = Rect::from_center_size(center, galley.size() + Vec2::new(12., 7.));
                painter.rect_filled(label_rect, 5, p.canvas);
                painter.rect_stroke(label_rect, 5, Stroke::new(1., color), StrokeKind::Inside);
                painter.galley(label_rect.min + Vec2::new(6., 3.5), galley, p.ink);
                let hit = ui
                    .interact(
                        label_rect.intersect(rect),
                        egui::Id::new(("squidy-edge", &edge.id)),
                        Sense::click(),
                    )
                    .on_hover_text(&edge.title);
                if hit.clicked() && self.picking.is_none() {
                    edit_link = Some(edge.clone());
                }
                design::context_menu(&hit).show(|ui| {
                    if design::button(ui, "Edit Connection").clicked() {
                        edit_link = Some(edge.clone());
                        ui.close();
                    }
                    ui.separator();
                    if design::button(
                        ui,
                        RichText::new("Delete Connection").color(design::colors(ui).red),
                    )
                    .clicked()
                    {
                        delete_link = Some(edge.id.clone());
                        ui.close();
                    }
                });
            }
        }
        let mut open = None;
        let mut link_from = None;
        let mut remove = None;
        let mut moved = None;
        let mut picked = None;
        let alt = ui.input(|i| i.modifiers.alt);
        let choosing = ui.is_enabled() && (self.picking.is_some() || alt);
        for card in &board.cards {
            let Some(position) = positions.get(&card.id) else {
                continue;
            };
            let card_rect = Rect::from_min_size(
                origin + Vec2::new(position[0], position[1]) * zoom,
                Vec2::new(250., 125.) * zoom,
            );
            if !card_rect.intersects(rect) {
                continue;
            }
            let hit = ui
                .interact(
                    card_rect.intersect(rect),
                    egui::Id::new(("squidy-card", &card.id)),
                    Sense::click_and_drag(),
                )
                .on_hover_text(if choosing {
                    match &self.picking {
                        Some(Picking::Connection(Some(_)) | Picking::Path(Some(_))) => {
                            format!("Select {} as destination", card.name)
                        }
                        _ => format!("Select {} as starting card", card.name),
                    }
                } else {
                    format!(
                        "{}\n{}\nDouble-click to edit; drag to move",
                        card.name, card.brief
                    )
                });
            if choosing {
                hit.clone().on_hover_cursor(egui::CursorIcon::Crosshair);
            }
            if hit.clicked() {
                self.selected = Some(card.id.clone());
                if choosing {
                    picked = Some(card.name.clone());
                }
            }
            if hit.double_clicked() && !choosing {
                open = Some(card.id.clone());
            }
            if hit.drag_started_by(egui::PointerButton::Primary) && !choosing {
                let press = ui
                    .input(|i| i.pointer.press_origin())
                    .unwrap_or(card_rect.min);
                self.drag = Some((
                    card.id.clone(),
                    (press - origin) / zoom - Vec2::new(position[0], position[1]),
                ));
            }
            if hit.dragged_by(egui::PointerButton::Primary)
                && !choosing
                && let Some((id, offset)) = &self.drag
                && id == &card.id
                && let Some(pointer) = hit.interact_pointer_pos()
            {
                let world = (pointer - origin) / zoom - *offset;
                moved = Some((
                    card.id.clone(),
                    [
                        world.x.clamp(-10_000_000., 10_000_000.),
                        world.y.clamp(-10_000_000., 10_000_000.),
                    ],
                ));
            }
            if hit.drag_stopped_by(egui::PointerButton::Primary) {
                self.drag = None;
            }
            design::context_menu(&hit).show(|ui| {
                if design::button(ui, "Open Card").clicked() {
                    open = Some(card.id.clone());
                    ui.close();
                }
                if design::button(ui, "Connect From This Card").clicked() {
                    link_from = Some(card.name.clone());
                    ui.close();
                }
                ui.separator();
                if design::button(
                    ui,
                    RichText::new("Delete Card").color(design::colors(ui).red),
                )
                .clicked()
                {
                    remove = Some(card.id.clone());
                    ui.close();
                }
            });
            let tint = Color32::from_rgb(card.color[0], card.color[1], card.color[2]);
            let source = match &self.picking {
                Some(Picking::Connection(Some(name)) | Picking::Path(Some(name))) => {
                    name == &card.name
                }
                _ => false,
            };
            let active = self
                .path
                .as_ref()
                .is_none_or(|path| choosing || path.names.contains(&card.name));
            let fill = if choosing {
                p.card
                    .lerp_to_gamma(p.blue_tint, if source || hit.hovered() { 1. } else { 0.55 })
            } else {
                Color32::from(egui::lerp(
                    egui::Rgba::from(p.card)..=egui::Rgba::from(tint),
                    if active { 0.14 } else { 0.04 },
                ))
            };
            painter.rect_filled(card_rect, 8, fill);
            painter.rect_stroke(
                card_rect,
                8,
                Stroke::new(
                    if choosing {
                        if source || hit.hovered() { 3. } else { 1.5 }
                    } else if self.selected.as_deref() == Some(&card.id) {
                        2.
                    } else {
                        1.
                    },
                    if choosing || self.selected.as_deref() == Some(&card.id) {
                        p.accent
                    } else {
                        tint.gamma_multiply(if active { 1. } else { 0.3 })
                    },
                ),
                StrokeKind::Inside,
            );
            if choosing {
                let radius = (9. * zoom).max(6.);
                let center = card_rect.right_top() + Vec2::new(-20., 22.) * zoom;
                painter.circle(
                    center,
                    radius,
                    if source { p.primary } else { p.card },
                    Stroke::new(1.5, p.accent),
                );
                if source {
                    design::paint_icon(
                        &painter,
                        Rect::from_center_size(center, Vec2::splat(radius * 1.4)),
                        Icon::Check,
                        Color32::WHITE,
                    );
                }
            }
            let clipped = painter.with_clip_rect(card_rect.shrink(10. * zoom).intersect(rect));
            let mut name_job = egui::text::LayoutJob::simple(
                card.name.clone(),
                FontId::proportional((17. * zoom).max(8.)),
                p.ink,
                ((if choosing { 198. } else { 226. }) * zoom).max(1.),
            );
            name_job.wrap.max_rows = 1;
            name_job.wrap.break_anywhere = true;
            let name = clipped.layout_job(name_job);
            clipped.galley(card_rect.min + Vec2::new(12., 10.) * zoom, name, p.ink);
            let mut brief_job = egui::text::LayoutJob::simple(
                card.brief.clone(),
                FontId::proportional((13. * zoom).max(7.)),
                p.muted,
                (226. * zoom).max(1.),
            );
            brief_job.wrap.max_rows = 4;
            let brief = clipped.layout_job(brief_job);
            clipped.galley(card_rect.min + Vec2::new(12., 43.) * zoom, brief, p.muted);
        }
        if let Some((id, position)) = moved {
            self.positions.insert(id.clone(), position);
            if let Some(card) = self
                .board
                .as_mut()
                .unwrap()
                .cards
                .iter_mut()
                .find(|c| c.id == id)
            {
                card.position = Some(position);
                self.changed();
            }
        }
        if let Some(id) = open {
            self.edit_card(&id);
        }
        if let Some(from) = link_from {
            self.picking = Some(Picking::Connection(Some(from)));
        }
        if let Some(id) = remove {
            self.confirm_delete(Delete::Card(id));
        }
        if let Some(link) = edit_link {
            self.link = Some(link);
            self.link_focus = true;
        }
        if let Some(id) = delete_link {
            self.confirm_delete(Delete::Link(id));
        }
        if let Some(name) = picked {
            self.picked_card(name, alt);
        }
        let pointer = response.interact_pointer_pos().unwrap_or(rect.center());
        let world = (pointer - origin) / zoom;
        design::context_menu(&response).show(|ui| {
            if design::button(ui, "New Card Here").clicked() {
                self.new_card(Some([world.x, world.y]));
                ui.close();
            }
        });
        if self.board.as_ref().unwrap().cards.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Create a card to begin your research",
                FontId::proportional(18.),
                p.muted,
            );
        }
    }
}
fn arrowhead(painter: &egui::Painter, from: Pos2, tip: Pos2, zoom: f32, color: Color32) {
    let direction = (tip - from).normalized();
    let normal = Vec2::new(-direction.y, direction.x);
    painter.add(egui::Shape::convex_polygon(
        vec![
            tip,
            tip - direction * 10. * zoom + normal * 4. * zoom,
            tip - direction * 10. * zoom - normal * 4. * zoom,
        ],
        color,
        Stroke::NONE,
    ));
}
fn bezier(p: [Pos2; 4], t: f32) -> Pos2 {
    let u = 1. - t;
    egui::pos2(
        u * u * u * p[0].x + 3. * u * u * t * p[1].x + 3. * u * t * t * p[2].x + t * t * t * p[3].x,
        u * u * u * p[0].y + 3. * u * u * t * p[1].y + 3. * u * t * t * p[2].y + t * t * t * p[3].y,
    )
}
