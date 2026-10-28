use super::*;

impl TasksUi<'_> {
    pub(super) fn task_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.state.editor.take() else {
            return;
        };
        let imported = attachments::poll(self.state, &mut editor);
        for path in ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect::<Vec<_>>()
        }) {
            if self.state.attachment_job.is_none() && !editor.pending_attachments.contains(&path) {
                editor.pending_attachments.push(path);
            }
        }
        let confirming_delete = editor.delete_confirm;
        let confirm_keys =
            design::ActionKeys::capture(ctx).blocked(self.state.attachment_job.is_some());
        let keys = confirm_keys
            .multiline(egui::Id::new("task-description"))
            .blocked(confirming_delete);
        let mut save = imported;
        let mut remove = false;
        let mut cancel = false;
        let title = if editor.is_new {
            "Create Task"
        } else {
            "Edit Task"
        };
        let valid_date = editor.draft.due_date.is_empty()
            || chrono::NaiveDate::parse_from_str(&editor.draft.due_date, "%Y-%m-%d").is_ok();
        let modal = design::modal(ctx, egui::Id::new("task-editor")).show(ctx, |ui| {
            if confirming_delete {
                ui.disable();
            }
            ui.set_width(design::dialog_width(ctx, 900.0));
            cancel = design::dialog_heading(ui, title).clicked();
            egui::ScrollArea::vertical()
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .id_salt(("task-form-body", &editor.draft.id))
                .max_height(design::dialog_body_height(ctx))
                .show(ui, |ui| {
                    let side_width = 320.0;
                    let main_width = (ui.available_width() - side_width - 32.0).max(200.0);
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(main_width, 0.0),
                            egui::Layout::top_down(egui::Align::LEFT),
                            |ui| {
                                field_label(ui, "Summary *");
                                let focus_summary = editor.focus_requested
                                    && ui.is_visible()
                                    && !ui.is_sizing_pass();
                                if focus_summary {
                                    ui.memory_mut(|m| {
                                        m.request_focus(egui::Id::new("task-summary"))
                                    });
                                }
                                let summary = ui.add(
                                    design::input(&mut editor.draft.title)
                                        .id(egui::Id::new("task-summary"))
                                        .hint_text("What needs to be done?")
                                        .desired_width(f32::INFINITY),
                                );
                                if focus_summary {
                                    summary.request_focus();
                                    editor.focus_requested = false;
                                }
                                ui.add_space(8.0);
                                field_label(ui, "Description");
                                let location = self.state.board_folder.as_ref().map(|root| {
                                    amigo_ui::media::Location {
                                        root: root.clone(),
                                        base: root.clone(),
                                    }
                                });
                                task_description(ui, &mut editor, location.as_ref());
                                attachments::show(self.state, &mut editor, ui);
                            },
                        );
                        ui.allocate_ui_with_layout(
                            Vec2::new(side_width, 0.0),
                            egui::Layout::top_down(egui::Align::LEFT),
                            |ui| {
                                let control_width = 160.0;
                                design::settings_group(ui, "Details", |ui| {
                                    design::property_row(ui, "Status", |ui| {
                                        egui::ComboBox::from_id_salt("task-status")
                                            .width(control_width)
                                            .truncate()
                                            .selected_text(
                                                self.workspace
                                                    .task_statuses
                                                    .iter()
                                                    .find(|s| s.id == editor.draft.status)
                                                    .map(|s| s.name.as_str())
                                                    .unwrap_or("Choose Status"),
                                            )
                                            .show_ui(ui, |ui| {
                                                for status in &self.workspace.task_statuses {
                                                    ui.selectable_value(
                                                        &mut editor.draft.status,
                                                        status.id.clone(),
                                                        &status.name,
                                                    );
                                                }
                                            });
                                    });
                                    design::property_row(ui, "Priority", |ui| {
                                        egui::ComboBox::from_id_salt("task-priority")
                                            .width(control_width)
                                            .selected_text(&editor.draft.priority)
                                            .show_ui(ui, |ui| {
                                                for priority in ["High", "Medium", "Low"] {
                                                    ui.selectable_value(
                                                        &mut editor.draft.priority,
                                                        priority.to_owned(),
                                                        priority,
                                                    );
                                                }
                                            });
                                    });
                                    design::property_row(ui, "Category", |ui| {
                                        egui::ComboBox::from_id_salt("task-category")
                                            .width(control_width)
                                            .truncate()
                                            .selected_text(
                                                self.workspace
                                                    .task_categories
                                                    .iter()
                                                    .find(|c| c.id == editor.draft.category_id)
                                                    .map(|c| c.name.as_str())
                                                    .unwrap_or("Uncategorized"),
                                            )
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(
                                                    &mut editor.draft.category_id,
                                                    String::new(),
                                                    "Uncategorized",
                                                );
                                                for category in &self.workspace.task_categories {
                                                    ui.selectable_value(
                                                        &mut editor.draft.category_id,
                                                        category.id.clone(),
                                                        &category.name,
                                                    );
                                                }
                                            });
                                    });
                                    design::property_row(ui, "Due Date", |ui| {
                                        ui.allocate_ui_with_layout(
                                            Vec2::new(control_width, design::CONTROL_HEIGHT),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                date_input(
                                                    ui,
                                                    "task-due-date",
                                                    &mut editor.draft.due_date,
                                                    true,
                                                );
                                            },
                                        );
                                    });
                                    if !valid_date {
                                        ui.colored_label(
                                            colors(ui).red,
                                            "Use YYYY-MM-DD for the due date.",
                                        );
                                    }
                                });
                            },
                        );
                    });
                });
            let valid_date = editor.draft.due_date.is_empty()
                || chrono::NaiveDate::parse_from_str(&editor.draft.due_date, "%Y-%m-%d").is_ok();
            design::dialog_footer(ui, |ui| {
                if !editor.is_new
                    && ui
                        .add_enabled_ui(self.state.attachment_job.is_none(), |ui| {
                            design::icon_button(
                                ui,
                                egui::Id::new("task-delete"),
                                Icon::Trash,
                                "Delete Task",
                                design::CONTROL_HEIGHT,
                            )
                        })
                        .inner
                        .clicked()
                {
                    editor.delete_confirm = true;
                    ui.memory_mut(|m| {
                        if let Some(id) = m.focused() {
                            m.surrender_focus(id);
                        }
                    });
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    save |= ui
                        .add_enabled_ui(
                            self.state.attachment_job.is_none()
                                && !editor.draft.title.trim().is_empty()
                                && valid_date
                                && self
                                    .workspace
                                    .task_statuses
                                    .iter()
                                    .any(|s| s.id == editor.draft.status),
                            |ui| design::dialog_primary(ui, "Save Task"),
                        )
                        .inner
                        .clicked();
                    cancel |= design::button(ui, "Cancel").clicked();
                });
            });
        });
        let valid = !editor.draft.title.trim().is_empty()
            && (editor.draft.due_date.is_empty()
                || chrono::NaiveDate::parse_from_str(&editor.draft.due_date, "%Y-%m-%d").is_ok())
            && self
                .workspace
                .task_statuses
                .iter()
                .any(|s| s.id == editor.draft.status);
        let action = keys.dialog(&modal, valid);
        if !editor.delete_confirm {
            cancel |= action.cancel;
            save |= action.confirm;
        }
        if editor.delete_confirm {
            save = false;
            let mut keep = false;
            let mut action = design::ActionShortcut::default();
            let confirmation_id = egui::Id::new("task-delete-confirmation");
            ctx.set_sublayer(
                modal.response.layer_id,
                egui::LayerId::new(egui::Order::Foreground, confirmation_id),
            );
            design::modal(ctx, confirmation_id).show(ctx, |ui| {
                ui.set_width(design::dialog_width(ctx, 440.0));
                keep = design::dialog_heading(ui, "Delete Task?").clicked();
                ui.label("Delete this task permanently?");
                ui.add(egui::Label::new(RichText::new(&editor.draft.title).strong()).truncate());
                design::dialog_footer(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        remove = design::dialog_destructive(ui, "Delete Task").clicked();
                        keep |= design::button(ui, "Cancel").clicked();
                    });
                });
                // Activating the trash icon with Enter must only open confirmation.
                if confirming_delete {
                    action = confirm_keys.in_ui(ui, true);
                }
            });
            if keep || action.cancel {
                editor.delete_confirm = false;
                editor.focus_requested = true;
                remove = false;
            } else {
                remove |= action.confirm;
            }
        }
        if cancel {
            self.state.attachment_job = None;
            return;
        }
        if remove {
            save = false;
            self.state.attachment_job = None;
        }
        if save && !valid {
            save = false;
            editor.attachment_error = "Complete the required task details before saving.".into();
        }
        if save {
            if !attachments::save(self.state, &mut editor, ctx) {
                self.state.editor = Some(editor);
                return;
            }
            if !editor.is_new
                && !self
                    .workspace
                    .tasks
                    .iter()
                    .any(|task| task.id == editor.draft.id)
            {
                editor.attachment_error =
                    "This task was removed from the board while it was open.".into();
                self.state.editor = Some(editor);
                return;
            }
            for batch in &mut editor.imported_attachments {
                batch.keep();
            }
            editor.draft.title = editor.draft.title.trim().into();
            if editor.is_new {
                self.workspace.next_task = self.workspace.next_task.saturating_add(1);
                self.workspace.tasks.push(editor.draft);
            } else if let Some(task) = self
                .workspace
                .tasks
                .iter_mut()
                .find(|t| t.id == editor.draft.id)
            {
                *task = editor.draft;
            }
        } else if remove {
            self.workspace.tasks.retain(|t| t.id != editor.draft.id);
        } else if !cancel {
            self.state.editor = Some(editor);
        }
    }
}

fn task_description(
    ui: &mut egui::Ui,
    editor: &mut TaskEditor,
    location: Option<&amigo_ui::media::Location>,
) {
    let rows = if editor.draft.attachments.is_empty() && editor.pending_attachments.is_empty() {
        6
    } else {
        3
    };
    let response = editor.description_editor.show(
        ui,
        egui::Id::new("task-description"),
        &mut editor.draft.description,
        location,
        rows,
    );
    ui.interact(
        response.rect,
        egui::Id::new("task-description-preview"),
        egui::Sense::hover(),
    );
}
