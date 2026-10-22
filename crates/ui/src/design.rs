//! Shared visual language for the native workspace.
mod controls;
pub use crate::dialogs::{
    DataAction, NameEdit, data_settings_heading, dialog_body_height, dialog_footer, dialog_heading,
    dialog_primary, dialog_section, dialog_should_close, dialog_width, modal, named_dialog_heading,
    property_row, settings_heading,
};
pub use crate::keyboard::{ActionKeys, ActionShortcut, contains_keyboard_focus};
pub use crate::search::SearchField;
pub use controls::{
    Button, button, checkbox, chip, icon_button, menu_button, small_button, small_checkbox,
};
use egui::{self, Color32, Rect, RichText, Stroke, StrokeKind, Vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub accent: Color32,
    pub primary: Color32,
    pub ink: Color32,
    pub muted: Color32,
    pub green: Color32,
    pub red: Color32,
    pub amber: Color32,
    pub canvas: Color32,
    pub sidebar: Color32,
    pub card: Color32,
    pub line: Color32,
    pub blue_tint: Color32,
    pub column: Color32,
    pub hover: Color32,
    pub hover_line: Color32,
    pub nav_ink: Color32,
    pub progress: Color32,
    pub violet: Color32,
    pub violet_tint: Color32,
    pub field: Color32,
    pub stripe: Color32,
}
impl Palette {
    pub fn for_dark(dark: bool) -> Self {
        if dark {
            Self {
                accent: Color32::from_rgb(100, 173, 255),
                primary: Color32::from_rgb(0, 102, 204),
                ink: Color32::from_rgb(240, 240, 242),
                muted: Color32::from_rgb(158, 157, 153),
                green: Color32::from_rgb(106, 216, 170),
                red: Color32::from_rgb(255, 143, 154),
                amber: Color32::from_rgb(234, 187, 111),
                canvas: Color32::from_rgb(25, 25, 25),
                sidebar: Color32::from_rgb(31, 31, 30),
                card: Color32::from_rgb(34, 34, 33),
                line: Color32::from_rgb(54, 54, 52),
                blue_tint: Color32::from_rgb(46, 46, 44),
                column: Color32::from_rgb(30, 30, 29),
                hover: Color32::from_rgb(46, 46, 44),
                hover_line: Color32::from_rgb(82, 82, 78),
                nav_ink: Color32::from_rgb(180, 179, 175),
                progress: Color32::from_rgb(40, 40, 38),
                violet: Color32::from_rgb(193, 168, 255),
                violet_tint: Color32::from_rgb(45, 40, 51),
                field: Color32::from_rgb(30, 30, 29),
                stripe: Color32::from_rgb(39, 39, 37),
            }
        } else {
            Self {
                accent: Color32::from_rgb(0, 113, 227),
                primary: Color32::from_rgb(0, 113, 227),
                ink: Color32::from_rgb(29, 29, 31),
                muted: Color32::from_rgb(120, 119, 116),
                green: Color32::from_rgb(36, 145, 102),
                red: Color32::from_rgb(211, 77, 87),
                amber: Color32::from_rgb(188, 130, 34),
                canvas: Color32::from_rgb(255, 255, 255),
                sidebar: Color32::from_rgb(245, 245, 247),
                card: Color32::WHITE,
                line: Color32::from_rgb(233, 233, 230),
                blue_tint: Color32::from_rgb(235, 235, 232),
                column: Color32::from_rgb(247, 247, 245),
                hover: Color32::from_rgb(237, 237, 234),
                hover_line: Color32::from_rgb(211, 210, 205),
                nav_ink: Color32::from_rgb(100, 99, 95),
                progress: Color32::from_rgb(239, 239, 236),
                violet: Color32::from_rgb(121, 93, 190),
                violet_tint: Color32::from_rgb(244, 241, 248),
                field: Color32::from_rgb(245, 245, 247),
                stripe: Color32::from_rgb(250, 250, 248),
            }
        }
    }
}
// Keep the palette and egui's built-in widgets on the same animation clock.
// The first value is immediate so restoring a saved theme never flashes light.
const THEME_SECONDS: f64 = 0.22;
#[derive(Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    started: f64,
}
impl Tween {
    fn settled(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            started: 0.0,
        }
    }
    fn value(self, now: f64, seconds: f64) -> f32 {
        let t = ((now - self.started) / seconds).clamp(0.0, 1.0) as f32;
        let eased = t * t * (3.0 - 2.0 * t);
        self.from + (self.to - self.from) * eased
    }
    fn retarget(&mut self, target: f32, now: f64, seconds: f64) {
        if self.to != target {
            self.from = self.value(now, seconds);
            self.to = target;
            self.started = now;
        }
    }
}
/// A smooth transition that reverses from its current position and stops repainting at rest.
pub fn animate_toggle(ctx: &egui::Context, id: egui::Id, active: bool, seconds: f64) -> f32 {
    let now = ctx.input(|i| i.time);
    let target = f32::from(active);
    let mut tween = ctx
        .data(|d| d.get_temp::<Tween>(id))
        .unwrap_or_else(|| Tween::settled(target));
    tween.retarget(target, now, seconds);
    let amount = tween.value(now, seconds);
    ctx.data_mut(|d| d.insert_temp(id, tween));
    if amount != target {
        ctx.request_repaint();
    }
    amount
}
impl Palette {
    fn mix(self, other: Self, t: f32) -> Self {
        macro_rules! blend {
            ($($field:ident),+ $(,)?) => {
                Self { $($field: self.$field.lerp_to_gamma(other.$field, t)),+ }
            };
        }
        blend!(
            accent,
            primary,
            ink,
            muted,
            green,
            red,
            amber,
            canvas,
            sidebar,
            card,
            line,
            blue_tint,
            column,
            hover,
            hover_line,
            nav_ink,
            progress,
            violet,
            violet_tint,
            field,
            stripe
        )
    }
}
pub fn colors(ui: &egui::Ui) -> Palette {
    palette(ui.ctx())
}
pub fn palette(ctx: &egui::Context) -> Palette {
    ctx.data(|d| d.get_temp(egui::Id::new("animated-palette")))
        .unwrap_or_else(|| Palette::for_dark(ctx.theme() == egui::Theme::Dark))
}
pub fn apply_theme(ctx: &egui::Context, dark: bool) {
    let theme = if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    let now = ctx.input(|i| i.time);
    let target = f32::from(dark);
    let id = egui::Id::new("theme-transition");
    let mut tween = ctx
        .data(|d| d.get_temp::<Tween>(id))
        .unwrap_or_else(|| Tween::settled(target));
    tween.retarget(target, now, THEME_SECONDS);
    let amount = tween.value(now, THEME_SECONDS);
    let p = Palette::for_dark(false).mix(Palette::for_dark(true), amount);
    let changed = ctx.theme() != theme || palette(ctx) != p;
    ctx.set_theme(theme);
    if changed {
        ctx.global_style_mut(|style| style.visuals = visuals(p, dark, amount));
    }
    ctx.data_mut(|d| {
        d.insert_temp(id, tween);
        d.insert_temp(egui::Id::new("animated-palette"), p);
    });
    if amount != target {
        ctx.request_repaint();
    }
}

/// A brief entrance on navigation; layout and hit targets stay stationary.
pub fn page_transition(ui: &mut egui::Ui, page: impl std::hash::Hash + std::fmt::Debug) {
    let id = ui.make_persistent_id("page-transition");
    let page = egui::Id::new(page);
    let now = ui.input(|i| i.time);
    let (old_page, started) = ui
        .data(|d| d.get_temp::<(egui::Id, f64)>(id))
        .unwrap_or((page, now - 1.0));
    let started = if old_page != page { now } else { started };
    ui.data_mut(|d| d.insert_temp(id, (page, started)));
    let t = ((now - started) / 0.16).clamp(0.0, 1.0) as f32;
    ui.multiply_opacity(0.65 + 0.35 * (1.0 - (1.0 - t).powi(3)));
    if t < 1.0 {
        ui.ctx().request_repaint();
    }
}

pub fn shadow() -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, 1],
        blur: 3,
        spread: 0,
        color: Color32::from_black_alpha(6),
    }
}
pub fn panel(ui: &egui::Ui) -> egui::Frame {
    let p = colors(ui);
    egui::Frame::new()
        .fill(p.card)
        .inner_margin(22)
        .corner_radius(7)
        .stroke(Stroke::new(1.0, p.line))
        .shadow(shadow())
}
pub fn dialog(ctx: &egui::Context) -> egui::Frame {
    let p = palette(ctx);
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.line))
        .inner_margin(24)
        .corner_radius(14)
        .shadow(egui::epaint::Shadow {
            offset: [0, 12],
            blur: 45,
            spread: 2,
            color: Color32::from_black_alpha(35),
        })
}
pub const CONTROL_HEIGHT: f32 = 38.0;

/// Keep hints, entered text, and adjacent selectors on the same baseline.
pub fn input(text: &mut dyn egui::TextBuffer) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .margin(egui::Margin::symmetric(12, 9))
        .min_size(Vec2::new(0.0, CONTROL_HEIGHT))
        .vertical_align(egui::Align::Center)
}

pub fn primary(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let p = colors(ui);
    ui.add(
        Button::new(RichText::new(text).color(Color32::WHITE).strong())
            .fill(p.primary)
            .stroke(Stroke::NONE)
            .corner_radius(8)
            .min_size(Vec2::new(110.0, CONTROL_HEIGHT)),
    )
}
pub fn hint(ui: &mut egui::Ui, text: &str) {
    let p = colors(ui);
    ui.label(RichText::new(text).color(p.muted).size(13.0));
}
pub fn eyebrow(ui: &mut egui::Ui, text: &str) {
    let p = colors(ui);
    ui.label(
        RichText::new(text)
            .size(10.0)
            .strong()
            .color(p.muted)
            .extra_letter_spacing(1.0),
    );
}
pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    badge_with_size(ui, text, color, 11.0);
}
pub fn badge_with_size(ui: &mut egui::Ui, text: &str, color: Color32, size: f32) {
    let p = colors(ui);
    let padding = Vec2::new(10.0, 5.0);
    let galley = egui::WidgetText::from(RichText::new(text).size(size).color(color)).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        (ui.max_rect().width() - padding.x * 2.0).max(0.0),
        egui::TextStyle::Small,
    );
    // Measuring the label avoids inheriting the enclosing toolbar's row height.
    let (rect, response) =
        ui.allocate_exact_size(galley.size() + padding * 2.0, egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        6,
        color.lerp_to_gamma(p.card, if ui.visuals().dark_mode { 0.84 } else { 0.91 }),
    );
    let text_rect = egui::Align2::CENTER_CENTER.align_size_within_rect(galley.size(), rect);
    ui.painter().galley(text_rect.min, galley, color);
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), text));
    response.on_hover_text(text);
}
pub fn dot(ui: &mut egui::Ui, color: Color32) {
    dot_with_size(ui, color, 8.0);
}
pub fn dot_with_size(ui: &mut egui::Ui, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), size * 0.375, color);
}

#[derive(Clone, Copy)]
pub enum Icon {
    Grip,
    Account,
    Board,
    Wallet,
    Income,
    Expenses,
    Savings,
    Budgets,
    Storage,
    Rates,
    Search,
    Check,
    Folder,
    Calendar,
    Email,
    Plus,
    Settings,
    Edit,
    Import,
    Export,
    Filter,
    Close,
    Cancel,
    Trash,
    ChevronDown,
    ChevronUp,
}
pub fn icon(ui: &mut egui::Ui, kind: Icon, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    paint_icon(ui.painter(), rect, kind, color);
}
pub fn paint_icon(painter: &egui::Painter, rect: Rect, kind: Icon, color: Color32) {
    let p = |x: f32, y: f32| rect.min + Vec2::new(x, y) * rect.width() / 24.0;
    let stroke = Stroke::new(1.6, color);
    let line = |points: &[(f32, f32)]| {
        painter.add(egui::Shape::line(
            points.iter().map(|&(x, y)| p(x, y)).collect(),
            stroke,
        ))
    };
    let box_at = |a: (f32, f32), b: (f32, f32), radius: u8| {
        painter.rect_stroke(
            Rect::from_min_max(p(a.0, a.1), p(b.0, b.1)),
            radius,
            stroke,
            StrokeKind::Inside,
        )
    };
    match kind {
        Icon::Grip => {
            for x in [9., 15.] {
                for y in [6., 12., 18.] {
                    painter.circle_filled(p(x, y), rect.width() / 18., color);
                }
            }
        }
        Icon::Account => {
            painter.circle_stroke(p(12., 7.), 4. * rect.width() / 24., stroke);
            line(&[
                (4., 21.),
                (4., 18.),
                (6., 14.),
                (18., 14.),
                (20., 18.),
                (20., 21.),
            ]);
        }
        Icon::Plus => {
            line(&[(5., 12.), (19., 12.)]);
            line(&[(12., 5.), (12., 19.)]);
        }
        Icon::Close => {
            line(&[(6., 6.), (18., 18.)]);
            line(&[(18., 6.), (6., 18.)]);
        }
        Icon::Cancel => {
            painter.circle_stroke(p(12., 12.), 8. * rect.width() / 24., stroke);
            line(&[(6.3, 6.3), (17.7, 17.7)]);
        }
        Icon::Trash => {
            line(&[(4., 7.), (20., 7.)]);
            line(&[(9., 7.), (9., 4.), (15., 4.), (15., 7.)]);
            line(&[(6., 7.), (7., 21.), (17., 21.), (18., 7.)]);
            line(&[(10., 10.), (10., 18.)]);
            line(&[(14., 10.), (14., 18.)]);
        }
        Icon::ChevronDown => {
            line(&[(6., 9.), (12., 15.), (18., 9.)]);
        }
        Icon::ChevronUp => {
            line(&[(6., 15.), (12., 9.), (18., 15.)]);
        }
        Icon::Filter => {
            line(&[
                (3., 4.),
                (21., 4.),
                (14., 12.),
                (14., 20.),
                (10., 18.),
                (10., 12.),
                (3., 4.),
            ]);
        }
        Icon::Edit => {
            line(&[
                (4., 20.),
                (5., 15.),
                (16., 4.),
                (20., 8.),
                (9., 19.),
                (4., 20.),
            ]);
            line(&[(14., 6.), (18., 10.)]);
        }
        Icon::Import => {
            line(&[(4., 15.), (4., 20.), (20., 20.), (20., 15.)]);
            line(&[(12., 3.), (12., 15.)]);
            line(&[(7., 10.), (12., 15.), (17., 10.)]);
        }
        Icon::Export => {
            line(&[(4., 15.), (4., 20.), (20., 20.), (20., 15.)]);
            line(&[(12., 15.), (12., 3.)]);
            line(&[(7., 8.), (12., 3.), (17., 8.)]);
        }
        Icon::Settings => {
            for (y, x) in [(6., 9.), (12., 16.), (18., 8.)] {
                line(&[(3., y), (x - 3., y)]);
                line(&[(x + 3., y), (21., y)]);
                painter.circle_stroke(p(x, y), 2.5, stroke);
            }
        }
        Icon::Board => {
            box_at((3., 4.), (21., 20.), 3);
            line(&[(9., 5.), (9., 19.)]);
            line(&[(15., 5.), (15., 19.)]);
            line(&[(5.5, 8.), (6.5, 8.)]);
            line(&[(11., 8.), (13., 8.)]);
            line(&[(17., 8.), (19., 8.)]);
        }
        Icon::Wallet => {
            box_at((3., 6.), (21., 20.), 3);
            line(&[(5., 6.), (5., 3.), (18., 3.), (18., 6.)]);
            box_at((14., 10.), (22., 16.), 2);
            painter.circle_filled(p(17., 13.), 1., color);
        }
        Icon::Income => {
            line(&[(5., 17.), (5., 20.), (20., 20.)]);
            line(&[(7., 14.), (17., 4.), (17., 11.)]);
            line(&[(10., 4.), (17., 4.)]);
        }
        Icon::Expenses => {
            line(&[(5., 17.), (5., 20.), (20., 20.)]);
            line(&[(7., 4.), (17., 14.), (10., 14.)]);
            line(&[(17., 7.), (17., 14.)]);
        }
        Icon::Savings => {
            painter.circle_stroke(p(12., 12.), rect.width() * 0.36, stroke);
            line(&[(12., 6.), (12., 18.)]);
            line(&[
                (16., 8.),
                (9., 8.),
                (8., 11.),
                (15., 13.),
                (15., 16.),
                (8., 16.),
            ]);
        }
        Icon::Budgets => {
            painter.circle_stroke(p(12., 12.), rect.width() * 0.36, stroke);
            painter.circle_stroke(p(12., 12.), rect.width() * 0.20, stroke);
            painter.circle_filled(p(12., 12.), 1.5, color);
        }
        Icon::Storage => {
            for y in [4., 10., 16.] {
                box_at((4., y), (20., y + 4.), 2);
            }
        }
        Icon::Rates => {
            line(&[(3., 8.), (20., 8.), (16., 4.)]);
            line(&[(21., 16.), (4., 16.), (8., 20.)]);
        }
        Icon::Search => {
            painter.circle_stroke(p(10., 10.), rect.width() * 0.26, stroke);
            line(&[(15., 15.), (21., 21.)]);
        }
        Icon::Check => {
            line(&[(5., 12.), (10., 17.), (20., 6.)]);
        }
        Icon::Folder => {
            line(&[
                (3., 7.),
                (3., 20.),
                (21., 20.),
                (21., 7.),
                (12., 7.),
                (9., 4.),
                (3., 4.),
                (3., 7.),
            ]);
        }
        Icon::Email => {
            box_at((3., 5.), (21., 19.), 3);
            line(&[(3., 6.), (12., 13.), (21., 6.)]);
        }
        Icon::Calendar => {
            box_at((3., 5.), (21., 21.), 3);
            line(&[(3., 10.), (21., 10.)]);
            line(&[(8., 3.), (8., 7.)]);
            line(&[(16., 3.), (16., 7.)]);
        }
    }
}
pub fn nav_row(
    ui: &mut egui::Ui,
    label: &str,
    kind: Icon,
    selected: bool,
    count: Option<usize>,
) -> egui::Response {
    let p = colors(ui);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), egui::Sense::click());
    let selected_t = animate_toggle(ui.ctx(), response.id.with("selected"), selected, 0.18);
    let hover_t = animate_toggle(
        ui.ctx(),
        response.id.with("hover"),
        response.hovered(),
        0.16,
    );
    let bg = p
        .sidebar
        .lerp_to_gamma(p.hover, hover_t)
        .lerp_to_gamma(p.blue_tint, selected_t);
    let color = p.nav_ink.lerp_to_gamma(p.ink, selected_t);
    ui.painter().rect_filled(rect, 5, bg);
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            egui::pos2(rect.left() + 22., rect.center().y),
            Vec2::splat(18.),
        ),
        kind,
        color,
    );
    let text_color = p.nav_ink.lerp_to_gamma(p.ink, selected_t);
    let galley = egui::WidgetText::from(RichText::new(label).size(14.0).color(text_color))
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            (rect.width() - if count.is_some() { 77.0 } else { 54.0 }).max(0.0),
            egui::TextStyle::Body,
        );
    ui.painter().galley(
        egui::pos2(rect.left() + 43.0, rect.center().y - galley.size().y / 2.0),
        galley,
        text_color,
    );
    if let Some(count) = count {
        ui.painter().text(
            egui::pos2(rect.right() - 13., rect.center().y),
            egui::Align2::RIGHT_CENTER,
            count.to_string(),
            egui::FontId::proportional(12.),
            color,
        );
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label)
}
pub fn segmented(ui: &mut egui::Ui, selected: &mut bool, left: &str, right: &str) {
    let p = colors(ui);
    let font = egui::FontId::proportional(12.0);
    let label_width = [left, right]
        .into_iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap(label.into(), font.clone(), p.ink)
                .size()
                .x
        })
        .fold(0.0, f32::max);
    let width = (label_width + 24.0).max(54.0);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width * 2.0 + 6.0, 32.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 9, p.progress);
    // Reserve a shape behind the buttons, then position the pill after input.
    let pill = ui.painter().add(egui::Shape::Noop);
    for (value, label) in [(false, left), (true, right)] {
        let button_rect = Rect::from_min_size(
            rect.min + Vec2::new(3.0 + if value { width } else { 0.0 }, 3.0),
            Vec2::new(width, 26.0),
        );
        let response = ui.put(
            button_rect,
            Button::new(
                RichText::new(label)
                    .size(12.0)
                    .color(if *selected == value { p.ink } else { p.muted }),
            )
            .frame(false)
            .small() // The indicator is 26 px tall; skip the normal control minimum.
            .selected(*selected == value),
        );
        if response.clicked() {
            *selected = value;
            ui.ctx().request_repaint();
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                button_rect,
                6,
                Stroke::new(1.0, p.accent),
                StrokeKind::Inside,
            );
        }
    }
    let now = ui.input(|i| i.time);
    let id = response.id.with("pill");
    let target = f32::from(*selected);
    let mut tween = ui
        .data(|d| d.get_temp::<Tween>(id))
        .unwrap_or_else(|| Tween::settled(target));
    tween.retarget(target, now, 0.16);
    let t = tween.value(now, 0.16);
    ui.data_mut(|d| d.insert_temp(id, tween));
    ui.painter().set(
        pill,
        egui::Shape::rect_filled(
            Rect::from_min_size(
                rect.min + Vec2::new(3.0 + width * t, 3.0),
                Vec2::new(width, 26.0),
            ),
            6,
            p.card,
        ),
    );
    if t != target {
        ui.ctx().request_repaint();
    }
}

fn visuals(p: Palette, dark: bool, amount: f32) -> egui::Visuals {
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    let v = &mut visuals;
    // egui's theme defaults use different glyph coverage curves. Changing that
    // setting discards the font atlas and rerasterizes all visible text. Share
    // one curve so appearance changes only update colors, including in debug.
    v.text_options.color_transfer_function = egui::epaint::FontColorTransferFunction::Off;
    v.panel_fill = p.canvas;
    v.window_fill = p.card;
    v.window_stroke = Stroke::new(1., p.line);
    v.extreme_bg_color = p.column;
    v.text_edit_bg_color = Some(p.field);
    v.selection.bg_fill = p.blue_tint;
    v.selection.stroke = Stroke::new(1., p.accent);
    v.override_text_color = Some(p.ink);
    v.weak_text_color = Some(p.muted);
    v.hyperlink_color = p.accent;
    v.faint_bg_color = p.stripe;
    v.code_bg_color = p.column;
    v.window_corner_radius = 14.into();
    v.window_highlight_topmost = false;
    v.menu_corner_radius = 10.into();
    v.window_shadow = egui::epaint::Shadow {
        offset: [0, 12],
        blur: 45,
        spread: 2,
        color: Color32::from_black_alpha((35.0 + 50.0 * amount).round() as u8),
    };
    v.popup_shadow = shadow();
    v.warn_fg_color = p.amber;
    v.error_fg_color = p.red;
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = 6.into();
        w.bg_stroke = Stroke::NONE;
        w.fg_stroke = Stroke::new(1., p.ink);
        w.expansion = 0.;
    }
    v.widgets.noninteractive.bg_fill = p.card;
    v.widgets.noninteractive.weak_bg_fill = p.card;
    v.widgets.inactive.bg_fill = p.column;
    v.widgets.inactive.weak_bg_fill = p.card;
    v.widgets.hovered.bg_fill = p.hover;
    v.widgets.hovered.weak_bg_fill = p.hover;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.active.bg_fill = p.blue_tint;
    v.widgets.active.weak_bg_fill = p.blue_tint;
    v.widgets.active.fg_stroke = Stroke::new(1., p.ink);
    v.widgets.active.bg_stroke = Stroke::new(1., p.accent);
    v.widgets.open.bg_fill = p.blue_tint;
    v.widgets.open.weak_bg_fill = p.blue_tint;
    visuals
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "workspace-sans".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/AdwaitaSans-Regular.ttf"))
            .into(),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "workspace-sans".into());
    ctx.set_fonts(fonts);
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let dark = theme == egui::Theme::Dark;
        let p = Palette::for_dark(dark);
        let mut style = egui::Style {
            visuals: visuals(p, dark, f32::from(dark)),
            animation_time: 0.16,
            ..Default::default()
        };
        style.spacing.item_spacing = Vec2::new(10., 10.);
        style.spacing.button_padding = Vec2::new(14., 9.);
        style.spacing.interact_size.y = CONTROL_HEIGHT;
        style.spacing.window_margin = egui::Margin::same(22);
        // Combo-box popups create their own scroll areas; hide those bars too.
        style.spacing.scroll = egui::style::ScrollStyle {
            bar_width: 0.0,
            bar_inner_margin: 0.0,
            bar_outer_margin: 0.0,
            floating_width: 0.0,
            floating_allocated_width: 0.0,
            dormant_background_opacity: 0.0,
            active_background_opacity: 0.0,
            interact_background_opacity: 0.0,
            dormant_handle_opacity: 0.0,
            active_handle_opacity: 0.0,
            interact_handle_opacity: 0.0,
            ..egui::style::ScrollStyle::floating()
        };
        for (text, size) in [
            (egui::TextStyle::Body, 15.),
            (egui::TextStyle::Button, 14.),
            (egui::TextStyle::Small, 13.),
            (egui::TextStyle::Heading, 32.),
        ] {
            style
                .text_styles
                .insert(text, egui::FontId::proportional(size));
        }
        ctx.set_style_of(theme, style);
    }
    ctx.set_theme(egui::Theme::Light);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme_frame(ctx: &egui::Context, time: f64, dark: bool) -> Palette {
        sample_theme_frame(ctx, time, dark).0
    }

    fn sample_theme_frame(ctx: &egui::Context, time: f64, dark: bool) -> (Palette, bool) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(800., 600.))),
                ..Default::default()
            },
            |ui| {
                apply_theme(ctx, dark);
                ui.set_style(ctx.global_style());
                let p = colors(ui);
                // Custom cards and native fields must share every intermediate frame.
                assert_eq!(ui.visuals().panel_fill, p.canvas);
                assert_eq!(ui.visuals().text_edit_bg_color, Some(p.field));
                assert_eq!(ui.visuals().override_text_color, Some(p.ink));
                assert_eq!(ui.visuals().dark_mode, dark);
                ui.label("Theme motion");
            },
        );
        let updated_textures = !output.textures_delta.is_empty();
        output.textures_delta.clear();
        (palette(ctx), updated_textures)
    }

    #[test]
    fn dynamic_controls_ease_and_reverse_without_a_position_jump() {
        let ctx = egui::Context::default();
        let sample = |time, active| {
            let mut value = 0.0;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |_| {
                    value = animate_toggle(&ctx, egui::Id::new("control-motion"), active, 0.22);
                },
            );
            output.textures_delta.clear();
            value
        };
        assert_eq!(sample(0.0, false), 0.0);
        assert_eq!(sample(1.0, true), 0.0);
        let early = sample(1.055, true);
        assert!(early > 0.0 && early < 0.25);
        let midpoint = sample(1.11, true);
        assert!((midpoint - 0.5).abs() < 0.001);
        assert_eq!(sample(1.11, false), midpoint);
        let closing = sample(1.22, false);
        assert!(closing > 0.0 && closing < midpoint);
        assert_eq!(sample(1.34, false), 0.0);
        assert_eq!(sample(2.0, true), 0.0);
        assert_eq!(sample(2.3, true), 1.0);
    }

    #[test]
    fn theme_fades_and_reaches_both_endpoints() {
        let ctx = egui::Context::default();
        install(&ctx);
        let light = theme_frame(&ctx, 0.0, false);
        assert_eq!(theme_frame(&ctx, 1.0, true), light);
        let midpoint = theme_frame(&ctx, 1.11, true);
        let dark = Palette::for_dark(true);
        assert_ne!(midpoint, light);
        assert_ne!(midpoint, dark);
        assert!(midpoint.canvas.r() > dark.canvas.r());
        assert!(midpoint.canvas.r() < light.canvas.r());
        assert_eq!(theme_frame(&ctx, 1.25, true), dark);
        assert_eq!(theme_frame(&ctx, 2.0, false), dark);
        assert_eq!(theme_frame(&ctx, 2.25, false), light);
        assert_eq!(theme_frame(&ctx, 3.0, false), light);
    }

    #[test]
    fn rapid_theme_reversal_continues_without_a_color_jump() {
        let ctx = egui::Context::default();
        install(&ctx);
        theme_frame(&ctx, 0.0, false);
        theme_frame(&ctx, 1.0, true);
        let before = theme_frame(&ctx, 1.08, true);
        assert_eq!(theme_frame(&ctx, 1.08, false), before);
        let reversing = theme_frame(&ctx, 1.16, false);
        assert!(reversing.canvas.r() > before.canvas.r());
        assert_eq!(theme_frame(&ctx, 1.31, false), Palette::for_dark(false));
    }

    #[test]
    fn changing_appearance_keeps_the_existing_font_atlas() {
        let ctx = egui::Context::default();
        install(&ctx);
        sample_theme_frame(&ctx, 0.0, false);
        sample_theme_frame(&ctx, 0.1, false);
        for (time, dark) in [
            (1.0, true),
            (1.11, true),
            (1.3, true),
            (2.0, false),
            (2.11, false),
            (2.3, false),
        ] {
            let (_, updated_textures) = sample_theme_frame(&ctx, time, dark);
            assert!(
                !updated_textures,
                "theme changes must not rebuild cached glyphs"
            );
        }
    }

    #[test]
    fn saved_dark_theme_starts_without_a_light_flash() {
        let ctx = egui::Context::default();
        install(&ctx);
        assert_eq!(theme_frame(&ctx, 0.0, true), Palette::for_dark(true));
        assert_eq!(theme_frame(&ctx, 0.1, true), Palette::for_dark(true));
    }
}
