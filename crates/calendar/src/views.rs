//! Month, agenda, and hourly calendar layouts.
use super::*;
impl CalendarTab {
    pub(super) fn month(
        &mut self,
        ui: &mut egui::Ui,
        events: &[CalendarEvent],
    ) -> Option<CalendarEvent> {
        let first = self.date.with_day(1).unwrap();
        let start = first - Duration::days(first.weekday().num_days_from_monday() as i64);
        let width = ui.available_width() / 7.;
        let next = first.checked_add_months(chrono::Months::new(1)).unwrap();
        let cells = first.weekday().num_days_from_monday() as i64 + (next - first).num_days();
        let rows = (cells + 6) / 7;
        let height = ((ui.available_height() - 50.) / rows as f32).max(120.);
        let mut clicked = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            for name in ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"] {
                ui.add_sized(
                    [width, 24.],
                    egui::Label::new(RichText::new(name).size(12.).color(colors(ui).muted)),
                );
            }
        });
        egui::ScrollArea::vertical()
            .id_salt(("calendar-month", self.date.year(), self.date.month()))
            .max_height(ui.available_height())
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(width * 7., height * rows as f32),
                    egui::Sense::hover(),
                );
                for index in 0..rows * 7 {
                    let d = start + Duration::days(index);
                    let cell = Rect::from_min_size(
                        rect.min
                            + Vec2::new((index % 7) as f32 * width, (index / 7) as f32 * height),
                        Vec2::new(width, height),
                    );
                    ui.painter().rect_stroke(
                        cell,
                        0,
                        Stroke::new(1., colors(ui).line),
                        egui::StrokeKind::Inside,
                    );
                    let today = d == Local::now().date_naive();
                    let header =
                        Rect::from_min_size(cell.min + Vec2::new(6., 4.), Vec2::new(28., 24.));
                    if today {
                        ui.painter().circle_filled(
                            Pos2::new(header.left() + 12., header.center().y),
                            12.,
                            colors(ui).accent,
                        );
                    }
                    ui.painter().text(
                        Pos2::new(header.left() + 12., header.center().y),
                        egui::Align2::CENTER_CENTER,
                        d.day().to_string(),
                        egui::FontId::proportional(14.),
                        if today {
                            egui::Color32::WHITE
                        } else if d.month() == self.date.month() {
                            colors(ui).ink
                        } else {
                            colors(ui).muted
                        },
                    );
                    let day_events: Vec<_> = events.iter().filter(|e| on_day(e, d)).collect();
                    let capacity = ((height - 32.) / 24.).floor().max(1.) as usize;
                    for (i, e) in day_events.iter().take(capacity).enumerate() {
                        let r = Rect::from_min_size(
                            cell.min + Vec2::new(5., 32. + i as f32 * 24.),
                            Vec2::new(width - 10., 22.),
                        );
                        if event_chip(ui, r, e).clicked() {
                            clicked = Some((*e).clone());
                        }
                    }
                    if day_events.len() > capacity {
                        let r = Rect::from_min_size(
                            cell.right_top() + Vec2::new(-42., 4.),
                            Vec2::new(36., 24.),
                        );
                        let more = ui.interact(
                            r,
                            ui.make_persistent_id(("calendar-more", d)),
                            egui::Sense::click(),
                        );
                        if more.hovered() {
                            ui.painter().rect_filled(r, 4, colors(ui).hover);
                        }
                        ui.painter().text(
                            r.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("+{}", day_events.len() - capacity),
                            egui::FontId::proportional(13.),
                            colors(ui).muted,
                        );
                        if more.on_hover_text("Show all events").clicked() {
                            self.date = d;
                            self.view = View::Agenda;
                        }
                    }
                    if ui
                        .interact(
                            header,
                            ui.make_persistent_id(("calendar-day", d)),
                            egui::Sense::click(),
                        )
                        .on_hover_text("Create event")
                        .clicked()
                    {
                        self.new_event(d);
                    }
                }
            });
        clicked
    }
    pub(super) fn agenda(
        &mut self,
        ui: &mut egui::Ui,
        events: &[CalendarEvent],
    ) -> Option<CalendarEvent> {
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt(("calendar-agenda", self.date))
            .max_height(ui.available_height())
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                let mut count = 0;
                for offset in 0..31 {
                    let d = self.date + Duration::days(offset);
                    let day: Vec<_> = events.iter().filter(|e| on_day(e, d)).collect();
                    if day.is_empty() {
                        continue;
                    }
                    count += day.len();
                    ui.add_space(12.);
                    ui.label(
                        RichText::new(d.format("%A, %B %-d").to_string())
                            .size(17.)
                            .strong(),
                    );
                    ui.add_space(8.);
                    for e in day {
                        let label = format!(
                            "{}   {}{}",
                            if e.all_day {
                                "All Day".into()
                            } else {
                                e.start.with_timezone(&Local).format("%H:%M").to_string()
                            },
                            e.title,
                            if e.completed { "  ✓" } else { "" }
                        );
                        if ui
                            .add_sized([ui.available_width(), 42.], design::Button::new(label))
                            .clicked()
                        {
                            clicked = Some(e.clone());
                        }
                    }
                    ui.separator();
                }
                if count == 0 {
                    design::hint(ui, "No events in the next 31 days.");
                    if design::button(ui, "Create Event").clicked() {
                        self.new_event(self.date);
                    }
                }
            });
        clicked
    }
    pub(super) fn timeline(
        &mut self,
        ui: &mut egui::Ui,
        events: &[CalendarEvent],
    ) -> Option<CalendarEvent> {
        let days = if self.view == View::Day { 1 } else { 7 };
        let start = if days == 1 {
            self.date
        } else {
            self.date - Duration::days(self.date.weekday().num_days_from_monday() as i64)
        };
        let width = (ui.available_width() - 52.) / days as f32;
        let mut clicked = None;
        ui.horizontal(|ui| {
            ui.add_space(52.);
            ui.spacing_mut().item_spacing.x = 0.;
            for i in 0..days {
                let d = start + Duration::days(i as i64);
                ui.add_sized(
                    [width, 26.],
                    egui::Label::new(RichText::new(d.format("%a %-d").to_string()).strong()),
                );
            }
        });
        let all_rows = (0..days)
            .map(|i| {
                let d = start + Duration::days(i as i64);
                events.iter().filter(|e| e.all_day && on_day(e, d)).count()
            })
            .max()
            .unwrap_or(0);
        let all_height = (all_rows as f32 * 24.).max(26.);
        egui::ScrollArea::vertical()
            .id_salt(("calendar-all-day", days, start))
            .max_height(72_f32.min(ui.available_height() * 0.25).max(26.))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), all_height),
                    egui::Sense::hover(),
                );
                for i in 0..days {
                    let d = start + Duration::days(i as i64);
                    for (j, e) in events
                        .iter()
                        .filter(|e| e.all_day && on_day(e, d))
                        .enumerate()
                    {
                        let r = Rect::from_min_size(
                            rect.min + Vec2::new(52. + i as f32 * width, j as f32 * 24.),
                            Vec2::new(width - 4., 22.),
                        );
                        if event_chip(ui, r, e).clicked() {
                            clicked = Some(e.clone());
                        }
                    }
                }
            });
        let mut scroll = egui::ScrollArea::vertical().id_salt("calendar-hours");
        if self.timeline_position != Some((self.view, self.date)) {
            scroll = scroll.vertical_scroll_offset(420.);
            self.timeline_position = Some((self.view, self.date));
        }
        scroll
            .max_height(ui.available_height())
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), 1440.),
                    egui::Sense::hover(),
                );
                for hour in 0..24 {
                    let y = rect.top() + hour as f32 * 60.;
                    ui.painter().text(
                        Pos2::new(rect.left(), y + 8.),
                        egui::Align2::LEFT_TOP,
                        format!("{hour:02}:00"),
                        egui::FontId::proportional(12.),
                        colors(ui).muted,
                    );
                    ui.painter().line_segment(
                        [Pos2::new(rect.left() + 52., y), Pos2::new(rect.right(), y)],
                        Stroke::new(1., colors(ui).line),
                    );
                }
                for i in 0..days {
                    let d = start + Duration::days(i as i64);
                    let x = rect.left() + 52. + i as f32 * width;
                    ui.painter().line_segment(
                        [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                        Stroke::new(1., colors(ui).line),
                    );
                    for hour in 0..24 {
                        let r = Rect::from_min_size(
                            Pos2::new(x, rect.top() + hour as f32 * 60.),
                            Vec2::new(width, 60.),
                        );
                        if ui
                            .interact(
                                r,
                                ui.make_persistent_id(("calendar-slot", d, hour)),
                                egui::Sense::click(),
                            )
                            .clicked()
                        {
                            self.editor = Some(editor::Editor::at(d, hour));
                        }
                    }
                    let day: Vec<_> = events
                        .iter()
                        .filter(|e| !e.all_day && on_day(e, d))
                        .collect();
                    for (e, lane, lanes) in lanes(&day) {
                        let s = e.start.with_timezone(&Local);
                        let en = e.end.with_timezone(&Local);
                        let minute = if s.date_naive() < d {
                            0.
                        } else {
                            (s.hour() * 60 + s.minute()) as f32
                        };
                        let end = if en.date_naive() > d {
                            1440.
                        } else {
                            (en.hour() * 60 + en.minute()) as f32
                        };
                        let lane_width = (width - 5.) / lanes as f32;
                        let r = Rect::from_min_size(
                            Pos2::new(x + 2. + lane as f32 * lane_width, rect.top() + minute),
                            Vec2::new(lane_width - 2., (end - minute).max(22.)),
                        );
                        if event_chip(ui, r, e).clicked() {
                            clicked = Some(e.clone());
                        }
                    }
                    if d == Local::now().date_naive() {
                        let now = Local::now();
                        let y = rect.top() + (now.hour() * 60 + now.minute()) as f32;
                        ui.painter().line_segment(
                            [Pos2::new(x, y), Pos2::new(x + width, y)],
                            Stroke::new(2., colors(ui).red),
                        );
                    }
                }
            });
        clicked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_and_touchpad_scroll_every_calendar_view_and_all_day_overflow() {
        for view in [View::Month, View::Week, View::Day, View::Agenda] {
            for unit in [egui::MouseWheelUnit::Line, egui::MouseWheelUnit::Point] {
                let ctx = egui::Context::default();
                let mut tab = CalendarTab {
                    view,
                    ..Default::default()
                };
                let mut events = Vec::new();
                for day in 0..31 {
                    let start = midnight(tab.date + Duration::days(day)).unwrap();
                    events.push(CalendarEvent {
                        title: format!("Meeting {day}"),
                        start,
                        end: start + Duration::hours(1),
                        ..Default::default()
                    });
                }
                // More than three all-day entries must remain accessible.
                for i in 0..8 {
                    let start = midnight(tab.date).unwrap();
                    events.push(CalendarEvent {
                        title: format!("All day {i}"),
                        start,
                        end: start + Duration::days(1),
                        all_day: true,
                        ..Default::default()
                    });
                }
                let mut scroll_id = egui::Id::NULL;
                let mut all_day_id = egui::Id::NULL;
                let mut run = |input_events: Vec<egui::Event>| {
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(720., 320.),
                            )),
                            events: input_events,
                            ..Default::default()
                        },
                        |ui| {
                            scroll_id = match view {
                                View::Month => ui.make_persistent_id(egui::IdSalt::new((
                                    "calendar-month",
                                    tab.date.year(),
                                    tab.date.month(),
                                ))),
                                View::Agenda => ui.make_persistent_id(egui::IdSalt::new((
                                    "calendar-agenda",
                                    tab.date,
                                ))),
                                _ => ui.make_persistent_id(egui::IdSalt::new("calendar-hours")),
                            };
                            let days = if view == View::Day { 1 } else { 7 };
                            let start = if days == 1 {
                                tab.date
                            } else {
                                tab.date
                                    - Duration::days(
                                        tab.date.weekday().num_days_from_monday() as i64
                                    )
                            };
                            all_day_id = ui.make_persistent_id(egui::IdSalt::new((
                                "calendar-all-day",
                                days,
                                start,
                            )));
                            match view {
                                View::Month => tab.month(ui, &events),
                                View::Agenda => tab.agenda(ui, &events),
                                _ => tab.timeline(ui, &events),
                            };
                        },
                    );
                    output.textures_delta.clear();
                    (
                        egui::scroll_area::State::load(&ctx, scroll_id)
                            .unwrap()
                            .offset
                            .y,
                        egui::scroll_area::State::load(&ctx, all_day_id).map(|s| s.offset.y),
                    )
                };
                let mut before = 0.;
                for _ in 0..3 {
                    before = run(vec![]).0;
                }
                run(vec![
                    egui::Event::PointerMoved(Pos2::new(350., 240.)),
                    egui::Event::MouseWheel {
                        unit,
                        phase: egui::TouchPhase::Move,
                        delta: Vec2::new(0., -100.),
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
                let mut after = 0.;
                for _ in 0..30 {
                    after = run(vec![]).0;
                }
                assert!(
                    after > before + 1.,
                    "calendar view did not scroll: {before} -> {after}"
                );
                if matches!(view, View::Week | View::Day) {
                    run(vec![
                        egui::Event::PointerMoved(Pos2::new(350., 45.)),
                        egui::Event::MouseWheel {
                            unit,
                            phase: egui::TouchPhase::Move,
                            delta: Vec2::new(0., -100.),
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]);
                    let mut all_day_offset = 0.;
                    for _ in 0..30 {
                        all_day_offset = run(vec![]).1.unwrap();
                    }
                    assert!(all_day_offset > 1., "all-day events did not scroll");
                }
            }
        }
    }
}
