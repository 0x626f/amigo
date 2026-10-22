use super::*;
use amigo_ui::forms::{date_field, field_label, text_field};
use chrono::{NaiveTime, TimeZone};
pub(super) struct Editor {
    event: CalendarEvent,
    start_date: String,
    end_date: String,
    start_time: String,
    end_time: String,
    repeat_until: String,
    reminder: bool,
    amount: i64,
    unit: i64,
    delete: bool,
    pending: bool,
}
impl Editor {
    pub fn new(date: NaiveDate) -> Self {
        Self::at(date, 9)
    }
    pub fn at(date: NaiveDate, hour: u32) -> Self {
        let start = Local
            .from_local_datetime(&date.and_hms_opt(hour, 0, 0).unwrap())
            .earliest()
            .unwrap_or_else(Local::now)
            .with_timezone(&Utc);
        Self::from(CalendarEvent {
            start,
            end: start + Duration::hours(1),
            ..Default::default()
        })
    }
    pub fn from(mut e: CalendarEvent) -> Self {
        e.writable = e.account.is_empty();
        let s = e.start.with_timezone(&Local);
        let en = e.end.with_timezone(&Local);
        Self {
            start_date: s.format("%Y-%m-%d").to_string(),
            end_date: en.format("%Y-%m-%d").to_string(),
            start_time: s.format("%H:%M").to_string(),
            end_time: en.format("%H:%M").to_string(),
            repeat_until: e.repeat_until.map(|d| d.to_string()).unwrap_or_default(),
            reminder: e.reminder_minutes.is_some(),
            amount: e.reminder_minutes.unwrap_or(10),
            unit: 1,
            event: e,
            delete: false,
            pending: false,
        }
    }
    fn validated(&self) -> Result<CalendarEvent, String> {
        let mut e = self.event.clone();
        let parse = |date: &str, time: &str| -> Result<chrono::DateTime<Utc>, String> {
            let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| "Use a valid date.")?;
            let t = NaiveTime::parse_from_str(if e.all_day { "00:00" } else { time }, "%H:%M")
                .map_err(|_| "Use HH:MM for the time.")?;
            match Local.from_local_datetime(&d.and_time(t)) {
                chrono::LocalResult::Single(v) => Ok(v.with_timezone(&Utc)),
                chrono::LocalResult::Ambiguous(_, _) => Err(
                    "This time occurs twice due to daylight saving. Choose another time.".into(),
                ),
                chrono::LocalResult::None => Err(
                    "This time does not exist due to daylight saving. Choose another time.".into(),
                ),
            }
        };
        e.start = parse(&self.start_date, &self.start_time)?;
        e.end = parse(&self.end_date, &self.end_time)?;
        e.reminder_minutes = if self.reminder {
            Some(self.amount.saturating_mul(self.unit))
        } else {
            None
        };
        e.repeat_until = if e.repeat == Repeat::Never || self.repeat_until.trim().is_empty() {
            None
        } else {
            Some(
                NaiveDate::parse_from_str(&self.repeat_until, "%Y-%m-%d")
                    .map_err(|_| "Use a valid repeat end date.")?,
            )
        };
        e.validate()?;
        Ok(e)
    }
}
pub(super) fn show(
    tab: &mut CalendarTab,
    o: &mut Organizer,
    ctx: &egui::Context,
    busy: bool,
) -> Option<CalendarAction> {
    let ed = tab.editor.as_mut()?;
    let description_id = egui::Id::new("event-description");
    let keys = design::ActionKeys::capture(ctx).multiline(description_id);
    let available = !ed.pending && (ed.event.account.is_empty() || !ed.event.writable || !busy);
    let mut save = false;
    let mut close = false;
    let mut delete = false;
    let response = design::modal(ctx, egui::Id::new("calendar-editor")).show(ctx, |ui| {
        ui.set_width(design::dialog_width(ctx, 610.));
        if design::dialog_heading(
            ui,
            if ed.event.kind == EventKind::Task {
                "Task"
            } else {
                "Event"
            },
        )
        .clicked()
            && !ed.pending
        {
            close = true;
        }
        egui::ScrollArea::vertical()
            .max_height(design::dialog_body_height(ctx))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.add_enabled_ui(!ed.pending, |ui| {
                    ui.add_enabled_ui(ed.event.writable, |ui| {
                        text_field(ui, "Title", &mut ed.event.title);
                        if ed.event.account.is_empty() {
                            ui.horizontal(|ui| {
                                ui.selectable_value(&mut ed.event.kind, EventKind::Event, "Event");
                                ui.selectable_value(&mut ed.event.kind, EventKind::Task, "Task");
                                if ed.event.kind == EventKind::Task {
                                    design::small_checkbox(ui, &mut ed.event.completed, "Completed");
                                }
                            });
                        }
                        if design::small_checkbox(ui, &mut ed.event.all_day, "All Day").changed()
                            && ed.event.all_day
                            && ed.end_date <= ed.start_date
                            && let Ok(date) = NaiveDate::parse_from_str(&ed.start_date, "%Y-%m-%d")
                        {
                            ed.end_date = (date + Duration::days(1)).to_string();
                        }

                        ui.columns(2, |cols| {
                            date_field(
                                &mut cols[0],
                                "Start Date",
                                "event-start-date",
                                &mut ed.start_date,
                                false,
                            );
                            date_field(
                                &mut cols[1],
                                if ed.event.all_day {
                                    "End Date (Exclusive)"
                                } else {
                                    "End Date"
                                },
                                "event-end-date",
                                &mut ed.end_date,
                                false,
                            );
                        });
                        if !ed.event.all_day {
                            ui.columns(2, |cols| {
                                text_field(&mut cols[0], "Start Time", &mut ed.start_time);
                                text_field(&mut cols[1], "End Time", &mut ed.end_time);
                            });
                        }
                        if ed.event.account.is_empty() {
                            field_label(ui, "Repeat");
                            egui::ComboBox::from_id_salt("event-repeat")
                                .selected_text(match ed.event.repeat {
                                    Repeat::Never => "Does Not Repeat",
                                    Repeat::Daily => "Daily",
                                    Repeat::Weekly => "Weekly",
                                    Repeat::Monthly => "Monthly",
                                })
                                .show_ui(ui, |ui| {
                                    for (rule, label) in [
                                        (Repeat::Never, "Does Not Repeat"),
                                        (Repeat::Daily, "Daily"),
                                        (Repeat::Weekly, "Weekly"),
                                        (Repeat::Monthly, "Monthly"),
                                    ] {
                                        ui.selectable_value(&mut ed.event.repeat, rule, label);
                                    }
                                });
                            ui.add_space(10.);
                            if ed.event.repeat != Repeat::Never {
                                date_field(
                                    ui,
                                    "Repeat Until",
                                    "event-repeat-until",
                                    &mut ed.repeat_until,
                                    true,
                                );
                                design::hint(ui, "Changes apply to the whole series.");
                            }
                        }
                        text_field(ui, "Location", &mut ed.event.location);
                        field_label(ui, "Description");
                        ui.add_enabled(
                            ed.event.description_editable,
                            egui::TextEdit::multiline(&mut ed.event.description)
                                .id(description_id)
                                .desired_width(f32::INFINITY)
                                .desired_rows(4)
                                .margin(Vec2::splat(12.)),
                        );
                        if !ed.event.description_editable {
                            design::hint(ui, "Event details are managed by the calendar publisher.");
                        }
                    });
                    ui.add_space(16.);
                    ui.separator();
                    ui.add_space(12.);
                    design::small_checkbox(ui, &mut ed.reminder, "Telegram Notification");
                    if ed.reminder {
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut ed.amount).range(0..=525600));
                            egui::ComboBox::from_id_salt("reminder-unit")
                                .selected_text(match ed.unit {
                                    60 => "Hours",
                                    1440 => "Days",
                                    _ => "Minutes",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut ed.unit, 1, "Minutes");
                                    ui.selectable_value(&mut ed.unit, 60, "Hours");
                                    ui.selectable_value(&mut ed.unit, 1440, "Days");
                                });
                            ui.label("Before");
                        });
                    }
                    if !ed.event.writable {
                        design::hint(
                            ui,
                            "Subscribed calendars are read only. You can set a Telegram reminder here.",
                        );
                    }
                    if !ed.event.url.is_empty() {
                        ui.hyperlink_to("Open in Provider", &ed.event.url);
                    }
                });
            });
        if !tab.error.is_empty() {
            ui.colored_label(colors(ui).red, &tab.error);
        }
        if ed.pending {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Saving calendar changes…");
            });
        }
        design::dialog_footer(ui, |ui| {
            if ed.delete {
                ui.colored_label(colors(ui).red, "Delete this event?");
                if ui
                    .add_enabled(available, design::Button::new("Delete Event"))
                    .clicked()
                {
                    delete = true;
                }
                if ui
                    .add_enabled(!ed.pending, design::Button::new("Keep"))
                    .clicked()
                {
                    ed.delete = false;
                }
            } else {
                if ui
                    .add_enabled(!ed.pending, design::Button::new("Cancel"))
                    .clicked()
                {
                    close = true;
                }
                if o.events.iter().any(|e| e.id == ed.event.id)
                    && ed.event.writable
                    && ui
                        .add_enabled(!ed.pending, design::Button::new("Delete…"))
                        .clicked()
                {
                    ed.delete = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled_ui(available, |ui| design::dialog_primary(ui, "Save Event"))
                        .inner
                        .clicked()
                    {
                        save = true;
                    }
                });
            }
        });
    });
    let shortcut = keys.blocked(ed.pending).dialog(&response, available);
    close |= shortcut.cancel;
    if shortcut.confirm {
        if ed.delete {
            delete = true;
        } else {
            save = true;
        }
    }
    if delete && available && ed.event.account.is_empty() {
        o.events.retain(|e| e.id != ed.event.id);
        close = true;
    }
    if save {
        let validated = if ed.event.account.is_empty() {
            ed.validated()
        } else {
            let mut event = ed.event.clone();
            event.reminder_minutes = if ed.reminder {
                Some(ed.amount.saturating_mul(ed.unit))
            } else {
                None
            };
            if event
                .reminder_minutes
                .is_some_and(|m| !(0..=525600).contains(&m))
            {
                Err("Notification must be between 0 minutes and 365 days before.".into())
            } else {
                Ok(event)
            }
        };
        match validated {
            Ok(event) => {
                if event.account.is_empty() {
                    o.upsert(event);
                } else if let Some(current) = o.events.iter_mut().find(|e| e.id == event.id) {
                    current.reminder_minutes = event.reminder_minutes;
                }
                close = true;
            }
            Err(error) => tab.error = error,
        }
    }
    if close {
        tab.editor = None;
        tab.error.clear();
    }
    None
}
impl CalendarTab {
    pub fn finish(&mut self, error: Option<&str>) {
        if let Some(error) = error {
            self.error = error.into();
            if let Some(ed) = &mut self.editor {
                ed.pending = false;
            }
        } else {
            self.editor = None;
            self.error.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        tab: &mut CalendarTab,
        o: &mut Organizer,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) {
        let releases: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let egui::Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    repeat,
                    modifiers,
                } = e
                {
                    Some(egui::Event::Key {
                        key: *key,
                        physical_key: *physical_key,
                        pressed: false,
                        repeat: *repeat,
                        modifiers: *modifiers,
                    })
                } else {
                    None
                }
            })
            .collect();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(980., 640.),
                )),
                events,
                ..Default::default()
            },
            |_| {
                tab.dialogs(o, ctx, false);
            },
        );
        output.textures_delta.clear();
        if !releases.is_empty() {
            frame(tab, o, ctx, releases);
        }
    }
    fn key(k: egui::Key, ctrl: bool) -> egui::Event {
        egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl,
                ..Default::default()
            },
        }
    }
    #[test]
    fn calendar_keyboard_validates_saves_discards_and_keeps_multiline_enter() {
        let ctx = egui::Context::default();
        amigo_ui::design::install(&ctx);
        let mut tab = CalendarTab::default();
        let mut o = Organizer::default();
        tab.new_event(Local::now().date_naive());
        frame(&mut tab, &mut o, &ctx, vec![]);
        frame(&mut tab, &mut o, &ctx, vec![]);
        frame(&mut tab, &mut o, &ctx, vec![key(egui::Key::Enter, false)]);
        assert!(o.events.is_empty());
        assert!(!tab.error.is_empty());
        tab.editor.as_mut().unwrap().event.title = "Meeting".into();
        frame(&mut tab, &mut o, &ctx, vec![]);
        frame(&mut tab, &mut o, &ctx, vec![key(egui::Key::Enter, false)]);
        assert_eq!(o.events.len(), 1);
        assert!(tab.editor.is_none());
        tab.editor = Some(Editor::from(o.events[0].clone()));
        tab.editor.as_mut().unwrap().event.title = "Discard me".into();
        frame(&mut tab, &mut o, &ctx, vec![key(egui::Key::Escape, false)]);
        assert_eq!(o.events[0].title, "Meeting");
        tab.editor = Some(Editor::from(o.events[0].clone()));
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("event-description")));
        frame(&mut tab, &mut o, &ctx, vec![key(egui::Key::Enter, false)]);
        assert!(tab.editor.is_some());
        frame(&mut tab, &mut o, &ctx, vec![]);
        frame(&mut tab, &mut o, &ctx, vec![key(egui::Key::Enter, true)]);
        assert!(tab.editor.is_none());
    }
}
