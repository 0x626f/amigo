//! Local repeat rules preserve wall-clock time across daylight saving changes.
use crate::organizer::{CalendarEvent, Repeat};
use chrono::{DateTime, Datelike, Duration, Local, Months, TimeZone, Utc};
pub fn expand(
    event: &CalendarEvent,
    from: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Vec<CalendarEvent> {
    if event.repeat == Repeat::Never || !event.account.is_empty() {
        return if event.start < until && event.end > from {
            vec![event.clone()]
        } else {
            vec![]
        };
    }
    let anchor = event.start.with_timezone(&Local).naive_local();
    let anchor_end = event.end.with_timezone(&Local).naive_local();
    let from_date = from.with_timezone(&Local).date_naive();
    let days = (from_date - anchor.date()).num_days().max(0);
    let mut index = match event.repeat {
        Repeat::Daily => days.saturating_sub(1),
        Repeat::Weekly => (days / 7).saturating_sub(1),
        Repeat::Monthly => ((from_date.year() - anchor.year()) as i64 * 12
            + from_date.month() as i64
            - anchor.month() as i64)
            .saturating_sub(1)
            .max(0),
        Repeat::Never => 0,
    };
    let mut result = vec![];
    loop {
        let date = match event.repeat {
            Repeat::Daily => anchor.date().checked_add_signed(Duration::days(index)),
            Repeat::Weekly => anchor
                .date()
                .checked_add_signed(Duration::days(index.saturating_mul(7))),
            Repeat::Monthly => anchor.date().checked_add_months(Months::new(index as u32)),
            Repeat::Never => None,
        };
        let Some(date) = date else {
            break;
        };
        if event.repeat_until.is_some_and(|end| date > end) {
            break;
        }
        if date > until.with_timezone(&Local).date_naive() + Duration::days(1) {
            break;
        }
        let delta = date - anchor.date();
        if let (Some(s), Some(e)) = (
            Local.from_local_datetime(&(anchor + delta)).earliest(),
            Local.from_local_datetime(&(anchor_end + delta)).earliest(),
        ) {
            let mut occurrence = event.clone();
            occurrence.start = s.with_timezone(&Utc);
            occurrence.end = e.with_timezone(&Utc);
            if occurrence.start < until && occurrence.end > from {
                result.push(occurrence);
            }
        }
        index += 1;
        if result.len() > 20000 {
            break;
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monthly_uses_original_day_after_short_month_and_stops_at_until() {
        let start = Local
            .with_ymd_and_hms(2026, 1, 31, 10, 0, 0)
            .earliest()
            .unwrap()
            .with_timezone(&Utc);
        let e = CalendarEvent {
            title: "Monthly".into(),
            start,
            end: start + Duration::hours(1),
            repeat: Repeat::Monthly,
            repeat_until: chrono::NaiveDate::from_ymd_opt(2026, 3, 31),
            ..Default::default()
        };
        let occurrences = expand(&e, start, start + Duration::days(150));
        assert_eq!(occurrences.len(), 3);
        assert_eq!(occurrences[1].start.with_timezone(&Local).day(), 28);
        assert_eq!(occurrences[2].start.with_timezone(&Local).day(), 31);
        assert_ne!(occurrences[0].receipt(), occurrences[1].receipt());
    }
}
