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
/// A centered caption with a reserved arrow gutter, including when the caption truncates.
pub fn dropdown_button(ui: &mut Ui, id: egui::Id, label: &str, size: Vec2) -> Response {
    let response = ui
        .push_id(id, |ui| ui.add_sized(size, Button::new("")))
        .inner;
    let text_rect = Rect::from_min_max(
        response.rect.min + Vec2::new(6., 0.),
        response.rect.max - Vec2::new(26., 0.),
    );
    let galley = egui::WidgetText::from(label).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        text_rect.width().max(0.),
        egui::TextStyle::Button,
    );
    let text_left = (response.rect.center().x - galley.size().x * 0.5)
        .min(text_rect.right() - galley.size().x)
        .max(text_rect.left());
    ui.painter()
        .with_clip_rect(text_rect.intersect(ui.clip_rect()))
        .galley(
            egui::pos2(text_left, text_rect.center().y - galley.size().y * 0.5),
            galley,
            colors(ui).ink,
        );
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            response.rect.right_center() - Vec2::new(12., 0.),
            Vec2::splat(12.),
        ),
        Icon::ChevronDown,
        colors(ui).muted,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}
pub fn small_button<'a>(ui: &mut Ui, text: impl IntoAtoms<'a>) -> Response {
    ui.add(Button::new(text).small())
}
/// Desktop checkboxes share native atom layout, input, and accessibility semantics.
pub fn checkbox<'a>(ui: &mut Ui, checked: &'a mut bool, label: impl IntoAtoms<'a>) -> Response {
    checkbox_sized(ui, checked, label, 16.)
}
/// Smaller indicator with the same row height and label hit target.
pub fn small_checkbox<'a>(
    ui: &mut Ui,
    checked: &'a mut bool,
    label: impl IntoAtoms<'a>,
) -> Response {
    checkbox_sized(ui, checked, label, 14.)
}
fn checkbox_sized<'a>(
    ui: &mut Ui,
    checked: &'a mut bool,
    label: impl IntoAtoms<'a>,
    size: f32,
) -> Response {
    let mut atoms = label.into_atoms();
    let text = atoms.text().map(String::from);
    let icon_id = egui::Id::new("amigo-checkbox-indicator");
    let height = ui.spacing().interact_size.y.max(size);
    atoms.push_left(egui::Atom::custom(icon_id, egui::vec2(size, height)));
    // Like egui's checkbox, the whole label is clickable and keyboard focusable.
    let mut allocated = egui::AtomLayout::new(atoms)
        .gap(8.)
        .sense(egui::Sense::click())
        .min_size(Vec2::splat(height))
        .allocate(ui);
    if allocated.response.clicked() {
        *checked = !*checked;
        allocated.response.mark_changed();
    }
    allocated.response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Checkbox,
            ui.is_enabled(),
            *checked,
            text.as_deref().unwrap_or(""),
        )
    });
    let painted = allocated.paint(ui);
    if let Some(icon) = painted.rect(icon_id) {
        let rect = Rect::from_center_size(icon.center(), Vec2::splat(size));
        let p = colors(ui);
        let response = &painted.response;
        let pressed = response.is_pointer_button_down_on();
        let hovered = response.hovered();
        let fill = if *checked {
            p.primary.lerp_to_gamma(
                Color32::BLACK,
                if pressed {
                    0.18
                } else if hovered {
                    0.08
                } else {
                    0.
                },
            )
        } else if pressed || hovered {
            p.hover
        } else {
            p.card
        };
        let stroke = if *checked {
            Stroke::NONE
        } else {
            Stroke::new(1., if hovered { p.muted } else { p.hover_line })
        };
        ui.painter().rect(rect, 3, fill, stroke, StrokeKind::Inside);
        if *checked {
            let point = |x, y| rect.min + egui::vec2(x * size, y * size);
            ui.painter().add(egui::Shape::line(
                vec![point(0.23, 0.51), point(0.43, 0.71), point(0.78, 0.29)],
                Stroke::new(1.8, Color32::WHITE),
            ));
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect.expand(3.),
                5,
                Stroke::new(1.5, p.accent),
                StrokeKind::Outside,
            );
        }
    }
    painted.response
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
    selected_icon_button(ui, id, kind, label, size, false)
}
pub fn selected_icon_button(
    ui: &mut Ui,
    id: egui::Id,
    kind: Icon,
    label: &str,
    size: f32,
    selected: bool,
) -> Response {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let response = ui.interact(rect, id, egui::Sense::click());
    let amount = hover_amount(ui, id, response.hovered() || response.has_focus());
    let p = colors(ui);
    let pressed = response.is_pointer_button_down_on();
    ui.painter().rect_filled(
        rect,
        8,
        if selected {
            p.blue_tint
        } else {
            p.hover.gamma_multiply(if pressed { 1.0 } else { amount })
        },
    );
    let glyph_size = size * 0.57 * (1.0 + 0.04 * amount - if pressed { 0.04 } else { 0.0 });
    paint_icon(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(glyph_size)),
        kind,
        if selected {
            p.accent
        } else {
            p.muted.lerp_to_gamma(p.ink, amount)
        },
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
    fn dropdowns_preserve_short_labels_and_leave_a_gap_before_the_arrow() {
        for dark in [false, true] {
            for (width, label) in [
                (65., "EUR"),
                (112., "Heading 2"),
                (84., "A long category name"),
            ] {
                let ctx = egui::Context::default();
                install(&ctx);
                apply_theme(&ctx, dark);
                let mut rect = Rect::NOTHING;
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        rect = dropdown_button(
                            ui,
                            egui::Id::new("dropdown"),
                            label,
                            Vec2::new(width, 38.),
                        )
                        .rect;
                    });
                });
                output.textures_delta.clear();
                let text = output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        egui::Shape::Text(t) if !t.galley.text().is_empty() => Some(t),
                        _ => None,
                    })
                    .unwrap();
                let painted = Rect::from_min_size(text.pos, text.galley.size());
                assert!(painted.left() >= rect.left() + 5.);
                assert!(
                    painted.right() <= rect.right() - 25.,
                    "{label}: {painted:?} {rect:?}"
                );
                if label == "EUR" {
                    assert!(
                        !text.galley.elided,
                        "three-letter currencies remain readable"
                    );
                }
            }
        }
    }
    #[test]
    fn checkboxes_show_the_current_state_and_keep_label_keyboard_and_disabled_input() {
        for dark in [false, true] {
            for small in [false, true] {
                let ctx = egui::Context::default();
                install(&ctx);
                apply_theme(&ctx, dark);
                let mut checked = false;
                let mut frame = |events, enabled, focus| {
                    let mut response = None;
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            egui::CentralPanel::default().show(ui, |ui| {
                                ui.add_enabled_ui(enabled, |ui| {
                                    let r = if small {
                                        small_checkbox(ui, &mut checked, "Enable preference")
                                    } else {
                                        checkbox(ui, &mut checked, "Enable preference")
                                    };
                                    if focus {
                                        r.request_focus();
                                    }
                                    response = Some(r);
                                });
                            });
                        },
                    );
                    output.textures_delta.clear();
                    let response = response.unwrap();
                    let size = if small { 14. } else { 16. };
                    let indicator = output
                        .shapes
                        .iter()
                        .find_map(|s| match &s.shape {
                            egui::Shape::Rect(r) if r.rect.size() == Vec2::splat(size) => Some(r),
                            _ => None,
                        })
                        .expect("square checkbox indicator");
                    assert_eq!(indicator.corner_radius, egui::CornerRadius::same(3));
                    assert!(response.rect.height() >= CONTROL_HEIGHT);
                    if enabled {
                        if checked {
                            assert!(indicator.fill.b() > indicator.fill.r());
                        }
                        assert_eq!(output.shapes.iter().any(|s| matches!(
                            &s.shape,
                            egui::Shape::Path(path) if path.points.len() == 3
                                && path.stroke.color == egui::epaint::ColorMode::Solid(Color32::WHITE)
                                && path.stroke.width > 0.
                        )), checked, "a checked box must contain a visible tick in the same frame");
                    }
                    (response, checked)
                };
                let (response, value) = frame(vec![], true, false);
                assert!(!value);
                let label = egui::pos2(response.rect.right() - 3., response.rect.center().y);
                let click = || {
                    vec![
                        egui::Event::PointerMoved(label),
                        egui::Event::PointerButton {
                            pos: label,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                        egui::Event::PointerButton {
                            pos: label,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]
                };
                let (response, value) = frame(click(), true, true);
                assert!(
                    value && response.changed(),
                    "clicking the label toggles the checkbox"
                );
                let (response, value) = frame(
                    vec![egui::Event::Key {
                        key: egui::Key::Space,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    true,
                    false,
                );
                assert!(
                    !value && response.changed(),
                    "Space toggles the focused checkbox"
                );
                let (response, value) = frame(click(), false, false);
                assert!(
                    !value && !response.changed(),
                    "disabled checkboxes cannot toggle"
                );
            }
        }
    }
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
