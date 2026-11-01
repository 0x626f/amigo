//! Shared single-line names. Overflow stays inside its slot and can be read with
//! a horizontal wheel, Shift+wheel, dragging, or the small scrollbar.
use crate::design::{self, colors};
use egui::{
    self, Align, Color32, Id, Rect, Response, Sense, TextStyle, Ui, Vec2, Widget, WidgetText,
};
use std::sync::Arc;
/// UI-stack tag for data tables whose horizontal wheel pans the entire table.
pub const NAME_SCROLL_SHIFT_ONLY: &str = "amigo.name-scroll.shift-only";
#[derive(Clone, Copy)]
struct Scroll {
    text: Id,
    offset: f32,
}
/// Paint within a name slot while retaining the owning widget's hit target.
pub(crate) fn paint(
    ui: &mut Ui,
    response: &Response,
    rect: Rect,
    galley: Arc<egui::Galley>,
    color: Color32,
    align: Align,
) {
    paint_inner(ui, response, rect, galley, color, align, true);
}
/// Scroll a canvas name without taking over the enclosing card's drag gesture.
pub fn paint_canvas_name(
    ui: &mut Ui,
    response: &Response,
    rect: Rect,
    galley: Arc<egui::Galley>,
    color: Color32,
    align: Align,
) {
    paint_inner(ui, response, rect, galley, color, align, false);
}
fn paint_inner(
    ui: &mut Ui,
    response: &Response,
    rect: Rect,
    galley: Arc<egui::Galley>,
    color: Color32,
    align: Align,
    allow_drag: bool,
) {
    if rect.width() <= 0. || !ui.is_rect_visible(rect) {
        return;
    }
    let max = (galley.size().x - rect.width()).max(0.);
    let key = response.id.with("name-scroll");
    let text = Id::new(galley.text());
    let mut state = ui
        .ctx()
        .data(|d| d.get_temp::<Scroll>(key))
        .filter(|s| s.text == text)
        .unwrap_or(Scroll { text, offset: 0. });
    state.offset = state.offset.clamp(0., max);
    if max > 0. && ui.is_enabled() {
        let hovered = response.hovered()
            && ui.input(|i| i.pointer.hover_pos().is_some_and(|p| rect.contains(p)));
        let focused = response.has_focus();
        let shift_only = ui
            .stack()
            .iter()
            .any(|s| s.tags().contains(NAME_SCROLL_SHIFT_ONLY));
        if hovered && (!shift_only || ui.input(|i| i.modifiers.shift)) {
            ui.input_mut(|i| {
                let horizontal = i.smooth_scroll_delta.x.abs() > 0.;
                let delta = if horizontal {
                    i.smooth_scroll_delta.x
                } else if i.modifiers.shift {
                    i.smooth_scroll_delta.y
                } else {
                    0.
                };
                let before = state.offset;
                state.offset = (state.offset - delta).clamp(0., max);
                let consumed = before - state.offset;
                if horizontal {
                    i.smooth_scroll_delta.x -= consumed;
                } else if i.modifiers.shift {
                    i.smooth_scroll_delta.y -= consumed;
                }
                if focused {
                    if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
                        state.offset = (state.offset + 48.).min(max);
                    }
                    if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
                        state.offset = (state.offset - 48.).max(0.);
                    }
                    if i.consume_key(egui::Modifiers::NONE, egui::Key::Home) {
                        state.offset = 0.;
                    }
                    if i.consume_key(egui::Modifiers::NONE, egui::Key::End) {
                        state.offset = max;
                    }
                }
            });
        }
        if allow_drag
            && response.dragged()
            && ui.input(|i| i.pointer.press_origin().is_some_and(|p| rect.contains(p)))
        {
            state.offset = (state.offset - ui.input(|i| i.pointer.delta().x)).clamp(0., max);
        }
        if response.hovered() || state.offset > 0. || response.dragged() {
            let track = Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom() - 4.),
                rect.right_bottom(),
            );
            let thumb_width = (rect.width() * rect.width() / galley.size().x)
                .clamp(12_f32.min(rect.width()), rect.width().max(0.));
            let bar = ui
                .interact(track, key.with("bar"), Sense::click_and_drag())
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            if (bar.dragged() || bar.clicked())
                && let Some(pos) = bar.interact_pointer_pos()
            {
                let travel = (track.width() - thumb_width).max(1.);
                state.offset =
                    ((pos.x - track.left() - thumb_width / 2.) / travel).clamp(0., 1.) * max;
            }
            ui.painter()
                .with_clip_rect(rect.intersect(ui.clip_rect()))
                .rect_filled(
                    Rect::from_min_size(
                        egui::pos2(
                            track.left() + (track.width() - thumb_width) * state.offset / max,
                            track.bottom() - 2.,
                        ),
                        Vec2::new(thumb_width, 2.),
                    ),
                    1,
                    colors(ui).muted.gamma_multiply(0.6),
                );
        }
    }
    let left = if max > 0. {
        rect.left() - state.offset
    } else {
        match align {
            Align::Min => rect.left(),
            Align::Center => rect.center().x - galley.size().x / 2.,
            Align::Max => rect.right() - galley.size().x,
        }
    };
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(
            egui::pos2(left, rect.center().y - galley.size().y / 2.),
            galley,
            color,
        );
    if max > 0. {
        ui.ctx().data_mut(|d| d.insert_temp(key, state));
    }
}
/// A label with the same short-caption behavior as egui and scrollable overflow.
pub struct NameLabel {
    text: WidgetText,
    align: Option<Align>,
    sense: Sense,
    tooltip: bool,
    selectable: Option<bool>,
}
impl NameLabel {
    pub fn new(text: impl Into<WidgetText>) -> Self {
        Self {
            text: text.into(),
            align: None,
            sense: Sense::hover(),
            tooltip: true,
            selectable: None,
        }
    }
    pub fn truncate(self) -> Self {
        self
    }
    pub fn halign(mut self, align: Align) -> Self {
        self.align = Some(align);
        self
    }
    pub fn sense(mut self, sense: Sense) -> Self {
        self.sense = sense;
        self
    }
    pub fn show_tooltip_when_elided(mut self, tooltip: bool) -> Self {
        self.tooltip = tooltip;
        self
    }
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = Some(selectable);
        self
    }
}
impl Widget for NameLabel {
    fn ui(self, ui: &mut Ui) -> Response {
        let galley = self.text.clone().into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            TextStyle::Body,
        );
        let width = ui.available_width().max(0.);
        if galley.size().x <= width {
            let mut label = egui::Label::new(self.text)
                .truncate()
                .sense(self.sense)
                .show_tooltip_when_elided(self.tooltip);
            if let Some(align) = self.align {
                label = label.halign(align);
            }
            if let Some(selectable) = self.selectable {
                label = label.selectable(selectable);
            }
            return ui.add(label);
        }
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(width, galley.size().y),
            self.sense | Sense::click_and_drag(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), self.text.text())
        });
        paint(
            ui,
            &response,
            rect,
            galley,
            colors(ui).ink,
            self.align.unwrap_or(Align::Min),
        );
        if self.tooltip {
            response.on_hover_text(self.text.text())
        } else {
            response
        }
    }
}
/// Native dropdown behavior with a scrollable selected-name caption.
pub struct ComboBox {
    inner: egui::ComboBox,
    text: WidgetText,
    width: Option<f32>,
}
impl ComboBox {
    pub fn from_id_salt(id: impl egui::AsIdSalt) -> Self {
        Self {
            inner: egui::ComboBox::from_id_salt(id),
            text: WidgetText::default(),
            width: None,
        }
    }
    pub fn width(mut self, width: f32) -> Self {
        self.inner = self.inner.width(width);
        self.width = Some(width);
        self
    }
    pub fn height(mut self, height: f32) -> Self {
        self.inner = self.inner.height(height);
        self
    }
    pub fn selected_text(mut self, text: impl Into<WidgetText>) -> Self {
        self.text = text.into();
        self
    }
    pub fn truncate(mut self) -> Self {
        self.inner = self.inner.truncate();
        self
    }
    pub fn show_ui<R>(
        self,
        ui: &mut Ui,
        contents: impl FnOnce(&mut Ui) -> R,
    ) -> egui::InnerResponse<Option<R>> {
        let galley = self.text.clone().into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            TextStyle::Button,
        );
        // Short captions keep the exact native layout and accessibility behavior.
        let available = ui
            .available_width()
            .min(self.width.unwrap_or(ui.spacing().combo_width))
            - ui.spacing().button_padding.x * 2.
            - ui.spacing().icon_width
            - ui.spacing().icon_spacing;
        if galley.size().x <= available {
            return self.inner.selected_text(self.text).show_ui(ui, contents);
        }
        let result = self
            .inner
            .width(
                ui.available_width()
                    .min(self.width.unwrap_or(ui.spacing().combo_width)),
            )
            .selected_text(" ")
            .show_ui(ui, contents);
        let rect = Rect::from_min_max(
            result.response.rect.min + Vec2::new(ui.spacing().button_padding.x, 0.),
            result.response.rect.max
                - Vec2::new(
                    ui.spacing().button_padding.x
                        + ui.spacing().icon_width
                        + ui.spacing().icon_spacing,
                    0.,
                ),
        );
        paint(
            ui,
            &result.response,
            rect,
            galley,
            colors(ui).ink,
            Align::Min,
        );
        result.response.widget_info(|| {
            let mut info = egui::WidgetInfo::new(egui::WidgetType::ComboBox);
            info.enabled = ui.is_enabled();
            info.current_text_value = Some(self.text.text().into());
            info
        });
        result
    }
}
/// Bounded name buttons, including flat table cells and attachment names.
pub struct NameButton<'a> {
    atoms: egui::Atoms<'a>,
    frame: bool,
}
impl<'a> NameButton<'a> {
    pub fn new(atoms: impl egui::IntoAtoms<'a>) -> Self {
        Self {
            atoms: atoms.into_atoms(),
            frame: true,
        }
    }
    pub fn truncate(self) -> Self {
        self
    }
    pub fn frame(mut self, frame: bool) -> Self {
        self.frame = frame;
        self
    }
}
impl Widget for NameButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let text = self.atoms.iter_texts().next().cloned().unwrap_or_default();
        let galley = text.clone().into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            TextStyle::Button,
        );
        let margin = if self.frame {
            ui.spacing().button_padding
        } else {
            Vec2::ZERO
        };
        let width = ui.available_width().max(0.);
        if galley.size().x <= width - margin.x * 2. {
            return ui.add(design::Button::new(self.atoms).frame(self.frame));
        }
        let response = ui.add(
            design::Button::new(" ")
                .frame(self.frame)
                .min_size(Vec2::new(width, galley.size().y + 2. * margin.y)),
        );
        let rect = response.rect.shrink2(Vec2::new(margin.x, 0.));
        paint(ui, &response, rect, galley, colors(ui).ink, Align::Min);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), text.text())
        });
        response.on_hover_text(text.text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone, Copy)]
    enum Kind {
        Label,
        Button,
        Combo,
        Dropdown,
        Nav,
        Checkbox,
        Table,
    }
    fn frame(
        ctx: &egui::Context,
        kind: Kind,
        text: &str,
        events: Vec<egui::Event>,
        time: &mut f64,
    ) -> (Response, egui::FullOutput) {
        *time += 1. / 60.;
        let mut response = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(*time),
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(400., 180.))),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.set_max_width(160.);
                    response = Some(match kind {
                        Kind::Table => {
                            ui.scope_builder(
                                egui::UiBuilder::new().ui_stack_info(
                                    egui::UiStackInfo::default().with_tag(NAME_SCROLL_SHIFT_ONLY),
                                ),
                                |ui| ui.add_sized([160., 32.], NameLabel::new(text)),
                            )
                            .inner
                        }
                        Kind::Label => {
                            ui.add_sized([160., 32.], NameLabel::new(text).sense(Sense::click()))
                        }
                        Kind::Button => ui.add_sized([160., 32.], NameButton::new(text)),
                        Kind::Combo => {
                            ComboBox::from_id_salt("test-name-combo")
                                .width(160.)
                                .selected_text(text)
                                .show_ui(ui, |ui| {
                                    ui.label("Choice");
                                })
                                .response
                        }
                        Kind::Checkbox => {
                            ui.allocate_ui_with_layout(
                                Vec2::new(160., 32.),
                                egui::Layout::top_down(Align::Min),
                                |ui| {
                                    let mut checked = false;
                                    design::checkbox(ui, &mut checked, text)
                                },
                            )
                            .inner
                        }
                        Kind::Dropdown => design::dropdown_button(
                            ui,
                            Id::new("test-name-dropdown"),
                            text,
                            Vec2::new(160., 32.),
                        ),
                        Kind::Nav => {
                            ui.allocate_ui_with_layout(
                                Vec2::new(160., 38.),
                                egui::Layout::top_down(Align::Min),
                                |ui| design::nav_row(ui, text, design::Icon::Note, false, Some(7)),
                            )
                            .inner
                        }
                    });
                });
            },
        );
        output.textures_delta.clear();
        (response.unwrap(), output)
    }
    fn wheel(x: f32, y: f32, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            phase: egui::TouchPhase::Move,
            delta: Vec2::new(x, y),
            modifiers,
        }
    }
    #[test]
    fn long_names_scroll_horizontally_in_each_shared_control_without_changing_layout() {
        let name = "A very long document board task account or category name — español Привет";
        for kind in [
            Kind::Label,
            Kind::Button,
            Kind::Combo,
            Kind::Dropdown,
            Kind::Nav,
            Kind::Checkbox,
        ] {
            let ctx = egui::Context::default();
            design::install(&ctx);
            let mut time = 0.;
            let (mut response, _) = frame(&ctx, kind, name, vec![], &mut time);
            for _ in 0..3 {
                response = frame(&ctx, kind, name, vec![], &mut time).0;
            }
            let rect = response.rect;
            assert!(rect.width() <= 162., "{rect:?}");
            frame(
                &ctx,
                kind,
                name,
                vec![
                    egui::Event::PointerMoved(rect.center()),
                    wheel(0., -30., egui::Modifiers::NONE),
                ],
                &mut time,
            );
            assert_eq!(
                ctx.data(|d| d.get_temp::<Scroll>(response.id.with("name-scroll")))
                    .unwrap()
                    .offset,
                0.
            );
            let (response, output) = frame(
                &ctx,
                kind,
                name,
                vec![wheel(-180., 0., egui::Modifiers::NONE)],
                &mut time,
            );
            let offset = ctx
                .data(|d| d.get_temp::<Scroll>(response.id.with("name-scroll")))
                .unwrap()
                .offset;
            assert!(offset > 0.);
            assert_eq!(response.rect, rect);
            assert!(!response.clicked());
            assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(t) if t.galley.text()==name&&shape.clip_rect.width()<=160.)));
            let old = offset;
            frame(
                &ctx,
                kind,
                name,
                vec![wheel(0., -120., egui::Modifiers::SHIFT)],
                &mut time,
            );
            assert!(
                ctx.data(|d| d.get_temp::<Scroll>(response.id.with("name-scroll")))
                    .unwrap()
                    .offset
                    > old
            );
        }
    }
    #[test]
    fn dragging_a_long_title_scrolls_and_double_click_still_works() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut time = 0.;
        let name = "An exceptionally long document name whose final words must stay reachable";
        let (response, _) = frame(&ctx, Kind::Label, name, vec![], &mut time);
        let start = response.rect.center();
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &ctx,
            Kind::Label,
            name,
            vec![egui::Event::PointerMoved(start), pointer(start, true)],
            &mut time,
        );
        frame(
            &ctx,
            Kind::Label,
            name,
            vec![egui::Event::PointerMoved(start - Vec2::new(60., 0.))],
            &mut time,
        );
        let (released, _) = frame(
            &ctx,
            Kind::Label,
            name,
            vec![pointer(start - Vec2::new(60., 0.), false)],
            &mut time,
        );
        assert!(!released.clicked());
        assert!(
            ctx.data(|d| d.get_temp::<Scroll>(response.id.with("name-scroll")))
                .unwrap()
                .offset
                >= 59.
        );
        time += 0.4;
        let mut double = false;
        for _ in 0..2 {
            frame(
                &ctx,
                Kind::Label,
                name,
                vec![egui::Event::PointerMoved(start), pointer(start, true)],
                &mut time,
            );
            double = frame(
                &ctx,
                Kind::Label,
                name,
                vec![pointer(start, false)],
                &mut time,
            )
            .0
            .double_clicked();
        }
        assert!(double);
    }
    #[test]
    fn table_names_require_shift_so_plain_horizontal_wheels_keep_panning_the_table() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut time = 0.;
        let name = "A long account or category name inside a horizontally scrolling finance table";
        let (response, _) = frame(&ctx, Kind::Table, name, vec![], &mut time);
        let pos = response.rect.center();
        frame(
            &ctx,
            Kind::Table,
            name,
            vec![
                egui::Event::PointerMoved(pos),
                wheel(-120., 0., egui::Modifiers::NONE),
            ],
            &mut time,
        );
        assert_eq!(
            ctx.data(|d| d.get_temp::<Scroll>(response.id.with("name-scroll")))
                .unwrap()
                .offset,
            0.
        );
        frame(
            &ctx,
            Kind::Table,
            name,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::SHIFT),
                wheel(0., -120., egui::Modifiers::SHIFT),
            ],
            &mut time,
        );
        assert!(
            ctx.data(|d| d.get_temp::<Scroll>(response.id.with("name-scroll")))
                .unwrap()
                .offset
                > 0.
        );
    }
}
