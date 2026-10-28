//! A readable route summary with card names and connection direction icons.
use super::*;
use egui::{Color32, FontId, Sense, Stroke, Vec2};

const HEIGHT: f32 = 34.;
const GAP: f32 = 8.;

impl SquidyTab {
    pub(super) fn path_result(&mut self, ui: &mut egui::Ui) {
        if self.path.is_none() && self.path_message.is_empty() {
            return;
        }
        let p = design::colors(ui);
        let content_width = (ui.available_width() - 24.).max(1.);
        let mut clear = false;
        let frame = egui::Frame::new()
            .fill(p.column)
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_min_width(content_width);
                ui.spacing_mut().item_spacing.x = GAP;
                ui.horizontal_top(|ui| {
                    let width = (ui.available_width() - 32. - GAP).max(1.);
                    ui.allocate_ui_with_layout(
                        Vec2::new(width, 0.),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_width(width);
                            if let (Some(path), Some(board)) = (&self.path, &self.board) {
                                route(ui, path, board, width);
                            } else {
                                design::hint(ui, &self.path_message);
                            }
                        },
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(32., HEIGHT),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            clear = design::icon_button(
                                ui,
                                egui::Id::new("squidy-clear-path"),
                                Icon::Close,
                                "Clear Path",
                                32.,
                            )
                            .clicked();
                        },
                    );
                });
            });
        ui.interact(
            frame.response.rect,
            egui::Id::new("squidy-path-result"),
            Sense::hover(),
        );
        if clear {
            self.path = None;
            self.path_message.clear();
        }
    }
}

fn name_width(ui: &egui::Ui, name: &str, maximum: f32) -> f32 {
    (ui.painter()
        .layout_no_wrap(
            name.into(),
            FontId::proportional(14.),
            design::colors(ui).ink,
        )
        .size()
        .x
        + 20.)
        .min(maximum)
}

fn route(ui: &mut egui::Ui, path: &GraphPath, board: &Board, width: f32) {
    let p = design::colors(ui);
    let maximum = (width * 0.5).clamp(100., 220.);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(GAP);
        for (index, name) in path.names.iter().enumerate() {
            let card_width = name_width(ui, name, maximum);
            let edge = index
                .checked_sub(1)
                .and_then(|i| path.links.get(i))
                .and_then(|id| board.links.iter().find(|link| &link.id == id));
            let connector_width = edge.map_or(20., |edge| {
                (ui.painter()
                    .layout_no_wrap(edge.title.clone(), FontId::proportional(12.), p.muted)
                    .size()
                    .x
                    + 16.)
                    .clamp(48., 124.)
            });
            // Keep each connector with its destination when the route wraps.
            ui.allocate_ui_with_layout(
                Vec2::new(connector_width + GAP + card_width, HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    if let Some(edge) = edge {
                        connector(ui, edge, &path.names[index - 1], connector_width);
                    } else {
                        design::icon(ui, Icon::Route, p.accent, 20.);
                    }
                    let color = board.card(name).map_or(p.accent, |card| {
                        Color32::from_rgb(card.color[0], card.color[1], card.color[2])
                    });
                    let response = egui::Frame::new()
                        .fill(p.card.lerp_to_gamma(color, 0.10))
                        .corner_radius(7)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.add_sized(
                                [card_width - 20., 22.],
                                egui::Label::new(RichText::new(name).size(14.).strong())
                                    .truncate()
                                    .show_tooltip_when_elided(false)
                                    .selectable(false)
                                    .halign(egui::Align::Min),
                            )
                        });
                    response.inner.on_hover_text(name);
                    ui.interact(
                        response.response.rect,
                        egui::Id::new(("squidy-path-card", name)),
                        Sense::hover(),
                    );
                },
            );
        }
    });
}

fn connector(ui: &mut egui::Ui, edge: &Link, from: &str, width: f32) {
    let p = design::colors(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, HEIGHT), Sense::hover());
    let color = Color32::from_rgb(edge.color[0], edge.color[1], edge.color[2]);
    let galley = egui::WidgetText::from(RichText::new(&edge.title).size(12.).color(p.muted))
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            width - 8.,
            egui::TextStyle::Small,
        );
    ui.painter().galley(
        egui::pos2(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - 7. - galley.size().y,
        ),
        galley,
        p.muted,
    );
    let left = egui::pos2(rect.left() + 3., rect.center().y);
    let right = egui::pos2(rect.right() - 3., rect.center().y);
    let stroke = Stroke::new(1.5, color);
    ui.painter().line_segment([left, right], stroke);
    for (show, tip, sign) in [
        (!edge.directed || edge.from != from, left, 1.),
        (!edge.directed || edge.from == from, right, -1.),
    ] {
        if show {
            ui.painter().add(egui::Shape::line(
                vec![
                    tip + Vec2::new(sign * 5., -4.),
                    tip,
                    tip + Vec2::new(sign * 5., 4.),
                ],
                stroke,
            ));
        }
    }
    let direction = if !edge.directed {
        "Bidirectional"
    } else {
        "Directed"
    };
    let description = format!("{} · {direction}: {} to {}", edge.title, edge.from, edge.to);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), &description)
    });
    response.on_hover_text(description);
    ui.interact(
        rect,
        egui::Id::new(("squidy-path-connection", &edge.id)),
        Sense::hover(),
    );
}
