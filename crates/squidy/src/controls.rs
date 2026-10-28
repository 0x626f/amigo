use amigo_ui::design::{self, Icon};
use egui::{Color32, Id, Sense, Stroke, Ui};

pub(super) fn choices(
    ui: &mut Ui,
    id: Id,
    value: &mut String,
    values: &[String],
) -> egui::Response {
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 32. - ui.spacing().item_spacing.x).max(60.);
        let input = ui.add(design::input(value).id(id).desired_width(width));
        let button = design::icon_button(
            ui,
            id.with("choices"),
            Icon::ChevronDown,
            "Choose from existing values",
            32.,
        );
        design::menu(&button).show(|ui| {
            ui.set_min_width(width.min(360.));
            let query = value.trim().to_lowercase();
            let exact = values.iter().any(|v| v.to_lowercase() == query);
            egui::ScrollArea::vertical()
                .max_height(200.)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    let mut count = 0;
                    for option in values
                        .iter()
                        .filter(|v| exact || query.is_empty() || v.to_lowercase().contains(&query))
                    {
                        count += 1;
                        if ui.selectable_label(option == value, option).clicked() {
                            *value = option.clone();
                            ui.close();
                        }
                    }
                    if count == 0 {
                        design::hint(ui, "No matching values.");
                    }
                });
        });
        input
    })
    .inner
}

pub(super) fn edge_filter(ui: &mut Ui, id: Id, value: &mut String, values: &[String]) {
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 32. - ui.spacing().item_spacing.x).max(60.);
        ui.add(
            egui::TextEdit::multiline(value)
                .id(id)
                .desired_rows(2)
                .desired_width(width),
        );
        let button = design::icon_button(
            ui,
            id.with("choices"),
            Icon::ChevronDown,
            "Choose edge titles",
            32.,
        );
        design::menu(&button)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_min_width(width.min(360.));
                egui::ScrollArea::vertical()
                    .max_height(200.)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        if values.is_empty() {
                            design::hint(ui, "This board has no connections yet.");
                        }
                        for option in values {
                            let mut selected = value
                                .lines()
                                .any(|line| line.trim().to_lowercase() == option.to_lowercase());
                            if design::checkbox(ui, &mut selected, option).changed() {
                                let mut lines: Vec<_> = value
                                    .lines()
                                    .filter(|line| {
                                        !line.trim().is_empty()
                                            && line.trim().to_lowercase() != option.to_lowercase()
                                    })
                                    .map(str::to_owned)
                                    .collect();
                                if selected {
                                    lines.push(option.clone());
                                }
                                *value = lines.join("\n");
                            }
                        }
                    });
            });
    });
}

pub(super) fn color(ui: &mut Ui, id: Id, rgb: &mut [u8; 3]) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(28., 28.), Sense::hover());
    let response = ui.interact(rect, id, Sense::click());
    if response.clicked() {
        ui.ctx()
            .data_mut(|d| d.remove_temp::<String>(id.with("hex")));
    }
    let ink = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    ui.painter().circle_filled(rect.center(), 9., ink);
    ui.painter()
        .circle_stroke(rect.center(), 9.5, Stroke::new(1., design::colors(ui).line));
    if response.has_focus() {
        ui.painter().circle_stroke(
            rect.center(),
            12.,
            Stroke::new(1.5, design::colors(ui).accent),
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "Choose color")
    });
    let response = response.on_hover_text("Choose color");
    egui::Popup::menu(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            let swatches = [
                ("Red", [255, 59, 48]),
                ("Orange", [255, 149, 0]),
                ("Yellow", [255, 204, 0]),
                ("Green", [52, 199, 89]),
                ("Teal", [0, 199, 190]),
                ("Cyan", [50, 173, 230]),
                ("Blue", [0, 122, 255]),
                ("Indigo", [88, 86, 214]),
                ("Purple", [175, 82, 222]),
                ("Pink", [255, 45, 85]),
                ("Brown", [162, 132, 94]),
                ("Gray", [142, 142, 147]),
            ];
            ui.spacing_mut().item_spacing = egui::vec2(5., 5.);
            egui::Grid::new(id.with("palette"))
                .num_columns(6)
                .show(ui, |ui| {
                    for (index, (name, value)) in swatches.into_iter().enumerate() {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(28., 28.), Sense::hover());
                        let response = ui.interact(rect, id.with(("swatch", name)), Sense::click());
                        let response = response.on_hover_text(name);
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                ui.is_enabled(),
                                name,
                            )
                        });
                        let fill = Color32::from_rgb(value[0], value[1], value[2]);
                        ui.painter().circle_filled(rect.center(), 10., fill);
                        if *rgb == value {
                            design::paint_icon(
                                ui.painter(),
                                rect.shrink(6.),
                                Icon::Check,
                                if index == 2 {
                                    Color32::BLACK
                                } else {
                                    Color32::WHITE
                                },
                            );
                        }
                        if response.clicked() {
                            *rgb = value;
                            ui.ctx()
                                .data_mut(|d| d.remove_temp::<String>(id.with("hex")));
                            ui.close();
                        }
                        if index % 6 == 5 {
                            ui.end_row();
                        }
                    }
                });
            ui.add_space(8.);
            ui.horizontal(|ui| {
                ui.label("Hex");
                let hex_id = id.with("hex");
                let mut hex = ui
                    .ctx()
                    .data_mut(|d| d.get_temp::<String>(hex_id))
                    .unwrap_or_else(|| format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
                if ui
                    .add(design::input(&mut hex).char_limit(7).desired_width(100.))
                    .changed()
                {
                    if let Ok(value) = u32::from_str_radix(hex.trim_start_matches('#'), 16)
                        && hex.trim_start_matches('#').len() == 6
                    {
                        *rgb = [(value >> 16) as u8, (value >> 8) as u8, value as u8];
                    }
                    ui.ctx().data_mut(|d| d.insert_temp(hex_id, hex));
                }
            });
        });
}

pub(super) fn sorted(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut values: Vec<_> = values.collect();
    values.sort_by_key(|s| s.to_lowercase());
    values.dedup_by(|a, b| a.to_lowercase() == b.to_lowercase());
    values
}
