//! Consistent, accessible modal structure shared by all tabs.
use crate::design::{self, Icon, colors};
use egui::{Color32, RichText, Vec2};

pub fn modal(ctx: &egui::Context, id: egui::Id) -> egui::Modal {
    egui::Modal::new(id)
        .frame(design::dialog(ctx))
        .backdrop_color(Color32::from_black_alpha(
            if ctx.theme() == egui::Theme::Dark {
                150
            } else {
                90
            },
        ))
}
pub fn dialog_width(ctx: &egui::Context, preferred: f32) -> f32 {
    preferred.min((ctx.content_rect().width() - 96.0).max(280.0))
}
pub fn dialog_body_height(ctx: &egui::Context) -> f32 {
    (ctx.content_rect().height() - 270.0).clamp(180.0, 580.0)
}
pub fn dialog_heading(ui: &mut egui::Ui, title: &str) -> egui::Response {
    heading(ui, title, false)
}
pub fn settings_heading(ui: &mut egui::Ui, title: &str) -> egui::Response {
    heading(ui, title, true)
}
fn heading(ui: &mut egui::Ui, title: &str, centered: bool) -> egui::Response {
    ui.spacing_mut().item_spacing = Vec2::new(24.0, 12.0);
    ui.visuals_mut().text_edit_bg_color = Some(colors(ui).column);
    let width = (ui.available_width() - 32.0 - ui.spacing().item_spacing.x).max(0.0);
    let close = ui
        .horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(width, 32.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(width);
                    let inset = if centered {
                        (56.0_f32).min(width * 0.5)
                    } else {
                        0.0
                    };
                    ui.add_space(inset);
                    ui.add_sized(
                        [width - inset, 32.0],
                        egui::Label::new(
                            RichText::new(crate::forms::title_case(title))
                                .size(23.0)
                                .strong(),
                        )
                        .truncate()
                        .halign(if centered {
                            egui::Align::Center
                        } else {
                            egui::Align::Min
                        }),
                    );
                },
            );
            design::icon_button(
                ui,
                ui.make_persistent_id("dialog-close"),
                Icon::Close,
                "Close Dialog",
                32.0,
            )
        })
        .inner;
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(10.0);
    close
}
pub fn dialog_section(ui: &mut egui::Ui, title: &str) {
    ui.label(
        RichText::new(crate::forms::title_case(title))
            .size(16.0)
            .strong(),
    );
    ui.add_space(10.0);
}

/// Related preferences share one quiet surface, with inset rules between rows.
pub fn settings_group<R>(
    ui: &mut egui::Ui,
    title: &str,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.push_id(("settings-group", title), |ui| {
        if !title.is_empty() {
            ui.label(
                RichText::new(title)
                    .size(14.)
                    .strong()
                    .color(colors(ui).muted),
            );
            ui.add_space(2.);
        }
        let width = ui.available_width().min(ui.clip_rect().width());
        let result = egui::Frame::new()
            .fill(colors(ui).column)
            .stroke(egui::Stroke::new(1., colors(ui).line))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_min_width((width - 34.).max(0.));
                ui.spacing_mut().item_spacing.y = 8.;
                contents(ui)
            })
            .inner;
        ui.add_space(12.);
        result
    })
    .inner
}

pub fn settings_separator(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(1., colors(ui).line),
    );
}

/// Keep preference labels and their controls aligned across a grouped list.
pub fn settings_row<R>(
    ui: &mut egui::Ui,
    label: &str,
    control: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.push_id(("settings-row", label), |ui| {
        ui.horizontal(|ui| {
            let label_width = (ui.available_width() * 0.45).clamp(100., 240.);
            ui.allocate_ui_with_layout(
                egui::vec2(label_width, design::CONTROL_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(label);
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control)
                .inner
        })
        .inner
    })
    .inner
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DataAction {
    Import,
    Export,
    Delete,
}

pub struct SettingsHeading {
    pub close: egui::Response,
    pub data_action: Option<DataAction>,
}

fn data_icons(
    ui: &mut egui::Ui,
    label: &str,
    prefix: &str,
    can_delete: bool,
) -> Option<DataAction> {
    let mut action = None;
    for (id, icon, verb, candidate) in [
        ("import", Icon::Import, "Import", DataAction::Import),
        ("export", Icon::Export, "Export", DataAction::Export),
        ("delete", Icon::Trash, "Delete", DataAction::Delete),
    ] {
        if candidate == DataAction::Delete && !can_delete {
            continue;
        }
        if design::icon_button(
            ui,
            egui::Id::new(format!("{prefix}-{id}")),
            icon,
            &format!("{verb} {label}"),
            32.0,
        )
        .clicked()
        {
            action = Some(candidate);
        }
    }
    action
}

pub fn data_settings_heading(
    ui: &mut egui::Ui,
    title: &str,
    label: &str,
    prefix: &str,
    can_delete: bool,
    enabled: bool,
) -> SettingsHeading {
    ui.spacing_mut().item_spacing = Vec2::new(8.0, 12.0);
    let actions_width = if can_delete { 160.0 } else { 120.0 };
    let width = (ui.available_width() - actions_width).max(0.0);
    let mut data_action = None;
    let close = ui
        .horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(width, 32.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(width);
                    let inset = actions_width.min(width * 0.5);
                    ui.add_space(inset);
                    ui.add_sized(
                        [width - inset, 32.0],
                        egui::Label::new(RichText::new(title).size(23.0).strong())
                            .truncate()
                            .halign(egui::Align::Center),
                    )
                    .on_hover_text(title);
                },
            );
            data_action = ui
                .add_enabled_ui(enabled, |ui| data_icons(ui, label, prefix, can_delete))
                .inner;
            design::icon_button(
                ui,
                egui::Id::new(format!("{prefix}-close")),
                Icon::Close,
                "Close Dialog",
                32.0,
            )
        })
        .inner;
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(10.0);
    SettingsHeading { close, data_action }
}
pub fn property_row<R>(
    ui: &mut egui::Ui,
    label: &str,
    control: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.push_id(label, |ui| {
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(86.0, design::CONTROL_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(86.0);
                    crate::forms::field_label(ui, label);
                },
            );
            control(ui)
        })
        .inner
    })
    .inner
}
pub fn dialog_footer<R>(ui: &mut egui::Ui, controls: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);
    ui.horizontal(controls).inner
}
pub fn dialog_primary(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let blue = colors(ui).primary;
    ui.add(
        design::Button::new(RichText::new(text).color(Color32::WHITE).strong())
            .fill(blue)
            .stroke(egui::Stroke::NONE)
            .corner_radius(8)
            .min_size(Vec2::new(110.0, design::CONTROL_HEIGHT)),
    )
}
/// Outside clicks are blocked without discarding an unfinished draft.
pub fn dialog_should_close<R>(response: &egui::ModalResponse<R>) -> bool {
    response.response.should_close()
        || (response.is_top_modal
            && !response.any_popup_open
            && response
                .response
                .ctx
                .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)))
}

/// Editable name only; the settings suffix stays outside the text field.
#[derive(Debug)]
pub struct NameEdit {
    pub value: String,
    focus: bool,
}

pub fn named_dialog_heading(
    ui: &mut egui::Ui,
    name: &str,
    suffix: &str,
    edit: &mut Option<NameEdit>,
    prefix: &str,
    data_label: &str,
) -> SettingsHeading {
    ui.spacing_mut().item_spacing = Vec2::new(8.0, 12.0);
    ui.visuals_mut().text_edit_bg_color = Some(colors(ui).column);
    let width = (ui.available_width() - 200.0).max(0.0);
    let mut data_action = None;
    let close = ui
        .horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(width, 32.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(width);
                    // Match the actions on the right so the title centers on the dialog.
                    let inset = (200.0_f32).min(width * 0.5);
                    ui.add_space(inset);
                    let title_width = width - inset;
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if let Some(edit) = edit {
                        let suffix_width = ui
                            .painter()
                            .layout_no_wrap(
                                suffix.into(),
                                egui::FontId::proportional(23.0),
                                colors(ui).ink,
                            )
                            .size()
                            .x
                            + 8.0;
                        let mut output = design::input(&mut edit.value)
                            .id(egui::Id::new(format!("{prefix}-name")))
                            .font(egui::FontId::proportional(23.0))
                            .margin(egui::Margin::symmetric(4, 2))
                            .desired_width((title_width - suffix_width - 8.0).max(40.0))
                            .show(ui);
                        if edit.focus {
                            output.response.request_focus();
                            output.state.cursor.set_char_range(Some(
                                egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(edit.value.chars().count()),
                                ),
                            ));
                            output.state.store(ui.ctx(), output.response.id);
                            edit.focus = false;
                        }
                        ui.label(RichText::new(suffix).size(23.0).strong());
                    } else {
                        let title = format!("{name} {suffix}");
                        ui.add_sized(
                            [title_width, 32.0],
                            egui::Label::new(RichText::new(&title).size(23.0).strong())
                                .truncate()
                                .halign(egui::Align::Center),
                        )
                        .on_hover_text(title);
                    }
                },
            );
            data_action = data_icons(ui, data_label, prefix, true);
            if design::icon_button(
                ui,
                egui::Id::new(format!("{prefix}-rename")),
                Icon::Edit,
                "Edit Name",
                32.0,
            )
            .clicked()
            {
                if let Some(edit) = edit {
                    edit.focus = true;
                } else {
                    *edit = Some(NameEdit {
                        value: name.into(),
                        focus: true,
                    });
                }
                ui.ctx().request_repaint();
            }
            design::icon_button(
                ui,
                egui::Id::new(format!("{prefix}-close")),
                Icon::Close,
                "Close Dialog",
                32.0,
            )
        })
        .inner;
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(10.0);
    SettingsHeading { close, data_action }
}
