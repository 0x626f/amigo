//! Notebook actions and tree; filesystem changes are returned to the shell.
use super::*;
impl JournalTab {
    pub fn sidebar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.;
            ui.add_space(12.);
            ui.label(
                RichText::new("NOTEBOOK")
                    .size(13.)
                    .strong()
                    .color(colors(ui).muted),
            );
            let buttons_width = 4. * 22. + 3. * ui.spacing().item_spacing.x;
            ui.add_space((ui.available_width() - buttons_width).max(0.));
            if design::icon_button(
                ui,
                egui::Id::new("journal-new-note"),
                Icon::Note,
                "New Note",
                22.,
            )
            .clicked()
            {
                self.dialog = Some(NameDialog::new(NameKind::Note(String::new()), "Untitled"));
            }
            if design::icon_button(
                ui,
                egui::Id::new("journal-new-folder"),
                Icon::Folder,
                "New Folder",
                22.,
            )
            .clicked()
            {
                self.dialog = Some(NameDialog::new(
                    NameKind::Folder(String::new()),
                    "New folder",
                ));
            }
            if design::icon_button(
                ui,
                egui::Id::new("journal-refresh"),
                Icon::Refresh,
                "Refresh Notes",
                22.,
            )
            .clicked()
            {
                action = Some(Action::Refresh);
            }
            if design::icon_button(
                ui,
                egui::Id::new("journal-search-open"),
                Icon::Search,
                "Search Notes",
                22.,
            )
            .clicked()
            {
                self.search = Some(search::SearchDialog::default());
            }
        });
        ui.add_space(8.);
        for entry in self.entries.clone() {
            let parent = entry.path.rsplit_once('/').map_or("", |(p, _)| p);
            if parent
                .split('/')
                .scan(String::new(), |path, part| {
                    if !path.is_empty() {
                        path.push('/');
                    }
                    path.push_str(part);
                    Some(path.clone())
                })
                .any(|p| !p.is_empty() && !self.expanded.contains(&p))
            {
                continue;
            }
            let depth = entry.path.matches('/').count();
            let row = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.;
                let can_rename = entry.folder && !is_in_trash(&entry.path);
                let actions_width = if can_rename { 52. } else { 26. };
                let width = (ui.available_width() - actions_width).max(0.);
                let name = amigo_core::journal::entry_name(&entry.path);
                let label = name.strip_suffix(".md").unwrap_or(name);
                let selected = self
                    .editor
                    .as_ref()
                    .is_some_and(|e| e.document.path == entry.path);
                let row = ui
                    .allocate_ui_with_layout(
                        egui::vec2(width, 38.),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            design::nav_row_with_options(
                                ui,
                                label,
                                if entry.path == TRASH_FOLDER {
                                    Icon::Trash
                                } else if entry.folder {
                                    Icon::Folder
                                } else {
                                    Icon::Note
                                },
                                selected,
                                design::NavRowOptions {
                                    indent: (depth as f32 * 12.).min((width - 96.).max(0.)),
                                    disclosure: entry
                                        .folder
                                        .then(|| self.expanded.contains(&entry.path)),
                                    leading_disclosure: true,
                                    ..Default::default()
                                },
                            )
                        },
                    )
                    .inner;
                ui.interact(
                    row.rect,
                    egui::Id::new(("journal-entry", &entry.path)),
                    egui::Sense::hover(),
                );
                if row.clicked() {
                    if entry.folder {
                        if !self.expanded.remove(&entry.path) {
                            self.expanded.insert(entry.path.clone());
                        }
                    } else {
                        action = Some(Action::Open(entry.path.clone()));
                    }
                }
                design::context_menu(&row).show(|ui| {
                    action = self.tree_menu(ui, Some(&entry)).or(action.take());
                });
                if !entry.folder
                    && ui
                        .add_enabled_ui(!self.loading, |ui| {
                            design::icon_button(
                                ui,
                                egui::Id::new(("journal-note-trash", &entry.path)),
                                Icon::Trash,
                                if is_in_trash(&entry.path) {
                                    "Permanently Delete Note"
                                } else {
                                    "Move Note to Trash"
                                },
                                22.,
                            )
                        })
                        .inner
                        .clicked()
                {
                    self.dialog = Some(NameDialog::new(
                        NameKind::Delete(entry.path.clone()),
                        String::new(),
                    ));
                }
                if can_rename
                    && ui
                        .add_enabled_ui(!self.loading, |ui| {
                            design::icon_button(
                                ui,
                                egui::Id::new(("journal-folder-rename", &entry.path)),
                                Icon::Edit,
                                "Rename Folder",
                                22.,
                            )
                        })
                        .inner
                        .clicked()
                {
                    self.rename_folder(&entry.path);
                }
                if entry.folder {
                    let menu = ui
                        .add_enabled_ui(!self.loading, |ui| {
                            design::icon_button(
                                ui,
                                egui::Id::new(("journal-entry-menu", &entry.path)),
                                Icon::More,
                                "Note and Folder Actions",
                                22.,
                            )
                        })
                        .inner;
                    design::menu(&menu).show(|ui| {
                        action = self.tree_menu(ui, Some(&entry)).or(action.take());
                    });
                }
            });
            // The space between children belongs to their containing folder.
            let related = if entry.folder && self.expanded.contains(&entry.path) {
                Some(entry.clone())
            } else {
                self.entries
                    .iter()
                    .find(|candidate| candidate.folder && candidate.path == parent)
                    .cloned()
            };
            let gap = egui::Rect::from_min_max(
                row.response.rect.left_bottom(),
                row.response.rect.right_bottom() + egui::vec2(0., ui.spacing().item_spacing.y),
            );
            let space = ui.interact(
                gap,
                egui::Id::new(("journal-entry-space", &entry.path)),
                egui::Sense::click(),
            );
            design::context_menu(&space).show(|ui| {
                action = self.tree_menu(ui, related.as_ref()).or(action.take());
            });
        }
        if !self.entries.iter().any(|entry| !is_in_trash(&entry.path)) && self.loaded {
            design::hint(ui, "Create a note or open a Markdown file.");
        }
        let blank = egui::Rect::from_min_max(
            ui.cursor().min,
            egui::pos2(
                ui.max_rect().right(),
                ui.clip_rect().bottom().max(ui.cursor().top()),
            ),
        );
        let root_space = ui.interact(
            blank,
            egui::Id::new("journal-root-space"),
            egui::Sense::click(),
        );
        design::context_menu(&root_space).show(|ui| {
            action = self.tree_menu(ui, None).or(action.take());
        });
        action
    }

    fn rename_folder(&mut self, path: &str) {
        self.dialog = Some(NameDialog::new(
            NameKind::RenameFolder(path.into()),
            amigo_core::journal::entry_name(path),
        ));
    }

    fn tree_menu(&mut self, ui: &mut egui::Ui, entry: Option<&Entry>) -> Option<Action> {
        ui.set_min_width(184.);
        let path = entry.map_or("", |entry| entry.path.as_str());
        let in_trash = is_in_trash(path);
        let trash_root = path.eq_ignore_ascii_case(TRASH_FOLDER);
        let folder = entry.is_none_or(|entry| entry.folder);
        let mut action = None;
        ui.add_enabled_ui(!self.loading, |ui| {
            if folder && !in_trash {
                if tree_item(ui, path, "new-note", "New Note", false) {
                    self.dialog = Some(NameDialog::new(NameKind::Note(path.into()), "Untitled"));
                }
                if tree_item(ui, path, "new-folder", "New Folder", false) {
                    self.dialog =
                        Some(NameDialog::new(NameKind::Folder(path.into()), "New folder"));
                }
                ui.separator();
            } else if !folder && !in_trash {
                if tree_item(ui, path, "open", "Open Note", false) {
                    action = Some(Action::Open(path.into()));
                }
                ui.separator();
            }
            if entry.is_some() && folder && !in_trash {
                if tree_item(ui, path, "rename", "Rename Folder…", false) {
                    self.rename_folder(path);
                }
                ui.separator();
            }
            if entry.is_none() || trash_root {
                if tree_item(ui, path, "refresh", "Refresh Notes", false) {
                    action = Some(Action::Refresh);
                }
            } else {
                if in_trash {
                    if tree_item(ui, path, "restore", "Restore", false) {
                        action = Some(Action::Restore(path.into()));
                    }
                    ui.separator();
                }
                if tree_item(
                    ui,
                    path,
                    "delete",
                    if in_trash {
                        "Delete Permanently…"
                    } else {
                        "Move to Trash…"
                    },
                    true,
                ) {
                    self.dialog = Some(NameDialog::new(
                        NameKind::Delete(path.into()),
                        String::new(),
                    ));
                }
            }
        });
        action
    }
}

fn tree_item(ui: &mut egui::Ui, path: &str, kind: &str, label: &str, destructive: bool) -> bool {
    let text = RichText::new(label).color(if destructive {
        colors(ui).red
    } else {
        colors(ui).ink
    });
    let response = ui.add(design::Button::new(text).frame(false));
    ui.interact(
        response.rect,
        egui::Id::new((format!("journal-entry-{kind}"), path)),
        egui::Sense::hover(),
    );
    if response.clicked() {
        ui.close();
        true
    } else {
        false
    }
}
