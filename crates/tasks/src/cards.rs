use super::*;
use std::collections::BTreeMap;

pub(super) struct TaskDrag(pub String);

struct PendingClick {
    task: String,
    position: egui::Pos2,
    released_at: f64,
    immediate: bool,
}
struct Press {
    task: String,
    origin: egui::Pos2,
    started_at: f64,
    moved: bool,
    second_click: bool,
}
struct CardHit {
    rect: egui::Rect,
    layer: egui::LayerId,
}
#[derive(Default)]
pub(super) struct CardInteraction {
    pending: Option<PendingClick>,
    armed: Option<String>,
    press: Option<Press>,
    dragging: Option<String>,
    drag_released: bool,
    hit_rects: BTreeMap<String, CardHit>,
    processed_frame: Option<u64>,
}
impl CardInteraction {
    pub fn clear(&mut self) {
        self.pending = None;
        self.armed = None;
        self.press = None;
        self.dragging = None;
        self.drag_released = false;
    }
    pub fn begin_frame(&mut self, ui: &egui::Ui, visible: &[Task]) {
        let frame = ui.ctx().cumulative_frame_nr();
        if ui.is_sizing_pass() || self.processed_frame == Some(frame) {
            return;
        }
        self.processed_frame = Some(frame);
        let visible_ids: BTreeSet<_> = visible.iter().map(|task| task.id.as_str()).collect();
        self.hit_rects
            .retain(|id, _| visible_ids.contains(id.as_str()));
        let (events, now, escape) =
            ui.input(|i| (i.events.clone(), i.time, i.key_down(egui::Key::Escape)));
        if escape
            || events
                .iter()
                .any(|event| matches!(event, egui::Event::WindowFocused(false)))
        {
            self.clear();
            egui::DragAndDrop::clear_payload(ui.ctx());
            return;
        }
        let (delay, distance, click_duration) = ui.ctx().options(|o| {
            (
                o.input_options.max_double_click_delay,
                o.input_options.max_click_dist,
                o.input_options.max_click_duration,
            )
        });
        // Use event positions and order: a press, release, second press, and
        // movement can all arrive in one frame. Aggregated Response flags lose
        // that order and may hit-test the press at the final pointer position.
        for event in events {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } => {
                    let layer = ui.ctx().layer_id_at(pos);
                    let hit = self
                        .hit_rects
                        .iter()
                        .find(|(_, hit)| hit.rect.contains(pos) && layer == Some(hit.layer))
                        .map(|(id, _)| id.clone());
                    let Some(task) = hit.filter(|_| ui.is_enabled()) else {
                        self.clear();
                        continue;
                    };
                    let second_click = self.pending.as_ref().is_some_and(|click| {
                        click.task == task
                            && now - click.released_at <= delay
                            && click.position.distance(pos) <= distance
                    });
                    self.pending = None;
                    if second_click {
                        self.armed = Some(task.clone());
                    } else if self.armed.as_deref() != Some(&task) {
                        self.armed = None;
                    }
                    self.press = Some(Press {
                        task,
                        origin: pos,
                        started_at: now,
                        moved: false,
                        second_click,
                    });
                }
                egui::Event::PointerMoved(pos) => {
                    if let Some(press) = &mut self.press {
                        press.moved |= pos.distance(press.origin) > distance;
                        // Touchpad tap-and-drag may be synthesized as one held
                        // primary button, with no intermediate click release.
                        // Movement past the click threshold is enough to drag;
                        // releasing without movement still opens the task.
                        if press.moved {
                            self.armed = Some(press.task.clone());
                            self.dragging = Some(press.task.clone());
                        }
                    }
                }
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                } => {
                    if let Some(press) = self.press.take() {
                        if self.dragging.is_some() {
                            self.drag_released = true;
                        } else if !press.moved
                            && pos.distance(press.origin) <= distance
                            && now - press.started_at <= click_duration
                        {
                            // Releasing the second click opens details. Holding
                            // and moving it is handled as a drag above.
                            self.pending = Some(PendingClick {
                                task: press.task,
                                position: pos,
                                released_at: now,
                                immediate: press.second_click,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(task) = &self.dragging {
            // egui clears its payload on any release, even if another primary
            // press followed it in the same frame. Keep the active gesture alive.
            egui::DragAndDrop::set_payload(ui.ctx(), TaskDrag(task.clone()));
            if ui.input(|i| i.pointer.primary_down()) {
                ui.ctx().set_dragged_id(egui::Id::new(task));
            }
        }
    }
    fn register(&mut self, ui: &egui::Ui, response: &egui::Response, task: &Task) {
        if response.enabled() {
            self.hit_rects.insert(
                task.id.clone(),
                CardHit {
                    rect: response.interact_rect,
                    layer: response.layer_id,
                },
            );
        }
        // Retain keyboard/accessibility activation without interpreting pointer
        // clicks again after they have been handled in event order above.
        if response.clicked() && !ui.input(|i| i.pointer.any_click()) {
            self.pending = Some(PendingClick {
                task: task.id.clone(),
                position: response.rect.center(),
                released_at: ui.input(|i| i.time),
                immediate: true,
            });
        }
    }
    pub fn finish_frame(&mut self, ui: &egui::Ui, workspace: &Workspace) -> Option<Task> {
        let (down, now) = ui.input(|i| (i.pointer.primary_down(), i.time));
        if self.drag_released && !down {
            self.clear();
        }
        if let Some(click) = &self.pending {
            let delay = ui.ctx().options(|o| o.input_options.max_double_click_delay);
            let remaining = click.released_at + delay - now;
            if (click.immediate || remaining <= 0.0) && !down {
                let task = workspace.tasks.iter().find(|t| t.id == click.task).cloned();
                self.clear();
                return task;
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(remaining.max(0.016)));
        }
        None
    }
}

impl TasksUi<'_> {
    pub(super) fn task_card(
        &mut self,
        ui: &mut egui::Ui,
        task: &Task,
        status: &TaskStatus,
        width: f32,
    ) -> egui::Response {
        let id = egui::Id::new(&task.id);
        let dragging = egui::DragAndDrop::payload::<TaskDrag>(ui.ctx())
            .is_some_and(|payload| payload.0 == task.id);
        let layer = egui::LayerId::new(egui::Order::Tooltip, id);
        let builder = if dragging {
            egui::UiBuilder::new().layer_id(layer)
        } else {
            egui::UiBuilder::new()
        };
        let armed = self.state.cards.armed.as_deref() == Some(&task.id);
        let focused = ui.memory(|m| m.has_focus(id));
        let hovered = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
        let hover = design::animate_toggle(ui.ctx(), id.with("card-hover"), hovered, 0.14);
        let p = colors(ui);
        let border = if armed || focused {
            p.accent
        } else {
            p.line.lerp_to_gamma(p.hover_line, hover)
        };
        let response = ui
            .scope_builder(builder, |ui| {
                panel(ui)
                    .fill(
                        p.card
                            .lerp_to_gamma(p.accent, if armed { 0.07 } else { 0.025 * hover }),
                    )
                    .stroke(Stroke::new(1.0, border))
                    .inner_margin(14)
                    .corner_radius(6)
                    .show(ui, |ui| {
                        ui.set_width((width - 28.0).max(100.0));
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 10.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(&task.title)
                                    .size(16.0)
                                    .strong()
                                    .color(if status.completed { p.muted } else { p.ink }),
                            )
                            .wrap()
                            .halign(egui::Align::LEFT)
                            .selectable(false),
                        );
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            design::badge_with_size(
                                ui,
                                &task.priority,
                                priority_color(ui, &task.priority),
                                13.0,
                            );
                            if let Some(category) = self
                                .workspace
                                .task_categories
                                .iter()
                                .find(|c| c.id == task.category_id)
                            {
                                design::badge_with_size(ui, &category.name, p.violet, 13.0);
                            }
                            if !task.attachments.is_empty() {
                                ui.label(
                                    RichText::new(format!(
                                        "{} {}",
                                        task.attachments.len(),
                                        if task.attachments.len() == 1 {
                                            "file"
                                        } else {
                                            "files"
                                        }
                                    ))
                                    .size(12.0)
                                    .color(p.muted),
                                );
                            }
                        });
                        if !task.due_date.is_empty() {
                            ui.add_space(2.0);
                            ui.horizontal(|ui| {
                                let color = if task.due_date < today() && !status.completed {
                                    p.red
                                } else {
                                    p.muted
                                };
                                design::icon(ui, Icon::Calendar, color, 16.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&task.due_date).size(13.0).color(color),
                                    )
                                    .selectable(false),
                                );
                            });
                        }
                    })
                    .response
            })
            .inner;
        let response = ui.interact(response.rect, id, egui::Sense::click_and_drag());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &task.title)
        });
        self.state.cards.register(ui, &response, task);
        if dragging {
            ui.painter()
                .rect_filled(response.rect, 6, p.accent.gamma_multiply(0.06));
            if let Some(pointer) = ui.ctx().pointer_interact_pos() {
                ui.ctx().transform_layer_shapes(
                    layer,
                    egui::emath::TSTransform::from_translation(pointer - response.rect.center()),
                );
            }
        }
        response.on_hover_cursor(egui::CursorIcon::Default)
    }
}
