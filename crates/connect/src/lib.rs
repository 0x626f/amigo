//! Provider APIs run only on shell-owned worker threads. No GUI or persistence here.
pub mod calendar_feed;
use amigo_core::{credentials::Credentials, organizer::*};
use reqwest::blocking::{Client, RequestBuilder};
use std::{ops::Deref, sync::atomic::Ordering, time::Instant};
pub(crate) struct Api {
    http: Client,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
}
impl Deref for Api {
    type Target = Client;
    fn deref(&self) -> &Client {
        &self.http
    }
}
impl Api {
    fn check(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::Relaxed) {
            Err("Operation cancelled.".into())
        } else if Instant::now() > self.deadline {
            Err("The provider sync took too long. Try again.".into())
        } else {
            Ok(())
        }
    }
    fn json(&self, r: RequestBuilder) -> Result<Value, String> {
        self.check()?;
        json(r)
    }
}
use serde_json::Value;
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
#[derive(Clone)]
pub enum Request {
    CalendarFeed { id: String, url: String },
    Notify(Notification),
}
pub enum Response {
    CalendarFeed {
        id: String,
        events: Vec<CalendarEvent>,
    },
    Notified(Vec<String>),
}
pub struct Outcome {
    pub result: Result<Response, String>,
}
pub fn execute(request: Request, credentials: Credentials, cancel: Arc<AtomicBool>) -> Outcome {
    let result = (|| {
        let http = Client::builder()
            .timeout(Duration::from_secs(35))
            .connect_timeout(Duration::from_secs(10))
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "Cannot initialize HTTPS client")?;
        let http = Api {
            http,
            cancel: cancel.clone(),
            deadline: Instant::now() + Duration::from_secs(180),
        };
        match request {
            Request::CalendarFeed { id, url } => {
                let feed = calendar_feed::fetch(&http, &id, &url)?;
                Ok(Response::CalendarFeed {
                    id,
                    events: feed.events,
                })
            }
            Request::Notify(n) => {
                if !credentials.telegram_enabled
                    || credentials.telegram_token.is_empty()
                    || credentials.telegram_chat.is_empty()
                {
                    return Err("Configure Telegram bot token and chat ID first.".into());
                }
                let v=http.json(http.post(format!("https://api.telegram.org/bot{}/sendMessage",credentials.telegram_token)).json(&serde_json::json!({"chat_id":credentials.telegram_chat,"text":n.text.chars().take(4000).collect::<String>()})))?;
                if v["ok"] != true {
                    return Err(
                        "Telegram rejected the message. Check the bot and chat permissions.".into(),
                    );
                }
                Ok(Response::Notified(n.keys))
            }
        }
    })();
    Outcome { result }
}
pub(crate) fn json(request: RequestBuilder) -> Result<Value, String> {
    let response = request.send().map_err(|e| {
        if e.is_timeout() {
            "Request timed out. Try again.".to_owned()
        } else {
            "Cannot reach the provider. Check your connection.".to_owned()
        }
    })?;
    let status = response.status();
    let value: Value = response.json().unwrap_or(Value::Null);
    if !status.is_success() {
        let message = value["error"]["message"]
            .as_str()
            .or_else(|| value["error_description"].as_str())
            .unwrap_or("The provider rejected the request.");
        return Err(format!(
            "Provider {}: {}",
            status.as_u16(),
            message.chars().take(350).collect::<String>()
        ));
    }
    if value.is_null() {
        return Err(
            "The provider returned an unreadable response. Refresh before trying again.".into(),
        );
    }
    Ok(value)
}
