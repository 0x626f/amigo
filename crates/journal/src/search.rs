//! Search lives in its own modal; the notebook tree never becomes a filtered list.
use super::*;

pub(super) struct SearchDialog {
    pub query: String,
    requested: String,
    pub(super) changed_at: Option<Instant>,
    pub(super) results: Option<Vec<Entry>>,
    selected: usize,
    focus: bool,
}
impl Default for SearchDialog {
    fn default() -> Self {
        Self {
            query: String::new(),
            requested: String::new(),
            changed_at: None,
            results: None,
            selected: 0,
            focus: true,
        }
    }
}
impl JournalTab {
    pub fn apply_search(&mut self, query: &str, entries: Vec<Entry>) {
        if let Some(search) = &mut self.search
            && search.query.trim() == query.trim()
            && search.requested == query.trim()
        {
            search.results = Some(entries);
            search.selected = 0;
        }
    }
    pub(super) fn search_dialog_ui(&mut self, ctx: &egui::Context) -> Option<Action> {
        let mut search = self.search.take()?;
        let keys = design::ActionKeys::capture(ctx);
        let mut close = false;
        let mut chosen = None;
        let entries: Vec<_> = if search.query.trim().is_empty() {
            self.entries
                .iter()
                .filter(|entry| !entry.folder && !is_in_trash(&entry.path))
                .cloned()
                .collect()
        } else {
            search.results.clone().unwrap_or_default()
        };
        search.selected = search.selected.min(entries.len().saturating_sub(1));
        let mut can_open = !entries.is_empty();
        let response = design::modal(ctx, egui::Id::new("journal-search-dialog")).show(ctx, |ui| {
            ui.set_width(design::dialog_width(ctx, 640.));
            close = design::dialog_heading(ui, "Search Notes").clicked();
            let mut navigate = false;
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
                search.selected = (search.selected + 1).min(entries.len().saturating_sub(1));
                navigate = true;
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                search.selected = search.selected.saturating_sub(1);
                navigate = true;
            }
            ui.horizontal(|ui| {
                design::icon(ui, Icon::Search, colors(ui).muted, 22.);
                let input = ui.add(
                    design::input(&mut search.query)
                        .id(egui::Id::new("journal-search-query"))
                        .font(egui::FontId::proportional(20.))
                        .hint_text("Search notes and contents…")
                        .desired_width(ui.available_width()),
                );
                if search.focus {
                    input.request_focus();
                    search.focus = false;
                }
                if input.changed() {
                    search.changed_at = Some(Instant::now());
                    search.results = None;
                    search.selected = 0;
                    can_open = false;
                    if search.query.trim().is_empty() {
                        search.requested.clear();
                    }
                }
            });
            ui.add_space(12.);
            let ready = search.query.trim().is_empty() || search.results.is_some();
            if !ready {
                can_open = false;
                ui.horizontal(|ui| {
                    ui.spinner();
                    design::hint(ui, "Searching notes…");
                });
            }
            let results_height = (entries.len().max(2) as f32
                * (38. + ui.spacing().item_spacing.y))
                .min((ctx.content_rect().height() - 280.).clamp(100., 360.));
            let results = egui::ScrollArea::vertical()
                .id_salt("journal-search-results")
                .min_scrolled_height(results_height)
                .max_height(results_height)
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    if ready && entries.is_empty() {
                        design::hint(
                            ui,
                            if search.query.trim().is_empty() {
                                "No notes yet."
                            } else {
                                "No matching notes."
                            },
                        );
                    }
                    for (index, entry) in entries.iter().enumerate() {
                        let row = design::nav_row(
                            ui,
                            &entry.path,
                            Icon::Note,
                            search.selected == index,
                            None,
                        );
                        ui.interact(
                            row.rect,
                            egui::Id::new(("journal-search-result", &entry.path)),
                            egui::Sense::hover(),
                        );
                        if navigate && search.selected == index {
                            row.scroll_to_me(Some(egui::Align::Center));
                        }
                        if row.clicked() && can_open {
                            chosen = Some(entry.path.clone());
                        }
                    }
                });
            ui.interact(
                results.inner_rect,
                egui::Id::new("journal-search-results-viewport"),
                egui::Sense::hover(),
            );
        });
        let shortcut = keys.dialog(&response, can_open);
        if close || shortcut.cancel {
            return None;
        }
        if shortcut.confirm {
            chosen = entries.get(search.selected).map(|entry| entry.path.clone());
        }
        if let Some(path) = chosen {
            return Some(Action::Open(path));
        }
        let mut action = None;
        if !search.query.trim().is_empty() && search.query.trim() != search.requested {
            if !self.busy
                && search
                    .changed_at
                    .is_some_and(|time| time.elapsed().as_millis() >= 200)
            {
                search.requested = search.query.trim().into();
                action = Some(Action::Search(search.requested.clone()));
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
        self.search = Some(search);
        action
    }
}
