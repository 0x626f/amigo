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
            .clamp(bounds.left(), (bounds.right() - 380.).max(bounds.left())),
        (selection_pos.y - 46.).max(bounds.top()),
    );
    let state = editor.formatting_state();
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
                        action = controls(ui, &state, &mut editor.toolbar_menus, false, false);
                    });
                });
        });
    action
}

fn menu_action(
    ui: &mut egui::Ui,
    label: &str,
    kind: Format,
    state: &formatting::SelectionState,
    action: &mut Option<Format>,
) {
    if ui
        .add_enabled_ui(state.can_apply(kind), |ui| {
            ui.selectable_label(state.is_active(kind), label)
        })
        .inner
        .clicked()
    {
        *action = Some(kind);
        ui.close();
    }
}

fn heading_items(
    ui: &mut egui::Ui,
    state: &formatting::SelectionState,
    action: &mut Option<Format>,
) {
    ui.set_min_width(180.);
    menu_action(ui, "Normal", Format::Heading(0), state, action);
    ui.separator();
    for level in 1..=6 {
        menu_action(
            ui,
            &format!("Heading {level}"),
            Format::Heading(level),
            state,
            action,
        );
    }
}

fn list_items(ui: &mut egui::Ui, state: &formatting::SelectionState, action: &mut Option<Format>) {
    for (label, kind) in [
        ("Bulleted List", Format::Bullet),
        ("Numbered List", Format::Numbered),
        ("Task List", Format::Task),
    ] {
        menu_action(ui, label, kind, state, action);
    }
}

fn language_items(ui: &mut egui::Ui, current: Option<&str>, action: &mut Option<Format>) {
    ui.set_min_width(180.);
    egui::ScrollArea::vertical()
        .max_height(280.)
        .show(ui, |ui| {
            for (label, language) in [
                ("Plain Text", ""),
                ("Bash", "bash"),
                ("C", "c"),
                ("C++", "cpp"),
                ("CSS", "css"),
                ("Go", "go"),
                ("HTML", "html"),
                ("Java", "java"),
                ("JavaScript", "javascript"),
                ("JSON", "json"),
                ("Markdown", "markdown"),
                ("Python", "python"),
                ("Rust", "rust"),
                ("SQL", "sql"),
                ("TypeScript", "typescript"),
                ("YAML", "yaml"),
            ] {
                if ui
                    .selectable_label(current == Some(language), label)
                    .clicked()
                {
                    *action = Some(Format::CodeBlock(language));
                    ui.close();
                }
            }
        });
}

fn block_items(
    ui: &mut egui::Ui,
    language: Option<&str>,
    state: &formatting::SelectionState,
    action: &mut Option<Format>,
) {
    menu_action(ui, "Quote", Format::Quote, state, action);
    ui.menu_button("Code Block", |ui| language_items(ui, language, action));
    menu_action(ui, "Table", Format::Table, state, action);
    menu_action(ui, "Horizontal Rule", Format::Rule, state, action);
}

pub(super) fn context_menu(editor: &mut Editor, ui: &mut egui::Ui) -> Option<Format> {
    let state = editor.formatting_state();
    let mut action = None;
    action = format_menu(ui, &state, false).or(action);
    ui.separator();
    if ui
        .add_enabled(!editor.selection.is_empty(), egui::Button::new("Copy"))
        .clicked()
    {
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
    action
}

/// Shared source and preview formatting controls.
pub(super) fn controls(
    ui: &mut egui::Ui,
    state: &formatting::SelectionState,
    menus: &mut Vec<egui::Id>,
    inline_only: bool,
    preview: bool,
) -> Option<Format> {
    let mut action = None;
    let control_id = |name: &str| {
        if preview {
            egui::Id::new(("preview", name))
        } else {
            egui::Id::new(name)
        }
    };
    let label = match state.heading {
        Some(0) => "Normal".into(),
        Some(level) => format!("H{level}"),
        None => "Style".into(),
    };
    let headings = design::dropdown_button(
        ui,
        control_id("journal-floating-headings-button"),
        &label,
        egui::vec2(112., 30.),
    );
    ui.interact(
        headings.rect,
        control_id("journal-floating-headings"),
        egui::Sense::hover(),
    );
    egui::Popup::menu(&headings).show(|ui| {
        if inline_only {
            ui.disable();
        }
        heading_items(ui, state, &mut action);
    });
    *menus = vec![egui::Popup::default_response_id(&headings)];
    ui.separator();
    for (label, icon, kind) in [
        ("Bold", Icon::Bold, Format::Bold),
        ("Italic", Icon::Italic, Format::Italic),
        ("Strikethrough", Icon::Strikethrough, Format::Strike),
        ("Link", Icon::Link, Format::Link),
    ] {
        if ui
            .add_enabled_ui(state.can_apply(kind), |ui| {
                design::selected_icon_button(
                    ui,
                    if preview {
                        egui::Id::new(("journal-preview-format", label))
                    } else {
                        egui::Id::new(("journal-floating-format", label))
                    },
                    icon,
                    label,
                    30.,
                    state.is_active(kind),
                )
            })
            .inner
            .clicked()
        {
            action = Some(kind);
        }
    }
    let code = design::icon_button(
        ui,
        control_id("journal-floating-code"),
        Icon::Code,
        "Code Formatting",
        30.,
    );
    let language = state.language.as_deref();
    egui::Popup::menu(&code).show(|ui| {
        menu_action(ui, "Inline Code", Format::Code, state, &mut action);
        ui.separator();
        design::eyebrow(ui, "CODE BLOCK LANGUAGE");
        ui.add_enabled_ui(!inline_only, |ui| language_items(ui, language, &mut action));
    });
    menus.push(egui::Popup::default_response_id(&code));
    ui.separator();
    let lists = design::icon_button(
        ui,
        control_id("journal-floating-lists"),
        Icon::List,
        "Lists and Blocks",
        30.,
    );
    egui::Popup::menu(&lists).show(|ui| {
        if inline_only {
            ui.disable();
        }
        design::eyebrow(ui, "LISTS");
        list_items(ui, state, &mut action);
        ui.separator();
        design::eyebrow(ui, "BLOCKS");
        block_items(ui, language, state, &mut action);
    });
    menus.push(egui::Popup::default_response_id(&lists));
    action
}

pub(super) fn format_menu(
    ui: &mut egui::Ui,
    state: &formatting::SelectionState,
    inline_only: bool,
) -> Option<Format> {
    let mut action = None;
    let language = state.language.as_deref();
    ui.set_min_width(200.);
    design::eyebrow(ui, "TEXT STYLE");
    for (label, kind) in [
        ("Bold", Format::Bold),
        ("Italic", Format::Italic),
        ("Strikethrough", Format::Strike),
        ("Inline Code", Format::Code),
        ("Link", Format::Link),
    ] {
        menu_action(ui, label, kind, state, &mut action);
    }
    ui.separator();
    ui.add_enabled_ui(!inline_only, |ui| {
        ui.menu_button("Headings", |ui| heading_items(ui, state, &mut action));
        ui.menu_button("Lists", |ui| list_items(ui, state, &mut action));
        ui.menu_button("Blocks", |ui| block_items(ui, language, state, &mut action));
    });

    action
}
