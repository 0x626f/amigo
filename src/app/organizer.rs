//! Calendar subscription workers, autosave, and settings.
mod jobs;
mod settings;
use super::*;
use amigo_calendar::CalendarAction;
use amigo_connect::{Outcome, Request, Response};
use amigo_core::{credentials::Credentials, organizer::*};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
pub(super) struct Productivity {
    pub saved: Organizer,
    credentials: Credentials,
    blocked: bool,
    credentials_blocked: bool,
    job: Option<Job>,
    queue: VecDeque<Request>,
    pub(super) settings: Option<SettingsDraft>,
    last_sync: Instant,
    last_notify: Instant,
}
#[derive(Clone, Copy, PartialEq)]
enum SettingsPage {
    Calendar,
    Notifications,
}
pub(super) struct SettingsDraft {
    page: SettingsPage,
    credentials: Credentials,
    subscriptions: Vec<SubscriptionDraft>,
    calendar_settings: CalendarSettings,
    error: String,
}
struct SubscriptionDraft {
    id: String,
    name: String,
    url: String,
}
struct Job {
    request: Request,
    receiver: mpsc::Receiver<Outcome>,
    cancel: Arc<AtomicBool>,
}
impl Productivity {
    pub fn load(root: &std::path::Path) -> (Organizer, Self, String) {
        let (o, blocked, mut error) = match Organizer::load(root) {
            Ok(o) => (o, false, String::new()),
            Err(e) => (Organizer::default(), true, e),
        };
        let (credentials, credentials_blocked) = match Credentials::load(root) {
            Ok(c) => (c, false),
            Err(e) => {
                error = e;
                (Credentials::default(), true)
            }
        };
        let s = Self {
            saved: o.clone(),
            credentials,
            blocked,
            credentials_blocked,
            job: None,
            queue: Default::default(),
            settings: None,
            last_sync: Instant::now() - Duration::from_secs(86_400),
            last_notify: Instant::now() - Duration::from_secs(30),
        };
        (o, s, error)
    }
    pub fn available(&self) -> bool {
        !self.blocked
    }
    pub fn busy(&self) -> bool {
        self.job.is_some() || !self.queue.is_empty()
    }
}
impl Drop for Productivity {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}
impl Amigo {
    pub(super) fn pause_calendar_connections(&mut self) {
        if let Some(job) = self.productivity.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.productivity.queue.clear();
    }
    pub(super) fn save_organizer(&mut self) -> bool {
        if self.productivity.blocked {
            return self.organizer == self.productivity.saved;
        }
        if self.organizer == self.productivity.saved {
            return true;
        }
        match self.organizer.save(&self.data_path) {
            Ok(()) => {
                self.productivity.saved = self.organizer.clone();
                true
            }
            Err(e) => {
                self.error = format!("Calendar save failed: {e}");
                false
            }
        }
    }
    pub(super) fn calendar_settings(&mut self) {
        self.productivity.settings = Some(SettingsDraft {
            credentials: self.productivity.credentials.clone(),
            page: SettingsPage::Calendar,
            subscriptions: self
                .organizer
                .subscriptions
                .iter()
                .map(|s| SubscriptionDraft {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    url: self
                        .productivity
                        .credentials
                        .calendar_urls
                        .get(&s.id)
                        .cloned()
                        .unwrap_or_default(),
                })
                .collect(),
            calendar_settings: self.organizer.calendar_settings.clone(),
            error: String::new(),
        });
    }
    pub(super) fn calendar_action(&mut self, action: CalendarAction) {
        match action {
            CalendarAction::Settings => self.open_settings(super::settings::SettingsPage::Calendar),
            CalendarAction::Sync => self.sync_calendars(),
            CalendarAction::Rename => self.prompt_calendar_rename(),
            CalendarAction::Import => self.open_data_dialog(
                amigo_core::backup::Scope::Calendar,
                design::DataAction::Import,
            ),
            CalendarAction::Export => self.open_data_dialog(
                amigo_core::backup::Scope::Calendar,
                design::DataAction::Export,
            ),
            CalendarAction::Delete => self.open_data_dialog(
                amigo_core::backup::Scope::Calendar,
                design::DataAction::Delete,
            ),
        }
    }
    fn request_allowed(&self, request: &Request) -> bool {
        match request {
            Request::CalendarFeed { id, url } => {
                self.organizer.subscriptions.iter().any(|s| s.id == *id)
                    && self.productivity.credentials.calendar_urls.get(id) == Some(url)
            }
            _ => true,
        }
    }

    fn queue_request(&mut self, request: Request) {
        if self.productivity.blocked || self.productivity.credentials_blocked {
            self.error="Calendar storage is unavailable. Existing files are preserved; fix the file error and restart Amigo.".into();
            self.calendar.finish(Some(&self.error));
            return;
        }
        if !self.request_allowed(&request) {
            return;
        }
        if let Request::CalendarFeed { id, url } = &request {
            if self.productivity.job.as_ref().is_some_and(
                |j| matches!(&j.request,Request::CalendarFeed{id:a,url:u}if a==id&&u==url),
            ) {
                return;
            }
            self.productivity
                .queue
                .retain(|r| !matches!(r,Request::CalendarFeed{id:a,..}if a==id));
        }
        self.productivity.queue.push_back(request);
    }
    fn sync_calendars(&mut self) {
        self.productivity.last_sync = Instant::now();
        for subscription in self.organizer.subscriptions.clone() {
            if let Some(url) = self
                .productivity
                .credentials
                .calendar_urls
                .get(&subscription.id)
            {
                self.queue_request(Request::CalendarFeed {
                    id: subscription.id,
                    url: url.clone(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{harness, render, render_sized, text_rect, text_rect_optional};
    #[test]
    fn calendar_navigation_and_views_fit_both_window_sizes() {
        let (mut app, ctx) = harness();
        let output = render(&mut app, &ctx, vec![]);
        assert!(text_rect_optional(&output, "Email").is_none());
        assert!(ctx.read_response(egui::Id::new("app-accounts")).is_none());
        let positions = ["Tasks", "Calendar", "Finance"].map(|s| text_rect(&output, s).top());
        assert!(positions.windows(2).all(|p| p[0] < p[1]));
        assert!(app.calendar.view == amigo_calendar::View::Day);
        app.section = Section::Calendar;
        for _ in 0..3 {
            render(&mut app, &ctx, vec![]);
        }
        let output = render(&mut app, &ctx, vec![]);
        let views = ["Day", "Week", "Month", "Agenda"].map(|s| text_rect(&output, s).left());
        assert!(views.windows(2).all(|p| p[0] < p[1]));
        for size in [Vec2::new(980., 640.), Vec2::new(1440., 900.)] {
            for view in [
                amigo_calendar::View::Month,
                amigo_calendar::View::Week,
                amigo_calendar::View::Day,
                amigo_calendar::View::Agenda,
            ] {
                app.section = Section::Calendar;
                app.calendar.view = view;
                let _ = render_sized(&mut app, &ctx, vec![], size);
                let create = ctx
                    .read_response(egui::Id::new("calendar-create"))
                    .unwrap()
                    .rect;
                assert!(create.right() <= size.x);
                assert!(create.top() >= 0.);
            }
        }
        std::fs::remove_dir_all(&app.data_path).unwrap();
    }
    #[test]
    fn calendar_persists_independently_of_board_and_vault() {
        let (mut app, _) = harness();
        app.organizer.events.push(CalendarEvent {
            title: "Personal meeting".into(),
            ..Default::default()
        });
        assert!(app.save_local());
        let reopened = Amigo::load(app.data_path.clone());
        assert_eq!(reopened.organizer, app.organizer);
        assert_eq!(reopened.workspace, app.workspace);
        std::fs::remove_dir_all(&app.data_path).unwrap();
    }
    #[test]
    fn damaged_organizer_and_credentials_are_not_overwritten() {
        let (mut app, _) = harness();
        std::fs::write(app.data_path.join("organizer.json"), b"damaged").unwrap();
        let reopened = Amigo::load(app.data_path.clone());
        assert!(reopened.productivity.blocked);
        assert_eq!(
            std::fs::read(app.data_path.join("organizer.json")).unwrap(),
            b"damaged"
        );
        app.productivity.credentials.telegram_token = "secret".into();
        app.productivity.credentials.save(&app.data_path).unwrap();
        std::fs::remove_file(app.data_path.join("credentials.key")).unwrap();
        let mut reopened = Amigo::load(app.data_path.clone());
        assert!(reopened.productivity.credentials_blocked);
        reopened.queue_request(Request::CalendarFeed {
            id: "subscription".into(),
            url: "https://example.com/calendar.ics".into(),
        });
        assert!(reopened.productivity.queue.is_empty());
        std::fs::remove_dir_all(&app.data_path).unwrap();
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    use crate::app::tests::{click, harness, render, text_rect, text_rect_optional};
    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }
    #[test]
    fn calendar_filter_escape_discards_and_enter_accepts() {
        let (mut app, ctx) = harness();
        app.calendar.view = amigo_calendar::View::Month;
        app.section = Section::Calendar;
        let mut task = amigo_core::model::Task::new(1, "todo");
        task.title = "Kanban Task".into();
        task.due_date = app.calendar.date.to_string();
        app.workspace.tasks.push(task.clone());
        app.organizer.events.push(CalendarEvent {
            title: "Filter Test".into(),
            ..Default::default()
        });
        for _ in 0..2 {
            render(&mut app, &ctx, vec![]);
        }
        let filter = ctx
            .read_response(egui::Id::new("calendar-filters"))
            .unwrap()
            .rect;
        click(&mut app, &ctx, filter.center());
        let out = render(&mut app, &ctx, vec![]);
        assert!(text_rect_optional(&out, "Kanban Due Dates").is_none());
        assert!(text_rect_optional(&out, "□ Kanban Task").is_none());
        assert_eq!(app.workspace.tasks, vec![task]);
        click(&mut app, &ctx, text_rect(&out, "Personal").center());
        let out = render(&mut app, &ctx, vec![]);
        assert!(text_rect_optional(&out, "Filter Test").is_none());
        render(&mut app, &ctx, vec![key(egui::Key::Escape)]);
        let out = render(&mut app, &ctx, vec![]);
        assert!(text_rect_optional(&out, "Filter Test").is_some());
        click(&mut app, &ctx, filter.center());
        let out = render(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, text_rect(&out, "Personal").center());
        render(&mut app, &ctx, vec![key(egui::Key::Enter)]);
        let out = render(&mut app, &ctx, vec![]);
        assert!(text_rect_optional(&out, "Filter Test").is_none());
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
}

#[cfg(test)]
mod url_subscription_tests {
    use super::*;
    use crate::app::tests::{click, harness, render, text_rect, text_rect_optional};
    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }
    #[test]
    fn calendar_settings_save_cancel_encrypt_and_remove_work_without_account_controls() {
        let (mut app, ctx) = harness();
        app.section = Section::Calendar;
        app.calendar_action(CalendarAction::Settings);
        for _ in 0..2 {
            render(&mut app, &ctx, vec![]);
        }
        let output = render(&mut app, &ctx, vec![]);
        assert!(text_rect_optional(&output, "Desktop OAuth Client ID").is_none());
        assert!(text_rect_optional(&output, "Connected Accounts").is_none());
        click(
            &mut app,
            &ctx,
            text_rect(&output, "+ Add Calendar URL").center(),
        );
        let draft = app.productivity.settings.as_mut().unwrap();
        draft.subscriptions[0].name = "Private Calendar".into();
        draft.subscriptions[0].url = "https://example.invalid/private-token.ics".into();
        render(&mut app, &ctx, vec![key(egui::Key::Escape)]);
        assert!(app.organizer.subscriptions.is_empty());
        assert!(app.productivity.credentials.calendar_urls.is_empty());
        app.calendar_action(CalendarAction::Settings);
        for _ in 0..2 {
            render(&mut app, &ctx, vec![]);
        }
        app.productivity
            .settings
            .as_mut()
            .unwrap()
            .subscriptions
            .push(SubscriptionDraft {
                id: "private".into(),
                name: "Private Calendar".into(),
                url: "webcal://example.invalid/private-token.ics".into(),
            });
        render(&mut app, &ctx, vec![key(egui::Key::Enter)]);
        assert!(app.productivity.settings.is_none());
        assert_eq!(app.organizer.subscriptions.len(), 1);
        assert!(matches!(
            app.productivity.queue.front(),
            Some(Request::CalendarFeed { .. })
        ));
        app.productivity.queue.clear();
        render(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let credentials = Credentials::load(&app.data_path).unwrap();
        assert_eq!(
            credentials.calendar_urls["private"],
            "https://example.invalid/private-token.ics"
        );
        let bytes = std::fs::read(app.data_path.join("organizer.json")).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("private-token"));
        let bytes = std::fs::read(app.data_path.join("credentials.enc")).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("private-token"));
        let reopened = Amigo::load(app.data_path.clone());
        assert_eq!(
            reopened.organizer.subscriptions,
            app.organizer.subscriptions
        );
        assert_eq!(
            reopened.productivity.credentials.calendar_urls,
            credentials.calendar_urls
        );
        app.organizer.events.push(CalendarEvent {
            account: "feed:private".into(),
            ..Default::default()
        });
        app.calendar_action(CalendarAction::Settings);
        for _ in 0..2 {
            render(&mut app, &ctx, vec![]);
        }
        app.productivity
            .settings
            .as_mut()
            .unwrap()
            .subscriptions
            .clear();
        render(&mut app, &ctx, vec![key(egui::Key::Enter)]);
        assert!(app.organizer.subscriptions.is_empty());
        assert!(app.organizer.events.is_empty());
        assert!(
            Credentials::load(&app.data_path)
                .unwrap()
                .calendar_urls
                .is_empty()
        );
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
    fn deliver(app: &mut Amigo, ctx: &egui::Context, url: &str, result: Result<Response, String>) {
        let (tx, rx) = mpsc::channel();
        tx.send(Outcome { result }).unwrap();
        app.productivity.last_sync = Instant::now();
        app.productivity.job = Some(Job {
            request: Request::CalendarFeed {
                id: "s".into(),
                url: url.into(),
            },
            receiver: rx,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        app.tick_organizer(ctx);
    }
    #[test]
    fn feed_refresh_preserves_reminders_and_ignores_changed_or_removed_urls() {
        let (mut app, ctx) = harness();
        app.organizer.subscriptions.push(CalendarSubscription {
            id: "s".into(),
            name: "Work".into(),
        });
        let url = "https://example.invalid/private.ics";
        app.productivity
            .credentials
            .calendar_urls
            .insert("s".into(), url.into());
        let mut event = CalendarEvent {
            account: "feed:s".into(),
            id: "stable".into(),
            title: "Meeting".into(),
            writable: false,
            reminder_minutes: Some(30),
            ..Default::default()
        };
        app.organizer.events.push(event.clone());
        event.reminder_minutes = None;
        event.title = "Updated".into();
        deliver(
            &mut app,
            &ctx,
            url,
            Ok(Response::CalendarFeed {
                id: "s".into(),
                events: vec![event.clone()],
            }),
        );
        assert_eq!(app.organizer.events[0].reminder_minutes, Some(30));
        assert_eq!(app.organizer.events[0].title, "Updated");
        assert!(!app.organizer.calendars[0].writable);
        app.productivity
            .credentials
            .calendar_urls
            .insert("s".into(), "https://example.invalid/new.ics".into());
        event.title = "Stale".into();
        deliver(
            &mut app,
            &ctx,
            url,
            Ok(Response::CalendarFeed {
                id: "s".into(),
                events: vec![event.clone()],
            }),
        );
        assert_eq!(app.organizer.events[0].title, "Updated");
        assert!(app.productivity.credentials.calendar_urls["s"].ends_with("new.ics"));
        let before = app.organizer.clone();
        deliver(
            &mut app,
            &ctx,
            "https://example.invalid/new.ics",
            Err("Invalid feed".into()),
        );
        assert_eq!(app.organizer, before);
        app.organizer.remove_subscription("s");
        deliver(
            &mut app,
            &ctx,
            "https://example.invalid/new.ics",
            Ok(Response::CalendarFeed {
                id: "s".into(),
                events: vec![event],
            }),
        );
        assert!(app.organizer.events.is_empty());
        std::fs::remove_dir_all(app.data_path).unwrap();
    }
}

#[cfg(test)]
mod refresh_timing_tests {
    use super::*;
    use crate::app::tests::harness;
    #[test]
    fn refresh_interval_schedules_url_feeds_without_duplicate_pending_work() {
        let (mut app, ctx) = harness();
        app.organizer.subscriptions.push(CalendarSubscription {
            id: "work".into(),
            name: "Work".into(),
        });
        app.productivity
            .credentials
            .calendar_urls
            .insert("work".into(), "https://example.invalid/calendar.ics".into());
        app.organizer.calendar_settings.refresh_interval_seconds = 300;
        app.productivity.last_sync = Instant::now() - Duration::from_secs(299);
        // Hold the worker slot: this checks scheduling without network access.
        let (_sender, receiver) = mpsc::channel();
        app.productivity.job = Some(Job {
            request: Request::Notify(Notification {
                keys: vec![],
                text: String::new(),
            }),
            receiver,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        app.tick_organizer(&ctx);
        assert!(app.productivity.queue.is_empty());
        app.productivity.last_sync = Instant::now() - Duration::from_secs(301);
        app.tick_organizer(&ctx);
        assert!(
            matches!(app.productivity.queue.front(), Some(Request::CalendarFeed { id, .. }) if id == "work")
        );
        app.productivity.last_sync = Instant::now() - Duration::from_secs(301);
        app.tick_organizer(&ctx);
        assert_eq!(app.productivity.queue.len(), 1);
        std::fs::remove_dir_all(app.data_path.clone()).unwrap();
    }
}
