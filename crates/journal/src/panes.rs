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
                    painter.line_segment(
                        [
                            egui::pos2(pane.left() + gutter - 5., pane.top()),
                            egui::pos2(pane.left() + gutter - 5., pane.bottom()),
                        ],
                        egui::Stroke::new(1., colors(ui).line),
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
                output.response.context_menu(|ui| {
                    ui.label(RichText::new("Format Selection").strong());
                    for (label, kind) in [
                        ("Bold", Format::Bold),
                        ("Italic", Format::Italic),
                        ("Strikethrough", Format::Strike),
                        ("Inline Code", Format::Code),
                        ("Heading 1", Format::Heading(1)),
                        ("Heading 2", Format::Heading(2)),
                        ("Heading 3", Format::Heading(3)),
                        ("Bulleted List", Format::Bullet),
                        ("Numbered List", Format::Numbered),
                        ("Task List", Format::Task),
                        ("Quote", Format::Quote),
                        ("Code Block", Format::CodeBlock),
                        ("Link", Format::Link),
                        ("Table", Format::Table),
                        ("Horizontal Rule", Format::Rule),
                    ] {
                        if ui
                            .push_id(("journal-format", label), |ui| design::button(ui, label))
                            .inner
                            .clicked()
                        {
                            format = Some(kind);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if design::button(ui, "Copy").clicked() {
                        ui.ctx().copy_text(
                            editor
                                .document
                                .text
                                .chars()
                                .skip(editor.selection.start)
                                .take(editor.selection.len())
                                .collect(),
                        );
                        ui.close();
                    }
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
                    undoer.add_undo(&(old_range, editor.document.text.clone()));
                    editor.selection = formatting::apply(
                        &mut editor.document.text,
                        editor.selection.clone(),
                        kind,
                    );
                    state.set_undoer(undoer);
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(editor.selection.start),
                            egui::text::CCursor::new(editor.selection.end),
                        )));
                    state.store(ui.ctx(), id);
                    editor.revision = editor.revision.wrapping_add(1);
                    editor.changed_at = Instant::now();
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
fn preview(editor: &mut Editor, prefs: &Preferences, ui: &mut egui::Ui, width: f32, height: f32) {
    if editor.preview_revision != editor.revision {
        editor.preview_revision = editor.revision;
        let source = markdown_source(&editor.document.text);
        let base = editor
            .document
            .text
            .lines()
            .count()
            .saturating_sub(source.lines().count());
        editor.preview = render::Preview::parse(&source, base);
        editor.preview_source = source;
    }
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
                        .show(ui, viewport, prefs.preview_font_size.clamp(11., 28.))
                        .or(editor.preview_link.take());
                    ui.add_space(24.);
                },
            );
        });
    });
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
