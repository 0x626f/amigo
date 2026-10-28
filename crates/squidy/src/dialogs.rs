use super::*;
use amigo_core::squidy::Filter;
use amigo_ui::{forms::field_label, media::Location};
impl SquidyTab {
    pub(super) fn card_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.editor.take() else {
            return;
        };
        let data_id = egui::Id::new(("squidy-card-data", &editor.card.id));
        let brief_id = egui::Id::new(("squidy-card-brief", &editor.card.id));
        let keys = design::ActionKeys::capture(ctx)
            .multiline(data_id)
            .multiline(brief_id)
            .blocked(self.loading);
        let mut save = false;
        let mut cancel = false;
        let mut delete = false;
        let location = self.root.as_ref().map(|root| Location {
            root: root.clone(),
            base: root.join("content/cards"),
        });
        let modal = design::modal(ctx, egui::Id::new("squidy-card-editor")).show(ctx, |ui| {
            ui.set_width(design::dialog_width(ctx, 840.));
            cancel = design::dialog_heading(
                ui,
                if editor.original.is_some() {
                    "Edit Squidy Card"
                } else {
                    "Create Squidy Card"
                },
            )
            .clicked();
            egui::ScrollArea::vertical()
                .id_salt("squidy-card-body")
                .max_height(design::dialog_body_height(ctx))
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.add_enabled_ui(!self.loading, |ui| {
                        field_label(ui, "Name *");
                        let response = ui.add(
                            design::input(&mut editor.card.name)
                                .char_limit(200)
                                .id(egui::Id::new(("squidy-card-name", &editor.card.id)))
                                .desired_width(f32::INFINITY),
                        );
                        if editor.focus && !ui.is_sizing_pass() {
                            response.request_focus();
                            editor.focus = false;
                        }
                        ui.add_space(10.);
                        field_label(ui, "Brief");
                        ui.add(
                            egui::TextEdit::multiline(&mut editor.card.brief)
                                .id(brief_id)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(10.);
                        ui.horizontal(|ui| {
                            field_label(ui, "Card Color");
                            controls::color(
                                ui,
                                egui::Id::new(("squidy-card-color", &editor.card.id)),
                                &mut editor.card.color,
                            );
                        });
                        ui.add_space(10.);
                        field_label(ui, "Data");
                        let preview = editor.data_editor.show(
                            ui,
                            data_id,
                            &mut editor.card.data,
                            location.as_ref(),
                            8,
                        );
                        ui.interact(
                            preview.rect,
                            egui::Id::new(("squidy-card-preview", &editor.card.id)),
                            egui::Sense::hover(),
                        );
                        ui.add_space(14.);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Attachments").strong());
                            if design::button(ui, "Attach Files").clicked() {
                                self.action = Some(Action::Attach(editor.card.id.clone()));
                            }
                        });
                        let mut remove = None;
                        for file in &editor.card.attachments {
                            ui.horizontal(|ui| {
                                if design::small_button(ui, &file.name).clicked() {
                                    self.action = Some(Action::OpenFile(file.clone()));
                                }
                                if design::icon_button(
                                    ui,
                                    egui::Id::new(("squidy-remove-attachment", &file.id)),
                                    Icon::Close,
                                    "Remove Attachment",
                                    24.,
                                )
                                .clicked()
                                {
                                    remove = Some(file.id.clone());
                                }
                            });
                            if amigo_core::media::kind(&file.name).is_some()
                                && editor.data_editor.editing
                                && let Some(location) = &location
                            {
                                amigo_ui::media::show(
                                    ui,
                                    location,
                                    &amigo_core::media::target(file, 2),
                                );
                            }
                        }
                        if let Some(id) = remove {
                            if let Some(file) = editor.card.attachments.iter().find(|f| f.id == id)
                            {
                                amigo_core::media::remove_embeds(
                                    &mut editor.card.data,
                                    &amigo_core::media::target(file, 2),
                                );
                            }
                            editor.card.attachments.retain(|file| file.id != id);
                        }
                        if !editor.error.is_empty() {
                            ui.colored_label(design::colors(ui).red, &editor.error);
                        }
                    });
                });
            design::dialog_footer(ui, |ui| {
                if editor.original.is_some()
                    && design::icon_button(
                        ui,
                        egui::Id::new("squidy-delete-card"),
                        Icon::Trash,
                        "Delete Card",
                        32.,
                    )
                    .clicked()
                {
                    delete = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    save = ui
                        .add_enabled_ui(
                            !self.loading && !editor.card.name.trim().is_empty(),
                            |ui| design::dialog_primary(ui, "Save Card"),
                        )
                        .inner
                        .clicked();
                    cancel |= design::button(ui, "Cancel").clicked();
                });
            });
        });
        let action = keys.dialog(&modal, !editor.card.name.trim().is_empty());
        save |= action.confirm;
        cancel |= action.cancel;
        if cancel {
            return;
        }
        if delete {
            self.confirm_delete(Delete::Card(editor.card.id.clone()));
            save = false;
        }
        if save && let Some(board) = &mut self.board {
            match board.put_card(editor.card.clone()) {
                Ok(()) => {
                    for batch in &mut editor.batches {
                        batch.keep();
                    }
                    self.contents_changed();
                    return;
                }
                Err(error) => editor.error = error,
            }
        }
        self.editor = Some(editor);
    }
    pub(super) fn link_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut link) = self.link.take() else {
            return;
        };
        let Some(board) = &self.board else {
            return;
        };
        let exists = board.links.iter().any(|e| e.id == link.id);
        let keys = design::ActionKeys::capture(ctx);
        let mut save = false;
        let mut cancel = false;
        let mut delete = false;
        let names = controls::sorted(board.cards.iter().map(|c| c.name.clone()));
        let titles = controls::sorted(board.links.iter().map(|e| e.title.clone()));
        let modal = design::modal(ctx, egui::Id::new("squidy-link-editor")).show(ctx, |ui| {
            ui.set_width(design::dialog_width(ctx, 520.));
            cancel = design::dialog_heading(
                ui,
                if exists {
                    "Edit Connection"
                } else {
                    "Connect Cards"
                },
            )
            .clicked();
            design::dialog_body(ui, "squidy-link-body", |ui| {
                ui.spacing_mut().item_spacing.y = 6.;
                ui.horizontal(|ui| {
                    field_label(ui, "Direction");
                    design::segmented(ui, &mut link.directed, "Bidirectional", "Directed");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if design::icon_button(
                            ui,
                            egui::Id::new("squidy-link-swap"),
                            Icon::Rates,
                            "Swap From and To",
                            32.,
                        )
                        .clicked()
                        {
                            std::mem::swap(&mut link.from, &mut link.to);
                        }
                    });
                });
                ui.add_space(4.);
                for (label, value) in [("From", &mut link.from), ("To", &mut link.to)] {
                    field_label(ui, label);
                    controls::choices(
                        ui,
                        egui::Id::new(("squidy-link-card", label)),
                        value,
                        &names,
                    );
                    ui.add_space(4.);
                }
                field_label(ui, "Connection Title *");
                let title = controls::choices(
                    ui,
                    egui::Id::new(("squidy-link-title", &link.id)),
                    &mut link.title,
                    &titles,
                );
                if self.link_focus && !ui.is_sizing_pass() {
                    title.request_focus();
                    self.link_focus = false;
                }
                ui.add_space(4.);
                ui.horizontal(|ui| {
                    field_label(ui, "Edge Color");
                    controls::color(
                        ui,
                        egui::Id::new(("squidy-edge-color", &link.id)),
                        &mut link.color,
                    );
                });
                if !self.error.is_empty() {
                    ui.colored_label(design::colors(ui).red, &self.error);
                }
            });
            design::dialog_footer(ui, |ui| {
                if exists
                    && design::icon_button(
                        ui,
                        egui::Id::new("squidy-delete-link"),
                        Icon::Trash,
                        "Delete Connection",
                        32.,
                    )
                    .clicked()
                {
                    delete = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    save = ui
                        .add_enabled_ui(!link.title.trim().is_empty(), |ui| {
                            design::dialog_primary(ui, "Save Connection")
                        })
                        .inner
                        .clicked();
                    cancel |= design::button(ui, "Cancel").clicked();
                });
            });
        });
        let action = keys.dialog(&modal, !link.title.trim().is_empty());
        save |= action.confirm;
        cancel |= action.cancel;
        if cancel {
            return;
        }
        if delete {
            self.confirm_delete(Delete::Link(link.id.clone()));
            save = false;
        }
        if save {
            match self.board.as_mut().unwrap().put_link(link.clone()) {
                Ok(()) => {
                    self.error.clear();
                    self.contents_changed();
                    return;
                }
                Err(error) => self.error = error,
            }
        }
        self.link = Some(link);
    }
    pub(super) fn name_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.name_dialog.take() else {
            return;
        };
        let keys = design::ActionKeys::capture(ctx);
        let mut save = false;
        let mut cancel = false;
        let mut remove = false;
        let title = match dialog {
            NameDialog::Create(_) => "New Squidy Board",
            NameDialog::Rename(_) => "Squidy Board Options",
            NameDialog::Refresh => "Refresh Squidy Board?",
        };
        let modal = design::modal(ctx, egui::Id::new("squidy-board-editor")).show(ctx, |ui| {
            ui.set_width(design::dialog_width(ctx, 460.));
            cancel = design::dialog_heading(ui, title).clicked();
            design::dialog_body(ui, "squidy-board-body", |ui| {
                match &mut dialog {
                    NameDialog::Create(name) | NameDialog::Rename(name) => {
                        field_label(ui, "Board Name");
                        let input = ui.add(
                            design::input(name)
                                .id(egui::Id::new("squidy-board-name"))
                                .char_limit(200)
                                .desired_width(f32::INFINITY),
                        );
                        if self.name_focus && !ui.is_sizing_pass() {
                            input.request_focus();
                            self.name_focus = false;
                        }
                    }
                    NameDialog::Refresh => {
                        ui.label(
                        "Reload this board from disk? Any unsaved graph changes will be discarded.",
                    );
                    }
                }
                if !self.error.is_empty() {
                    ui.colored_label(design::colors(ui).red, &self.error);
                }
            });
            design::dialog_footer(ui, |ui| {
                if matches!(dialog, NameDialog::Rename(_))
                    && design::icon_button(
                        ui,
                        egui::Id::new("squidy-delete-board"),
                        Icon::Trash,
                        "Delete Board",
                        32.,
                    )
                    .clicked()
                {
                    remove = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let valid = match &dialog {
                        NameDialog::Create(name) | NameDialog::Rename(name) => {
                            !name.trim().is_empty()
                        }
                        NameDialog::Refresh => true,
                    };
                    save = ui
                        .add_enabled_ui(valid, |ui| {
                            if matches!(dialog, NameDialog::Refresh) {
                                design::dialog_destructive(ui, "Reload")
                            } else {
                                design::dialog_primary(ui, "Save Board")
                            }
                        })
                        .inner
                        .clicked();
                    cancel |= design::button(ui, "Cancel").clicked();
                });
            });
        });
        let valid = match &dialog {
            NameDialog::Create(s) | NameDialog::Rename(s) => !s.trim().is_empty(),
            _ => true,
        };
        let action = keys.dialog(&modal, valid);
        if cancel || action.cancel {
            return;
        }
        if remove {
            if let Some(board) = &self.board {
                self.confirm_delete(Delete::Board(board.id.clone()));
            }
            return;
        }
        if (save || action.confirm) && valid {
            if let NameDialog::Create(name) | NameDialog::Rename(name) = &dialog {
                let validation = amigo_core::library::name(name).and_then(|name| {
                    let current = if matches!(dialog, NameDialog::Rename(_)) {
                        self.board.as_ref().map(|b| b.id.as_str())
                    } else {
                        None
                    };
                    if self.boards.iter().any(|b| {
                        Some(b.id.as_str()) != current
                            && b.name.to_lowercase() == name.to_lowercase()
                    }) {
                        Err("A board with this name already exists.".into())
                    } else {
                        Ok(())
                    }
                });
                if let Err(error) = validation {
                    self.error = error;
                    self.name_dialog = Some(dialog);
                    return;
                }
            }
            self.error.clear();
            self.action = Some(match dialog {
                NameDialog::Create(s) => Action::Create(s),
                NameDialog::Rename(s) => Action::Rename(s),
                NameDialog::Refresh => Action::Refresh,
            });
            return;
        }
        self.name_dialog = Some(dialog);
    }
    pub(super) fn path_dialog(&mut self, ctx: &egui::Context) {
        if !self.path_open {
            return;
        }
        let keys = design::ActionKeys::capture(ctx)
            .multiline(egui::Id::new("squidy-include-titles"))
            .multiline(egui::Id::new("squidy-exclude-titles"));
        let mut find = false;
        let mut cancel = false;
        let board = self.board.as_ref().unwrap();
        let names = controls::sorted(board.cards.iter().map(|c| c.name.clone()));
        let titles = controls::sorted(board.links.iter().map(|e| e.title.clone()));
        let modal = design::modal(ctx,egui::Id::new("squidy-find-path")).show(ctx,|ui| {
            ui.set_width(design::dialog_width(ctx,580.));
            cancel=design::dialog_heading(ui,"Find a Path").clicked();
            design::dialog_body(ui,"squidy-path-body",|ui| {
            for (label,value) in [("From Card Name",&mut self.query.from),("To Card Name",&mut self.query.to)] {
                field_label(ui,label);
                controls::choices(ui,egui::Id::new(("squidy-path-name",label)),value,&names);
                ui.add_space(8.);
            }
            field_label(ui,"Include Edge Titles (one per line)");
            controls::edge_filter(ui,egui::Id::new("squidy-include-titles"),&mut self.query.include,&titles);
            field_label(ui,"Exclude Edge Titles (one per line)");
            controls::edge_filter(ui,egui::Id::new("squidy-exclude-titles"),&mut self.query.exclude,&titles);
            design::checkbox(ui,&mut self.query.directed,"Respect Connection Direction");
            design::hint(ui,"An empty include list allows every title. Exclusions take priority. Names and titles ignore case.");
            if !self.error.is_empty(){ui.colored_label(design::colors(ui).red,&self.error);}
            });
            design::dialog_footer(ui,|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                    let confirm=ui.add_enabled_ui(!self.query.from.trim().is_empty() && !self.query.to.trim().is_empty(),|ui| design::dialog_primary(ui,"Find Path")).inner;
                    ui.interact(confirm.rect,egui::Id::new("squidy-find-path-confirm"),egui::Sense::hover());
                    find=confirm.clicked();
                    cancel|=design::button(ui,"Cancel").clicked();
                });
            });
        });
        let action = keys.dialog(
            &modal,
            !self.query.from.trim().is_empty() && !self.query.to.trim().is_empty(),
        );
        if cancel || action.cancel {
            self.path_open = false;
            return;
        }
        if find || action.confirm {
            let filter = Filter {
                directed: self.query.directed,
                include: self
                    .query
                    .include
                    .lines()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
                exclude: self
                    .query
                    .exclude
                    .lines()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
            };
            match self
                .board
                .as_ref()
                .unwrap()
                .path(&self.query.from, &self.query.to, &filter)
            {
                Ok(Some(path)) => {
                    self.error.clear();
                    let board = self.board.as_ref().unwrap();
                    let mut description = path.names[0].clone();
                    for (index, (id, next)) in
                        path.links.iter().zip(path.names.iter().skip(1)).enumerate()
                    {
                        let edge = board.links.iter().find(|e| &e.id == id).unwrap();
                        let arrows = if !edge.directed {
                            ("←", "→")
                        } else if edge.from == path.names[index] {
                            ("—", "→")
                        } else {
                            ("←", "—")
                        };
                        description
                            .push_str(&format!(" {}[{}]{} {next}", arrows.0, edge.title, arrows.1));
                    }
                    self.path_message = description;
                    self.path = Some(path);
                    self.path_open = false;
                }
                Ok(None) => {
                    self.error.clear();
                    self.path_message = "No path matches these edge filters.".into();
                    self.path = None;
                    self.path_open = false;
                }
                Err(error) => self.error = error,
            }
        }
    }
    pub(super) fn delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(delete) = self.delete.take() else {
            return;
        };
        let armed = self.delete_armed;
        self.delete_armed = true;
        if !armed {
            ctx.request_repaint();
        }
        let keys = design::ActionKeys::capture(ctx).blocked(!armed);
        let mut confirm = false;
        let mut cancel = false;
        let title = match &delete {
            Delete::Card(_) => "Delete Card?",
            Delete::Link(_) => "Delete Connection?",
            Delete::Board(_) => "Delete Squidy Board?",
        };
        let modal =
            design::modal(ctx, egui::Id::new("squidy-delete-confirmation")).show(ctx, |ui| {
                ui.set_width(design::dialog_width(ctx, 460.));
                cancel = design::dialog_heading(ui, title).clicked();
                ui.label(match &delete {
                    Delete::Card(_) => "Delete this card and all its connections?",
                    Delete::Link(_) => {
                        "Delete this connection? Other parallel connections will be kept."
                    }
                    Delete::Board(_) => {
                        "Delete this board, its cards, connections, and attachments permanently?"
                    }
                });
                design::dialog_footer(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        confirm = ui
                            .add_enabled_ui(armed, |ui| design::dialog_destructive(ui, "Delete"))
                            .inner
                            .clicked();
                        cancel |= design::button(ui, "Cancel").clicked();
                    })
                });
            });
        let action = keys.dialog(&modal, true);
        if cancel || action.cancel {
            return;
        }
        if confirm || action.confirm {
            match delete {
                Delete::Card(id) => {
                    self.board.as_mut().unwrap().remove_card(&id);
                    self.editor = None;
                    self.contents_changed();
                }
                Delete::Link(id) => {
                    self.board.as_mut().unwrap().links.retain(|e| e.id != id);
                    self.link = None;
                    self.contents_changed();
                }
                Delete::Board(id) => self.action = Some(Action::Delete(id)),
            }
        } else {
            self.delete = Some(delete);
        }
    }
}
