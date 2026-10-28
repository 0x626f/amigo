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
        let dialog = self.dialog.take()?;
        if !matches!(dialog.kind, NameKind::Delete(_)) {
            return self.name_editor_ui(ctx, dialog);
        }
        let keys = design::ActionKeys::capture(ctx);
        let mut close = false;
        let mut submit = false;
        let permanent = matches!(&dialog.kind, NameKind::Delete(path) if is_in_trash(path));
        let delete_label = if permanent {
            "Delete Permanently"
        } else {
            "Move to Trash"
        };
        let response = design::modal(ctx, egui::Id::new("journal-name-dialog")).show(ctx, |ui| {
            ui.set_width(design::dialog_width(ctx, 520.));
            close = design::dialog_heading(ui, delete_label).clicked();
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
            }
            if !dialog.error.is_empty() {
                ui.colored_label(colors(ui).red, &dialog.error);
            }
            design::dialog_footer(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    submit = design::dialog_destructive(ui, delete_label).clicked();
                    close |= design::button(ui, "Cancel").clicked();
                });
            });
        });
        let shortcut = keys.dialog(&response, false);
        if close || shortcut.cancel {
            return None;
        }
        if submit || shortcut.confirm {
            return Some(match dialog.kind {
                NameKind::Note(_) | NameKind::Folder(_) | NameKind::RenameFolder(_) => {
                    unreachable!()
                }
                NameKind::Delete(path) if permanent => Action::DeletePermanently(path),
                NameKind::Delete(path) => Action::Trash(path),
            });
        }
        self.dialog = Some(dialog);
        None
    }
    fn name_editor_ui(&mut self, ctx: &egui::Context, mut dialog: NameDialog) -> Option<Action> {
        let keys = design::ActionKeys::capture(ctx).blocked(dialog.pending);
        let renaming = matches!(dialog.kind, NameKind::RenameFolder(_));
        let note = matches!(dialog.kind, NameKind::Note(_));
        let kind = if note { "note" } else { "folder" };
        let parent = match &dialog.kind {
            NameKind::Note(parent) | NameKind::Folder(parent) => parent.as_str(),
            NameKind::RenameFolder(path) => path.rsplit_once('/').map_or("", |(parent, _)| parent),
            _ => unreachable!(),
        };
        let mut close = false;
        let mut submit = false;
        let response = design::modal(ctx, egui::Id::new("journal-name-dialog"))
            .frame(design::dialog(ctx).inner_margin(10).corner_radius(12))
            .show(ctx, |ui| {
                ui.set_width(design::dialog_width(ctx, 640.));
                ui.spacing_mut().item_spacing = egui::vec2(8., 4.);
                ui.add_enabled_ui(!dialog.pending, |ui| {
                    ui.horizontal(|ui| {
                        design::icon(
                            ui,
                            if note { Icon::Note } else { Icon::Folder },
                            colors(ui).muted,
                            22.,
                        );
                        let width = (ui.available_width() - 120. - 2. * 28. - 3. * 8.).max(80.);
                        let id = egui::Id::new("journal-note-name");
                        let mut input = design::input(&mut dialog.value)
                            .id(id)
                            .font(egui::FontId::proportional(20.))
                            .hint_text(if note {
                                "New note name"
                            } else if renaming {
                                "Folder name"
                            } else {
                                "New folder name"
                            })
                            .frame(egui::Frame::NONE)
                            .desired_width(width)
                            .show(ui);
                        if dialog.focus && !ui.is_sizing_pass() {
                            input.response.request_focus();
                            input
                                .state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(dialog.value.chars().count()),
                                )));
                            input.state.store(ctx, id);
                            dialog.focus = false;
                        }
                        let destination = if parent.is_empty() {
                            "Journal Root"
                        } else {
                            parent
                        };
                        ui.add_sized(
                            [120., 28.],
                            egui::Label::new(
                                RichText::new(destination).size(12.).color(colors(ui).muted),
                            )
                            .truncate(),
                        )
                        .on_hover_text(destination);
                        submit = ui
                            .add_enabled_ui(!dialog.value.trim().is_empty(), |ui| {
                                design::icon_button(
                                    ui,
                                    egui::Id::new(format!("journal-{kind}-confirm")),
                                    Icon::Check,
                                    if note {
                                        "Create Note"
                                    } else if renaming {
                                        "Rename Folder"
                                    } else {
                                        "Create Folder"
                                    },
                                    28.,
                                )
                            })
                            .inner
                            .clicked();
                        close = design::icon_button(
                            ui,
                            egui::Id::new(format!("journal-{kind}-cancel")),
                            Icon::Close,
                            "Cancel",
                            28.,
                        )
                        .clicked();
                    });
                });
                if !dialog.error.is_empty() {
                    ui.colored_label(colors(ui).red, &dialog.error);
                }
            });
        let shortcut = keys.dialog(&response, !dialog.value.trim().is_empty());
        if close || shortcut.cancel {
            return None;
        }
        let mut action = None;
        if (submit || shortcut.confirm) && !dialog.pending {
            let name = dialog.value.trim();
            let target = child_path(parent, name);
            let stored_target = if note && !target.to_ascii_lowercase().ends_with(".md") {
                format!("{target}.md")
            } else {
                target.clone()
            };
            let old = if let NameKind::RenameFolder(path) = &dialog.kind {
                Some(path.as_str())
            } else {
                None
            };
            if name.contains(['/', '\\']) || name == "." || name == ".." {
                dialog.error = format!("Enter a {kind} name, without a path.");
            } else if self.entries.iter().any(|entry| {
                Some(entry.path.as_str()) != old
                    && entry.path.to_lowercase() == stored_target.to_lowercase()
            }) {
                dialog.error = "A note or folder with this name already exists.".into();
            } else {
                action = Some(if let Some(old) = old {
                    Action::Rename {
                        old: old.into(),
                        name: target,
                    }
                } else if note {
                    Action::NewNote(target)
                } else {
                    Action::NewFolder(target)
                });
                dialog.pending = true;
                dialog.error.clear();
            }
        }
        self.dialog = Some(dialog);
        action
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
