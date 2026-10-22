//! Short-lived status messages float above the app without changing page layout.
use super::*;
use std::collections::VecDeque;

const LIFETIME: f64 = 5.0;
const FADE: f64 = 0.25;
const MAX_VISIBLE: usize = 3;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Success,
    Error,
}
struct Notification {
    id: u64,
    text: String,
    kind: Kind,
    created: f64,
}
#[derive(Default)]
pub(super) struct Notifications {
    items: VecDeque<Notification>,
    next_id: u64,
    last_error: String,
}
impl Notifications {
    fn push(&mut self, text: String, kind: Kind, now: f64) {
        if text.trim().is_empty() {
            return;
        }
        if let Some(item) = self
            .items
            .iter_mut()
            .find(|item| item.text == text && item.kind == kind)
        {
            item.created = now;
            return;
        }
        self.next_id = self.next_id.wrapping_add(1);
        self.items.push_back(Notification {
            id: self.next_id,
            text,
            kind,
            created: now,
        });
        while self.items.len() > MAX_VISIBLE {
            self.items.pop_front();
        }
    }
    fn observe_error(&mut self, error: &str, now: f64) {
        if self.last_error != error {
            self.last_error = error.into();
            self.push(error.into(), Kind::Error, now);
        }
    }
    fn show(&mut self, ctx: &egui::Context, now: f64) {
        self.items.retain(|item| now - item.created < LIFETIME);
        let max_width = (ctx.content_rect().width() - 40.0).clamp(120.0, 420.0);
        let mut bottom = 20.0;
        let mut dismissed = None;
        for item in self.items.iter().rev() {
            let remaining = (LIFETIME - (now - item.created)).clamp(0.0, LIFETIME);
            let p = design::palette(ctx);
            let tint = match item.kind {
                Kind::Success => p.green,
                Kind::Error => p.red,
            };
            let mut job = egui::text::LayoutJob::simple(
                item.text.clone(),
                egui::FontId::proportional(14.0),
                p.ink,
                max_width - 76.0,
            );
            job.wrap.max_rows = 4;
            job.wrap.break_anywhere = true;
            let galley = ctx.fonts_mut(|fonts| fonts.layout_job(job));
            let width = galley.size().x + 76.0;
            let height = galley.size().y.max(20.0) + 20.0;
            if bottom + height > ctx.content_rect().height() - 20.0 {
                break;
            }
            let id = egui::Id::new(("app-notification", item.id));
            let response = egui::Area::new(id)
                .order(egui::Order::Foreground)
                .movable(false)
                .fade_in(false)
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -bottom))
                .default_width(width)
                .show(ctx, |ui| {
                    ui.multiply_opacity((remaining / FADE).min(1.0) as f32);
                    ui.set_width(width);
                    ui.spacing_mut().interact_size.y = 20.0;
                    egui::Frame::new()
                        .fill(p.card)
                        .stroke(egui::Stroke::new(1.0, p.line))
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(12, 10))
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 4],
                            blur: 16,
                            spread: 0,
                            color: egui::Color32::from_black_alpha(24),
                        })
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            ui.horizontal(|ui| {
                                design::icon(
                                    ui,
                                    if item.kind == Kind::Success {
                                        Icon::Check
                                    } else {
                                        Icon::Cancel
                                    },
                                    tint,
                                    16.0,
                                );
                                ui.add(egui::Label::new(galley)).on_hover_text(&item.text);
                                if design::icon_button(
                                    ui,
                                    id.with("close"),
                                    Icon::Close,
                                    "Dismiss Notification",
                                    20.0,
                                )
                                .clicked()
                                {
                                    dismissed = Some(item.id);
                                }
                            });
                        });
                });
            bottom += response.response.rect.height() + 8.0;
            if remaining <= FADE {
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(Duration::from_secs_f64(remaining - FADE));
            }
        }
        if let Some(id) = dismissed {
            self.items.retain(|item| item.id != id);
        }
    }
}
impl Amigo {
    pub(super) fn show_notifications(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        for message in [&mut self.message, &mut self.finance.message] {
            self.notifications
                .push(std::mem::take(message), Kind::Success, now);
        }
        if !self.finance.error.is_empty() {
            self.error = std::mem::take(&mut self.finance.error);
            self.notifications
                .push(self.error.clone(), Kind::Error, now);
        }
        // Keep the error available to save/retry logic without repeatedly showing it each frame.
        self.notifications.observe_error(&self.error, now);
        self.notifications.show(ctx, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{harness, render_sized, text_rect};

    fn frame(
        notifications: &mut Notifications,
        ctx: &egui::Context,
        now: f64,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(980.0, 640.0),
                )),
                time: Some(now),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |_| {});
                notifications.show(ui.ctx(), now);
            },
        );
        output.textures_delta.clear();
        output
    }
    #[test]
    fn app_notifications_are_compact_and_leave_the_board_in_place() {
        for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
            let (mut app, ctx) = harness();
            let before = render_sized(&mut app, &ctx, vec![], size);
            let header = text_rect(&before, "Backlog");
            app.message = "Vault deleted.".into();
            let mut output = render_sized(&mut app, &ctx, vec![], size);
            for _ in 0..3 {
                output = render_sized(&mut app, &ctx, vec![], size);
            }
            assert_eq!(text_rect(&output, "Backlog"), header);
            let text = text_rect(&output, "Vault deleted.");
            let popup = ctx
                .memory(|memory| memory.area_rect(egui::Id::new(("app-notification", 1_u64))))
                .unwrap();
            assert!(popup.contains_rect(text));
            assert!(popup.width() < 240.0 && popup.height() < 80.0);
            assert!((popup.right() - (size.x - 20.0)).abs() < 1.0);
            assert!((popup.bottom() - (size.y - 20.0)).abs() < 1.0);
            assert!(app.message.is_empty());
            std::fs::remove_dir_all(app.data_path).unwrap();
        }
    }
    #[test]
    fn notifications_fit_their_content_stack_without_overlap_and_expire() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut notifications = Notifications::default();
        notifications.push("Saved".into(), Kind::Success, 100.0);
        let long = "The setup could not be imported because a task attachment is missing. Choose a complete backup and try again.";
        notifications.push(long.into(), Kind::Error, 100.0);
        for now in [100.0, 100.01, 100.02] {
            let _ = frame(&mut notifications, &ctx, now, vec![]);
        }
        let rect = |id| {
            ctx.memory(|memory| memory.area_rect(egui::Id::new(("app-notification", id))))
                .unwrap()
        };
        let short = rect(1_u64);
        let long_rect = rect(2_u64);
        assert!(short.width() < long_rect.width());
        assert!(long_rect.width() <= 421.0);
        assert!(short.bottom() <= long_rect.top() - 7.0);
        assert!(
            (short.right() - long_rect.right()).abs() < 1.0,
            "{short:?} / {long_rect:?}"
        );
        assert!(long_rect.bottom() > 619.0 && long_rect.bottom() < 621.0);
        let _ = frame(&mut notifications, &ctx, 104.9, vec![]);
        assert_eq!(notifications.items.len(), 2);
        let output = frame(&mut notifications, &ctx, 105.01, vec![]);
        assert!(notifications.items.is_empty());
        assert!(crate::app::tests::text_rect_optional(&output, "Saved").is_none());
        assert!(crate::app::tests::text_rect_optional(&output, long).is_none());
    }
    #[test]
    fn notifications_can_be_dismissed_before_the_delay() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut notifications = Notifications::default();
        notifications.push("Board exported.".into(), Kind::Success, 10.0);
        for now in [10.0, 10.01, 10.02] {
            let _ = frame(&mut notifications, &ctx, now, vec![]);
        }
        let pos = ctx
            .read_response(egui::Id::new(("app-notification", 1_u64)).with("close"))
            .unwrap()
            .rect
            .center();
        for (index, pressed) in [true, false].iter().enumerate() {
            let _ = frame(
                &mut notifications,
                &ctx,
                10.1 + index as f64 * 0.1,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: *pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(notifications.items.is_empty());
    }
    #[test]
    fn persistent_error_does_not_reset_the_timer_or_fill_the_stack() {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut notifications = Notifications::default();
        for now in [0.0, 1.0, 3.0, 6.0] {
            notifications.observe_error("Cannot save", now);
            let _ = frame(&mut notifications, &ctx, now, vec![]);
        }
        assert!(notifications.items.is_empty());
        notifications.observe_error("", 6.1);
        notifications.observe_error("Cannot save", 7.0);
        assert_eq!(notifications.items.len(), 1);
        for number in 0..5 {
            notifications.push(format!("Action {number}"), Kind::Success, 8.0);
        }
        assert_eq!(notifications.items.len(), MAX_VISIBLE);
    }
}
