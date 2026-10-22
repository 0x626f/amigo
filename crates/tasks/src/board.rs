use super::*;

impl TasksUi<'_> {
    fn new_task(&mut self, status: &str) {
        self.state.new_task(self.workspace, status);
    }
    fn manage_board(&mut self) {
        self.state.manage_board(self.workspace);
    }
    fn filters(&mut self, ui: &mut egui::Ui) {
        let response = design::icon_button(
            ui,
            egui::Id::new("task-filters"),
            Icon::Filter,
            "Filter tasks",
            design::CONTROL_HEIGHT,
        );
        if !self.state.categories.is_empty() || !self.state.priorities.is_empty() {
            ui.painter().circle_filled(
                response.rect.right_top() + Vec2::new(-6.0, 6.0),
                3.0,
                colors(ui).accent,
            );
        }
        let popup_id = egui::Popup::default_response_id(&response);
        let keys = if response.clicked() || egui::Popup::is_id_open(ui.ctx(), popup_id) {
            design::ActionKeys::capture_popup(ui.ctx(), popup_id)
        } else {
            Default::default()
        };
        let mut shortcut = design::ActionShortcut::default();
        if response.clicked() && !egui::Popup::is_id_open(ui.ctx(), popup_id) {
            self.state.filter_snapshot = Some((
                self.state.categories.clone(),
                self.state.priorities.clone(),
                self.state.search.clone(),
            ));
        }
        egui::Popup::menu(&response)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(280.0)
            .frame(
                egui::Frame::popup(ui.style())
                    .inner_margin(12)
                    .corner_radius(12),
            )
            .show(|ui| {
                ui.spacing_mut().interact_size.y = 28.0;
                ui.spacing_mut().item_spacing.y = 4.0;
                egui::ScrollArea::vertical()
                    .id_salt("task-filter-menu")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .max_height((ui.ctx().content_rect().height() - 260.0).clamp(220.0, 460.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new("Categories").strong());
                        design::hint(ui, "No selection includes every category.");
                        egui::ScrollArea::vertical()
                            .id_salt("task-filter-categories")
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                            )
                            .max_height(160.0)
                            .show(ui, |ui| {
                                filter_checkbox(
                                    ui,
                                    &mut self.state.categories,
                                    "",
                                    "Uncategorized",
                                );
                                for category in &self.workspace.task_categories {
                                    filter_checkbox(
                                        ui,
                                        &mut self.state.categories,
                                        &category.id,
                                        &category.name,
                                    );
                                }
                            });
                        ui.add_space(8.0);
                        ui.separator();
                        ui.label(RichText::new("Priorities").strong());
                        design::hint(ui, "No selection includes every priority.");
                        for priority in ["High", "Medium", "Low"] {
                            filter_checkbox(ui, &mut self.state.priorities, priority, priority);
                        }
                        ui.add_space(8.0);
                        ui.separator();
                        if design::button(ui, "Clear Filters").clicked() {
                            self.state.categories.clear();
                            self.state.priorities.clear();
                            self.state.search.clear();
                        }
                    });
                shortcut = keys.in_popup(ui, popup_id);
                if shortcut.cancel || shortcut.confirm {
                    ui.close();
                }
            });
        if !egui::Popup::is_id_open(ui.ctx(), popup_id)
            && let Some((categories, priorities, search)) = self.state.filter_snapshot.take()
            && shortcut.cancel
        {
            self.state.categories = categories;
            self.state.priorities = priorities;
            self.state.search = search;
        }
    }
    pub(super) fn tasks(&mut self, ui: &mut egui::Ui) {
        let expanded_width = (ui.available_width() * 0.23).clamp(180.0, 260.0);
        let search_width = self
            .state
            .search_control
            .width(ui, "task-search", expanded_width);
        let create_width = egui::WidgetText::from(RichText::new("+ Create Task").strong())
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Button,
            )
            .size()
            .x
            + ui.spacing().button_padding.x * 2.0;
        let title_width = (ui.available_width()
            - search_width
            - create_width.max(110.0)
            - design::CONTROL_HEIGHT * 2.0
            - ui.spacing().item_spacing.x * 4.0)
            .max(0.0);
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(title_width, design::CONTROL_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(self.board_name).size(32.0).strong())
                            .truncate(),
                    )
                    .on_hover_text(self.board_name);
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary(ui, "+ Create Task").clicked() {
                    let status = self.workspace.default_task_status().to_owned();
                    self.new_task(&status);
                }
                self.filters(ui);
                if design::icon_button(
                    ui,
                    egui::Id::new("task-board-options"),
                    Icon::Settings,
                    "Board Settings",
                    design::CONTROL_HEIGHT,
                )
                .clicked()
                {
                    self.manage_board();
                }
                self.state.search_control.show(
                    ui,
                    "task-search",
                    &mut self.state.search,
                    search_width,
                    "Search Tasks…",
                );
            });
        });
        let mut visible: Vec<_> = self
            .workspace
            .tasks
            .iter()
            .filter(|t| self.state.task_matches_filters(self.workspace, t))
            .cloned()
            .collect();
        visible.sort_by_key(Task::priority_rank);
        ui.add_space(18.0);
        let statuses = self.workspace.task_statuses.clone();
        let mut movement = None;
        let allow_drop = ui.input(|i| i.pointer.primary_released() && !i.pointer.primary_down());
        let board_width = ui.available_width();
        if self.state.editor.is_some() || self.state.board_editor.is_some() {
            self.state.cards.clear();
        } else {
            self.state.cards.begin_frame(ui, &visible);
        }
        let payload = egui::DragAndDrop::payload::<cards::TaskDrag>(ui.ctx());
        let dragged = payload
            .as_ref()
            .and_then(|payload| visible.iter().find(|task| task.id == payload.0));
        egui::ScrollArea::both()
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .id_salt("task-board-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let viewport = ui.clip_rect();
                let count = statuses.len().max(1) as f32;
                let width = ((board_width - 16.0 * (count - 1.0)) / count).clamp(216.0, 400.0);
                ui.spacing_mut().item_spacing.x = 16.0;
                ui.horizontal_top(|ui| {
                    for status in &statuses {
                        ui.push_id(&status.id, |ui| {
                            let p = colors(ui);
                            let accent = status_color(ui, status.color);
                            let mut card_rects = Vec::new();
                            let mut list_top = 0.0;
                            let column = egui::Frame::new()
                                .corner_radius(8)
                                .inner_margin(12)
                                .fill(p.column.lerp_to_gamma(accent, 0.035))
                                .stroke(Stroke::new(1.0, p.line))
                                .show(ui, |ui| {
                                    ui.with_layout(
                                        egui::Layout::top_down(egui::Align::LEFT),
                                        |ui| {
                                            ui.set_width(width - 26.0);
                                            ui.set_min_height(112.0);
                                            ui.spacing_mut().item_spacing = Vec2::new(8.0, 12.0);
                                            ui.add_space(6.0);
                                            ui.horizontal(|ui| {
                                                let title_width =
                                                    (ui.available_width() - 40.0).max(0.0);
                                                egui::Frame::new()
                                                    .inner_margin(egui::Margin::symmetric(8, 6))
                                                    .show(ui, |ui| {
                                                        ui.set_width((title_width - 16.0).max(0.0));
                                                        ui.horizontal(|ui| {
                                                            ui.spacing_mut().item_spacing.x = 7.0;
                                                            design::dot_with_size(ui, accent, 8.0);
                                                            ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(&status.name)
                                                                        .strong()
                                                                        .size(14.0),
                                                                )
                                                                .truncate(),
                                                            )
                                                            .on_hover_text(&status.name);
                                                        });
                                                    });
                                                if design::icon_button(
                                                    ui,
                                                    ui.make_persistent_id((
                                                        "create-task",
                                                        &status.id,
                                                    )),
                                                    Icon::Plus,
                                                    "Create task in this column",
                                                    32.0,
                                                )
                                                .clicked()
                                                {
                                                    self.new_task(&status.id);
                                                }
                                            });
                                            ui.add_space(2.0);
                                            ui.separator();
                                            ui.add_space(2.0);
                                            list_top = ui.next_widget_position().y;
                                            for task in visible
                                                .iter()
                                                .filter(|task| task.status == status.id)
                                            {
                                                let response =
                                                    self.task_card(ui, task, status, width - 28.0);
                                                if dragged
                                                    .is_none_or(|dragged| dragged.id != task.id)
                                                {
                                                    card_rects.push((task, response.rect));
                                                }
                                            }
                                        },
                                    );
                                });
                            let rect = column.response.rect;
                            ui.interact(
                                rect,
                                egui::Id::new(("task-column", &status.id)),
                                egui::Sense::hover(),
                            );
                            let band = egui::Rect::from_min_max(
                                rect.left_top() + Vec2::new(8.0, 1.0),
                                rect.right_top() + Vec2::new(-8.0, 4.0),
                            );
                            ui.painter().rect_filled(band, 2, accent);
                            // Short/empty columns accept drops throughout the board viewport.
                            let hit = egui::Rect::from_min_max(
                                rect.min,
                                egui::pos2(rect.right(), rect.bottom().max(viewport.bottom())),
                            );
                            if let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
                                && hit.contains(pointer)
                                && viewport.contains(pointer)
                                && let Some(dragged) = dragged
                            {
                                let raw = card_rects
                                    .iter()
                                    .position(|(_, rect)| pointer.y <= rect.center().y)
                                    .unwrap_or(card_rects.len());
                                let first = card_rects
                                    .iter()
                                    .take_while(|(task, _)| {
                                        task.priority_rank() < dragged.priority_rank()
                                    })
                                    .count();
                                let last = card_rects
                                    .iter()
                                    .take_while(|(task, _)| {
                                        task.priority_rank() <= dragged.priority_rank()
                                    })
                                    .count();
                                let index = raw.clamp(first, last);
                                let y = match (
                                    index.checked_sub(1).and_then(|index| card_rects.get(index)),
                                    card_rects.get(index),
                                ) {
                                    (Some((_, before)), Some((_, after))) => {
                                        (before.bottom() + after.top()) * 0.5
                                    }
                                    (None, Some((_, after))) => after.top() - 6.0,
                                    (Some((_, before)), None) => before.bottom() + 6.0,
                                    (None, None) => list_top + 4.0,
                                };
                                let line = egui::Rect::from_min_max(
                                    egui::pos2(rect.left() + 12.0, y - 1.5),
                                    egui::pos2(rect.right() - 12.0, y + 1.5),
                                );
                                ui.painter()
                                    .hline(line.x_range(), y, Stroke::new(3.0, accent));
                                ui.painter()
                                    .circle_filled(egui::pos2(line.left(), y), 3.0, accent);
                                ui.interact(
                                    line,
                                    egui::Id::new(("task-drop-position", &status.id)),
                                    egui::Sense::hover(),
                                );
                                if allow_drop {
                                    movement = Some((
                                        dragged.id.clone(),
                                        status.id.clone(),
                                        card_rects.get(index).map(|(task, _)| task.id.clone()),
                                    ));
                                }
                            }
                        });
                    }
                });
                if dragged.is_some()
                    && ui.input(|input| input.pointer.primary_down())
                    && let Some(pointer) = ui
                        .input(|input| input.pointer.interact_pos())
                        .filter(|pointer| viewport.contains(*pointer))
                {
                    let delta = if pointer.y < viewport.top() + 32.0 {
                        14.0
                    } else if pointer.y > viewport.bottom() - 32.0 {
                        -14.0
                    } else {
                        0.0
                    };
                    if delta != 0.0 {
                        ui.scroll_with_delta(Vec2::new(0.0, delta));
                        ui.ctx().request_repaint();
                    }
                }
            });
        if let Some(task) = self.state.cards.finish_frame(ui, self.workspace) {
            self.state.edit_task(task);
        }
        if let Some((id, status, before)) = movement {
            self.workspace.move_task(&id, &status, before.as_deref());
        }
    }
}

fn filter_checkbox(ui: &mut egui::Ui, selected: &mut BTreeSet<String>, value: &str, label: &str) {
    ui.push_id(value, |ui| {
        let mut checked = selected.contains(value);
        if design::checkbox(ui, &mut checked, label).changed() {
            if checked {
                selected.insert(value.to_owned());
            } else {
                selected.remove(value);
            }
        }
    });
}
