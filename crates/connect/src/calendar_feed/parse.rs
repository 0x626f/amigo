//! RFC 5545 parsing, recurrence expansion, exception replacement, and stable occurrence identities.
use amigo_core::organizer::{CalendarEvent, EventKind};
use chrono::{DateTime, Duration, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};
use ical::{parser::ical::component::IcalCalendar, property::Property};
use rrule::{RRule, RRuleSet, Tz, Unvalidated};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::BufReader, sync::OnceLock};
const MAX_EVENTS: usize = 20000;
fn property<'a>(p: &'a [Property], key: &str) -> Option<&'a Property> {
    p.iter().find(|p| p.name == key)
}
fn value<'a>(p: &'a [Property], key: &str) -> Option<&'a str> {
    property(p, key).and_then(|p| p.value.as_deref())
}
fn param<'a>(p: &'a Property, key: &str) -> Option<&'a str> {
    p.params
        .as_ref()?
        .iter()
        .find(|(k, _)| k == key)?
        .1
        .first()
        .map(String::as_str)
}
fn text(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(c) => out.push(c),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}
#[derive(Default)]
struct Zones {
    aliases: BTreeMap<String, String>,
    fixed: BTreeMap<String, FixedOffset>,
    default: Option<String>,
}
fn iana(name: &str) -> Option<Tz> {
    if let Ok(zone) = name.parse::<chrono_tz::Tz>() {
        return Some(zone.into());
    }
    static WINDOWS: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    let map = WINDOWS.get_or_init(|| {
        serde_json::from_str(include_str!("../../data/windows-zones.json"))
            .expect("bundled CLDR mapping")
    });
    if let Some(name) = map.get(name)
        && let Ok(zone) = name.parse::<chrono_tz::Tz>()
    {
        return Some(zone.into());
    }
    if let Some(at) = name.find("/Tzfile/") {
        return iana(&name[at + 8..]);
    }
    None
}
fn offset(s: &str) -> Option<FixedOffset> {
    let sign = if s.starts_with('-') { -1 } else { 1 };
    let s = s.strip_prefix(['+', '-'])?;
    let h = s.get(..2)?.parse::<i32>().ok()?;
    let m = s.get(2..4)?.parse::<i32>().ok()?;
    FixedOffset::east_opt(sign * (h * 3600 + m * 60))
}
impl Zones {
    fn new(c: &IcalCalendar) -> Self {
        let mut result = Self {
            default: value(&c.properties, "X-WR-TIMEZONE").map(String::from),
            ..Default::default()
        };
        for z in &c.timezones {
            let Some(id) = value(&z.properties, "TZID") else {
                continue;
            };
            if let Some(location) = value(&z.properties, "X-LIC-LOCATION") {
                result.aliases.insert(id.into(), location.into());
            }
            let offsets: Vec<_> = z
                .transitions
                .iter()
                .filter_map(|t| value(&t.properties, "TZOFFSETTO"))
                .collect();
            if let Some(first) = offsets.first()
                && offsets.iter().all(|o| o == first)
                && let Some(off) = offset(first)
            {
                result.fixed.insert(id.into(), off);
            }
        }
        result
    }
    fn date(&self, p: &Property, fallback: Option<Tz>) -> Result<DateTime<Tz>, String> {
        let v = p
            .value
            .as_deref()
            .ok_or("Calendar contains a missing date.")?;
        let all_day = v.len() == 8;
        let naive = if all_day {
            NaiveDate::parse_from_str(v, "%Y%m%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
        } else {
            NaiveDateTime::parse_from_str(v.trim_end_matches('Z'), "%Y%m%dT%H%M%S")
                .ok()
                .or_else(|| {
                    NaiveDateTime::parse_from_str(v.trim_end_matches('Z'), "%Y%m%dT%H%M").ok()
                })
        }
        .ok_or("Calendar contains an invalid date or time.")?;
        if all_day {
            return Tz::LOCAL
                .from_local_datetime(&naive)
                .earliest()
                .ok_or("Invalid local all-day calendar date.".into());
        }
        if v.ends_with('Z') {
            return Ok(naive.and_utc().with_timezone(&Tz::UTC));
        }
        let zone = param(p, "TZID").or(self.default.as_deref());
        let tz = if let Some(name) = zone {
            let name = self.aliases.get(name).map(String::as_str).unwrap_or(name);
            if let Some(tz) = iana(name) {
                tz
            } else if let Some(off) = self.fixed.get(name) {
                return off
                    .from_local_datetime(&naive)
                    .earliest()
                    .map(|d| d.with_timezone(&Tz::UTC))
                    .ok_or("Invalid calendar time zone.".into());
            } else {
                return Err(
                    "The feed uses an unsupported time zone. Cached events were kept.".into(),
                );
            }
        } else {
            fallback.unwrap_or(Tz::LOCAL)
        };
        tz.from_local_datetime(&naive)
            .earliest()
            .ok_or("Calendar contains a time that does not exist in its time zone.".into())
    }
}
fn duration(s: &str) -> Result<Duration, String> {
    if !s.starts_with('P') {
        return Err("Unsupported calendar duration.".into());
    }
    let mut number = String::new();
    let mut total = 0i64;
    let mut time = false;
    for c in s[1..].chars() {
        if c == 'T' {
            time = true;
            continue;
        }
        if c.is_ascii_digit() {
            number.push(c);
            continue;
        }
        let n = number
            .parse::<i64>()
            .map_err(|_| "Invalid calendar duration.")?;
        number.clear();
        let factor = match (c, time) {
            ('W', false) => 604800,
            ('D', false) => 86400,
            ('H', true) => 3600,
            ('M', true) => 60,
            ('S', true) => 1,
            _ => return Err("Unsupported calendar duration.".into()),
        };
        total = total
            .checked_add(
                n.checked_mul(factor)
                    .ok_or("Calendar duration is too large.")?,
            )
            .ok_or("Calendar duration is too large.")?;
    }
    if !number.is_empty() || total <= 0 || total > 366 * 86400 {
        return Err("Invalid or excessive calendar duration.".into());
    }
    Ok(Duration::seconds(total))
}
fn key(uid: &str, slot: Option<DateTime<Tz>>, all_day: bool) -> String {
    let mut hash = Sha256::new();
    hash.update(uid.as_bytes());
    if let Some(slot) = slot {
        hash.update(b"\0");
        hash.update(
            if all_day {
                slot.format("%Y%m%d").to_string()
            } else {
                slot.with_timezone(&Utc).to_rfc3339()
            }
            .as_bytes(),
        );
    }
    format!("{:x}", hash.finalize())
}
fn list_dates(p: &Property, zones: &Zones, tz: Tz) -> Result<Vec<DateTime<Tz>>, String> {
    p.value
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .map(|v| {
            let mut one = p.clone();
            one.value = Some(v.into());
            zones.date(&one, Some(tz))
        })
        .collect()
}
fn rule(raw: &str, start: DateTime<Tz>) -> Result<RRule, String> {
    let mut parts = vec![];
    for part in raw.split(';') {
        if let Some(until) = part.strip_prefix("UNTIL=")
            && until.len() == 8
        {
            let date = NaiveDate::parse_from_str(until, "%Y%m%d")
                .map_err(|_| "Invalid recurrence end date.")?;
            let end = start
                .timezone()
                .from_local_datetime(&date.and_hms_opt(23, 59, 59).unwrap())
                .earliest()
                .ok_or("Invalid recurrence end time.")?;
            parts.push(format!(
                "UNTIL={}",
                end.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ")
            ));
        } else {
            parts.push(part.into());
        }
    }
    parts
        .join(";")
        .parse::<RRule<Unvalidated>>()
        .and_then(|r| r.validate(start))
        .map_err(|_| "The calendar contains an unsupported or invalid recurrence rule.".into())
}
#[derive(Clone)]
struct Item {
    p: Vec<Property>,
    task: bool,
}
fn event(
    item: &Item,
    subscription: &str,
    uid: &str,
    start: DateTime<Tz>,
    end: DateTime<Tz>,
    identity: Option<DateTime<Tz>>,
) -> CalendarEvent {
    let p = &item.p;
    let all_day = property(p, "DTSTART")
        .or_else(|| property(p, "DUE"))
        .and_then(|p| p.value.as_deref())
        .is_some_and(|v| v.len() == 8);
    let identity_is_date = property(p, "RECURRENCE-ID")
        .and_then(|p| p.value.as_deref())
        .map(|v| v.len() == 8)
        .unwrap_or(all_day);
    let remote_id = key(uid, identity, identity_is_date);
    let description = text(value(p, "DESCRIPTION").unwrap_or_default());
    let url = value(p, "URL")
        .and_then(|s| reqwest::Url::parse(s).ok())
        .filter(|u| matches!(u.scheme(), "https" | "http"))
        .map(String::from)
        .unwrap_or_default();
    CalendarEvent {
        id: format!("feed:{subscription}:{remote_id}"),
        account: format!("feed:{subscription}"),
        calendar: subscription.into(),
        remote_id,
        title: text(
            value(p, "SUMMARY")
                .filter(|v| !v.is_empty())
                .unwrap_or("Untitled Event"),
        ),
        original_description: description.clone(),
        description,
        location: text(value(p, "LOCATION").unwrap_or_default()),
        start: start.with_timezone(&Utc),
        end: end.with_timezone(&Utc),
        all_day,
        kind: if item.task {
            EventKind::Task
        } else {
            EventKind::Event
        },
        completed: value(p, "STATUS") == Some("COMPLETED") || value(p, "COMPLETED").is_some(),
        writable: false,
        description_editable: false,
        url,
        ..Default::default()
    }
}
fn event_end(item: &Item, zones: &Zones, start: DateTime<Tz>) -> Result<DateTime<Tz>, String> {
    let p = &item.p;
    if let Some(end) = property(p, if item.task { "DUE" } else { "DTEND" }) {
        let end = zones.date(end, Some(start.timezone()))?;
        if end > start {
            return Ok(end);
        }
        if !item.task {
            return Err("Calendar event ends before it starts.".into());
        }
    }
    let all_day = property(p, "DTSTART")
        .or_else(|| property(p, "DUE"))
        .and_then(|p| p.value.as_deref())
        .is_some_and(|v| v.len() == 8);
    let elapsed = if let Some(d) = value(p, "DURATION") {
        duration(d)?
    } else if all_day {
        Duration::days(1)
    } else {
        Duration::minutes(1)
    };
    if all_day {
        return start
            .timezone()
            .from_local_datetime(&(start.naive_local() + elapsed))
            .earliest()
            .ok_or("Invalid all-day end date.".into());
    }
    start
        .checked_add_signed(elapsed)
        .ok_or("Calendar duration is too large.".into())
}
fn add(
    out: &mut BTreeMap<String, CalendarEvent>,
    e: CalendarEvent,
    from: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<(), String> {
    if e.start < until && e.end > from {
        out.insert(e.id.clone(), e);
        if out.len() > MAX_EVENTS {
            return Err("Calendar feed has too many events to synchronize safely.".into());
        }
    }
    Ok(())
}
pub(super) fn calendar(
    input: &str,
    subscription: &str,
    from: DateTime<Utc>,
    until: DateTime<Utc>,
    check: impl Fn() -> Result<(), String>,
) -> Result<Vec<CalendarEvent>, String> {
    let input = input.trim_start_matches('\u{feff}').trim();
    if !input.starts_with("BEGIN:VCALENDAR") || !input.ends_with("END:VCALENDAR") {
        return Err("This URL is not an iCalendar feed. Use the .ics subscription link, not the browser calendar page.".into());
    }
    let mut out = BTreeMap::new();
    let mut count = 0;
    for parsed in ical::IcalParser::new(BufReader::new(input.as_bytes())) {
        check()?;
        let c = parsed.map_err(
            |_| "The server returned a malformed iCalendar feed. Cached events were kept.",
        )?;
        count += 1;
        let zones = Zones::new(&c);
        let mut groups: BTreeMap<String, BTreeMap<String, Item>> = BTreeMap::new();
        for item in c
            .events
            .into_iter()
            .map(|e| Item {
                p: e.properties,
                task: false,
            })
            .chain(c.todos.into_iter().map(|e| Item {
                p: e.properties,
                task: true,
            }))
        {
            if item.task
                && property(&item.p, "DTSTART")
                    .or_else(|| property(&item.p, "DUE"))
                    .is_none()
            {
                continue;
            }
            let uid = value(&item.p, "UID")
                .filter(|v| !v.is_empty())
                .ok_or("A calendar event is missing its UID. Cached events were kept.")?
                .to_string();
            let identity = if let Some(p) = property(&item.p, "RECURRENCE-ID") {
                if param(p, "RANGE").is_some() {
                    return Err("This feed contains an unsupported recurrence-range change.".into());
                }
                zones.date(p, None)?.with_timezone(&Utc).to_rfc3339()
            } else {
                String::new()
            };
            let entries = groups.entry(uid).or_default();
            let version = |p: &[Property]| {
                (
                    value(p, "SEQUENCE")
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(0),
                    value(p, "LAST-MODIFIED")
                        .or_else(|| value(p, "DTSTAMP"))
                        .unwrap_or_default()
                        .to_string(),
                )
            };
            if entries
                .get(&identity)
                .is_none_or(|old| version(&item.p) >= version(&old.p))
            {
                entries.insert(identity, item);
            }
        }
        for (uid, mut entries) in groups {
            check()?;
            let master = entries.remove("");
            let mut replacements = BTreeMap::new();
            for item in entries.into_values() {
                let identity = zones.date(property(&item.p, "RECURRENCE-ID").unwrap(), None)?;
                replacements.insert(identity.with_timezone(&Utc), (identity, item));
            }
            let template = master.clone();
            if let Some(master) = master {
                if value(&master.p, "STATUS") == Some("CANCELLED") {
                    continue;
                }
                let p = property(&master.p, "DTSTART")
                    .or_else(|| property(&master.p, "DUE"))
                    .ok_or("Calendar event has no start date.")?;
                let start = zones.date(p, None)?;
                let end = event_end(&master, &zones, start)?;
                let recurring = master
                    .p
                    .iter()
                    .any(|p| matches!(p.name.as_str(), "RRULE" | "RDATE" | "EXDATE" | "EXRULE"));
                if recurring {
                    let mut set = RRuleSet::new(start).rdate(start);
                    for p in &master.p {
                        match p.name.as_str() {
                            "RRULE" => {
                                set =
                                    set.rrule(rule(p.value.as_deref().unwrap_or_default(), start)?)
                            }
                            "EXRULE" => {
                                set =
                                    set.exrule(rule(p.value.as_deref().unwrap_or_default(), start)?)
                            }
                            "EXDATE" => {
                                for date in list_dates(p, &zones, start.timezone())? {
                                    set = set.exdate(date);
                                }
                            }
                            "RDATE" => {
                                for date in list_dates(p, &zones, start.timezone())? {
                                    set = set.rdate(date);
                                }
                            }
                            _ => {}
                        }
                    }
                    let dates = set
                        .after((from - (end - start)).with_timezone(&Tz::UTC))
                        .before(until.with_timezone(&Tz::UTC))
                        .all(MAX_EVENTS as u16);
                    if dates.limited {
                        return Err("Calendar recurrence exceeds the safe expansion limit. Cached events were kept.".into());
                    }
                    let all_day = p.value.as_deref().is_some_and(|v| v.len() == 8);
                    let days = (end.date_naive() - start.date_naive()).num_days();
                    for slot in dates.dates {
                        check()?;
                        if replacements.contains_key(&slot.with_timezone(&Utc)) {
                            continue;
                        }
                        let finish = if all_day {
                            slot.timezone()
                                .from_local_datetime(&(slot.naive_local() + Duration::days(days)))
                                .earliest()
                                .ok_or("Invalid recurring all-day end date.")?
                        } else {
                            slot + (end - start)
                        };
                        add(
                            &mut out,
                            event(&master, subscription, &uid, slot, finish, Some(slot)),
                            from,
                            until,
                        )?;
                    }
                } else {
                    add(
                        &mut out,
                        event(&master, subscription, &uid, start, end, None),
                        from,
                        until,
                    )?;
                }
            }
            for (_, (identity, mut item)) in replacements {
                if let Some(base) = &template {
                    for name in ["SUMMARY", "DESCRIPTION", "LOCATION", "URL"] {
                        if property(&item.p, name).is_none()
                            && let Some(p) = property(&base.p, name)
                        {
                            item.p.push(p.clone());
                        }
                    }
                }
                if value(&item.p, "STATUS") == Some("CANCELLED") {
                    continue;
                }
                let Some(p) = property(&item.p, "DTSTART").or_else(|| property(&item.p, "DUE"))
                else {
                    return Err("Calendar exception has no start date.".into());
                };
                let start = zones.date(p, None)?;
                let end = event_end(&item, &zones, start)?;
                add(
                    &mut out,
                    event(&item, subscription, &uid, start, end, Some(identity)),
                    from,
                    until,
                )?;
            }
        }
    }
    if count == 0 {
        return Err("The URL returned no iCalendar data.".into());
    }
    Ok(out.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};
    fn parse(body: &str) -> Result<Vec<CalendarEvent>, String> {
        calendar(
            &format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{body}\r\nEND:VCALENDAR\r\n"),
            "test-feed",
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap(),
            || Ok(()),
        )
    }
    #[test]
    fn parses_folded_text_all_day_and_due_tasks() {
        let events=parse("BEGIN:VEVENT\r\nUID:day\r\nDTSTART;VALUE=DATE:20261024\r\nDTEND;VALUE=DATE:20261026\r\nSUMMARY:Weekend\\, away\r\nDESCRIPTION:First line\\nLong descrip\r\n tion\\; with UTF-8 café\r\nEND:VEVENT\r\nBEGIN:VTODO\r\nUID:todo\r\nDUE:20261027T120000Z\r\nSUMMARY:Pay invoice\r\nSTATUS:COMPLETED\r\nEND:VTODO").unwrap();
        assert_eq!(events.len(), 2);
        let e = events.iter().find(|e| e.all_day).unwrap();
        assert_eq!(e.title, "Weekend, away");
        assert_eq!(
            e.description,
            "First line\nLong description; with UTF-8 café"
        );
        assert_eq!(
            (e.end.with_timezone(&chrono::Local).date_naive()
                - e.start.with_timezone(&chrono::Local).date_naive())
            .num_days(),
            2
        );
        assert!(!e.writable);
        let task = events.iter().find(|e| e.kind == EventKind::Task).unwrap();
        assert!(task.completed);
    }
    #[test]
    fn recurring_exclusions_additions_moved_and_cancelled_exceptions_keep_ids() {
        let master = "BEGIN:VEVENT\r\nUID:series\r\nDTSTART;TZID=America/New_York:20261019T090000\r\nDTEND;TZID=America/New_York:20261019T100000\r\nSUMMARY:Standup\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE;COUNT=4\r\nEXDATE;TZID=America/New_York:20261021T090000\r\nRDATE;TZID=America/New_York:20261030T090000\r\nEND:VEVENT";
        let original = parse(master).unwrap();
        assert_eq!(original.len(), 4);
        let id = original
            .iter()
            .find(|e| e.start.day() == 26)
            .unwrap()
            .id
            .clone();
        let modified = format!(
            "{master}\r\nBEGIN:VEVENT\r\nUID:series\r\nRECURRENCE-ID;TZID=America/New_York:20261026T090000\r\nDTSTART;TZID=America/New_York:20261026T150000\r\nDTEND;TZID=America/New_York:20261026T160000\r\nSUMMARY:Moved Standup\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:series\r\nRECURRENCE-ID;TZID=America/New_York:20261028T090000\r\nSTATUS:CANCELLED\r\nEND:VEVENT"
        );
        let updated = parse(&modified).unwrap();
        assert_eq!(updated.len(), 3);
        let moved = updated.iter().find(|e| e.title == "Moved Standup").unwrap();
        assert_eq!(moved.id, id);
        assert_eq!(moved.start.hour(), 19);
        assert!(!updated.iter().any(|e| e.start.day() == 28));
    }
    #[test]
    fn microsoft_and_iana_time_zones_preserve_recurrence_through_dst() {
        let body = |zone: &str| {
            format!(
                "BEGIN:VEVENT\r\nUID:dst\r\nDTSTART;TZID={zone}:20261025T090000\r\nDURATION:PT1H\r\nSUMMARY:Weekly\r\nRRULE:FREQ=WEEKLY;COUNT=3\r\nEND:VEVENT"
            )
        };
        let mut iana = parse(&body("America/New_York")).unwrap();
        let mut windows = parse(&body("Eastern Standard Time")).unwrap();
        iana.sort_by_key(|e| e.start);
        windows.sort_by_key(|e| e.start);
        assert_eq!(iana.len(), 3);
        assert_eq!(
            iana.iter().map(|e| e.start).collect::<Vec<_>>(),
            windows.iter().map(|e| e.start).collect::<Vec<_>>()
        );
        assert_eq!(iana[0].start.hour(), 13);
        assert_eq!(iana[1].start.hour(), 14);
        assert_eq!((iana[2].end - iana[2].start).num_minutes(), 60);
    }
    #[test]
    fn invalid_html_unsupported_zones_and_rules_fail_instead_of_clearing_cache() {
        assert!(
            parse(
                "BEGIN:VEVENT\r\nUID:a\r\nDTSTART;TZID=Unknown/Zone:20261025T090000\r\nEND:VEVENT"
            )
            .is_err()
        );
        assert!(parse("BEGIN:VEVENT\r\nUID:a\r\nDTSTART:20261025T090000Z\r\nRRULE:NOT_A_RULE\r\nEND:VEVENT").is_err());
        assert!(
            calendar(
                "<html>Sign in</html>",
                "a",
                Utc::now(),
                Utc::now() + Duration::days(1),
                || Ok(())
            )
            .unwrap_err()
            .contains("subscription link")
        );
        assert!(parse("BEGIN:VEVENT\r\nUID:broken\r\nDTSTART:20261025T090000Z").is_err());
        assert!(parse("").unwrap().is_empty());
    }
    #[test]
    fn newest_sequence_wins_and_one_off_ids_survive_rescheduling() {
        let event = |date: &str, sequence: i32, title: &str| {
            format!(
                "BEGIN:VEVENT\r\nUID:stable\r\nDTSTART:{date}T090000Z\r\nDURATION:PT30M\r\nSEQUENCE:{sequence}\r\nSUMMARY:{title}\r\nEND:VEVENT"
            )
        };
        let a = parse(&event("20261019", 1, "Original")).unwrap();
        let b = parse(&format!(
            "{}\r\n{}",
            event("20261020", 2, "Updated"),
            event("20261019", 1, "Original")
        ))
        .unwrap();
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].title, "Updated");
        assert_eq!(a[0].id, b[0].id);
        assert_ne!(a[0].start, b[0].start);
    }
}
