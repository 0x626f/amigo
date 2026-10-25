//! Preload the workspace off-thread, then populate egui's font atlas in small slices.
//! A separate context is essential: rendering a whole page with the live context
//! on a worker would hold its locks and freeze the window just like before.
use super::*;
use egui::text::LayoutJob;
use std::{
    collections::{HashSet, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const FRAME_BUDGET: Duration = Duration::from_millis(4);

#[derive(Default)]
pub(super) struct PageLoader {
    ready: HashSet<egui::Id>,
    pending: Option<Pending>,
}

struct Pending {
    key: egui::Id,
    cancelled: Arc<AtomicBool>,
    receiver: mpsc::Receiver<VecDeque<LayoutJob>>,
    warmup: Option<VecDeque<LayoutJob>>,
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl PageLoader {
    pub(super) fn cancel(&mut self) {
        self.pending = None;
    }
    fn needs_start(&self, key: egui::Id) -> bool {
        !self.ready.contains(&key) && self.pending.as_ref().is_none_or(|p| p.key != key)
    }

    fn start(
        &mut self,
        key: egui::Id,
        ctx: &egui::Context,
        prepare: impl FnOnce(&AtomicBool) -> Vec<LayoutJob> + Send + 'static,
    ) {
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.pending = Some(Pending {
            key,
            cancelled: cancelled.clone(),
            receiver,
            warmup: None,
        });
        let ctx = ctx.clone();
        let pixels_per_point = ctx.pixels_per_point();
        std::thread::spawn(move || {
            if !cancelled.load(Ordering::Relaxed) {
                let jobs = warmup_jobs(prepare(&cancelled), pixels_per_point);
                if !cancelled.load(Ordering::Relaxed) {
                    let _ = sender.send(jobs);
                    ctx.request_repaint();
                }
            }
        });
    }

    fn tick(&mut self, key: egui::Id, ctx: &egui::Context) -> bool {
        if self.ready.contains(&key) {
            self.pending = None;
            return false;
        }
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        if pending.warmup.is_none() {
            match pending.receiver.try_recv() {
                Ok(jobs) => pending.warmup = Some(jobs),
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(16));
                    return true;
                }
                // Recover to the normal renderer if the preparation worker failed.
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.ready.insert(key);
                    self.pending = None;
                    ctx.request_repaint();
                    return true;
                }
            }
        }
        let jobs = pending.warmup.as_mut().unwrap();
        let started = Instant::now();
        while started.elapsed() < FRAME_BUDGET {
            let Some(job) = jobs.pop_front() else {
                break;
            };
            ctx.fonts_mut(|fonts| {
                fonts.layout_job(job);
            });
        }
        if jobs.is_empty() {
            self.ready.insert(key);
        }
        ctx.request_repaint_after(Duration::from_millis(16));
        true
    }
}

// Warm one character/subpixel offset at a time. Whole labels can otherwise take
// hundreds of milliseconds in debug builds. Replay the complete jobs afterwards
// to prepare shaping (including ligatures) and text layout too.
fn warmup_jobs(jobs: Vec<LayoutJob>, pixels_per_point: f32) -> VecDeque<LayoutJob> {
    let mut warmup = VecDeque::new();
    let mut seen = HashSet::new();
    for job in &jobs {
        for section in &job.sections {
            for character in job.text[section.byte_range.start.0..section.byte_range.end.0].chars()
            {
                if character.is_control() {
                    continue;
                }
                let mut format = section.format.clone();
                format.color = egui::Color32::WHITE;
                if !seen.insert(egui::Id::new((character, &format))) {
                    continue;
                }
                for bin in 0..4 {
                    let mut glyph = LayoutJob::default();
                    glyph.append(
                        &character.to_string(),
                        bin as f32 / (4.0 * pixels_per_point),
                        format.clone(),
                    );
                    glyph.round_output_to_gui = false;
                    warmup.push_back(glyph);
                }
            }
        }
    }
    warmup.extend(jobs);
    warmup
}

impl Amigo {
    /// Whether workspace screens are still being prepared at startup.
    pub fn is_page_loading(&self) -> bool {
        self.page_loader.pending.is_some()
    }

    pub(super) fn prepare_page(&mut self, ui: &mut egui::Ui) -> bool {
        let ctx = ui.ctx();
        let key = egui::Id::new((
            &self.library.settings.board,
            &self.library.settings.vault,
            self.vault_unlocked,
            ctx.pixels_per_point().to_bits(),
            ui.available_width() < 900.0,
        ));
        if self.page_loader.needs_start(key) {
            let workspace = self.workspace.clone();
            let organizer = self.organizer.clone();
            let board = self.current_board_name().to_owned();
            let vault = self.current_vault().cloned();
            let vault_unlocked = self.vault_unlocked;
            let mut finance = FinanceTab::new(&workspace.finance);
            finance.view = self.finance.view;
            finance.storage_unit = self.finance.storage_unit;
            finance.month = self.finance.month.clone();
            finance.wallet_month = self.finance.wallet_month.clone();
            finance.actual = self.finance.actual;
            finance.search = self.finance.search.clone();
            let mut calendar = amigo_calendar::CalendarTab::default();
            calendar.view = self.calendar.view;
            calendar.date = self.calendar.date;
            let rect = ui.max_rect();
            let pixels_per_point = ctx.pixels_per_point();
            self.page_loader.start(key, ctx, move |cancelled| {
                let ctx = egui::Context::default();
                design::install(&ctx);
                design::apply_theme(&ctx, workspace.dark_mode);
                ctx.set_pixels_per_point(pixels_per_point);
                let mut workspace = workspace;
                let mut tasks = TasksTab::default();
                let mut jobs = Vec::new();
                // Share one worker context across screens: fonts already prepared
                // for one screen can be reused by the next, just as in the real UI.
                let mut prepare = |show: &mut dyn FnMut(&mut egui::Ui)| {
                    if cancelled.load(Ordering::Relaxed) {
                        return;
                    }
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(rect),
                            ..Default::default()
                        },
                        |ui| {
                            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| show(ui));
                        },
                    );
                    for shape in &output.shapes {
                        collect_jobs(&shape.shape, &mut jobs);
                    }
                    output.textures_delta.clear();
                };
                prepare(&mut |ui| tasks.show(&mut workspace, &board, ui));
                prepare(&mut |ui| {
                    design::eyebrow(ui, "VAULTS");
                    finance.sidebar(ui);
                    calendar.sidebar(&organizer, ui);
                });
                for view in [
                    amigo_calendar::View::Day,
                    amigo_calendar::View::Week,
                    amigo_calendar::View::Month,
                    amigo_calendar::View::Agenda,
                ] {
                    calendar.view = view;
                    prepare(&mut |ui| {
                        calendar.show(&organizer, ui);
                    });
                }
                if vault_unlocked {
                    for view in amigo_finance::FinanceView::ALL {
                        finance.view = view;
                        if view == amigo_finance::FinanceView::Expenses {
                            for group in [
                                amigo_finance::ExpenseGroup::Regular,
                                amigo_finance::ExpenseGroup::Monthly,
                            ] {
                                finance.expense_group = group;
                                prepare(&mut |ui| {
                                    finance.show(&mut workspace.finance, vault.as_ref(), ui)
                                });
                            }
                        } else {
                            prepare(&mut |ui| {
                                finance.show(&mut workspace.finance, vault.as_ref(), ui)
                            });
                        }
                    }
                    finance.view = amigo_finance::FinanceView::Storage;
                    for page in amigo_finance::FinanceReference::ALL {
                        prepare(&mut |ui| finance.reference_in(page, &mut workspace.finance, ui));
                    }
                }
                jobs
            });
            // Let the loading indicator reach the screen before doing any work.
            ctx.request_repaint();
        } else if !self.page_loader.tick(key, ctx) {
            return false;
        }
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.add_space((ui.available_height() * 0.4).max(24.0));
            ui.add(egui::Spinner::new().size(24.0).color(colors(ui).accent));
            hint(ui, "Loading workspace…");
        });
        true
    }
}

fn collect_jobs(shape: &egui::Shape, jobs: &mut Vec<LayoutJob>) {
    match shape {
        egui::Shape::Text(text) => jobs.push((*text.galley.job).clone()),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_jobs(shape, jobs);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{harness, render, render_frame, text_rect, text_rect_optional};

    #[test]
    fn startup_preloads_every_view_without_changing_data() {
        let (mut app, ctx) = harness();
        app.finance.view = amigo_finance::FinanceView::Expenses;
        app.finance.expense_group = amigo_finance::ExpenseGroup::Regular;
        app.finance.storage_unit = 4;
        app.calendar.view = amigo_calendar::View::Week;
        let workspace = app.workspace.clone();
        let organizer = app.organizer.clone();
        let output = render_frame(&mut app, &ctx, vec![]);
        assert!(app.is_page_loading());
        assert!(text_rect_optional(&output, "Loading workspace…").is_some());
        let _ = render(&mut app, &ctx, vec![]);
        assert!(!app.is_page_loading());
        assert_eq!(app.workspace, workspace);
        assert_eq!(app.organizer, organizer);
        assert_eq!(app.finance.view, amigo_finance::FinanceView::Expenses);
        assert_eq!(
            app.finance.expense_group,
            amigo_finance::ExpenseGroup::Regular
        );
        assert_eq!(app.finance.storage_unit, 4);
        assert!(app.calendar.view == amigo_calendar::View::Week);
        app.section = Section::Finance;
        for view in amigo_finance::FinanceView::ALL {
            app.finance.view = view;
            let output = render_frame(&mut app, &ctx, vec![]);
            assert!(!app.is_page_loading(), "{view:?} should be preloaded");
            assert!(text_rect_optional(&output, "Loading workspace…").is_none());
        }
        app.finance.view = amigo_finance::FinanceView::Storage;
        for unit in [0, 1, 2, 3, 4, 5, 7] {
            app.finance.storage_unit = unit;
            let _ = render_frame(&mut app, &ctx, vec![]);
            assert!(
                !app.is_page_loading(),
                "Stored values {unit} should be preloaded"
            );
        }
        app.section = Section::Calendar;
        for view in [
            amigo_calendar::View::Day,
            amigo_calendar::View::Week,
            amigo_calendar::View::Month,
            amigo_calendar::View::Agenda,
        ] {
            app.calendar.view = view;
            let output = render_frame(&mut app, &ctx, vec![]);
            assert!(!app.is_page_loading());
            assert!(text_rect_optional(&output, "Today").is_some());
        }
        std::fs::remove_dir_all(&app.data_path).unwrap();
    }

    #[test]
    fn navigating_during_startup_reuses_the_preload_and_finishes_the_selected_page() {
        let (mut app, ctx) = harness();
        let output = render_frame(&mut app, &ctx, vec![]);
        assert!(app.is_page_loading());
        let cancelled = app.page_loader.pending.as_ref().unwrap().cancelled.clone();
        // Use the real sidebar while the page is loading.
        let pos = text_rect(&output, "Calendar").center();
        for pressed in [true, false] {
            let _ = render_frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(app.section, Section::Calendar);
        assert!(!cancelled.load(Ordering::Relaxed));
        assert!(Arc::ptr_eq(
            &cancelled,
            &app.page_loader.pending.as_ref().unwrap().cancelled
        ));
        let output = render(&mut app, &ctx, vec![]);
        assert!(!app.is_page_loading());
        assert!(text_rect_optional(&output, "Today").is_some());
        assert!(text_rect_optional(&output, "Net income").is_none());
        std::fs::remove_dir_all(&app.data_path).unwrap();
    }

    #[test]
    fn unlocking_a_vault_preloads_finance_for_the_new_workspace() {
        let (mut app, ctx) = harness();
        app.vault_unlocked = false;
        app.section = Section::Calendar;
        let _ = render(&mut app, &ctx, vec![]);
        assert!(!app.is_page_loading());
        let locked_scope = app.page_loader.ready.clone();
        app.vault_unlocked = true;
        app.section = Section::Finance;
        let output = render_frame(&mut app, &ctx, vec![]);
        assert!(app.is_page_loading());
        assert!(text_rect_optional(&output, "Loading workspace…").is_some());
        assert!(!locked_scope.contains(&app.page_loader.pending.as_ref().unwrap().key));
        let output = render(&mut app, &ctx, vec![]);
        assert!(text_rect_optional(&output, "Net income").is_some());
        assert!(!app.is_page_loading());
        std::fs::remove_dir_all(&app.data_path).unwrap();
    }

    #[test]
    #[ignore = "manual frame timing benchmark"]
    fn first_visit_frame_timings() {
        let (mut app, ctx) = harness();
        let startup = Instant::now();
        let _ = render(&mut app, &ctx, vec![]);
        eprintln!("Startup preload: {:?}", startup.elapsed());
        for view in amigo_finance::FinanceView::ALL {
            app.section = Section::Finance;
            app.finance.view = view;
            let mut maximum = Duration::ZERO;
            let mut frames = 0;
            let total = Instant::now();
            loop {
                let start = Instant::now();
                let _ = render_frame(&mut app, &ctx, vec![]);
                maximum = maximum.max(start.elapsed());
                frames += 1;
                if !app.is_page_loading() {
                    break;
                }
                assert!(total.elapsed() < Duration::from_secs(30));
                std::thread::sleep(Duration::from_millis(1));
            }
            let start = Instant::now();
            let _ = render_frame(&mut app, &ctx, vec![]);
            eprintln!(
                "{view:?}: loader max={maximum:?}, content={:?}, frames={frames}, total={:?}",
                start.elapsed(),
                total.elapsed()
            );
        }
        std::fs::remove_dir_all(&app.data_path).unwrap();
    }
}
