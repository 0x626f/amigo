//! Animated controls retain egui's layout, keyboard, and accessibility behavior.
use super::*;
use egui::{IntoAtoms, Response, Ui, Widget};

const HOVER_SECONDS: f64 = 0.14;
fn hover_amount(ui: &Ui, id: egui::Id, hovered: bool) -> f32 {
    let now = ui.input(|i| i.time);
    let key = id.with("hover-motion");
    let mut tween = ui
        .ctx()
        .data(|d| d.get_temp::<Tween>(key))
        .unwrap_or_else(|| Tween::settled(0.0));
    if !ui.is_enabled() {
        tween = Tween::settled(0.0);
    }
    tween.retarget(f32::from(hovered && ui.is_enabled()), now, HOVER_SECONDS);
    let amount = tween.value(now, HOVER_SECONDS);
    ui.ctx().data_mut(|d| d.insert_temp(key, tween));
    if amount != tween.to {
        ui.ctx().request_repaint();
    }
    amount
}

pub struct Button<'a> {
    inner: egui::Button<'a>,
    fill: Option<Color32>,
    framed: bool,
}
impl<'a> Button<'a> {
    pub fn new(atoms: impl IntoAtoms<'a>) -> Self {
        Self {
            inner: egui::Button::new(atoms)
                .stroke(Stroke::NONE)
                .corner_radius(8),
            fill: None,
            framed: true,
        }
    }
    pub fn fill(mut self, fill: impl Into<Color32>) -> Self {
        self.fill = Some(fill.into());
        self
    }
    pub fn stroke(mut self, stroke: impl Into<Stroke>) -> Self {
        self.inner = self.inner.stroke(stroke);
        self
    }
    pub fn corner_radius(mut self, radius: impl Into<egui::CornerRadius>) -> Self {
        self.inner = self.inner.corner_radius(radius);
        self
    }
    pub fn min_size(mut self, size: Vec2) -> Self {
        self.inner = self.inner.min_size(size);
        self
    }
    pub fn frame(mut self, frame: bool) -> Self {
        self.framed = frame;
        self.inner = self.inner.frame(frame);
        self
    }
    pub fn small(mut self) -> Self {
        self.inner = self.inner.small();
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.inner = self.inner.selected(selected);
        self
    }
    pub fn truncate(mut self) -> Self {
        self.inner = self.inner.truncate();
        self
    }
}
impl Widget for Button<'_> {
    fn ui(mut self, ui: &mut Ui) -> Response {
        let id = ui.next_auto_id();
        let previous = ui.ctx().read_response(id);
        let hovered = previous
            .as_ref()
            .is_some_and(|r| r.hovered() || r.has_focus());
        let pressed = ui.is_enabled()
            && ui.input(|i| i.pointer.primary_down())
            && previous.as_ref().is_some_and(|r| {
                r.is_pointer_button_down_on()
                    || (ui.input(|i| i.pointer.primary_pressed())
                        && ui
                            .input(|i| i.pointer.interact_pos())
                            .is_some_and(|p| r.rect.contains(p)))
            });
        let amount = hover_amount(ui, id, hovered);
        let original = ui.style().clone();
        let widgets = &mut ui.style_mut().visuals.widgets;
        let from = widgets.inactive;
        let to = widgets.hovered;
        let mut animated = from;
        animated.bg_fill = from.bg_fill.lerp_to_gamma(to.bg_fill, amount);
        animated.weak_bg_fill = from.weak_bg_fill.lerp_to_gamma(to.weak_bg_fill, amount);
        animated.bg_stroke.color = from
            .bg_stroke
            .color
            .lerp_to_gamma(to.bg_stroke.color, amount);
        animated.fg_stroke.color = from
            .fg_stroke
            .color
            .lerp_to_gamma(to.fg_stroke.color, amount);
        widgets.inactive = animated;
        widgets.hovered = animated;
        if let Some(fill) = self.fill {
            let hover_color = if fill.intensity() < 0.5 {
                Color32::WHITE
            } else {
                colors(ui).ink
            };
            let fill = fill.lerp_to_gamma(hover_color, amount * 0.12);
            self.inner = self.inner.fill(if pressed {
                fill.lerp_to_gamma(Color32::BLACK, 0.14)
            } else {
                fill
            });
        }
        let backdrop = ui.painter().add(egui::Shape::Noop);
        let response = self.inner.ui(ui);
        if !self.framed && (amount > 0.0 || response.is_pointer_button_down_on()) {
            ui.painter().set(
                backdrop,
                egui::Shape::rect_filled(
                    response.rect,
                    8,
                    colors(ui)
                        .hover
                        .gamma_multiply(if response.is_pointer_button_down_on() {
                            1.0
                        } else {
                            amount
                        }),
                ),
            );
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                response.rect.expand(2.),
                10,
                Stroke::new(1., colors(ui).accent),
                StrokeKind::Outside,
            );
        }
        ui.set_style(original);
        response
    }
}
pub fn button<'a>(ui: &mut Ui, text: impl IntoAtoms<'a>) -> Response {
    ui.add(Button::new(text))
}
pub fn small_button<'a>(ui: &mut Ui, text: impl IntoAtoms<'a>) -> Response {
    ui.add(Button::new(text).small())
}
/// Rounded checkboxes use a solid selected fill and retain native input behavior.
pub fn checkbox<'a>(ui: &mut Ui, checked: &'a mut bool, label: impl IntoAtoms<'a>) -> Response {
    let original = ui.style().clone();
    let p = colors(ui);
    let visuals = &mut ui.style_mut().visuals;
    // The foreground stroke paints the check mark; keep label color independent.
    visuals.override_text_color = Some(visuals.override_text_color.unwrap_or(p.ink));
    let widgets = &mut visuals.widgets;
    for visual in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        visual.corner_radius = egui::CornerRadius::same(255);
        visual.fg_stroke = Stroke::NONE;
        visual.bg_stroke = if *checked {
            visual.bg_fill = p.accent;
            Stroke::NONE
        } else {
            Stroke::new(1., p.hover_line)
        };
    }
    let response = ui.checkbox(checked, label);
    ui.set_style(original);
    if response.changed() {
        ui.ctx().request_repaint();
    }
    response
}
/// Smaller indicator with the same row height and native label hit target.
pub fn small_checkbox<'a>(
    ui: &mut Ui,
    checked: &'a mut bool,
    label: impl IntoAtoms<'a>,
) -> Response {
    let original = ui.spacing().clone();
    ui.spacing_mut().icon_width = 10.;
    ui.spacing_mut().icon_width_inner = 6.;
    let response = checkbox(ui, checked, label);
    *ui.spacing_mut() = original;
    response
}
pub fn menu_button<'a, R>(
    ui: &mut Ui,
    text: impl IntoAtoms<'a>,
    contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let response = button(ui, text);
    let inner = egui::Popup::menu(&response).show(contents).map(|r| r.inner);
    egui::InnerResponse::new(inner, response)
}
pub fn icon_button(ui: &mut Ui, id: egui::Id, kind: Icon, label: &str, size: f32) -> Response {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let response = ui.interact(rect, id, egui::Sense::click());
    let amount = hover_amount(ui, id, response.hovered() || response.has_focus());
    let p = colors(ui);
    let pressed = response.is_pointer_button_down_on();
    ui.painter().rect_filled(
        rect,
        8,
        p.hover.gamma_multiply(if pressed { 1.0 } else { amount }),
    );
    let glyph_size = size * 0.57 * (1.0 + 0.04 * amount - if pressed { 0.04 } else { 0.0 });
    paint_icon(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(glyph_size)),
        kind,
        p.muted.lerp_to_gamma(p.ink, amount),
    );
    if response.has_focus() {
        ui.painter()
            .rect_stroke(rect, 8, Stroke::new(1.0, p.accent), StrokeKind::Inside);
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}
pub fn chip(ui: &mut Ui, text: &str, selected: bool, tint: Color32) -> Response {
    let p = colors(ui);
    ui.add(
        Button::new(
            RichText::new(text)
                .size(13.0)
                .color(if selected { tint } else { p.muted }),
        )
        .fill(if selected {
            tint.lerp_to_gamma(p.card, 0.84)
        } else {
            p.column
        })
        .corner_radius(16)
        .min_size(Vec2::new(0.0, 32.0))
        .small(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hover_fades_in_out_for_text_primary_frameless_and_icon_buttons() {
        for kind in 0..4 {
            let ctx = egui::Context::default();
            install(&ctx);
            let frame = |time, pointer, enabled| {
                let mut result = None;
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        time: Some(time),
                        events: vec![egui::Event::PointerMoved(pointer)],
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            ui.add_enabled_ui(enabled, |ui| {
                                let response = match kind {
                                    0 => button(ui, "Button"),
                                    1 => primary(ui, "Primary"),
                                    2 => ui.add(Button::new("Edit").frame(false)),
                                    _ => icon_button(
                                        ui,
                                        egui::Id::new("test-icon"),
                                        Icon::Plus,
                                        "Add",
                                        32.0,
                                    ),
                                };
                                let tween = ctx
                                    .data(|d| d.get_temp::<Tween>(response.id.with("hover-motion")))
                                    .unwrap();
                                result = Some((response.rect, tween.value(time, HOVER_SECONDS)));
                            });
                        });
                    },
                );
                output.textures_delta.clear();
                if kind == 1 && time == 1.2 {
                    fn visible_hover(
                        shape: &egui::Shape,
                        center: egui::Pos2,
                        idle: Color32,
                    ) -> bool {
                        match shape {
                            egui::Shape::Rect(r) => {
                                r.rect.center() == center
                                    && r.fill != idle
                                    && r.fill != Color32::TRANSPARENT
                            }
                            egui::Shape::Vec(shapes) => {
                                shapes.iter().any(|s| visible_hover(s, center, idle))
                            }
                            _ => false,
                        }
                    }
                    assert!(
                        output.shapes.iter().any(|s| visible_hover(
                            &s.shape,
                            result.unwrap().0.center(),
                            palette(&ctx).primary
                        )),
                        "the primary button must visibly change on hover"
                    );
                }
                result.unwrap()
            };
            let (rect, amount) = frame(0.0, egui::pos2(500.0, 500.0), true);
            assert_eq!(amount, 0.0);
            frame(1.0, rect.center(), true);
            let (hover_rect, halfway) = frame(1.07, rect.center(), true);
            assert_eq!(hover_rect, rect, "hover must not move the hit target");
            assert!(halfway > 0.0 && halfway < 1.0, "kind {kind}: {halfway}");
            assert_eq!(frame(1.2, rect.center(), true).1, 1.0);
            assert_eq!(frame(2.0, egui::pos2(500.0, 500.0), true).1, 1.0);
            let fading = frame(2.07, egui::pos2(500.0, 500.0), true).1;
            assert!(fading > 0.0 && fading < 1.0);
            // Reversing direction samples the current value without a jump.
            assert_eq!(frame(2.07, rect.center(), true).1, fading);
            assert_eq!(frame(2.3, rect.center(), true).1, 1.0);
            assert_eq!(frame(3.0, rect.center(), false).1, 0.0);
        }
    }
}
