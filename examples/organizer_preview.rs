//! Isolated synthetic Calendar previews. Never downloads feeds or sends notifications.
use amigo::{Amigo, Section};
use amigo_core::organizer::*;
use chrono::{Duration, Local, TimeZone, Utc};
use eframe::{App, egui};
use std::path::PathBuf;
struct Preview {
    app: Amigo,
    out: PathBuf,
    step: usize,
    frames: u32,
    pending: bool,
    scene_started: std::time::Instant,
}
const SCENES: [&str; 6] = [
    "calendar-day",
    "calendar-week",
    "calendar-month",
    "calendar-agenda",
    "calendar-editor",
    "calendar-url-settings",
];
impl App for Preview {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if self.app.is_page_loading() {
            return;
        }
        if self.step == 5
            && self.frames == 5
            && let Some(r) = ctx.read_response(egui::Id::new("calendar-settings"))
        {
            let p = r.rect.center();
            input.events.push(egui::Event::PointerMoved(p));
            for pressed in [true, false] {
                input.events.push(egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if self.step >= SCENES.len() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if let Some(image) = ui.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        }) {
            let path = self.out.join(format!("{}.png", SCENES[self.step]));
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            image::save_buffer(
                &path,
                &bytes,
                image.width() as u32,
                image.height() as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            println!("Captured {}", path.display());
            self.step += 1;
            if self.step == 5 {
                self.app.calendar = Default::default();
            }
            self.scene_started = std::time::Instant::now();
            self.frames = 0;
            self.pending = false;
            if self.step == SCENES.len() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        self.app.section = Section::Calendar;
        self.app.calendar.view = match self.step {
            1 => amigo_calendar::View::Week,
            2 => amigo_calendar::View::Month,
            3 => amigo_calendar::View::Agenda,
            _ => amigo_calendar::View::Day,
        };
        if self.step == 4 && self.frames == 0 {
            self.app.calendar.new_event(Local::now().date_naive());
        }

        self.app.ui(ui, frame);
        if self.app.is_page_loading() {
            self.frames = self.frames.max(1);
            ui.ctx().request_repaint();
            return;
        }

        self.frames += 1;

        if self.frames >= 25 && self.scene_started.elapsed().as_millis() >= 400 && !self.pending {
            self.pending = true;
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        ui.ctx().request_repaint();
    }
}
fn main() -> eframe::Result {
    assert!(
        std::env::var_os("AMIGO_DATA_FILE").is_some(),
        "Use isolated preview data"
    );
    let out = PathBuf::from(std::env::args().nth(1).expect("Output directory"));
    std::fs::create_dir_all(&out).unwrap();
    let compact = std::env::args().any(|a| a == "--compact");
    let dark = std::env::args().any(|a| a == "--dark");
    eframe::run_native(
        "Organizer preview",
        eframe::NativeOptions {
            glow_options: eframe::egui_glow::GlowConfiguration {
                vsync: false,
                ..Default::default()
            },
            viewport: egui::ViewportBuilder::default().with_inner_size(if compact {
                [980., 640.]
            } else {
                [1440., 900.]
            }),
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut app = Amigo::new(cc);
            app.workspace.dark_mode = dark;
            app.organizer.subscriptions.push(CalendarSubscription {
                id: "sample-feed".into(),
                name: "Work".into(),
            });
            app.organizer.calendars.push(CalendarSource {
                id: "sample-feed".into(),
                account: "feed:sample-feed".into(),
                name: "Work".into(),
                writable: false,
                tasks: false,
            });
            let today = Local::now().date_naive();
            for (i, (title, hour, kind)) in [
                ("Weekly planning", 9, EventKind::Event),
                ("Review trip budget", 11, EventKind::Task),
                ("Design review", 14, EventKind::Event),
                ("Dinner with friends", 19, EventKind::Event),
            ]
            .into_iter()
            .enumerate()
            {
                let start = Local
                    .from_local_datetime(&today.and_hms_opt(hour, 0, 0).unwrap())
                    .earliest()
                    .unwrap()
                    .with_timezone(&Utc);
                app.organizer.events.push(CalendarEvent {
                    title: title.into(),
                    start,
                    end: start + Duration::hours(1),
                    kind,
                    account: if i == 2 {
                        "feed:sample-feed".into()
                    } else {
                        String::new()
                    },
                    calendar: if i == 2 {
                        "sample-feed".into()
                    } else {
                        String::new()
                    },
                    description: "Notes, decisions, and next steps for a calmer week.".into(),
                    location: if i == 2 {
                        "Studio".into()
                    } else {
                        String::new()
                    },
                    reminder_minutes: Some(15),
                    ..Default::default()
                });
            }
            Ok(Box::new(Preview {
                app,
                out,
                step: 0,
                frames: 0,
                pending: false,
                scene_started: std::time::Instant::now(),
            }))
        }),
    )
}
