//! Journal creation, confirmation, and external-file destination dialogs.
use super::*;
pub(super) struct ExternalFileDialog {
    source: std::path::PathBuf,
    folder: String,
    error: String,
}
impl JournalTab {
    pub fn choose_external_destination(
        &mut self,
        source: std::path::PathBuf,
        folder: String,
        error: String,
    ) {
        self.search = None;
        self.external_file = Some(ExternalFileDialog {
            source,
            folder,
            error,
        });
    }
    pub fn dialogs(&mut self, ctx: &egui::Context) -> Option<Action> {
        if self.external_file.is_some() {
            return self.external_file_ui(ctx);
        }
        if self.search.is_some() {
            return self.search_dialog_ui(ctx);
        }
        self.name_dialog_ui(ctx)
    }
    fn external_file_ui(&mut self, ctx: &egui::Context) -> Option<Action> {
        let mut dialog = self.external_file.take()?;
        let keys = design::ActionKeys::capture(ctx);
        let mut close = false;
        let mut save = false;
        let response =
            design::modal(ctx, egui::Id::new("journal-external-destination")).show(ctx, |ui| {
                ui.set_width(design::dialog_width(ctx, 540.));
                close = design::dialog_heading(ui, "Open External File").clicked();
                let name = dialog
                    .source
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                ui.label(RichText::new(name).strong());
                design::hint(
                    ui,
                    "Choose where to save a copy in Journal. The original file stays unchanged.",
                );
                ui.add_space(12.);
                crate::dialogs::destination_picker(ui, &mut dialog.folder, &self.entries);
                if !dialog.error.is_empty() {
                    ui.colored_label(colors(ui).red, &dialog.error);
                }
                design::dialog_footer(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        save = design::dialog_primary(ui, "Save and Open").clicked();
                        close |= design::button(ui, "Cancel").clicked();
                    });
                });
            });
        let shortcut = keys.dialog(&response, true);
        if close || shortcut.cancel {
            return None;
        }
        if save || shortcut.confirm {
            return Some(Action::StoreExternal {
                source: dialog.source,
                folder: dialog.folder,
            });
        }
        self.external_file = Some(dialog);
        None
    }
    fn name_dialog_ui(&mut self, ctx: &egui::Context) -> Option<Action> {
        let mut dialog = self.dialog.take()?;
        let keys = design::ActionKeys::capture(ctx);
        let mut close = false;
        let mut submit = false;
        let deleting = matches!(dialog.kind, NameKind::Delete(_));
        let permanent = matches!(&dialog.kind, NameKind::Delete(path) if is_in_trash(path));
        let delete_label = if permanent {
            "Delete Permanently"
        } else {
            "Move to Trash"
        };
        let title = match &dialog.kind {
            NameKind::Note(_) => "New Note",
            NameKind::Folder(_) => "New Folder",
            NameKind::Delete(_) => delete_label,
        };
        let response = design::modal(ctx, egui::Id::new("journal-name-dialog")).show(ctx, |ui| {
            ui.set_width(design::dialog_width(ctx, 520.));
            close = design::dialog_heading(ui, title).clicked();
            if let NameKind::Delete(path) = &dialog.kind {
                ui.label(amigo_core::journal::entry_name(path));
                design::hint(
                    ui,
                    if permanent {
                        "This item and any contents will be permanently deleted. This cannot be undone."
                    } else {
                        "The item and any contents will be moved together to Trash. You can restore them there."
                    },
                );
            } else {
                let parent = match &dialog.kind {
                    NameKind::Note(parent) | NameKind::Folder(parent) => parent.as_str(),
                    NameKind::Delete(_) => unreachable!(),
                };
                amigo_ui::forms::field_label(ui, "Create in");
                ui.label(if parent.is_empty() { "Journal Root" } else { parent });
                ui.add_space(12.);
                amigo_ui::forms::field_label(ui, "Name");
                ui.add(
                    design::input(&mut dialog.value)
                        .id(egui::Id::new("journal-note-name"))
                        .desired_width(f32::INFINITY),
                );
            }
            if !dialog.error.is_empty() {
                ui.colored_label(colors(ui).red, &dialog.error);
            }
            design::dialog_footer(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    submit = ui
                        .add_enabled_ui(deleting || !dialog.value.trim().is_empty(), |ui| {
                            design::dialog_primary(
                                ui,
                                if deleting { delete_label } else { "Save" },
                            )
                        })
                        .inner
                        .clicked();
                    close |= design::button(ui, "Cancel").clicked();
                });
            });
        });
        let shortcut = keys.dialog(&response, !deleting && !dialog.value.trim().is_empty());
        if close || shortcut.cancel {
            return None;
        }
        if submit || shortcut.confirm {
            return Some(match dialog.kind {
                NameKind::Note(parent) => Action::NewNote(child_path(&parent, &dialog.value)),
                NameKind::Folder(parent) => Action::NewFolder(child_path(&parent, &dialog.value)),
                NameKind::Delete(path) if permanent => Action::DeletePermanently(path),
                NameKind::Delete(path) => Action::Trash(path),
            });
        }
        self.dialog = Some(dialog);
        None
    }
}

fn child_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.trim().into()
    } else {
        format!("{parent}/{}", name.trim())
    }
}

fn destination_picker(ui: &mut egui::Ui, folder: &mut String, entries: &[Entry]) {
    amigo_ui::forms::field_label(ui, "Save in");
    let picker = egui::ComboBox::from_id_salt("journal-external-folder")
        .width(ui.available_width())
        .selected_text(if folder.is_empty() {
            "Journal Root"
        } else {
            folder.as_str()
        })
        .show_ui(ui, |ui| {
            let root = ui.selectable_value(folder, String::new(), "Journal Root");
            ui.interact(
                root.rect,
                egui::Id::new(("journal-destination-folder", "")),
                egui::Sense::hover(),
            );
            for entry in entries
                .iter()
                .filter(|entry| entry.folder && !is_in_trash(&entry.path))
            {
                let row = ui.selectable_value(folder, entry.path.clone(), &entry.path);
                ui.interact(
                    row.rect,
                    egui::Id::new(("journal-destination-folder", &entry.path)),
                    egui::Sense::hover(),
                );
            }
        });
    ui.interact(
        picker.response.rect,
        egui::Id::new("journal-external-folder-open"),
        egui::Sense::hover(),
    );
}
