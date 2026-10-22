//! Serial calendar and notification workers. Stale subscription results are discarded.
use super::*;
impl Amigo {
    pub(in crate::app) fn tick_organizer(&mut self, ctx: &egui::Context) {
        let outcome = self
            .productivity
            .job
            .as_ref()
            .and_then(|j| match j.receiver.try_recv() {
                Ok(outcome) => Some(outcome),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Outcome {
                    result: Err("The calendar worker stopped unexpectedly. Try again.".into()),
                }),
            });
        if let Some(outcome) = outcome {
            let job = self.productivity.job.take().unwrap();
            if !job.cancel.load(Ordering::Relaxed) && self.request_allowed(&job.request) {
                match outcome.result {
                    Ok(Response::CalendarFeed { id, events }) => {
                        if let Request::CalendarFeed { id: requested, .. } = &job.request
                            && requested == &id
                            && let Some(subscription) = self
                                .organizer
                                .subscriptions
                                .iter()
                                .find(|s| s.id == id)
                                .cloned()
                        {
                            let source = CalendarSource {
                                id,
                                account: subscription.account(),
                                name: subscription.name.clone(),
                                writable: false,
                                tasks: false,
                            };
                            if self
                                .error
                                .starts_with(&format!("Calendar “{}”: ", subscription.name))
                            {
                                self.error.clear();
                            }
                            self.organizer.merge_calendar(
                                &subscription.account(),
                                events,
                                vec![source],
                            );
                        }
                    }
                    Ok(Response::Notified(keys)) => {
                        if matches!(job.request, Request::Notify(_)) {
                            for key in keys {
                                self.organizer.delivered.insert(key, chrono::Utc::now());
                            }
                        }
                    }
                    Err(error) => {
                        let error = if let Request::CalendarFeed { id, .. } = &job.request {
                            self.organizer
                                .subscriptions
                                .iter()
                                .find(|s| s.id == *id)
                                .map(|s| format!("Calendar “{}”: {error}", s.name))
                                .unwrap_or(error)
                        } else {
                            error
                        };
                        self.error = error.clone();
                        if let Some(settings) = &mut self.productivity.settings {
                            settings.error = error;
                        }
                    }
                }
                self.save_organizer();
            }
        }
        let now = Instant::now();
        if now.duration_since(self.productivity.last_sync)
            >= Duration::from_secs(
                self.organizer
                    .calendar_settings
                    .refresh_interval_seconds
                    .clamp(1, 86_400),
            )
        {
            self.sync_calendars();
        }
        if !self.productivity.busy()
            && now.duration_since(self.productivity.last_notify) >= Duration::from_secs(30)
        {
            self.productivity.last_notify = now;
            if self.productivity.credentials.telegram_enabled
                && let Some(notification) = self
                    .organizer
                    .notifications(chrono::Utc::now())
                    .into_iter()
                    .next()
            {
                self.queue_request(Request::Notify(notification));
            }
        }
        while self.productivity.job.is_none()
            && let Some(request) = self.productivity.queue.pop_front()
        {
            if !self.request_allowed(&request) {
                continue;
            }
            let (tx, rx) = mpsc::channel();
            let credentials = self.productivity.credentials.clone();
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            let r = request.clone();
            let repaint = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(amigo_connect::execute(r, credentials, worker_cancel));
                repaint.request_repaint();
            });
            self.productivity.job = Some(Job {
                request,
                receiver: rx,
                cancel,
            });
        }
        ctx.request_repaint_after(if self.productivity.busy() {
            Duration::from_millis(150)
        } else {
            Duration::from_secs(1)
        });
    }
}
