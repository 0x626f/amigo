use amigo_core::model::*;
use amigo_ui::design::{self, colors};
use egui::{self, RichText};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct BoardEditor {
    statuses: Vec<TaskStatus>,
    categories: Vec<StoredValue>,
    reassignments: BTreeMap<String, String>,
    categories_tab: bool,
    removing_status: Option<String>,
    removing_category: Option<String>,
    replacement: String,
    error: String,
    name_edit: Option<design::NameEdit>,
}
impl BoardEditor {
    pub fn new(workspace: &Workspace) -> Self {
        Self {
            statuses: workspace.task_statuses.clone(),
            categories: workspace.task_categories.clone(),
            reassignments: BTreeMap::new(),
            categories_tab: false,
            removing_status: None,
            removing_category: None,
            replacement: String::new(),
            error: String::new(),
            name_edit: None,
        }
    }
    fn affected_tasks(&self, workspace: &Workspace, status: &str) -> usize {
        workspace
            .tasks
            .iter()
            .filter(|task| {
                let mut current = task.status.as_str();
                for _ in 0..=self.reassignments.len() {
                    if current == status {
                        return true;
                    }
                    match self.reassignments.get(current) {
                        Some(next) => current = next,
                        None => return false,
                    }
                }
                false
            })
            .count()
    }
}
pub fn status_color(ui: &egui::Ui, color: StatusColor) -> egui::Color32 {
    let p = colors(ui);
    match color {
        StatusColor::Gray => p.muted,
        StatusColor::Blue => p.accent,
        StatusColor::Amber => p.amber,
        StatusColor::Green => p.green,
        StatusColor::Violet => p.violet,
        StatusColor::Rose => p.red,
    }
}
#[derive(Default)]
pub struct SettingsOutcome {
    pub saved: bool,
    pub rename: Option<String>,
    pub data_action: Option<design::DataAction>,
}
#[derive(Clone)]
struct StatusDrag(String);

pub fn show(
    state: &mut Option<BoardEditor>,
    workspace: &mut Workspace,
    board_name: &str,
    ctx: &egui::Context,
) -> SettingsOutcome {
    let Some(mut editor) = state.take() else {
        return SettingsOutcome::default();
    };
    let keys = design::ActionKeys::capture(ctx);
    let mut save = false;
    let mut cancel = false;
    let mut outcome = SettingsOutcome::default();
    let modal = design::modal(ctx, egui::Id::new("board-settings")).show(ctx, |ui| {
        ui.set_width(design::dialog_width(ctx, 790.0));
        let heading = design::named_dialog_heading(
            ui,
            board_name,
            "Board Settings",
            &mut editor.name_edit,
            "board-settings",
            "Board",
        );
        cancel = heading.close.clicked();
        outcome.data_action = heading.data_action;
        design::segmented(ui, &mut editor.categories_tab, "Statuses", "Categories");
        ui.add_space(8.0);
        egui::ScrollArea::both()
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .id_salt("board-settings-content")
            .max_height((design::dialog_body_height(ctx) - 130.0).max(140.0))
            .show(ui, |ui| {
                if editor.categories_tab {
                    categories(&mut editor, workspace, ui);
                } else {
                    statuses(&mut editor, ui);
                }
                removal_confirmation(&mut editor, workspace, ui);
            });
        if editor.categories_tab {
            if design::button(ui, "+ Add Category").clicked() {
                editor.categories.push(StoredValue {
                    id: id(),
                    name: String::new(),
                });
            }
        } else if design::button(ui, "+ Add Status").clicked() {
            editor.statuses.push(TaskStatus {
                id: id(),
                name: String::new(),
                ..TaskStatus::default()
            });
        }
        if !editor.error.is_empty() {
            ui.colored_label(colors(ui).red, &editor.error);
        }
        design::dialog_footer(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                save = ui
                    .add_enabled_ui(
                        editor.removing_status.is_none() && editor.removing_category.is_none(),
                        |ui| design::dialog_primary(ui, "Save Board"),
                    )
                    .inner
                    .clicked();
                cancel |= design::button(ui, "Cancel").clicked();
            });
        });
    });
    let shortcut = keys.dialog(&modal, true);
    if editor.removing_status.is_some() || editor.removing_category.is_some() {
        if shortcut.cancel {
            editor.removing_status = None;
            editor.removing_category = None;
            editor.replacement.clear();
        } else if shortcut.confirm {
            confirm_removal(&mut editor, workspace);
        }
    } else {
        cancel |= shortcut.cancel;
        save |= shortcut.confirm;
    }
    if cancel {
        if egui::DragAndDrop::has_payload_of_type::<StatusDrag>(ctx) {
            egui::DragAndDrop::clear_payload(ctx);
        }
        return outcome;
    }
    if save {
        let rename = match editor
            .name_edit
            .as_ref()
            .map(|edit| amigo_core::library::name(&edit.value))
            .transpose()
        {
            Ok(rename) => rename,
            Err(error) => {
                editor.error = error;
                *state = Some(editor);
                return outcome;
            }
        };
        match workspace.update_task_board(
            editor.statuses.clone(),
            editor.categories.clone(),
            &editor.reassignments,
        ) {
            Ok(()) => {
                outcome.saved = true;
                outcome.rename = rename;
                return outcome;
            }
            Err(error) => editor.error = error,
        }
    }
    if !cancel {
        *state = Some(editor);
    }
    outcome
}
fn statuses(editor: &mut BoardEditor, ui: &mut egui::Ui) {
    let mut movement = None;
    let mut insertion = None;
    egui::Grid::new("status-settings-table")
        .spacing([10.0, 12.0])
        .show(ui, |ui| {
            for title in ["", "Status Name", "Color", "Completed", "Column Order", ""] {
                ui.label(
                    RichText::new(amigo_ui::forms::title_case(title))
                        .size(16.0)
                        .strong(),
                );
            }
            ui.end_row();
            let count = editor.statuses.len();
            for (index, status) in editor.statuses.iter_mut().enumerate() {
                let background = ui.painter().add(egui::Shape::Noop);
                let (handle_rect, _) = ui.allocate_exact_size(
                    egui::vec2(28., design::CONTROL_HEIGHT),
                    egui::Sense::hover(),
                );
                let handle = ui
                    .interact(
                        handle_rect,
                        egui::Id::new(("status-drag", &status.id)),
                        egui::Sense::drag(),
                    )
                    .on_hover_text("Drag to reorder status");
                handle.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        ui.is_enabled(),
                        format!("Reorder {}", status.name),
                    )
                });
                design::paint_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(handle_rect.center(), egui::Vec2::splat(20.)),
                    design::Icon::Grip,
                    colors(ui).muted,
                );
                handle.dnd_set_drag_payload(StatusDrag(status.id.clone()));
                let name = ui.add_sized(
                    [205.0, design::CONTROL_HEIGHT],
                    design::input(&mut status.name)
                        .id(egui::Id::new(("status-name", &status.id)))
                        .desired_width(205.0)
                        .hint_text("Status Name"),
                );
                let color = egui::ComboBox::from_id_salt(("status-color", &status.id))
                    .width(82.0)
                    .selected_text(
                        RichText::new(status.color.name()).color(status_color(ui, status.color)),
                    )
                    .show_ui(ui, |ui| {
                        for color in StatusColor::ALL {
                            ui.selectable_value(&mut status.color, color, color.name());
                        }
                    });
                let completed = design::checkbox(ui, &mut status.completed, "");
                let order = ui.horizontal(|ui| {
                    if ui
                        .add_enabled(index > 0, design::Button::new("↑"))
                        .on_hover_text("Move column left")
                        .clicked()
                    {
                        movement = Some((index, index - 1));
                    }
                    if ui
                        .add_enabled(index + 1 < count, design::Button::new("↓"))
                        .on_hover_text("Move column right")
                        .clicked()
                    {
                        movement = Some((index, index + 1));
                    }
                });
                let delete = ui
                    .add_enabled(count > 1, design::Button::new("Delete…"))
                    .on_hover_text("Keep at least one status");
                if delete.clicked() {
                    editor.removing_status = Some(status.id.clone());
                    editor.removing_category = None;
                    editor.replacement.clear();
                    ui.memory_mut(|m| {
                        if let Some(id) = m.focused() {
                            m.surrender_focus(id);
                        }
                    });
                }
                let row_rect = handle_rect
                    .union(name.rect)
                    .union(color.response.rect)
                    .union(completed.rect)
                    .union(order.response.rect)
                    .union(delete.rect);
                ui.interact(
                    row_rect,
                    egui::Id::new(("status-row", &status.id)),
                    egui::Sense::hover(),
                );
                if let Some(dragged) = egui::DragAndDrop::payload::<StatusDrag>(ui.ctx()) {
                    if dragged.0 == status.id {
                        ui.painter().set(
                            background,
                            egui::Shape::rect_filled(
                                row_rect.expand(4.),
                                8,
                                colors(ui).accent.gamma_multiply(0.08),
                            ),
                        );
                    }
                    if ui.rect_contains_pointer(row_rect) && dragged.0 != status.id {
                        let after = ui
                            .input(|i| i.pointer.interact_pos())
                            .is_some_and(|p| p.y >= row_rect.center().y);
                        let y = if after {
                            row_rect.bottom() + 6.
                        } else {
                            row_rect.top() - 6.
                        };
                        ui.painter().line_segment(
                            [
                                egui::pos2(row_rect.left(), y),
                                egui::pos2(row_rect.right(), y),
                            ],
                            egui::Stroke::new(2., colors(ui).accent),
                        );
                        if ui.input(|i| i.pointer.any_released())
                            && let Some(dragged) =
                                egui::DragAndDrop::take_payload::<StatusDrag>(ui.ctx())
                        {
                            insertion = Some((dragged.0.clone(), index + usize::from(after)));
                        }
                    }
                }
                ui.end_row();
            }
        });
    if let Some((from, to)) = movement {
        editor.statuses.swap(from, to);
    }
    if let Some((id, insertion)) = insertion
        && let Some(from) = editor.statuses.iter().position(|s| s.id == id)
    {
        let status = editor.statuses.remove(from);
        let to = insertion - usize::from(from < insertion);
        editor
            .statuses
            .insert(to.min(editor.statuses.len()), status);
    }
    if let Some(dragged) = egui::DragAndDrop::payload::<StatusDrag>(ui.ctx())
        && let Some(status) = editor.statuses.iter().find(|s| s.id == dragged.0)
        && let Some(pointer) = ui.input(|i| i.pointer.interact_pos())
    {
        egui::Area::new(egui::Id::new("status-drag-preview"))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .fixed_pos(pointer + egui::vec2(16., 16.))
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(colors(ui).card)
                    .corner_radius(8)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.label(&status.name);
                    });
            });
    }
}
fn categories(editor: &mut BoardEditor, workspace: &Workspace, ui: &mut egui::Ui) {
    if editor.categories.is_empty() {
        design::hint(ui, "No categories yet. Add one to organize your tasks.");
    }
    egui::Grid::new("category-settings-table")
        .spacing([14.0, 12.0])
        .show(ui, |ui| {
            for category in &mut editor.categories {
                ui.add_sized(
                    [340.0, design::CONTROL_HEIGHT],
                    design::input(&mut category.name)
                        .id(egui::Id::new(("category-name", &category.id)))
                        .desired_width(340.0)
                        .hint_text("Category Name"),
                );
                design::hint(
                    ui,
                    &format!(
                        "{} tasks",
                        workspace
                            .tasks
                            .iter()
                            .filter(|t| t.category_id == category.id)
                            .count()
                    ),
                );
                if design::button(ui, "Delete…").clicked() {
                    editor.removing_category = Some(category.id.clone());
                    editor.removing_status = None;
                    ui.memory_mut(|m| {
                        if let Some(id) = m.focused() {
                            m.surrender_focus(id);
                        }
                    });
                }
                ui.end_row();
            }
        });
}
fn removal_confirmation(editor: &mut BoardEditor, workspace: &Workspace, ui: &mut egui::Ui) {
    if let Some(id) = editor.removing_status.clone() {
        ui.separator();
        let name = editor
            .statuses
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.as_str())
            .unwrap_or("status");
        let affected = editor.affected_tasks(workspace, &id);
        ui.colored_label(colors(ui).red, format!("Delete \"{name}\"?"));
        if affected > 0 {
            design::hint(
                ui,
                &format!("Move {affected} tasks to another status when you save the board:"),
            );
            egui::ComboBox::from_id_salt("replacement-status")
                .selected_text(
                    editor
                        .statuses
                        .iter()
                        .find(|s| s.id == editor.replacement)
                        .map(|s| s.name.as_str())
                        .unwrap_or("Choose a status"),
                )
                .show_ui(ui, |ui| {
                    for status in editor.statuses.iter().filter(|s| s.id != id) {
                        ui.selectable_value(
                            &mut editor.replacement,
                            status.id.clone(),
                            &status.name,
                        );
                    }
                });
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    affected == 0
                        || editor
                            .statuses
                            .iter()
                            .any(|s| s.id == editor.replacement && s.id != id),
                    design::Button::new("Remove status"),
                )
                .clicked()
            {
                confirm_removal(editor, workspace);
            }
            if design::button(ui, "Keep status").clicked() {
                editor.removing_status = None;
            }
        });
    }
    if let Some(id) = editor.removing_category.clone() {
        ui.separator();
        let name = editor
            .categories
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.as_str())
            .unwrap_or("category");
        ui.colored_label(colors(ui).red, format!("Delete \"{name}\"?"));
        design::hint(
            ui,
            "Its tasks will become Uncategorized when you save. Tasks are kept.",
        );
        ui.horizontal(|ui| {
            if design::button(ui, "Remove category").clicked() {
                confirm_removal(editor, workspace);
            }
            if design::button(ui, "Keep category").clicked() {
                editor.removing_category = None;
            }
        });
    }
}

fn confirm_removal(editor: &mut BoardEditor, workspace: &Workspace) {
    if let Some(id) = editor.removing_status.clone() {
        let affected = editor.affected_tasks(workspace, &id);
        if affected > 0 {
            if !editor
                .statuses
                .iter()
                .any(|s| s.id == editor.replacement && s.id != id)
            {
                return;
            }
            editor
                .reassignments
                .insert(id.clone(), editor.replacement.clone());
        }
        editor.statuses.retain(|s| s.id != id);
        editor.removing_status = None;
    } else if let Some(id) = editor.removing_category.take() {
        editor.categories.retain(|c| c.id != id);
    }
}

#[cfg(test)]
mod keyboard_tests {
    use super::*;

    #[test]
    fn board_keyboard_discards_and_saves_drafts() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut workspace = Workspace::default();
        let original = workspace.clone();
        for key in [egui::Key::Escape, egui::Key::Enter] {
            let mut state = Some(BoardEditor::new(&workspace));
            state.as_mut().unwrap().statuses[0].name = "Updated".into();
            for event in [None, None, Some(key)] {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        events: event
                            .map(|key| egui::Event::Key {
                                key,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: egui::Modifiers::NONE,
                            })
                            .into_iter()
                            .collect(),
                        ..Default::default()
                    },
                    |ui| {
                        show(&mut state, &mut workspace, "Personal", ui.ctx());
                    },
                );
                output.textures_delta.clear();
            }
            assert!(state.is_none());
            assert_eq!(workspace == original, key == egui::Key::Escape);
        }
        assert_eq!(workspace.task_statuses[0].name, "Updated");
    }
}

#[cfg(test)]
mod drag_tests {
    use super::*;
    use egui::{Event, Key, Modifiers, Pos2, Vec2};

    fn frame(
        state: &mut Option<BoardEditor>,
        workspace: &mut Workspace,
        ctx: &egui::Context,
        size: Vec2,
        events: Vec<Event>,
    ) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| {
                show(state, workspace, "Personal", ui.ctx());
            },
        );
        output.textures_delta.clear();
    }
    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }
    fn key(key: Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }
    fn drag(
        state: &mut Option<BoardEditor>,
        workspace: &mut Workspace,
        ctx: &egui::Context,
        size: Vec2,
        source: &str,
        target: &str,
        after: bool,
    ) {
        let source = ctx
            .read_response(egui::Id::new(("status-drag", source)))
            .unwrap()
            .rect
            .center();
        let target = ctx
            .read_response(egui::Id::new(("status-row", target)))
            .unwrap()
            .rect;
        let target = egui::pos2(
            target.center().x,
            if after {
                target.bottom() - 2.
            } else {
                target.top() + 2.
            },
        );
        frame(
            state,
            workspace,
            ctx,
            size,
            vec![Event::PointerMoved(source), button(source, true)],
        );
        frame(
            state,
            workspace,
            ctx,
            size,
            vec![Event::PointerMoved(source + egui::vec2(14., 0.))],
        );
        assert!(egui::DragAndDrop::has_payload_of_type::<StatusDrag>(ctx));
        frame(
            state,
            workspace,
            ctx,
            size,
            vec![Event::PointerMoved(target)],
        );
        frame(state, workspace, ctx, size, vec![button(target, false)]);
        assert!(!egui::DragAndDrop::has_payload_of_type::<StatusDrag>(ctx));
    }
    #[test]
    fn status_drag_moves_in_both_directions_preserves_records_and_only_commits_on_save() {
        for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
            for (source, target, after, confirm) in [
                ("backlog", "done", true, Key::Escape),
                ("done", "backlog", false, Key::Enter),
            ] {
                let ctx = egui::Context::default();
                design::install(&ctx);
                let mut workspace = Workspace::default();
                workspace.tasks.push(Task::new(1, "done"));
                let original = workspace.clone();
                let mut expected = original.task_statuses.clone();
                if after {
                    expected.rotate_left(1);
                } else {
                    expected.rotate_right(1);
                }
                let mut state = Some(BoardEditor::new(&workspace));
                for _ in 0..10 {
                    frame(&mut state, &mut workspace, &ctx, size, vec![]);
                }
                drag(
                    &mut state,
                    &mut workspace,
                    &ctx,
                    size,
                    source,
                    target,
                    after,
                );
                assert_eq!(state.as_ref().unwrap().statuses, expected);
                assert_eq!(
                    workspace, original,
                    "reordering must remain a settings draft"
                );
                frame(&mut state, &mut workspace, &ctx, size, vec![key(confirm)]);
                assert!(state.is_none());
                assert_eq!(
                    workspace.task_statuses,
                    if confirm == Key::Enter {
                        expected
                    } else {
                        original.task_statuses
                    }
                );
                assert_eq!(workspace.tasks, original.tasks);
            }
        }
    }
    #[test]
    fn escape_cancels_an_active_drag_and_dropping_outside_keeps_the_order() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let size = egui::vec2(980., 640.);
        let mut workspace = Workspace::default();
        let mut state = Some(BoardEditor::new(&workspace));
        let original = workspace.task_statuses.clone();
        for _ in 0..10 {
            frame(&mut state, &mut workspace, &ctx, size, vec![]);
        }
        let start = ctx
            .read_response(egui::Id::new(("status-drag", "todo")))
            .unwrap()
            .rect
            .center();
        for cancel in [true, false] {
            frame(
                &mut state,
                &mut workspace,
                &ctx,
                size,
                vec![Event::PointerMoved(start), button(start, true)],
            );
            frame(
                &mut state,
                &mut workspace,
                &ctx,
                size,
                vec![Event::PointerMoved(start + egui::vec2(14., 0.))],
            );
            assert!(egui::DragAndDrop::has_payload_of_type::<StatusDrag>(&ctx));
            if cancel {
                frame(
                    &mut state,
                    &mut workspace,
                    &ctx,
                    size,
                    vec![key(Key::Escape)],
                );
                assert!(
                    state.is_some(),
                    "Escape during drag should keep settings open"
                );
            }
            let outside = egui::pos2(10., 10.);
            frame(
                &mut state,
                &mut workspace,
                &ctx,
                size,
                vec![Event::PointerMoved(outside), button(outside, false)],
            );
            assert!(!egui::DragAndDrop::has_any_payload(&ctx));
            assert_eq!(state.as_ref().unwrap().statuses, original);
        }
    }
}
