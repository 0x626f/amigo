//! Read-only iCalendar URL subscriptions. URLs are credentials and never enter errors or records.
mod parse;
use super::Api;
use amigo_core::organizer::CalendarEvent;
use chrono::{Duration, Utc};
use reqwest::{Url, blocking::Client};
use std::{io::Read, time::Duration as StdDuration};
pub struct FeedSnapshot {
    pub events: Vec<CalendarEvent>,
}
const MAX_BYTES: u64 = 16 * 1024 * 1024;
pub fn normalize_url(value: &str) -> Result<String, String> {
    let value = value.trim();
    let value = if let Some(rest) = value
        .strip_prefix("webcal://")
        .or_else(|| value.strip_prefix("webcals://"))
    {
        format!("https://{rest}")
    } else {
        value.into()
    };
    let mut url = Url::parse(&value)
        .map_err(|_| "Enter a valid https://, http://, or webcal:// calendar URL.")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Use an HTTP(S) iCalendar subscription URL.".into());
    }
    url.set_fragment(None);
    Ok(url.to_string())
}
pub(super) fn fetch(api: &Api, id: &str, value: &str) -> Result<FeedSnapshot, String> {
    let url = normalize_url(value)?;
    let client = Client::builder()
        .timeout(StdDuration::from_secs(35))
        .connect_timeout(StdDuration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Cannot initialize calendar connection.")?;
    let text = download(&client, &url, || api.check())?;
    let now = Utc::now();
    let events = parse::calendar(
        &text,
        id,
        now - Duration::days(365),
        now + Duration::days(730),
        || api.check(),
    )?;
    Ok(FeedSnapshot { events })
}
fn download(
    client: &Client,
    value: &str,
    check: impl Fn() -> Result<(), String>,
) -> Result<String, String> {
    let mut url = Url::parse(value).map_err(|_| "Invalid calendar URL.")?;
    for _ in 0..=5 {
        check()?;
        let response = client
            .get(url.clone())
            .header("Accept", "text/calendar, application/ics;q=0.9, */*;q=0.1")
            .send()
            .map_err(|_| "Cannot download the calendar. Check its URL and your connection.")?;
        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|h| h.to_str().ok())
                .ok_or("Calendar server returned an invalid redirect.")?;
            let next = url
                .join(location)
                .map_err(|_| "Invalid calendar redirect.")?;
            if url.scheme() == "https" && next.scheme() != "https" {
                return Err("A secure calendar redirected to an insecure address.".into());
            }
            url = Url::parse(&normalize_url(next.as_str())?)
                .map_err(|_| "Invalid calendar redirect.")?;
            continue;
        }
        if !status.is_success() {
            return Err(format!(
                "Calendar download returned HTTP {}. Check the subscription link and sharing permissions.",
                status.as_u16()
            ));
        }
        if response.content_length().is_some_and(|n| n > MAX_BYTES) {
            return Err("The calendar feed exceeds the 16 MiB limit.".into());
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Calendar download was interrupted. Cached events were kept.")?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("The calendar feed exceeds the 16 MiB limit.".into());
        }
        return String::from_utf8(bytes)
            .map_err(|_| "The calendar is not a UTF-8 iCalendar feed.".into());
    }
    Err("Calendar URL redirected too many times.".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_subscription_schemes_and_keeps_private_tokens() {
        assert_eq!(
            normalize_url(" webcal://example.com/private.ics?token=secret#ignored ").unwrap(),
            "https://example.com/private.ics?token=secret"
        );
        assert!(normalize_url("https://example.com/feed").is_ok());
        assert!(normalize_url("file:///tmp/calendar.ics").is_err());
        assert!(normalize_url("https://user:password@example.com/feed").is_err());
    }
    #[test]
    #[ignore = "requires a local TCP listener"]
    fn downloads_follow_redirects_and_do_not_expose_private_urls_on_failure() {
        use std::{io::Write, net::TcpListener};
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            for n in 0..3 {
                let (mut stream, _) = server.accept().unwrap();
                let mut buf = [0; 1024];
                let _ = stream.read(&mut buf);
                let response = match n {
                    0 => {
                        "HTTP/1.1 302 Found\r\nLocation: /real.ics\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    }
                    1 => {
                        "HTTP/1.1 200 OK\r\nContent-Length: 32\r\nConnection: close\r\n\r\nBEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n"
                    }
                    _ => "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                };
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        let client = Client::builder()
            .no_proxy()
            .timeout(StdDuration::from_secs(3))
            .build()
            .unwrap();
        let feed = download(
            &client,
            &format!("http://{addr}/private-secret.ics"),
            || Ok(()),
        )
        .unwrap();
        assert!(feed.contains("VCALENDAR"));
        let error = download(
            &client,
            &format!("http://{addr}/private-secret.ics"),
            || Ok(()),
        )
        .unwrap_err();
        assert!(error.contains("403"));
        assert!(!error.contains("private-secret"));
        worker.join().unwrap();
    }
}
