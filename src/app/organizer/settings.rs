//! Calendar URL subscriptions, refresh timing, and Telegram settings.
use super::*;
use amigo_ui::forms::field_label;
impl Amigo {
    pub(in crate::app) fn calendar_settings_in(&mut self, ui: &mut egui::Ui) -> bool {
        let ctx = ui.ctx().clone();
        self.calendar_settings_mode(&ctx, Some(ui))
    }
    fn calendar_settings_mode(
        &mut self,
        ctx: &egui::Context,
        embedded: Option<&mut egui::Ui>,
    ) -> bool {
        let available = !self.productivity.credentials_blocked && !self.productivity.blocked;
        let Some(settings) = &mut self.productivity.settings else {
            return false;
        };
        let keys = design::ActionKeys::capture(ctx);
        let mut close = false;
        let mut save = false;
        let mut remove_subscription = None;
        let mut refresh = false;
        let data_action: Option<design::DataAction> = None;
        let inline = embedded.is_some();
        let mut body = |ui: &mut egui::Ui| {
            if inline {
                ui.heading("Calendar Settings");
            } else {
                ui.set_width(design::dialog_width(ctx, 680.));
                close = design::settings_heading(ui, "Calendar Settings").clicked();
            }
            ui.horizontal(|ui| {
                for (page, label) in [
                    (SettingsPage::Calendar, "Calendars"),
                    (SettingsPage::Notifications, "Notifications"),
                ] {
                    ui.selectable_value(&mut settings.page, page, label);
                }
            });
            ui.add_space(12.);
            egui::ScrollArea::vertical()
                .max_height((design::dialog_body_height(ctx) - 52.).max(160.))
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.add_enabled_ui(available, |ui| {
                        if settings.page == SettingsPage::Calendar {
                            design::settings_group(ui, "Refresh", |ui| {
                                ui.horizontal(|ui| {
                                    field_label(ui, "Refresh Every:");
                                    ui.add(
                                        egui::DragValue::new(
                                            &mut settings
                                                .calendar_settings
                                                .refresh_interval_seconds,
                                        )
                                        .range(1..=86_400)
                                        .suffix(" s"),
                                    );
                                    refresh = design::button(ui, "Refresh Now")
                                        .on_hover_text("Refresh saved calendars")
                                        .clicked();
                                });
                            });
                            remove_subscription = calendar_form(settings, ui);
                        }
                        if settings.page == SettingsPage::Notifications {
                            telegram_form(settings, ui);
                        }
                    });
                });
            if !settings.error.is_empty() {
                ui.colored_label(colors(ui).red, &settings.error);
            }
            design::dialog_footer(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled_ui(available, |ui| design::dialog_primary(ui, "Save Settings"))
                        .inner
                        .clicked()
                    {
                        save = true;
                    }
                    close |= design::button(ui, "Cancel").clicked();
                });
            });
        };
        let shortcut = if let Some(ui) = embedded {
            body(ui);
            keys.in_ui(ui, available)
        } else {
            let response =
                design::modal(ctx, egui::Id::new("calendar-settings-dialog")).show(ctx, body);
            keys.dialog(&response, available)
        };
        if let Some(id) = remove_subscription {
            settings.subscriptions.retain(|s| s.id != id);
        }
        close |= shortcut.cancel;
        save |= shortcut.confirm;
        if save {
            let result = (|| -> Result<(Credentials, Organizer), String> {
                if settings.credentials.telegram_enabled
                    && (settings.credentials.telegram_token.trim().is_empty()
                        || settings.credentials.telegram_chat.trim().is_empty())
                {
                    return Err(
                        "Enter the Telegram bot token and chat ID, or disable notifications."
                            .into(),
                    );
                }
                let mut credentials = self.productivity.credentials.clone();
                credentials.telegram_enabled = settings.credentials.telegram_enabled;
                credentials.telegram_token = settings.credentials.telegram_token.clone();
                credentials.telegram_chat = settings.credentials.telegram_chat.clone();
                let mut organizer = self.organizer.clone();
                {
                    organizer.calendar_settings.refresh_interval_seconds = settings
                        .calendar_settings
                        .refresh_interval_seconds
                        .clamp(1, 86_400);
                    let mut subscriptions = vec![];
                    let mut urls = std::collections::BTreeMap::new();
                    for row in &settings.subscriptions {
                        if row.name.trim().is_empty()
                            && row.url.trim().is_empty()
                            && !self.organizer.subscriptions.iter().any(|s| s.id == row.id)
                        {
                            continue;
                        }
                        let url = amigo_connect::calendar_feed::normalize_url(&row.url)?;
                        if urls.values().any(|other| other == &url) {
                            return Err("This calendar URL is already subscribed.".into());
                        }
                        if self.productivity.credentials.calendar_urls.get(&row.id) != Some(&url) {
                            organizer.remove_subscription(&row.id);
                        }
                        urls.insert(row.id.clone(), url);
                        subscriptions.push(CalendarSubscription {
                            id: row.id.clone(),
                            name: if row.name.trim().is_empty() {
                                "Calendar".into()
                            } else {
                                row.name.trim().into()
                            },
                        });
                    }
                    for old in &self.organizer.subscriptions {
                        if !subscriptions.iter().any(|s| s.id == old.id) {
                            organizer.remove_subscription(&old.id);
                        }
                    }
                    organizer.subscriptions = subscriptions;
                    credentials.calendar_urls = urls;
                    for c in &mut organizer.calendars {
                        if let Some(s) = organizer
                            .subscriptions
                            .iter()
                            .find(|s| s.account() == c.account)
                        {
                            c.name = s.name.clone();
                        }
                    }
                }
                credentials.save(&self.data_path)?;
                if let Err(error) = organizer.save(&self.data_path) {
                    if self.productivity.credentials.save(&self.data_path).is_err() {
                        self.productivity.credentials_blocked = true;
                    }
                    return Err(error);
                }
                Ok((credentials, organizer))
            })();
            match result {
                Ok((credentials, organizer)) => {
                    self.productivity.credentials = credentials;
                    self.organizer = organizer;
                    self.productivity.saved = self.organizer.clone();
                    close = true;
                    self.sync_calendars();
                }
                Err(e) => {
                    if let Some(settings) = &mut self.productivity.settings {
                        settings.error = e;
                    }
                }
            }
        }
        if refresh {
            self.sync_calendars();
        }
        if close {
            self.productivity.settings = None;
        }
        if let Some(action) = data_action {
            self.open_data_dialog(amigo_core::backup::Scope::Calendar, action);
        }
        close && !save
    }
}

fn calendar_form(settings: &mut SettingsDraft, ui: &mut egui::Ui) -> Option<String> {
    let mut remove = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Calendar URLs").size(16.).strong());
        if design::button(ui, "+ Add Calendar URL").clicked() {
            settings.subscriptions.push(SubscriptionDraft {
                id: id(),
                name: String::new(),
                url: String::new(),
            });
        }
    });
    ui.add_space(12.);
    for row in &mut settings.subscriptions {
        ui.push_id(&row.id, |ui| {
            design::settings_group(ui, "", |ui| {
                ui.horizontal(|ui| {
                    let width =
                        (ui.available_width() - 32. - ui.spacing().item_spacing.x).max(100.);
                    ui.add(
                        design::input(&mut row.name)
                            .hint_text("Calendar Name")
                            .desired_width(width),
                    );
                    if design::icon_button(
                        ui,
                        ui.make_persistent_id("remove-calendar-url"),
                        Icon::Trash,
                        "Remove Subscription",
                        32.,
                    )
                    .clicked()
                    {
                        remove = Some(row.id.clone());
                    }
                });
                design::settings_separator(ui);
                field_label(ui, "Calendar URL");
                ui.add(
                    design::input(&mut row.url)
                        .password(true)
                        .hint_text("https://…/calendar.ics or webcal://…")
                        .desired_width(f32::INFINITY),
                );
            });
        });
        ui.add_space(12.);
    }
    ui.add_space(12.);
    design::hint(
        ui,
        "Use an iCalendar (.ics) subscription link. Subscribed events are read only; reminders are managed here.",
    );
    ui.hyperlink_to(
        "Find your Google iCal URL",
        "https://support.google.com/calendar/answer/37648",
    );
    ui.hyperlink_to("Publish an Outlook ICS URL","https://support.microsoft.com/en-gb/outlook/sharing/share-an-outlook-calendar-as-view-only-with-others?ocmsassetid=ha010354420");

    remove
}
fn telegram_form(settings: &mut SettingsDraft, ui: &mut egui::Ui) {
    design::settings_group(ui, "Telegram Notification", |ui| {
        design::small_checkbox(
            ui,
            &mut settings.credentials.telegram_enabled,
            "Enable Telegram Notifications",
        );
        design::settings_separator(ui);
        field_label(ui, "Bot Token");
        ui.add(
            design::input(&mut settings.credentials.telegram_token)
                .password(true)
                .desired_width(f32::INFINITY),
        );
        text_field(ui, "Chat ID", &mut settings.credentials.telegram_chat);
    });
    ui.hyperlink_to("Create a bot with BotFather", "https://t.me/BotFather");
    design::hint(
        ui,
        "Amigo sends reminders while running and a catch-up summary after restart.",
    );
}
