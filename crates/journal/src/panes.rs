use super::*;
fn source(editor: &mut Editor, prefs: &Preferences, ui: &mut egui::Ui, width: f32, height: f32) {
    let id = egui::Id::new(("journal-editor", &editor.document.path));
    if !ui.is_enabled() {
        ui.memory_mut(|m| m.surrender_focus(id));
    }

    if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary))
        && !editor.selection.is_empty()
        && ui.ctx().read_response(id).is_some_and(|r| {
            ui.input(|i| i.pointer.interact_pos().is_some_and(|p| r.rect.contains(p)))
        })
    {
        editor.context_selection = Some(editor.selection.clone());
    }
    let mut format = None;
    if ui.is_enabled() && ui.memory(|m| m.has_focus(id)) && !egui::Popup::is_any_open(ui.ctx()) {
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::B)) {
            format = Some(Format::Bold);
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::I)) {
            format = Some(Format::Italic);
        }
    }
    if ui.is_enabled() && ui.memory(|m| m.has_focus(id)) && !egui::Popup::is_any_open(ui.ctx()) {
        let shifted = egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        };
        for (key, action) in [
            (egui::Key::S, Format::Strike),
            (egui::Key::C, Format::Code),
            (egui::Key::U, Format::Link),
        ] {
            if ui.input_mut(|i| i.consume_key(shifted, key)) {
                format = Some(action);
            }
        }
    }
    let pane = ui.available_rect_before_wrap();
    let mut scrolling = egui::ScrollArea::both()
        .id_salt(("journal-source-scroll", &editor.document.path))
        .max_height(height)
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    let synced = editor.scroll.source_target.is_some();
    if let Some(y) = editor.scroll.source_target.take() {
        scrolling = scrolling.vertical_scroll_offset(y);
    }
    let output = scrolling.show_viewport(ui, |ui, _viewport| {
        let gutter = if prefs.line_numbers { 46. } else { 12. };
        ui.add_space(10.);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            ui.add_space(gutter);
            ui.vertical(|ui| {
                let width = (width - gutter - 12.).max(24.);
                let background = ui.painter().add(egui::Shape::Noop);

                let syntax = &mut editor.syntax;
                let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                    syntax.layout(
                        ui,
                        text.as_str(),
                        if prefs.word_wrap { wrap } else { f32::INFINITY },
                        prefs.font_size.clamp(11., 28.),
                    )
                };
                let enabled = ui.is_enabled();
                let output = egui::TextEdit::multiline(&mut editor.document.text)
                    .interactive(enabled)
                    .id(id)
                    .code_editor()
                    .return_key(None)
                    .font(egui::FontId::monospace(prefs.font_size))
                    .frame(egui::Frame::NONE)
                    .desired_width(width)
                    .min_size(egui::vec2(width, height - 12.))
                    .layouter(&mut layouter)
                    .show(ui);
                editor
                    .scroll
                    .update_source(&editor.document.text, output.galley.clone());
                if let Some(range) = output.cursor_range
                    && output.response.has_focus()
                {
                    let caret = output.galley.pos_from_cursor(range.primary);
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(pane.left(), output.galley_pos.y + caret.top()),
                        egui::vec2(pane.width(), caret.height()),
                    );
                    ui.painter().set(
                        background,
                        egui::Shape::rect_filled(
                            rect.intersect(ui.clip_rect()),
                            0,
                            colors(ui).blue_tint.gamma_multiply(0.5),
                        ),
                    );
                }
                if prefs.line_numbers {
                    let painter = ui.painter().with_clip_rect(pane.intersect(ui.clip_rect()));
                    painter.rect_filled(
                        egui::Rect::from_min_max(
                            pane.min,
                            egui::pos2(pane.left() + gutter - 5., pane.bottom()),
                        ),
                        0,
                        colors(ui).sidebar,
                    );
                    let mut line = 1;
                    let mut first = true;
                    for row in &output.galley.rows {
                        let y = output.galley_pos.y + row.pos.y;
                        if first && y + row.size.y >= pane.top() && y <= pane.bottom() {
                            painter.text(
                                egui::pos2(pane.left() + gutter - 12., y + row.size.y / 2.),
                                egui::Align2::RIGHT_CENTER,
                                line.to_string(),
                                egui::FontId::monospace((prefs.font_size - 2.).max(10.)),
                                colors(ui).muted,
                            );
                        }
                        first = row.ends_with_newline;
                        if row.ends_with_newline {
                            line += 1;
                        }
                    }
                }
                if output.response.changed() {
                    editor.revision = editor.revision.wrapping_add(1);
                    editor.changed_at = Instant::now();
                }
                if let Some(range) = &editor.context_selection {
                    editor.selection = range.clone();
                    let mut state = output.state.clone();
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(range.start),
                            egui::text::CCursor::new(range.end),
                        )));
                    state.store(ui.ctx(), id);
                } else if let Some(range) = output.cursor_range {
                    let range = range.as_sorted_char_range();
                    editor.selection = range.start.0..range.end.0;
                }
                source_enter(editor, ui, id);
                output.response.context_menu(|ui| {
                    format = toolbar::context_menu(editor, ui).or(format);
                });
                if !output.response.context_menu_opened()
                    && !ui.input(|i| i.pointer.button_down(egui::PointerButton::Secondary))
                {
                    editor.context_selection = None;
                }
                format = toolbar::selection_toolbar(editor, ui, &output).or(format);
                if let Some(kind) = format {
                    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                    let old_range = egui::text::CCursorRange::two(
                        egui::text::CCursor::new(editor.selection.start),
                        egui::text::CCursor::new(editor.selection.end),
                    );
                    let mut undoer = state.undoer();
                    let previous = editor.document.text.clone();
                    editor.selection = formatting::apply(
                        &mut editor.document.text,
                        editor.selection.clone(),
                        kind,
                    );
                    if editor.document.text != previous {
                        undoer.add_undo(&(old_range, previous));
                        editor.revision = editor.revision.wrapping_add(1);
                        editor.changed_at = Instant::now();
                    }
                    state.set_undoer(undoer);
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(editor.selection.start),
                            egui::text::CCursor::new(editor.selection.end),
                        )));
                    state.store(ui.ctx(), id);
                    ui.memory_mut(|m| m.request_focus(id));
                }
            });
        });
    });
    let y = output.state.offset.y;
    if prefs.sync_scroll
        && !synced
        && (y - editor.scroll.source_offset).abs() > 0.5
        && (ui.input(|i| {
            i.pointer
                .hover_pos()
                .is_some_and(|p| output.inner_rect.contains(p))
        }) || (ui.memory(|m| m.has_focus(id))
            && ui.input(|i| i.smooth_scroll_delta == egui::Vec2::ZERO)))
    {
        let line = editor.scroll.line_for_y((y - 10.).max(0.));
        editor.scroll.preview_target = Some(editor.preview.y_for_line(line));
        ui.ctx().request_repaint();
    }
    editor.scroll.source_offset = y;
}
fn source_enter(editor: &mut Editor, ui: &egui::Ui, id: egui::Id) {
    if !ui.memory(|m| m.has_focus(id)) {
        return;
    }
    let Some(soft) = preview_format::enter_key(ui) else {
        return;
    };
    let old = editor.document.text.clone();
    let before = editor.selection.clone();
    editor.selection = if let Some(selection) =
        formatting::list_newline(&mut editor.document.text, before.clone(), soft)
    {
        selection
    } else {
        let point = |n| old.char_indices().nth(n).map_or(old.len(), |(i, _)| i);
        let eol = if old.contains("\r\n") { "\r\n" } else { "\n" };
        editor
            .document
            .text
            .replace_range(point(before.start)..point(before.end), eol);
        let caret = before.start + eol.chars().count();
        caret..caret
    };
    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    let mut history = state.undoer();
    history.add_undo(&(
        egui::text::CCursorRange::two(
            egui::text::CCursor::new(before.start),
            egui::text::CCursor::new(before.end),
        ),
        old,
    ));
    state.set_undoer(history);
    state.store(ui.ctx(), id);
    preview_format::restore(ui, id, Some(editor.selection.clone()));
    editor.revision = editor.revision.wrapping_add(1);
    editor.changed_at = Instant::now();
    ui.ctx().request_repaint();
}
fn preview(editor: &mut Editor, prefs: &Preferences, ui: &mut egui::Ui, width: f32, height: f32) {
    let source_id = egui::Id::new(("journal-editor", &editor.document.path));
    let editable = editor.preview_editing.unwrap_or(prefs.preview_editable);
    let focused = editable && editor.preview.has_focus(ui.ctx());
    let mut format = None;
    if editable {
        editor.preview_tools.begin(ui.ctx());
        if focused && ui.is_enabled() {
            format = editor.preview_tools.shortcut(ui.ctx());
        }
    }
    let mut undone = false;
    let mut undo_selection = None;
    if focused && ui.is_enabled() && !egui::Popup::is_any_open(ui.ctx()) {
        let shifted = egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        };
        let redo = ui.input_mut(|i| i.consume_key(shifted, egui::Key::Z));
        let undo = !redo && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
        if undo || redo {
            let mut state = egui::TextEdit::load_state(ui.ctx(), source_id).unwrap_or_default();
            let mut history = state.undoer();
            let range =
                egui::text::CCursorRange::one(egui::text::CCursor::new(editor.selection.start));
            let current = (range, editor.document.text.clone());
            let next = if redo {
                history.redo(&current).cloned()
            } else {
                history.undo(&current).cloned()
            };
            if let Some((range, text)) = next {
                editor.document.text = text;
                let range = range.as_sorted_char_range();
                editor.selection = range.start.0..range.end.0;
                let point = |n| {
                    editor
                        .document
                        .text
                        .char_indices()
                        .nth(n)
                        .map_or(editor.document.text.len(), |(i, _)| i)
                };
                undo_selection = Some(point(editor.selection.start)..point(editor.selection.end));
                editor.revision = editor.revision.wrapping_add(1);
                editor.changed_at = Instant::now();
                undone = true;
            }
            state.set_undoer(history);
            state.store(ui.ctx(), source_id);
        }
    }
    if (editor.preview_revision != editor.revision && !focused) || undone {
        editor.preview_revision = editor.revision;
        editor.preview_source = markdown_source(&editor.document.text);
        editor.preview = render::Preview::document(&editor.document.text);
        if let Some(selection) = undo_selection {
            editor.preview.focus_source(selection);
        }
    }
    let top = ui.cursor().top();
    if editable {
        format = editor.preview_tools.bar(ui, width).or(format);
    }
    let height = (height - (ui.cursor().top() - top)).max(24.);
    let mut scrolling = egui::ScrollArea::vertical()
        .id_salt(("journal-preview", &editor.document.path))
        .auto_shrink([false, false])
        .max_height(height)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    let synced = editor.scroll.preview_target.is_some();
    if let Some(y) = editor.scroll.preview_target.take() {
        scrolling = scrolling.vertical_scroll_offset(y);
    }
    let output = scrolling.show_viewport(ui, |ui, viewport| {
        let content = (width - 48.).clamp(60., 820.);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            ui.add_space((width - content) / 2.);
            ui.allocate_ui_with_layout(
                egui::vec2(content, 0.),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0., 0.);
                    ui.add_space(20.);
                    editor.preview_link = editor
                        .preview
                        .show(
                            ui,
                            viewport,
                            prefs.preview_font_size.clamp(11., 28.),
                            editable,
                        )
                        .or(editor.preview_link.take());
                    ui.add_space(24.);
                },
            );
        });
    });
    sync_preview(editor, ui);
    if editable && ui.is_enabled() {
        format = editor
            .preview_tools
            .update(&mut editor.preview, editor.revision, ui)
            .or(format);
        preview_format::enter(editor, ui);
        preview_format::auto_list(editor, ui);
        sync_preview(editor, ui);
        if let Some(kind) = format {
            preview_format::apply(editor, kind, ui);
        }
        preview_format::link_dialog(editor, ui);
    }
    let y = output.state.offset.y;
    if prefs.sync_scroll
        && !synced
        && (y - editor.scroll.preview_offset).abs() > 0.5
        && ui.input(|i| {
            i.pointer
                .hover_pos()
                .is_some_and(|p| output.inner_rect.contains(p))
        })
    {
        let line = editor.preview.line_for_y((y - 20.).max(0.));
        editor.scroll.source_target = Some(editor.scroll.y_for_line(line) + 10.);
        ui.ctx().request_repaint();
    }
    editor.scroll.preview_offset = y;
}
fn sync_preview(editor: &mut Editor, ui: &egui::Ui) {
    let source_id = egui::Id::new(("journal-editor", &editor.document.path));
    if let Some(text) = editor.preview.edited_document()
        && text != editor.document.text
    {
        let mut state = egui::TextEdit::load_state(ui.ctx(), source_id).unwrap_or_default();
        let mut history = state.undoer();
        let range = egui::text::CCursorRange::one(egui::text::CCursor::new(editor.selection.start));
        history.add_undo(&(range, editor.document.text.clone()));
        state.set_undoer(history);
        state.store(ui.ctx(), source_id);
        editor.document.text = text;
        editor.revision = editor.revision.wrapping_add(1);
        editor.changed_at = Instant::now();
        ui.ctx().request_repaint();
    }
}
pub(super) fn show(
    editor: &mut Editor,
    prefs: &Preferences,
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        if prefs.preview_layout == PreviewLayout::Stacked && prefs.mode == Mode::Split {
            egui::Layout::top_down(egui::Align::Min)
        } else {
            egui::Layout::left_to_right(egui::Align::Min)
        },
        |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            match prefs.mode {
                Mode::Source => pane(editor, prefs, ui, width, height, true),
                Mode::Preview => pane(editor, prefs, ui, width, height, false),
                Mode::Split => {
                    let stacked = prefs.preview_layout == PreviewLayout::Stacked;
                    let available = if stacked { height } else { width };
                    let ratio = if editor.scroll.split_ratio > 0. {
                        editor.scroll.split_ratio
                    } else {
                        0.5
                    };
                    let first = ((available - 5.) * ratio).max(80.);
                    let second = (available - first - 5.).max(80.);
                    let origin = ui.cursor().min;
                    pane(
                        editor,
                        prefs,
                        ui,
                        if stacked { width } else { first },
                        if stacked { first } else { height },
                        true,
                    );
                    let (rect, response) = ui.allocate_exact_size(
                        if stacked {
                            egui::vec2(width, 5.)
                        } else {
                            egui::vec2(5., height)
                        },
                        egui::Sense::drag(),
                    );
                    let response = response.on_hover_cursor(if stacked {
                        egui::CursorIcon::ResizeVertical
                    } else {
                        egui::CursorIcon::ResizeHorizontal
                    });
                    ui.interact(
                        rect,
                        egui::Id::new("journal-split-divider"),
                        egui::Sense::hover(),
                    );
                    let ink = if response.hovered() || response.dragged() {
                        colors(ui).accent
                    } else {
                        colors(ui).line
                    };
                    ui.painter().line_segment(
                        if stacked {
                            [rect.left_center(), rect.right_center()]
                        } else {
                            [rect.center_top(), rect.center_bottom()]
                        },
                        egui::Stroke::new(1., ink),
                    );
                    if response.dragged()
                        && let Some(pos) = response.interact_pointer_pos()
                    {
                        editor.scroll.split_ratio = (if stacked {
                            (pos.y - origin.y) / available
                        } else {
                            (pos.x - origin.x) / available
                        })
                        .clamp(0.2, 0.8);
                    }
                    if response.double_clicked() {
                        editor.scroll.split_ratio = 0.5;
                    }
                    pane(
                        editor,
                        prefs,
                        ui,
                        if stacked { width } else { second },
                        if stacked { second } else { height },
                        false,
                    );
                }
            }
        },
    );
}
fn pane(
    editor: &mut Editor,
    prefs: &Preferences,
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    editing: bool,
) {
    let result = ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            egui::Frame::new()
                .fill(if editing {
                    colors(ui).canvas
                } else {
                    colors(ui).card
                })
                .show(ui, |ui| {
                    ui.set_min_size(egui::vec2(width, height));
                    ui.set_max_size(egui::vec2(width, height));
                    if editing {
                        source(editor, prefs, ui, width, height);
                    } else {
                        preview(editor, prefs, ui, width, height);
                    }
                });
        },
    );
    ui.interact(
        result.response.rect,
        egui::Id::new(if editing {
            "journal-source-pane"
        } else {
            "journal-preview-pane"
        }),
        egui::Sense::hover(),
    );
}
