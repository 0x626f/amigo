//! Named visual folders. Only their title strip and handles capture pointer input.
use super::*;
use egui::{Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
pub(super) struct Draft {
    background: Background,
    focus: bool,
    error: String,
}
fn world_rect(background: &Background) -> Rect {
    Rect::from_min_size(
        egui::pos2(background.position[0], background.position[1]),
        egui::vec2(background.size[0], background.size[1]),
    )
}
pub(super) fn paint(
    painter: &egui::Painter,
    background: &Background,
    origin: Pos2,
    zoom: f32,
    selected: bool,
    dark: bool,
) {
    let rect = world_rect(background);
    let rect = Rect::from_min_max(
        origin + rect.min.to_vec2() * zoom,
        origin + rect.max.to_vec2() * zoom,
    );
    if !rect.intersects(painter.clip_rect()) {
        return;
    }
    paint_frame(painter, rect, selected, dark);
    let title = Rect::from_min_max(
        rect.min + egui::vec2(12., 6.) * zoom,
        egui::pos2(rect.right() - 10. * zoom, rect.top() + 34. * zoom),
    );
    let mut job = egui::text::LayoutJob::simple(
        background.title.clone(),
        FontId::proportional((16. * zoom).max(7.)),
        if dark {
            Color32::from_gray(220)
        } else {
            Color32::from_rgb(55, 73, 103)
        },
        title.width().max(1.),
    );
    job.wrap.max_rows = 1;
    let text = painter.layout_job(job);
    painter.with_clip_rect(title).galley(
        title.min,
        text,
        if dark {
            Color32::from_gray(220)
        } else {
            Color32::from_rgb(55, 73, 103)
        },
    );
}
fn paint_frame(painter: &egui::Painter, rect: Rect, selected: bool, dark: bool) {
    let fill = if dark {
        Color32::from_rgb(37, 47, 65)
    } else {
        Color32::from_rgb(231, 237, 247)
    };
    let border = if selected {
        Color32::from_rgb(70, 135, 230)
    } else if dark {
        Color32::from_rgb(80, 102, 136)
    } else {
        Color32::from_rgb(169, 186, 211)
    };
    painter.rect_filled(rect, 6, fill);
    painter.rect_stroke(
        rect,
        6,
        Stroke::new(if selected { 2. } else { 1. }, border),
        StrokeKind::Inside,
    );
}
impl SquidyTab {
    pub(super) fn begin_background(&mut self) {
        if self.background_drawing {
            self.background_drawing = false;
            self.background_start = None;
            return;
        }
        self.picking = None;
        self.drag = None;
        self.resize = None;
        self.selected_background = None;
        let bounds = capture::card_rects(self.board.as_ref().unwrap());
        if let Some(rect) = bounds
            .iter()
            .filter(|(id, _)| self.selected.contains(*id))
            .map(|(_, r)| *r)
            .reduce(Rect::union)
        {
            let rect =
                Rect::from_min_max(rect.min - egui::vec2(24., 48.), rect.max + Vec2::splat(24.));
            self.background_draft = Some(Draft {
                background: Background::new(
                    "",
                    [rect.min.x, rect.min.y],
                    [rect.width(), rect.height()],
                ),
                focus: true,
                error: String::new(),
            });
        } else {
            self.background_drawing = true;
            self.background_start = None;
        }
    }
    pub(super) fn backgrounds_canvas(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        origin: Pos2,
        zoom: f32,
    ) {
        let mut update = None;
        let mut edit = None;
        let mut delete = None;
        let drawing = self.background_drawing;
        for background in &self.board.as_ref().unwrap().backgrounds {
            let world = world_rect(background);
            let screen = Rect::from_min_max(
                origin + world.min.to_vec2() * zoom,
                origin + world.max.to_vec2() * zoom,
            );
            if !screen.intersects(rect) {
                continue;
            }
            let selected = self.selected_background.as_deref() == Some(&background.id);
            paint_frame(
                &ui.painter().with_clip_rect(rect),
                screen,
                selected,
                ui.visuals().dark_mode,
            );
            let title = Rect::from_min_size(screen.min, egui::vec2(screen.width(), 36. * zoom))
                .intersect(rect);
            let hit = ui
                .interact(
                    title,
                    egui::Id::new(("squidy-background", &background.id)),
                    if drawing {
                        Sense::hover()
                    } else {
                        Sense::click_and_drag()
                    },
                )
                .on_hover_text("Drag title to move; double-click to rename");
            let ink = if ui.visuals().dark_mode {
                Color32::from_gray(220)
            } else {
                Color32::from_rgb(55, 73, 103)
            };
            let name = ui.painter().layout_no_wrap(
                background.title.clone(),
                FontId::proportional((16. * zoom).max(7.)),
                ink,
            );
            let caption = Rect::from_min_size(
                screen.min + Vec2::new(12., 6.) * zoom,
                Vec2::new((screen.width() - 22. * zoom).max(1.), name.size().y),
            );
            let clip = ui.clip_rect();
            ui.set_clip_rect(clip.intersect(rect));
            design::paint_canvas_name(ui, &hit, caption, name, ink, egui::Align::Min);
            ui.set_clip_rect(clip);
            if hit.clicked() {
                self.selected_background = Some(background.id.clone());
                self.selected.clear();
            }
            if hit.double_clicked() {
                edit = Some(background.clone());
            }
            if hit.drag_started() {
                let press = ui.input(|i| i.pointer.press_origin()).unwrap_or(screen.min);
                self.selected_background = Some(background.id.clone());
                self.selected.clear();
                self.background_drag = Some((background.id.clone(), 8, world, press));
            }
            let mut active = hit.dragged();
            if selected && !drawing {
                for (corner, point) in [
                    screen.left_top(),
                    screen.right_top(),
                    screen.left_bottom(),
                    screen.right_bottom(),
                ]
                .into_iter()
                .enumerate()
                {
                    let handle = ui
                        .interact(
                            Rect::from_center_size(point, Vec2::splat(16.)).intersect(rect),
                            egui::Id::new(("squidy-background-resize", &background.id, corner)),
                            Sense::drag(),
                        )
                        .on_hover_cursor(if corner == 0 || corner == 3 {
                            egui::CursorIcon::ResizeNwSe
                        } else {
                            egui::CursorIcon::ResizeNeSw
                        });
                    ui.painter().with_clip_rect(rect).rect_filled(
                        Rect::from_center_size(point, Vec2::splat(7.)),
                        1,
                        design::colors(ui).accent,
                    );
                    if handle.drag_started() {
                        self.background_drag = Some((
                            background.id.clone(),
                            corner,
                            world,
                            ui.input(|i| i.pointer.press_origin()).unwrap_or(point),
                        ));
                    }
                    active |= handle.dragged();
                }
            }
            if active
                && let Some((id, corner, original, press)) = &self.background_drag
                && id == &background.id
                && let Some(pointer) = ui.input(|i| i.pointer.interact_pos())
            {
                let delta = (pointer - *press) / zoom;
                let mut area = *original;
                if *corner == 8 {
                    area = area.translate(delta);
                } else {
                    if corner % 2 == 0 {
                        area.min.x =
                            (area.min.x + delta.x).clamp(area.max.x - 100_000., area.max.x - 120.);
                    } else {
                        area.max.x =
                            (area.max.x + delta.x).clamp(area.min.x + 120., area.min.x + 100_000.);
                    }
                    if corner / 2 == 0 {
                        area.min.y =
                            (area.min.y + delta.y).clamp(area.max.y - 100_000., area.max.y - 80.);
                    } else {
                        area.max.y =
                            (area.max.y + delta.y).clamp(area.min.y + 80., area.min.y + 100_000.);
                    }
                }
                area.min.x = area.min.x.clamp(-10_000_000., 10_000_000.);
                area.min.y = area.min.y.clamp(-10_000_000., 10_000_000.);
                update = Some((id.clone(), area));
            }
            if !drawing {
                design::context_menu(&hit).show(|ui| {
                    if ui.button("Rename File Border").clicked() {
                        edit = Some(background.clone());
                        ui.close();
                    }
                    if ui.button("Delete File Border").clicked() {
                        delete = Some(background.id.clone());
                        ui.close();
                    }
                });
            }
        }
        if ui.input(|i| i.pointer.any_released()) {
            self.background_drag = None;
        }
        if let Some((id, area)) = update
            && let Some(b) = self
                .board
                .as_mut()
                .unwrap()
                .backgrounds
                .iter_mut()
                .find(|b| b.id == id)
        {
            b.position = [area.min.x, area.min.y];
            b.size = [
                area.width().clamp(120., 100_000.),
                area.height().clamp(80., 100_000.),
            ];
            self.changed();
        }
        if let Some(background) = edit {
            self.background_draft = Some(Draft {
                background,
                focus: true,
                error: String::new(),
            });
        }
        if let Some(id) = delete {
            self.confirm_delete(Delete::Background(id));
        }
    }
    pub(super) fn draw_background_frame(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        origin: Pos2,
        zoom: f32,
    ) {
        if !self.background_drawing {
            return;
        }
        response
            .clone()
            .on_hover_cursor(egui::CursorIcon::Crosshair);
        ui.painter().text(
            self.viewport.left_top() + Vec2::splat(12.),
            egui::Align2::LEFT_TOP,
            "Drag a file border around cards · Esc to cancel",
            FontId::proportional(14.),
            design::colors(ui).ink,
        );
        if response.drag_started() {
            self.background_start = ui.input(|i| i.pointer.press_origin());
        }
        if let Some(start) = self.background_start
            && let Some(pointer) = ui.input(|i| i.pointer.interact_pos())
        {
            let rect = Rect::from_two_pos(start, pointer).intersect(self.viewport);
            ui.painter().with_clip_rect(self.viewport).rect_stroke(
                rect,
                4,
                Stroke::new(2., design::colors(ui).accent),
                StrokeKind::Inside,
            );
            if response.drag_stopped() {
                if rect.width() >= 8. && rect.height() >= 8. {
                    let min = (rect.min - origin) / zoom;
                    let size = rect.size() / zoom;
                    self.background_draft = Some(Draft {
                        background: Background::new(
                            "",
                            [min.x, min.y],
                            [size.x.max(120.), size.y.max(80.)],
                        ),
                        focus: true,
                        error: String::new(),
                    });
                }
                self.background_drawing = false;
                self.background_start = None;
            }
        }
    }
    pub(super) fn background_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.background_draft.take() else {
            return;
        };
        let mut cancel = false;
        let mut save = false;
        let keys = design::ActionKeys::capture(ctx);
        let modal = design::modal(ctx, egui::Id::new("squidy-background-editor")).show(ctx, |ui| {
            ui.set_width(design::dialog_width(ctx, 440.));
            cancel = design::dialog_heading(ui, "File Border").clicked();
            ui.label("Title");
            if draft.focus && !ui.is_sizing_pass() {
                ui.memory_mut(|m| m.request_focus(egui::Id::new("squidy-background-title")));
            }
            let input = ui.add(
                design::input(&mut draft.background.title)
                    .id(egui::Id::new("squidy-background-title"))
                    .char_limit(200)
                    .desired_width(f32::INFINITY),
            );
            if draft.focus && !ui.is_sizing_pass() {
                input.request_focus();
                draft.focus = false;
            }
            if !draft.error.is_empty() {
                ui.colored_label(design::colors(ui).red, &draft.error);
            }
            design::dialog_footer(ui, |ui| {
                ui.horizontal(|ui| {
                    cancel |= design::button(ui, "Cancel").clicked();
                    save = design::dialog_primary(ui, "Save File Border").clicked();
                });
            });
        });
        let shortcut = keys.dialog(&modal, !draft.background.title.trim().is_empty());
        if cancel || shortcut.cancel {
            return;
        }
        if save || shortcut.confirm {
            draft.background.title = draft.background.title.trim().into();
            match draft.background.validate() {
                Err(error) => draft.error = error,
                Ok(()) => {
                    let board = self.board.as_mut().unwrap();
                    self.selected_background = Some(draft.background.id.clone());
                    if let Some(existing) = board
                        .backgrounds
                        .iter_mut()
                        .find(|b| b.id == draft.background.id)
                    {
                        *existing = draft.background;
                    } else {
                        board.backgrounds.push(draft.background);
                    }
                    self.changed();
                    return;
                }
            }
        }
        self.background_draft = Some(draft);
    }
}
