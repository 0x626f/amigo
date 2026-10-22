//! Date and month inputs with keyboard entry and a calendar popup.
use crate::design::{self, Icon, colors};
use chrono::{Datelike, Local, Months, NaiveDate};
use egui::{RichText, Vec2};

pub fn date_input(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut String,
    optional: bool,
) -> egui::Response {
    picker(ui, id, value, optional, false, false)
}
pub fn month_input(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut String,
    optional: bool,
) -> egui::Response {
    picker(ui, id, value, optional, true, false)
}
/// A date/month cell with room for its calendar control in compact tables.
pub fn table_date_input(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut String,
    month_only: bool,
) -> egui::Response {
    picker(ui, id, value, !month_only, month_only, true)
}
pub fn date_field(ui: &mut egui::Ui, label: &str, id: &str, value: &mut String, optional: bool) {
    crate::forms::field_label(ui, label);
    date_input(ui, id, value, optional);
    ui.add_space(6.0);
}
pub fn month_field(ui: &mut egui::Ui, label: &str, id: &str, value: &mut String) {
    crate::forms::field_label(ui, label);
    month_input(ui, id, value, false);
    ui.add_space(6.0);
}
fn parse(value: &str, month_only: bool) -> Option<NaiveDate> {
    if month_only {
        NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d").ok()
    } else {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
    }
}
fn picker(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut String,
    optional: bool,
    month_only: bool,
    compact: bool,
) -> egui::Response {
    let key = egui::Id::new((id, "calendar-month"));
    let today = Local::now().date_naive();
    let mut month = ui
        .ctx()
        .data(|d| d.get_temp::<NaiveDate>(key))
        .unwrap_or(today.with_day(1).unwrap());
    let mut changed = false;
    let width = ui.available_width();
    let response = ui
        .horizontal(|ui| {
            let calendar_id = egui::Id::new((id, "calendar"));
            let calendar_size = if compact {
                18.0
            } else {
                design::CONTROL_HEIGHT - 8.0
            };
            let mut input = design::input(value)
                .id(egui::Id::new((id, "input")))
                .desired_width(width)
                .margin(egui::Margin {
                    left: if compact { 4 } else { 12 },
                    right: 4,
                    top: 4,
                    bottom: 4,
                })
                .font(egui::TextStyle::Body.resolve(ui.style()))
                .suffix(egui::Atom::custom(calendar_id, Vec2::splat(calendar_size)))
                .hint_text(if month_only { "YYYY-MM" } else { "YYYY-MM-DD" });
            if compact {
                input = input
                    .horizontal_align(egui::Align::Center)
                    .prefix(egui::Atom::custom(
                        calendar_id.with("balance"),
                        Vec2::splat(calendar_size),
                    ));
            }
            let output = input.show(ui);
            let Some(calendar_rect) = output.response.rect(calendar_id) else {
                return output.response.response;
            };
            let response = output.response.response;
            let calendar = ui
                .scope_builder(egui::UiBuilder::new().max_rect(calendar_rect), |ui| {
                    design::icon_button(
                        ui,
                        calendar_id,
                        Icon::Calendar,
                        "Choose Date",
                        calendar_size,
                    )
                })
                .inner;
            if calendar.clicked() {
                month = parse(value, month_only)
                    .unwrap_or(today)
                    .with_day(1)
                    .unwrap();
            }
            egui::Popup::menu(&calendar)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .frame(
                    egui::Frame::popup(ui.style())
                        .inner_margin(12)
                        .corner_radius(8),
                )
                .show(|ui| {
                    ui.set_width(270.0);
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 6.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                design::Button::new(RichText::new("‹").size(22.0))
                                    .small()
                                    .frame(false)
                                    .min_size(Vec2::new(32.0, 38.0)),
                            )
                            .on_hover_text(if month_only {
                                "Previous Year"
                            } else {
                                "Previous Month"
                            })
                            .clicked()
                        {
                            month = month
                                .checked_sub_months(Months::new(if month_only { 12 } else { 1 }))
                                .unwrap_or(month);
                        }
                        ui.allocate_ui_with_layout(
                            Vec2::new(198.0, design::CONTROL_HEIGHT),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_min_width(198.0);
                                ui.label(
                                    RichText::new(if month_only {
                                        month.format("%Y").to_string()
                                    } else {
                                        month.format("%B %Y").to_string()
                                    })
                                    .size(16.0)
                                    .strong(),
                                );
                            },
                        );
                        if ui
                            .add(
                                design::Button::new(RichText::new("›").size(22.0))
                                    .small()
                                    .frame(false)
                                    .min_size(Vec2::new(32.0, 38.0)),
                            )
                            .on_hover_text(if month_only {
                                "Next Year"
                            } else {
                                "Next Month"
                            })
                            .clicked()
                        {
                            month = month
                                .checked_add_months(Months::new(if month_only { 12 } else { 1 }))
                                .unwrap_or(month);
                        }
                    });
                    ui.separator();
                    let selected = parse(value, month_only);
                    if month_only {
                        egui::Grid::new((id, "months"))
                            .spacing([6.0, 6.0])
                            .show(ui, |ui| {
                                for n in 1..=12 {
                                    let date = NaiveDate::from_ymd_opt(month.year(), n, 1).unwrap();
                                    if calendar_button(
                                        ui,
                                        &date.format("%b").to_string(),
                                        selected.is_some_and(|d| {
                                            d.year() == date.year() && d.month() == n
                                        }),
                                        Vec2::new(85.0, 34.0),
                                    )
                                    .clicked()
                                    {
                                        *value = date.format("%Y-%m").to_string();
                                        changed = true;
                                        ui.close();
                                    }
                                    if n % 3 == 0 {
                                        ui.end_row();
                                    }
                                }
                            });
                    } else {
                        egui::Grid::new((id, "days"))
                            .min_col_width(34.0)
                            .min_row_height(32.0)
                            .spacing([4.0, 4.0])
                            .show(ui, |ui| {
                                for name in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
                                    ui.label(
                                        RichText::new(name)
                                            .size(13.0)
                                            .strong()
                                            .color(colors(ui).muted),
                                    );
                                }
                                ui.end_row();
                                let first = month.weekday().num_days_from_monday();
                                let days = days_in_month(month);
                                let cells = (first + days).div_ceil(7) * 7;
                                for cell in 0..cells {
                                    if cell >= first && cell < first + days {
                                        let day = cell - first + 1;
                                        let date = month.with_day(day).unwrap();
                                        if calendar_button(
                                            ui,
                                            &day.to_string(),
                                            selected == Some(date),
                                            Vec2::new(34.0, 32.0),
                                        )
                                        .clicked()
                                        {
                                            *value = date.format("%Y-%m-%d").to_string();
                                            changed = true;
                                            ui.close();
                                        }
                                    } else {
                                        ui.allocate_space(Vec2::new(34.0, 32.0));
                                    }
                                    if cell % 7 == 6 {
                                        ui.end_row();
                                    }
                                }
                            });
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 16.0;
                        if design::button(ui, if month_only { "This Month" } else { "Today" })
                            .clicked()
                        {
                            *value = today
                                .format(if month_only { "%Y-%m" } else { "%Y-%m-%d" })
                                .to_string();
                            changed = true;
                            ui.close();
                        }
                        if optional && design::button(ui, "Clear Date").clicked() {
                            value.clear();
                            changed = true;
                            ui.close();
                        }
                    });
                });
            response
        })
        .inner;
    ui.ctx().data_mut(|d| d.insert_temp(key, month));
    let mut response = response;
    if changed {
        response.mark_changed();
    }
    response.on_hover_text(value.as_str())
}
fn calendar_button(ui: &mut egui::Ui, text: &str, selected: bool, size: Vec2) -> egui::Response {
    let p = colors(ui);
    ui.add_sized(
        size,
        design::Button::new(RichText::new(text).size(14.0).color(if selected {
            p.accent
        } else {
            p.ink
        }))
        .small()
        .fill(if selected { p.blue_tint } else { p.card })
        .corner_radius(8),
    )
}
fn days_in_month(month: NaiveDate) -> u32 {
    (28..=31)
        .rev()
        .find(|&day| month.with_day(day).is_some())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calendar_handles_leap_years_and_month_boundaries() {
        for (year, month, days) in [
            (2024, 2, 29),
            (2023, 2, 28),
            (1900, 2, 28),
            (2000, 2, 29),
            (2024, 12, 31),
            (2024, 4, 30),
        ] {
            assert_eq!(
                days_in_month(NaiveDate::from_ymd_opt(year, month, 1).unwrap()),
                days
            );
        }
        assert_eq!(parse("2024-02", true), NaiveDate::from_ymd_opt(2024, 2, 1));
        assert!(parse("2023-02-29", false).is_none());
    }
    #[test]
    fn month_picker_selects_a_month_without_inventing_a_day() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut value = "2024-01".to_string();
        let frame = |value: &mut String, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.set_width(300.0);
                        month_input(ui, "month-test", value, false);
                    });
                },
            );
            output.textures_delta.clear();
            output
        };
        let click = |value: &mut String, pos| {
            for pressed in [true, false] {
                let _ = frame(
                    value,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
        };
        let _ = frame(&mut value, vec![]);
        let _ = frame(&mut value, vec![]);
        click(
            &mut value,
            ctx.read_response(egui::Id::new(("month-test", "calendar")))
                .unwrap()
                .rect
                .center(),
        );
        let _ = frame(&mut value, vec![]);
        let output = frame(&mut value, vec![]);
        fn find(shape: &egui::Shape) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == "Feb" => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(find),
                _ => None,
            }
        }
        let pos = output
            .shapes
            .iter()
            .find_map(|s| find(&s.shape))
            .expect("February must be selectable");
        click(&mut value, pos);
        assert_eq!(value, "2024-02");
    }
}
