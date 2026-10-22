pub use crate::calendar::{date_field, date_input, month_field, month_input, table_date_input};
use crate::design::{self, colors};
use egui::{self, Color32, RichText};

pub fn field_label(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.label(
        RichText::new(title_case(label))
            .size(15.0)
            .strong()
            .color(colors(ui).ink),
    )
}
pub fn text_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    field_label(ui, label);
    ui.add(design::input(value).desired_width(f32::INFINITY));
    ui.add_space(6.0);
}
pub fn choice(ui: &mut egui::Ui, salt: &str, value: &mut String, options: &[&str]) {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(value.as_str())
        .show_ui(ui, |ui| {
            for option in options {
                ui.selectable_value(value, (*option).into(), *option);
            }
        });
}
pub fn labels_field(ui: &mut egui::Ui, labels: &mut Vec<String>) {
    let field_id = ui.make_persistent_id("labels-text");
    let mut text = if ui.memory(|m| m.has_focus(field_id)) {
        ui.data(|d| d.get_temp::<String>(field_id))
            .unwrap_or_else(|| labels.join(", "))
    } else {
        labels.join(", ")
    };
    let response = ui.add(
        design::input(&mut text)
            .id(field_id)
            .hint_text("Labels, separated by commas")
            .desired_width(f32::INFINITY),
    );
    if response.changed() {
        *labels = text
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    ui.data_mut(|d| d.insert_temp(field_id, text));
}

pub fn priority_color(ui: &egui::Ui, priority: &str) -> Color32 {
    match priority {
        "High" => colors(ui).red,
        "Low" => colors(ui).green,
        _ => colors(ui).muted,
    }
}

/// Format interface names without rewriting user-entered content.
pub fn title_case(text: &str) -> String {
    let mut start = true;
    let mut result = String::new();
    for ch in text.chars() {
        if start && ch.is_alphabetic() {
            result.extend(ch.to_uppercase());
            start = false;
        } else {
            result.push(ch);
            if ch.is_alphanumeric() {
                start = false;
            }
        }
        if ch.is_whitespace() || ch == '/' {
            start = true;
        }
    }
    result
}
