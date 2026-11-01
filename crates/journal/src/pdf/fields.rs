//! Form widgets use the shared text-input wrapper (spelling / dictionary / emoji).
use super::*;
use amigo_core::journal::pdf::{FieldKind, Value};
pub(super) fn show(
    ui: &mut egui::Ui,
    document: &mut PdfDocument,
    page: &Page,
    selected: &mut Option<i32>,
    reveal: &mut Option<i32>,
) {
    ui.heading("Page Forms");
    if !page.can_fill {
        design::hint(ui, "Form filling is disabled by this PDF.");
    }
    if page.fields.is_empty() {
        design::hint(
            ui,
            "This page has no fillable form fields. Use Add Text to fill a scanned or flat form.",
        );
    }
    for field in &page.fields {
        ui.push_id(("pdf-field-editor", field.id), |ui| {
            if *selected == Some(field.id) {
                ui.colored_label(colors(ui).accent, RichText::new(&field.name).strong());
            } else {
                ui.label(RichText::new(&field.name).strong());
            }
            let focus = *reveal == Some(field.id);
            if focus {
                ui.scroll_to_cursor(Some(egui::Align::Center));
                *reveal = None;
            }
            if field.read_only {
                design::hint(ui, "Read-only field");
            }
            let mut value = document
                .edits
                .fields
                .get(&field.id)
                .cloned()
                .unwrap_or_else(|| field.value.clone());
            let mut changed = false;
            ui.add_enabled_ui(page.can_fill && !field.read_only, |ui| {
                match (&field.kind, &mut value) {
                    (
                        FieldKind::Text {
                            multiline,
                            password,
                            max_len,
                        },
                        Value::Text(text),
                    ) => {
                        let input = if *multiline {
                            text_input::TextEdit::multiline(text).desired_rows(3)
                        } else {
                            text_input::TextEdit::singleline(text)
                        };
                        let input = input
                            .id(egui::Id::new(("pdf-field-input", field.id)))
                            .desired_width(f32::INFINITY)
                            .password(*password)
                            .char_limit(if *max_len > 0 { *max_len } else { 65536 })
                            .show(ui);
                        if focus {
                            input.response.request_focus();
                        }
                        changed = input.response.changed();
                    }
                    (FieldKind::Button { radio }, Value::Button(state)) => {
                        if *radio {
                            if ui.radio(*state, "Select").clicked() && !*state {
                                *state = true;
                                changed = true;
                            }
                        } else {
                            changed = design::checkbox(ui, state, "Checked").changed();
                        }
                    }
                    (
                        FieldKind::Choice {
                            items,
                            multiple,
                            editable,
                        },
                        Value::Choice {
                            selected: choices,
                            text,
                        },
                    ) => {
                        if *multiple {
                            for (index, item) in items.iter().enumerate() {
                                let mut enabled = choices.contains(&(index as i32));
                                if design::checkbox(ui, &mut enabled, item).changed() {
                                    if enabled {
                                        choices.push(index as i32);
                                    } else {
                                        choices.retain(|n| *n != index as i32);
                                    }
                                    changed = true;
                                }
                            }
                        } else {
                            let label = choices
                                .first()
                                .and_then(|i| items.get(*i as usize))
                                .map_or("Choose…", String::as_str);
                            amigo_ui::design::ComboBox::from_id_salt(field.id)
                                .selected_text(label)
                                .width(ui.available_width())
                                .show_ui(ui, |ui| {
                                    if ui.selectable_label(choices.is_empty(), "None").clicked() {
                                        choices.clear();
                                        changed = true;
                                    }
                                    for (index, item) in items.iter().enumerate() {
                                        if ui
                                            .selectable_label(
                                                choices.contains(&(index as i32)),
                                                item,
                                            )
                                            .clicked()
                                        {
                                            *choices = vec![index as i32];
                                            if *editable {
                                                *text = None;
                                            }
                                            changed = true;
                                        }
                                    }
                                });
                        }
                        if *editable {
                            let mut custom = text.clone().unwrap_or_else(|| {
                                choices
                                    .first()
                                    .and_then(|i| items.get(*i as usize))
                                    .cloned()
                                    .unwrap_or_default()
                            });
                            if text_input::TextEdit::singleline(&mut custom)
                                .id_salt((field.id, "custom"))
                                .hint_text("Custom value")
                                .desired_width(f32::INFINITY)
                                .show(ui)
                                .response
                                .changed()
                            {
                                choices.clear();
                                *text = Some(custom);
                                changed = true;
                            }
                        }
                    }
                    _ => {
                        design::hint(
                            ui,
                            "Signatures and action buttons are read-only in Journal.",
                        );
                    }
                }
            });
            if changed {
                if matches!(field.kind, FieldKind::Button { radio: true })
                    && matches!(value, Value::Button(true))
                {
                    for other in &page.fields {
                        if other.id != field.id
                            && other.group == field.group
                            && matches!(other.kind, FieldKind::Button { radio: true })
                        {
                            document.edits.fields.insert(other.id, Value::Button(false));
                        }
                    }
                }
                document.edits.fields.insert(field.id, value);
            }
            ui.add_space(8.);
        });
    }
    ui.separator();
    ui.heading("Annotations");
    if !page.can_annotate {
        design::hint(ui, "Annotations are disabled by this PDF.");
    }
    for annotation in &page.annotations {
        if document
            .edits
            .annotations
            .get(&(page.number, annotation.index))
            .is_some_and(Option::is_none)
        {
            continue;
        }
        let mut value = document
            .edits
            .annotations
            .get(&(page.number, annotation.index))
            .and_then(Clone::clone)
            .unwrap_or_else(|| annotation.text.clone());
        ui.push_id(("pdf-annotation", annotation.index), |ui| {
            ui.add_enabled_ui(page.can_annotate && !annotation.read_only, |ui| {
                if text_input::TextEdit::multiline(&mut value)
                    .desired_width(f32::INFINITY)
                    .desired_rows(2)
                    .char_limit(16384)
                    .show(ui)
                    .response
                    .changed()
                {
                    document
                        .edits
                        .annotations
                        .insert((page.number, annotation.index), Some(value));
                }
                if design::button(ui, "Remove Annotation").clicked() {
                    document
                        .edits
                        .annotations
                        .insert((page.number, annotation.index), None);
                }
            });
        });
    }
    let mut remove = None;
    for (index, annotation) in document
        .edits
        .added
        .iter_mut()
        .enumerate()
        .filter(|(_, a)| a.page == page.number)
    {
        ui.push_id(&annotation.id, |ui| {
            ui.label(if annotation.note {
                "Added note"
            } else {
                "Added text"
            });
            text_input::TextEdit::multiline(&mut annotation.text)
                .desired_width(f32::INFINITY)
                .desired_rows(2)
                .char_limit(16384)
                .show(ui);
            if design::button(ui, "Remove").clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        document.edits.added.remove(index);
    }
}
