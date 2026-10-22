//! Calendar records, subscriptions, and durable reminder receipts.
use crate::{model::id, storage::atomic_write};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarSettings {
    pub refresh_interval_seconds: u64,
}
impl Default for CalendarSettings {
    fn default() -> Self {
        Self {
            refresh_interval_seconds: 60,
        }
    }
}
/// Subscription metadata is public record data; its potentially secret URL lives in Credentials.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarSubscription {
    pub id: String,
    pub name: String,
}
impl CalendarSubscription {
    pub fn account(&self) -> String {
        format!("feed:{}", self.id)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarSource {
    pub id: String,
    pub account: String,
    pub name: String,
    pub writable: bool,
    pub tasks: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    #[default]
    Event,
    Task,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat {
    #[default]
    Never,
    Daily,
    Weekly,
    Monthly,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarEvent {
    pub repeat: Repeat,
    pub repeat_until: Option<chrono::NaiveDate>,
    pub id: String,
    pub account: String,
    pub calendar: String,
    pub remote_id: String,
    pub etag: String,
    pub title: String,
    pub description: String,
    pub original_description: String,
    pub description_editable: bool,
    pub location: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub all_day: bool,
    pub kind: EventKind,
    pub completed: bool,
    pub reminder_minutes: Option<i64>,
    pub url: String,
    pub writable: bool,
}
impl Default for CalendarEvent {
    fn default() -> Self {
        let start = Utc::now();
        Self {
            repeat: Repeat::Never,
            repeat_until: None,
            id: id(),
            account: String::new(),
            calendar: String::new(),
            remote_id: String::new(),
            etag: String::new(),
            title: String::new(),
            description: String::new(),
            original_description: String::new(),
            description_editable: true,
            location: String::new(),
            start,
            end: start + Duration::hours(1),
            all_day: false,
            kind: EventKind::Event,
            completed: false,
            reminder_minutes: None,
            url: String::new(),
            writable: true,
        }
    }
}
impl CalendarEvent {
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() {
            return Err("Title is required.".into());
        }
        if self.repeat != Repeat::Never
            && self
                .repeat_until
                .is_some_and(|d| d < self.start.with_timezone(&chrono::Local).date_naive())
        {
            return Err("Repeat end must be on or after the first event date.".into());
        }
        if self.end <= self.start {
            return Err("End must be after start.".into());
        }
        if self
            .reminder_minutes
            .is_some_and(|v| !(0..=525600).contains(&v))
        {
            return Err("Notification must be between 0 minutes and 365 days before.".into());
        }
        Ok(())
    }
    pub fn receipt(&self) -> String {
        format!(
            "{}:{}:{}",
            self.id,
            self.start.timestamp(),
            self.reminder_minutes.unwrap_or(0)
        )
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Organizer {
    pub version: u32,
    pub calendars: Vec<CalendarSource>,
    pub subscriptions: Vec<CalendarSubscription>,
    pub calendar_settings: CalendarSettings,
    pub events: Vec<CalendarEvent>,
    /// Preserve fields written by older integrations without activating them.
    #[serde(flatten)]
    pub legacy: BTreeMap<String, serde_json::Value>,
    pub delivered: BTreeMap<String, DateTime<Utc>>,
}
impl Default for Organizer {
    fn default() -> Self {
        Self {
            version: 3,
            calendars: vec![],
            subscriptions: vec![],
            calendar_settings: CalendarSettings::default(),
            events: vec![],
            legacy: Default::default(),
            delivered: Default::default(),
        }
    }
}
impl Organizer {
    pub fn load(root: &Path) -> Result<Self, String> {
        let p = root.join("organizer.json");
        if !p.exists() {
            return Ok(Self::default());
        }
        let mut organizer: Self =
            serde_json::from_slice(&std::fs::read(p).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Cannot read Calendar: {e}"))?;
        if organizer.version > 3 {
            return Err(
                "This Calendar file requires a newer Amigo version. Existing files were preserved."
                    .into(),
            );
        }
        organizer.version = 3;
        Ok(organizer)
    }
    pub fn save(&self, root: &Path) -> Result<(), String> {
        atomic_write(
            &root.join("organizer.json"),
            &serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
    }
    pub fn upsert(&mut self, event: CalendarEvent) {
        if let Some(old) = self.events.iter_mut().find(|e| e.id == event.id) {
            *old = event;
        } else {
            self.events.push(event);
        }
    }
    pub fn remove_subscription(&mut self, id: &str) {
        self.subscriptions.retain(|s| s.id != id);
        let account = format!("feed:{id}");
        self.calendars.retain(|c| c.account != account);
        self.events.retain(|e| e.account != account);
    }
    /// A successful, complete snapshot replaces only this account's remote cache.
    pub fn merge_calendar(
        &mut self,
        account: &str,
        mut events: Vec<CalendarEvent>,
        calendars: Vec<CalendarSource>,
    ) {
        for event in &mut events {
            if let Some(old) = self.events.iter().find(|e| e.id == event.id) {
                event.reminder_minutes = old.reminder_minutes;
            }
        }
        let now = Utc::now();
        let min = now - Duration::days(365);
        let max = now + Duration::days(730);
        self.events.retain(|e| {
            e.account != account
                || ((e.end < min || e.start > max)
                    && e.reminder_minutes.is_some()
                    && !self.delivered.contains_key(&e.receipt())
                    && !events.iter().any(|new| new.id == e.id))
        });
        self.events.extend(events);
        self.calendars.retain(|c| c.account != account);
        self.calendars.extend(calendars);
    }
    /// Local records and explicitly subscribed URL sources are the active calendars.
    pub fn source_enabled(&self, account: &str) -> bool {
        account.is_empty() || self.subscriptions.iter().any(|s| s.account() == account)
    }
    pub fn occurrences(&self, from: DateTime<Utc>, until: DateTime<Utc>) -> Vec<CalendarEvent> {
        self.events
            .iter()
            .filter(|e| self.source_enabled(&e.account))
            .flat_map(|e| crate::recurrence::expand(e, from, until))
            .collect()
    }
    pub fn notifications(&self, now: DateTime<Utc>) -> Vec<Notification> {
        let from = self
            .events
            .iter()
            .filter(|e| self.source_enabled(&e.account) && e.reminder_minutes.is_some())
            .map(|e| e.start)
            .min()
            .unwrap_or(now)
            - Duration::days(1);
        let occurrences = self.occurrences(from, now + Duration::days(366));
        let due: Vec<_> = occurrences
            .iter()
            .filter(|e| {
                !e.completed
                    && e.reminder_minutes
                        .is_some_and(|m| now >= e.start - Duration::minutes(m))
                    && !self.delivered.contains_key(&e.receipt())
            })
            .collect();
        let past: Vec<_> = due.iter().filter(|e| e.start < now).copied().collect();
        let mut result = Vec::new();
        if !past.is_empty() {
            let mut text = format!("Amigo · {} past tasks/events\n", past.len());
            for e in past.iter().take(12) {
                text.push_str(&format!(
                    "• {} — {}\n",
                    e.title.chars().take(180).collect::<String>(),
                    e.start.with_timezone(&chrono::Local).format("%b %d %H:%M")
                ));
            }
            if past.len() > 12 {
                text.push_str(&format!("…and {} more", past.len() - 12));
            }
            result.push(Notification {
                keys: past.iter().map(|e| e.receipt()).collect(),
                text,
            });
        }
        let upcoming: Vec<_> = due.into_iter().filter(|e| e.start >= now).collect();
        if !upcoming.is_empty() {
            let mut text = format!("Amigo · {} upcoming tasks/events\n", upcoming.len());
            for e in upcoming.iter().take(12) {
                text.push_str(&format!(
                    "• {} — {}\n",
                    e.title.chars().take(180).collect::<String>(),
                    e.start.with_timezone(&chrono::Local).format("%b %d %H:%M")
                ));
            }
            if upcoming.len() > 12 {
                text.push_str(&format!("…and {} more", upcoming.len() - 12));
            }
            result.push(Notification {
                keys: upcoming.iter().map(|e| e.receipt()).collect(),
                text,
            });
        }
        result
    }
}
#[derive(Clone, Debug)]
pub struct Notification {
    pub keys: Vec<String>,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retired_integration_data_survives_saves_but_is_not_shown_or_notified() {
        let root = std::env::temp_dir().join(id());
        let mut organizer: Organizer = serde_json::from_value(serde_json::json!({
            "version": 3,
            "accounts": [{"id": "old-account", "principal": "test@example.invalid"}],
            "drafts": [{"id": "old-draft", "body": "Keep this draft"}]
        }))
        .unwrap();
        let now = Utc::now();
        for account in ["", "old-account", "feed:subscribed", "feed:removed"] {
            organizer.events.push(CalendarEvent {
                account: account.into(),
                title: format!("Event from {account}"),
                start: now - Duration::hours(1),
                end: now,
                reminder_minutes: Some(10),
                ..Default::default()
            });
        }
        organizer.subscriptions.push(CalendarSubscription {
            id: "subscribed".into(),
            name: "Work".into(),
        });
        organizer.save(&root).unwrap();
        let reopened = Organizer::load(&root).unwrap();
        assert_eq!(reopened, organizer);
        assert_eq!(reopened.legacy["drafts"][0]["body"], "Keep this draft");
        let visible = reopened.occurrences(now - Duration::days(1), now + Duration::days(1));
        assert_eq!(visible.len(), 2);
        assert!(
            visible
                .iter()
                .all(|e| e.account.is_empty() || e.account == "feed:subscribed")
        );
        assert_eq!(reopened.notifications(now)[0].keys.len(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn receipts_survive_restart_and_missed_events_are_grouped() {
        let root = std::env::temp_dir().join(id());
        let now = Utc::now();
        let mut o = Organizer::default();
        for h in [1, 2, 3] {
            o.events.push(CalendarEvent {
                title: format!("Task {h}"),
                start: now - Duration::hours(h),
                reminder_minutes: Some(10),
                ..Default::default()
            });
        }
        let n = o.notifications(now);
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].keys.len(), 3);
        for k in &n[0].keys {
            o.delivered.insert(k.clone(), now);
        }
        o.save(&root).unwrap();
        assert!(
            Organizer::load(&root)
                .unwrap()
                .notifications(now)
                .is_empty()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn snapshot_preserves_reminders_and_other_accounts() {
        let mut o = Organizer::default();
        let e = CalendarEvent {
            account: "a".into(),
            reminder_minutes: Some(90),
            ..Default::default()
        };
        let mut remote = e.clone();
        remote.reminder_minutes = None;
        o.events.push(e);
        o.events.push(CalendarEvent {
            account: "b".into(),
            ..Default::default()
        });
        o.merge_calendar("a", vec![remote], vec![]);
        assert_eq!(o.events.len(), 2);
        assert_eq!(
            o.events
                .iter()
                .find(|e| e.account == "a")
                .unwrap()
                .reminder_minutes,
            Some(90)
        );
    }
    #[test]
    fn rejects_invalid_event() {
        assert!(CalendarEvent::default().validate().is_err());
    }
}

#[cfg(test)]
mod reminder_tests {
    use super::*;
    #[test]
    fn reminders_obey_offsets_retry_until_ack_and_ignore_completed() {
        let now = Utc::now();
        let mut o = Organizer::default();
        o.events.push(CalendarEvent {
            title: "Meeting".into(),
            start: now + Duration::hours(2),
            end: now + Duration::hours(3),
            reminder_minutes: Some(60),
            ..Default::default()
        });
        assert!(o.notifications(now).is_empty());
        let due = o.notifications(now + Duration::hours(1));
        assert_eq!(due.len(), 1);
        assert_eq!(o.notifications(now + Duration::hours(1)).len(), 1);
        for key in &due[0].keys {
            o.delivered.insert(key.clone(), now);
        }
        assert!(o.notifications(now + Duration::hours(1)).is_empty());
        o.delivered.clear();
        o.events[0].completed = true;
        assert!(o.notifications(now + Duration::days(1)).is_empty());
    }
    #[test]
    fn long_titles_are_bounded_and_newer_versions_are_preserved() {
        let mut o = Organizer::default();
        let now = Utc::now();
        for _ in 0..20 {
            o.events.push(CalendarEvent {
                title: "A".repeat(5000),
                start: now - Duration::hours(1),
                reminder_minutes: Some(5),
                ..Default::default()
            });
        }
        let summary = o.notifications(now);
        assert_eq!(summary.len(), 1);
        assert!(summary[0].text.chars().count() < 4000);
        assert_eq!(summary[0].keys.len(), 20);
        let root = std::env::temp_dir().join(id());
        o.version = 4;
        o.save(&root).unwrap();
        assert!(Organizer::load(&root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
