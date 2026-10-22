//! Selection toolbar follows the Markdown editor; right-click formatting remains available.
use super::*;
pub(super) fn selection_toolbar(
    editor: &mut Editor,
    ui: &mut egui::Ui,
    output: &egui::text_edit::TextEditOutput,
) -> Option<Format> {
    let ctx = ui.ctx();
    let source_id = egui::Id::new(("journal-editor", &editor.document.path));
    let area_id = egui::Id::new(("journal-format-toolbar", &editor.document.path));
    let previous = ctx.memory(|m| m.area_rect(area_id));
    let over_toolbar = ui
        .input(|i| i.pointer.interact_pos())
        .is_some_and(|p| previous.is_some_and(|r| r.expand(8.).contains(p)));
    let owns_popup = editor
        .toolbar_menus
        .iter()
        .any(|id| egui::Popup::is_id_open(ctx, *id));
    if editor.selection.is_empty()
        || !ui.is_enabled()
        || ctx.memory(|m| m.top_modal_layer().is_some())
        || (!ui.memory(|m| m.has_focus(source_id)) && !over_toolbar && !owns_popup)
        || (egui::Popup::is_any_open(ctx) && !owns_popup)
        || output.response.context_menu_opened()
        || ui.input(|i| i.pointer.primary_down() && output.response.is_pointer_button_down_on())
    {
        return None;
    }
    let cursor = output
        .galley
        .pos_from_cursor(egui::text::CCursor::new(editor.selection.start));
    let selection_pos = output.galley_pos + cursor.min.to_vec2();
    if !output.text_clip_rect.expand(30.).contains(selection_pos) {
        return None;
    }
    let bounds = ctx.content_rect().shrink(8.);
    let pos = egui::pos2(
        selection_pos
            .x
            .clamp(bounds.left(), (bounds.right() - 345.).max(bounds.left())),
        (selection_pos.y - 46.).max(bounds.top()),
    );
    let mut action = None;
    egui::Area::new(area_id)
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(colors(ui).card)
                .stroke(egui::Stroke::new(1., colors(ui).line))
                .corner_radius(7)
                .inner_margin(5)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(2., 0.);
                    ui.horizontal(|ui| {
                        let caret = editor
                            .document
                            .text
                            .char_indices()
                            .nth(editor.selection.start)
                            .map_or(editor.document.text.len(), |(i, _)| i);
                        let line = editor.document.text[..caret]
                            .rfind('\n')
                            .map_or(0, |i| i + 1);
                        let level = editor.document.text[line..]
                            .chars()
                            .take_while(|c| *c == '#')
                            .count()
                            .min(6);
                        let label = if level == 0 {
                            "Normal".into()
                        } else {
                            format!("H{level}")
                        };
                        let headings = ui.add_sized([84., 30.], design::Button::new(label));
                        ui.interact(
                            headings.rect,
                            egui::Id::new("journal-floating-headings"),
                            egui::Sense::hover(),
                        );
                        design::paint_icon(
                            ui.painter(),
                            egui::Rect::from_center_size(
                                headings.rect.right_center() - egui::vec2(9., 0.),
                                egui::vec2(12., 12.),
                            ),
                            Icon::ChevronDown,
                            colors(ui).muted,
                        );
                        egui::Popup::menu(&headings).show(|ui| {
                            for (value, label) in [
                                (0, "Normal"),
                                (1, "Heading 1"),
                                (2, "Heading 2"),
                                (3, "Heading 3"),
                                (4, "Heading 4"),
                                (5, "Heading 5"),
                                (6, "Heading 6"),
                            ] {
                                if design::button(ui, label).clicked() {
                                    action = Some(Format::Heading(value));
                                    ui.close();
                                }
                            }
                        });
                        editor.toolbar_menus = vec![egui::Popup::default_response_id(&headings)];
                        ui.separator();
                        for (label, icon, kind) in [
                            ("Bold", Icon::Bold, Format::Bold),
                            ("Italic", Icon::Italic, Format::Italic),
                            ("Strikethrough", Icon::Strikethrough, Format::Strike),
                            ("Inline Code", Icon::Code, Format::Code),
                            ("Link", Icon::Link, Format::Link),
                        ] {
                            if design::icon_button(
                                ui,
                                egui::Id::new(("journal-floating-format", label)),
                                icon,
                                label,
                                30.,
                            )
                            .clicked()
                            {
                                action = Some(kind);
                            }
                        }
                        ui.separator();
                        let lists = design::icon_button(
                            ui,
                            egui::Id::new("journal-floating-lists"),
                            Icon::List,
                            "Lists and Blocks",
                            30.,
                        );
                        egui::Popup::menu(&lists).show(|ui| {
                            for (label, kind) in [
                                ("Bulleted List", Format::Bullet),
                                ("Numbered List", Format::Numbered),
                                ("Task List", Format::Task),
                                ("Quote", Format::Quote),
                                ("Code Block", Format::CodeBlock),
                                ("Table", Format::Table),
                                ("Horizontal Rule", Format::Rule),
                            ] {
                                if design::button(ui, label).clicked() {
                                    action = Some(kind);
                                    ui.close();
                                }
                            }
                        });
                        editor
                            .toolbar_menus
                            .push(egui::Popup::default_response_id(&lists));
                    });
                });
        });
    action
}
