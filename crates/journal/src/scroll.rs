//! Source-line mapping avoids percentage drift between prose and rendered headings/code.
use egui::{self, Galley};
use std::sync::Arc;
#[derive(Default)]
pub(super) struct ScrollSync {
    pub source_target: Option<f32>,
    pub preview_target: Option<f32>,
    pub source_offset: f32,
    pub preview_offset: f32,
    pub galley: Option<Arc<Galley>>,
    line_chars: Vec<usize>,
    pub split_ratio: f32,
}
impl ScrollSync {
    pub fn update_source(&mut self, text: &str, galley: Arc<Galley>) {
        if self.galley.as_ref().is_none_or(|g| g.text() != text) {
            self.line_chars = vec![0];
            self.line_chars.extend(
                text.chars()
                    .enumerate()
                    .filter_map(|(i, c)| (c == '\n').then_some(i + 1)),
            );
        }
        self.galley = Some(galley);
    }
    pub fn line_for_y(&self, y: f32) -> f32 {
        let Some(galley) = &self.galley else {
            return 0.;
        };
        let cursor = galley.cursor_from_pos(egui::vec2(0., y));
        let line = self
            .line_chars
            .partition_point(|start| *start <= cursor.index.0)
            .saturating_sub(1);
        let top = galley
            .pos_from_cursor(egui::text::CCursor::new(
                self.line_chars.get(line).copied().unwrap_or(0),
            ))
            .top();
        let bottom = self
            .line_chars
            .get(line + 1)
            .map_or(galley.size().y, |start| {
                galley
                    .pos_from_cursor(egui::text::CCursor::new(*start))
                    .top()
            });
        line as f32 + ((y - top) / (bottom - top).max(1.)).clamp(0., 1.)
    }
    pub fn y_for_line(&self, line: f32) -> f32 {
        let Some(galley) = &self.galley else {
            return 0.;
        };
        let i = line.max(0.).floor() as usize;
        let start = self
            .line_chars
            .get(i)
            .copied()
            .unwrap_or_else(|| galley.text().chars().count());
        let top = galley
            .pos_from_cursor(egui::text::CCursor::new(start))
            .top();
        let bottom = self.line_chars.get(i + 1).map_or(galley.size().y, |n| {
            galley.pos_from_cursor(egui::text::CCursor::new(*n)).top()
        });
        top + (bottom - top).max(0.) * line.fract()
    }
}
