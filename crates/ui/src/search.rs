//! Shared expanding search control for both tabs.
use crate::design::{self, Icon, colors};
use egui::{Id, Vec2};

#[derive(Default)]
pub struct SearchField {
    pub open: bool,
    focus_requested: bool,
    amount: f32,
    initial_query: String,
    focused: bool,
}
impl SearchField {
    pub fn width(&mut self, ui: &egui::Ui, id: &str, expanded: f32) -> f32 {
        self.amount = design::animate_toggle(ui.ctx(), Id::new((id, "width")), self.open, 0.22);
        design::CONTROL_HEIGHT + (expanded - design::CONTROL_HEIGHT) * self.amount
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        query: &mut String,
        width: f32,
        hint: &str,
    ) {
        let input_id = Id::new(format!("{id}-input"));
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(width, design::CONTROL_HEIGHT),
            egui::Sense::hover(),
        );
        if self.open || self.amount > 0.0 {
            let had_focus = self.focused || ui.memory(|m| m.has_focus(input_id));
            let keys = design::ActionKeys::capture(ui.ctx());
            let shortcut = if had_focus {
                keys.in_ui(ui, true)
            } else {
                Default::default()
            };
            let just_opened = std::mem::take(&mut self.focus_requested);
            if just_opened {
                ui.memory_mut(|m| m.request_focus(input_id));
            }
            let response = ui
                .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    ui.set_clip_rect(ui.clip_rect().intersect(rect));
                    ui.multiply_opacity(self.amount);
                    ui.put(
                        rect,
                        design::input(query)
                            .id(input_id)
                            .desired_width(width)
                            .interactive(self.open)
                            .hint_text(hint),
                    )
                })
                .inner;
            let outside = ui.input(|i| i.pointer.any_pressed()) && !response.contains_pointer();
            if shortcut.cancel {
                *query = self.initial_query.clone();
            }
            if self.open
                && !just_opened
                && (response.lost_focus() || outside || shortcut.cancel || shortcut.confirm)
            {
                self.open = false;
                response.surrender_focus();
                ui.ctx().request_repaint();
            }
        }
        self.focused = self.open && ui.memory(|m| m.has_focus(input_id));
        let icon_rect = egui::Rect::from_min_max(
            egui::pos2(rect.right() - design::CONTROL_HEIGHT, rect.top()),
            rect.max,
        );
        if self.open {
            design::paint_icon(
                ui.painter(),
                icon_rect.shrink(8.0),
                Icon::Search,
                colors(ui).muted.gamma_multiply(1.0 - self.amount),
            );
        } else {
            ui.scope_builder(egui::UiBuilder::new().max_rect(icon_rect), |ui| {
                ui.multiply_opacity(1.0 - self.amount);
                let response = design::icon_button(
                    ui,
                    Id::new(id),
                    Icon::Search,
                    hint.trim_end_matches('…'),
                    design::CONTROL_HEIGHT,
                );
                if !query.is_empty() {
                    ui.painter().circle_filled(
                        response.rect.right_top() + Vec2::new(-6.0, 6.0),
                        3.0,
                        colors(ui).accent,
                    );
                    response.clone().on_hover_text(format!("Search: {query}"));
                }
                if response.clicked() {
                    self.initial_query = query.clone();
                    self.open = true;
                    self.focus_requested = true;
                    ui.ctx().request_repaint();
                }
            });
        }
    }
}
