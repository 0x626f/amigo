//! Calendar presentation and drafts; the shell owns connections and background work.
mod editor;
mod views;
use amigo_core::organizer::*;
use amigo_ui::design::{self, Icon, colors};
use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Timelike, Utc};
use egui::{Pos2, Rect, RichText, Stroke, Vec2};
use std::collections::BTreeSet;
#[derive(Clone, Copy, Default, PartialEq)]
pub enum View {
    #[default]
    Day,
    Week,
    Month,
    Agenda,
}
pub enum CalendarAction {
    Settings,
    Import,
    Export,
    Delete,
    Rename,
    Sync,
}
pub struct CalendarTab {
    pub date: NaiveDate,
    pub view: View,
    search: String,
    search_control: design::SearchField,
    hidden: BTreeSet<String>,
    filter_snapshot: Option<BTreeSet<String>>,
    timeline_position: Option<(View, NaiveDate)>,
    editor: Option<editor::Editor>,
    pub error: String,
}
impl Default for CalendarTab {
    fn default() -> Self {
        Self {
            date: Local::now().date_naive(),
            view: View::default(),
            search: String::new(),
            search_control: Default::default(),
            hidden: Default::default(),
            filter_snapshot: None,
            timeline_position: None,
            editor: None,
            error: String::new(),
        }
    }
}
impl CalendarTab {
    pub fn new_event(&mut self, date: NaiveDate) {
        self.editor = Some(editor::Editor::new(date));
    }
    pub fn editing(&self) -> bool {
        self.editor.is_some()
    }
    pub fn sidebar(&mut self, o: &Organizer, ui: &mut egui::Ui) -> Option<CalendarAction> {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            design::eyebrow(ui, "MY CALENDARS");
            let mut local = !self.hidden.contains("local");
            if calendar_name(ui, &mut local, &o.name).changed() {
                self.toggle("local", local);
            }
            for c in o.calendars.iter().filter(|c| o.source_enabled(&c.account)) {
                let key = format!("{}:{}", c.account, c.id);
                let mut show = !self.hidden.contains(&key);
                if calendar_name(ui, &mut show, &c.name).changed() {
                    self.toggle(&key, show);
                }
            }
        });
        None
    }

    fn toggle(&mut self, key: &str, show: bool) {
        if show {
            self.hidden.remove(key);
        } else {
            self.hidden.insert(key.into());
        }
    }
    pub fn show(&mut self, o: &Organizer, ui: &mut egui::Ui) -> Option<CalendarAction> {
        let mut action = None;
        let search_width = self.search_control.width(ui, "calendar-search", 190.);
        let tools_width = search_width + 2. * 38. + 4. * 30. + 6. * ui.spacing().item_spacing.x;
        let heading_width =
            (ui.available_width() - tools_width - ui.spacing().item_spacing.x).max(160.);
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(heading_width, 38.),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    if design::small_button(ui, "‹")
                        .on_hover_text("Previous")
                        .clicked()
                    {
                        self.navigate(-1);
                    }
                    if design::small_button(ui, "Today").clicked() {
                        self.date = Local::now().date_naive();
                    }
                    if design::small_button(ui, "›")
                        .on_hover_text("Next")
                        .clicked()
                    {
                        self.navigate(1);
                    }
                    ui.add(
                        egui::Label::new(
                            RichText::new(self.date.format("%B %Y").to_string())
                                .size(23.)
                                .strong(),
                        )
                        .truncate(),
                    );
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if design::icon_button(
                    ui,
                    egui::Id::new("calendar-create"),
                    Icon::Plus,
                    "Create Event",
                    38.,
                )
                .clicked()
                {
                    self.new_event(self.date);
                }
                let f = design::icon_button(
                    ui,
                    egui::Id::new("calendar-filters"),
                    Icon::Filter,
                    "Calendar Filters",
                    38.,
                );
                let popup = egui::Popup::default_response_id(&f);
                let keys = design::ActionKeys::capture_popup(ui.ctx(), popup);
                let mut shortcut = design::ActionShortcut::default();
                if f.clicked() && !egui::Popup::is_id_open(ui.ctx(), popup) {
                    self.filter_snapshot = Some(self.hidden.clone());
                }
                egui::Popup::menu(&f)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .width(220.0)
                    .show(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        let mut local = !self.hidden.contains("local");
                        if calendar_name(ui, &mut local, "Personal").changed() {
                            self.toggle("local", local);
                        }
                        for c in o.calendars.iter().filter(|c| o.source_enabled(&c.account)) {
                            let key = format!("{}:{}", c.account, c.id);
                            let mut show = !self.hidden.contains(&key);
                            if calendar_name(ui, &mut show, &c.name).changed() {
                                self.toggle(&key, show);
                            }
                        }
                        shortcut = keys.in_popup(ui, popup);
                        if shortcut.cancel || shortcut.confirm {
                            ui.close();
                        }
                    });
                if !egui::Popup::is_id_open(ui.ctx(), popup)
                    && let Some(hidden) = self.filter_snapshot.take()
                    && shortcut.cancel
                {
                    self.hidden = hidden;
                }
                for (key, icon, label, candidate) in [
                    (
                        "rename",
                        Icon::Edit,
                        "Rename Calendar",
                        CalendarAction::Rename,
                    ),
                    (
                        "delete",
                        Icon::Trash,
                        "Delete Calendar",
                        CalendarAction::Delete,
                    ),
                    (
                        "export",
                        Icon::Export,
                        "Export Calendar",
                        CalendarAction::Export,
                    ),
                    (
                        "import",
                        Icon::Import,
                        "Import Calendar",
                        CalendarAction::Import,
                    ),
                ] {
                    if design::icon_button(
                        ui,
                        egui::Id::new(format!("calendar-{key}")),
                        icon,
                        label,
                        30.,
                    )
                    .clicked()
                    {
                        action = Some(candidate);
                    }
                }

                self.search_control.show(
                    ui,
                    "calendar-search",
                    &mut self.search,
                    search_width,
                    "Search Calendar…",
                );
            });
        });
        ui.add_space(12.);
        ui.horizontal(|ui| {
            for (view, name) in [
                (View::Day, "Day"),
                (View::Week, "Week"),
                (View::Month, "Month"),
                (View::Agenda, "Agenda"),
            ] {
                ui.selectable_value(&mut self.view, view, name);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                design::hint(
                    ui,
                    &format!("Local Time · UTC{}", Local::now().format("%:z")),
                );
            });
        });
        ui.add_space(12.);
        let from = midnight(self.date.with_day(1).unwrap() - Duration::days(7)).unwrap();
        let until = midnight(self.date + Duration::days(70)).unwrap();
        let occurrences = o.occurrences(from, until);
        let mut events: Vec<_> = occurrences
            .iter()
            .filter(|e| {
                !self.hidden.contains(&if e.account.is_empty() {
                    "local".into()
                } else {
                    format!("{}:{}", e.account, e.calendar)
                }) && format!("{} {} {}", e.title, e.description, e.location)
                    .to_lowercase()
                    .contains(&self.search.to_lowercase())
            })
            .cloned()
            .collect();
        events.sort_by_key(|e| (e.start, e.end));
        let clicked = match self.view {
            View::Month => self.month(ui, &events),
            View::Week | View::Day => self.timeline(ui, &events),
            View::Agenda => self.agenda(ui, &events),
        };
        if let Some(e) = clicked {
            let original = if e.repeat != Repeat::Never {
                o.events
                    .iter()
                    .find(|item| item.id == e.id)
                    .cloned()
                    .unwrap_or(e)
            } else {
                e
            };
            self.editor = Some(editor::Editor::from(original));
        }
        action
    }
    fn navigate(&mut self, n: i64) {
        self.date = match self.view {
            View::Month => {
                let m = chrono::Months::new(1);
                if n > 0 {
                    self.date.checked_add_months(m)
                } else {
                    self.date.checked_sub_months(m)
                }
                .unwrap_or(self.date)
            }
            View::Week => self.date + Duration::days(n * 7),
            View::Day => self.date + Duration::days(n),
            View::Agenda => self.date + Duration::days(n * 30),
        };
    }
    pub fn dialogs(
        &mut self,
        o: &mut Organizer,
        ctx: &egui::Context,
        busy: bool,
    ) -> Option<CalendarAction> {
        editor::show(self, o, ctx, busy)
    }
}
fn calendar_name(ui: &mut egui::Ui, visible: &mut bool, name: &str) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 24.0;
        ui.spacing_mut().button_padding = Vec2::new(2.0, 2.0);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        design::small_checkbox(ui, visible, RichText::new(name).size(13.0)).on_hover_text(name)
    })
    .inner
}

fn midnight(d: NaiveDate) -> Option<chrono::DateTime<Utc>> {
    Local
        .from_local_datetime(&d.and_hms_opt(0, 0, 0)?)
        .earliest()
        .map(|d| d.with_timezone(&Utc))
}
fn on_day(e: &CalendarEvent, d: NaiveDate) -> bool {
    let Some(s) = midnight(d) else {
        return false;
    };
    let Some(en) = midnight(d + Duration::days(1)) else {
        return false;
    };
    e.start < en && e.end > s
}
fn event_chip(ui: &mut egui::Ui, r: Rect, e: &CalendarEvent) -> egui::Response {
    let accent = if e.completed {
        colors(ui).muted
    } else if e.kind == EventKind::Task {
        colors(ui).green
    } else if e.account.is_empty() {
        colors(ui).accent
    } else {
        colors(ui).violet
    };
    let id = ui.make_persistent_id((
        "event-chip",
        &e.id,
        e.start.timestamp(),
        r.left().to_bits(),
        r.top().to_bits(),
    ));
    let response = ui.interact(r, id, egui::Sense::click());
    let hover = design::animate_toggle(ui.ctx(), id.with("hover"), response.hovered(), 0.14);
    let painter = ui.painter().with_clip_rect(r.intersect(ui.clip_rect()));
    painter.rect_filled(r, 4, accent.gamma_multiply(0.10 + 0.10 * hover));
    painter.line_segment(
        [
            r.left_top() + Vec2::new(1.5, 4.),
            r.left_bottom() + Vec2::new(1.5, -4.),
        ],
        Stroke::new(2., accent),
    );
    let label = format!(
        "{}{}",
        if e.completed {
            "✓ "
        } else if e.kind == EventKind::Task {
            "□ "
        } else {
            ""
        },
        e.title
    );
    let galley = egui::WidgetText::from(RichText::new(label).size(13.).color(accent)).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        (r.width() - 14.).max(1.),
        egui::TextStyle::Body,
    );
    let y = ((r.height() - galley.size().y) / 2.).clamp(0., 8.);
    painter.galley(r.left_top() + Vec2::new(7., y), galley, accent);
    response.on_hover_text(format!(
        "{}\n{}",
        e.title,
        if e.all_day {
            "All Day".into()
        } else {
            e.start.with_timezone(&Local).format("%H:%M").to_string()
        }
    ))
}

/// A connected overlap group shares a fixed lane count, avoiding intersecting blocks.
fn lanes<'a>(events: &[&'a CalendarEvent]) -> Vec<(&'a CalendarEvent, usize, usize)> {
    let mut sorted = events.to_vec();
    sorted.sort_by_key(|e| (e.start, e.end));
    let mut result = vec![];
    let mut at = 0;
    while at < sorted.len() {
        let mut end = sorted[at].end;
        let mut stop = at + 1;
        while stop < sorted.len() && sorted[stop].start < end {
            end = end.max(sorted[stop].end);
            stop += 1;
        }
        let mut occupied = Vec::new();
        let mut group = vec![];
        for e in &sorted[at..stop] {
            let lane = occupied
                .iter()
                .position(|end| *end <= e.start)
                .unwrap_or(occupied.len());
            if lane == occupied.len() {
                occupied.push(e.end);
            } else {
                occupied[lane] = e.end;
            }
            group.push((*e, lane));
        }
        result.extend(group.into_iter().map(|(e, lane)| (e, lane, occupied.len())));
        at = stop;
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chain_overlaps_share_fixed_lanes() {
        let now = Utc::now();
        let events: Vec<_> = [(0, 60), (30, 120), (60, 120)]
            .into_iter()
            .map(|(s, e)| CalendarEvent {
                start: now + Duration::minutes(s),
                end: now + Duration::minutes(e),
                ..Default::default()
            })
            .collect();
        let layout = lanes(&events.iter().collect::<Vec<_>>());
        assert!(layout.iter().all(|(_, _, n)| *n == 2));
        assert_eq!(layout[0].1, layout[2].1);
        assert_ne!(layout[1].1, layout[2].1);
    }
}
