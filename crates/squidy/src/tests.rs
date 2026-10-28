use super::*;
fn setup() -> (SquidyTab, egui::Context) {
    let mut board = Board::new("Research");
    for name in ["Alpha", "Beta", "Gamma"] {
        board.put_card(Card::new(name)).unwrap();
    }
    board
        .put_link(Link::new("Alpha", "Beta", "supports"))
        .unwrap();
    board
        .put_link(Link::new("Alpha", "Beta", "contradicts"))
        .unwrap();
    board
        .put_link(Link::new("Beta", "Gamma", "supports"))
        .unwrap();
    let tab = SquidyTab {
        board: Some(board),
        ..Default::default()
    };
    let ctx = egui::Context::default();
    design::install(&ctx);
    (tab, ctx)
}
fn frame(
    tab: &mut SquidyTab,
    ctx: &egui::Context,
    mut events: Vec<egui::Event>,
    size: egui::Vec2,
) -> egui::FullOutput {
    if let Some(modifiers) = events.iter().rev().find_map(|event| match event {
        egui::Event::PointerButton { modifiers, .. } | egui::Event::Key { modifiers, .. } => {
            Some(*modifiers)
        }
        _ => None,
    }) {
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
    }
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                tab.show(ui);
            });
            tab.dialogs(ui.ctx());
        },
    );
    output.textures_delta.clear();
    output
}
fn text_rect(output: &egui::FullOutput, text: &str) -> egui::Rect {
    fn find(shape: &egui::Shape, text: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(s) if s.galley.text() == text => {
                Some(egui::Rect::from_min_size(s.pos, s.galley.size()))
            }
            egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, text)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .rev()
        .find_map(|s| find(&s.shape, text).filter(|r| s.clip_rect.contains_rect(*r)))
        .unwrap_or_else(|| panic!("Missing {text}"))
}
fn click(
    tab: &mut SquidyTab,
    ctx: &egui::Context,
    output: &egui::FullOutput,
    text: &str,
    size: egui::Vec2,
) {
    let pos = text_rect(output, text).center();
    for pressed in [true, false] {
        frame(
            tab,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            size,
        );
    }
}
fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}
#[test]
fn board_dialog_keeps_duplicate_name_for_correction() {
    let (mut tab, ctx) = setup();
    let size = egui::vec2(980., 640.);
    tab.boards.push(Entry {
        id: tab.board.as_ref().unwrap().id.clone(),
        name: "Research".into(),
    });
    tab.name_dialog = Some(NameDialog::Create("research".into()));
    let mut output = frame(&mut tab, &ctx, vec![], size);
    for _ in 0..3 {
        output = frame(&mut tab, &ctx, vec![], size);
    }
    click(&mut tab, &ctx, &output, "Save Board", size);
    assert!(matches!(&tab.name_dialog, Some(NameDialog::Create(name)) if name == "research"));
    assert!(tab.error.contains("already exists"));
    tab.name_dialog = Some(NameDialog::Create("Second board".into()));
    let output = frame(&mut tab, &ctx, vec![], size);
    click(&mut tab, &ctx, &output, "Save Board", size);
    assert!(tab.name_dialog.is_none());
    assert!(tab.error.is_empty());
}
#[test]
fn card_dialog_rejects_duplicate_names_keeps_drafts_and_commits_on_save() {
    for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
        let (mut tab, ctx) = setup();
        tab.new_card(None);
        {
            let e = tab.editor.as_mut().unwrap();
            e.card.name = "Alpha".into();
            e.card.brief = "Duplicate".into();
        }
        let mut output = frame(&mut tab, &ctx, vec![], size);
        for _ in 0..3 {
            output = frame(&mut tab, &ctx, vec![], size);
        }
        click(&mut tab, &ctx, &output, "Save Card", size);
        assert_eq!(tab.board.as_ref().unwrap().cards.len(), 3);
        assert!(
            tab.editor
                .as_ref()
                .unwrap()
                .error
                .contains("already exists")
        );
        tab.editor.as_mut().unwrap().card.name = "Delta".into();
        let output = frame(&mut tab, &ctx, vec![], size);
        click(&mut tab, &ctx, &output, "Save Card", size);
        assert!(tab.editor.is_none());
        assert_eq!(
            tab.board.as_ref().unwrap().card("Delta").unwrap().brief,
            "Duplicate"
        );
        let original = tab.board.clone();
        tab.edit_card(&tab.board.as_ref().unwrap().cards[0].id.clone());
        tab.editor.as_mut().unwrap().card.data = "unsaved".into();
        frame(&mut tab, &ctx, vec![key(egui::Key::Escape)], size);
        assert!(tab.editor.is_none());
        assert_eq!(tab.board, original);
    }
}
#[test]
fn canvas_drag_and_pointer_zoom_preserve_world_positions() {
    let (mut tab, ctx) = setup();
    let size = egui::vec2(1440., 900.);
    for _ in 0..4 {
        frame(&mut tab, &ctx, vec![], size);
    }
    let card = tab.board.as_ref().unwrap().cards[0].id.clone();
    let pos = ctx
        .read_response(egui::Id::new(("squidy-card", &card)))
        .unwrap()
        .rect
        .center();
    let before = tab.positions[&card];
    frame(
        &mut tab,
        &ctx,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        size,
    );
    let destination = pos + egui::vec2(95., 62.);
    frame(
        &mut tab,
        &ctx,
        vec![egui::Event::PointerMoved(destination)],
        size,
    );
    frame(
        &mut tab,
        &ctx,
        vec![egui::Event::PointerButton {
            pos: destination,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        size,
    );
    let after = tab.board.as_ref().unwrap().cards[0].position.unwrap();
    assert!((after[0] - before[0] - 95.).abs() < 1.);
    assert!((after[1] - before[1] - 62.).abs() < 1.);
    let board = tab.board.as_ref().unwrap();
    let anchor = tab.viewport.center();
    let world = (anchor - tab.viewport.min - egui::vec2(board.pan[0], board.pan[1])) / board.zoom;
    frame(
        &mut tab,
        &ctx,
        vec![egui::Event::PointerMoved(anchor), egui::Event::Zoom(1.25)],
        size,
    );
    let board = tab.board.as_ref().unwrap();
    let actual = (anchor - tab.viewport.min - egui::vec2(board.pan[0], board.pan[1])) / board.zoom;
    assert!((actual - world).length() < 1.);
    assert!((board.zoom - 1.25).abs() < 0.01);
    let pan = board.pan;
    let saved_position = board.cards[0].position;
    let pos = ctx
        .read_response(egui::Id::new(("squidy-card", &card)))
        .unwrap()
        .rect
        .center();
    frame(
        &mut tab,
        &ctx,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Middle,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        size,
    );
    let destination = pos + egui::vec2(60., 25.);
    frame(
        &mut tab,
        &ctx,
        vec![egui::Event::PointerMoved(destination)],
        size,
    );
    frame(
        &mut tab,
        &ctx,
        vec![egui::Event::PointerButton {
            pos: destination,
            button: egui::PointerButton::Middle,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        size,
    );
    let board = tab.board.as_ref().unwrap();
    assert_eq!(board.cards[0].position, saved_position);
    assert!((board.pan[0] - pan[0] - 60.).abs() < 1.);
    assert!((board.pan[1] - pan[1] - 25.).abs() < 1.);
}
#[test]
fn path_dialog_filters_parallel_edges_and_highlights_the_selected_route() {
    let (mut tab, ctx) = setup();
    let size = egui::vec2(980., 640.);
    tab.query.from = "Alpha".into();
    tab.query.to = "Gamma".into();
    tab.query.include = "supports".into();
    tab.query.exclude = "contradicts".into();
    tab.path_open = true;
    let mut output = frame(&mut tab, &ctx, vec![], size);
    for _ in 0..4 {
        output = frame(&mut tab, &ctx, vec![], size);
    }
    click(&mut tab, &ctx, &output, "Find Path", size);
    assert!(tab.error.is_empty());
    assert_eq!(tab.path.as_ref().unwrap().links.len(), 2);
    assert!(tab.path_message.contains("supports"));
    assert!(!tab.path_open);
}
#[test]
fn opening_delete_confirmation_with_enter_cannot_delete_in_the_same_frame() {
    let (mut tab, ctx) = setup();
    let size = egui::vec2(980., 640.);
    let id = tab.board.as_ref().unwrap().cards[0].id.clone();
    tab.confirm_delete(Delete::Card(id.clone()));
    frame(&mut tab, &ctx, vec![key(egui::Key::Enter)], size);
    assert_eq!(tab.board.as_ref().unwrap().cards.len(), 3);
    assert!(tab.delete.is_some());
    frame(&mut tab, &ctx, vec![key(egui::Key::Escape)], size);
    assert!(tab.delete.is_none());
    assert_eq!(tab.board.as_ref().unwrap().cards.len(), 3);
    tab.confirm_delete(Delete::Card(id));
    for _ in 0..4 {
        frame(&mut tab, &ctx, vec![], size);
    }
    let output = frame(&mut tab, &ctx, vec![], size);
    click(&mut tab, &ctx, &output, "Delete", size);
    assert_eq!(tab.board.as_ref().unwrap().cards.len(), 2);
    assert!(
        tab.board
            .as_ref()
            .unwrap()
            .links
            .iter()
            .all(|e| e.from != "Alpha" && e.to != "Alpha")
    );
}

fn click_id(
    tab: &mut SquidyTab,
    ctx: &egui::Context,
    id: egui::Id,
    size: egui::Vec2,
    modifiers: egui::Modifiers,
) {
    // Let initial focus finish scrolling before targeting a control.
    for _ in 0..12 {
        frame(tab, ctx, vec![], size);
    }
    let pos = ctx
        .read_response(id)
        .unwrap_or_else(|| panic!("Missing {id:?}"))
        .rect
        .center();
    for pressed in [true, false] {
        frame(
            tab,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                },
            ],
            size,
        );
    }
    frame(tab, ctx, vec![], size);
}
fn click_card(tab: &mut SquidyTab, ctx: &egui::Context, name: &str, size: egui::Vec2, alt: bool) {
    let id = tab.board.as_ref().unwrap().card(name).unwrap().id.clone();
    click_id(
        tab,
        ctx,
        egui::Id::new(("squidy-card", id)),
        size,
        egui::Modifiers {
            alt,
            ..Default::default()
        },
    );
}
#[test]
fn alt_click_selects_connection_endpoints_before_editing_and_committing_properties() {
    for alt_target in [true, false] {
        let (mut tab, ctx) = setup();
        let size = egui::vec2(1440., 900.);
        let original = tab.board.clone();
        click_card(&mut tab, &ctx, "Alpha", size, true);
        assert!(matches!(&tab.picking,Some(Picking::Connection(Some(name))) if name=="Alpha"));
        assert!(tab.link.is_none() && tab.editor.is_none());
        click_card(&mut tab, &ctx, "Beta", size, alt_target);
        let link = tab.link.as_ref().unwrap();
        assert_eq!((&*link.from, &*link.to), ("Alpha", "Beta"));
        assert!(link.title.is_empty());
        assert!(ctx.memory(|m| m.has_focus(egui::Id::new(("squidy-link-title", &link.id)))));
        assert_eq!(tab.board, original);
        tab.link.as_mut().unwrap().title = "new relationship".into();
        let output = frame(&mut tab, &ctx, vec![], size);
        click(&mut tab, &ctx, &output, "Save Connection", size);
        assert!(tab.link.is_none());
        assert_eq!(tab.board.as_ref().unwrap().links.len(), 4);
    }
}
#[test]
fn alt_f_and_route_tool_select_two_cards_and_escape_cancels_selection() {
    let (mut tab, ctx) = setup();
    let size = egui::vec2(1440., 900.);
    frame(
        &mut tab,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::F,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        }],
        size,
    );
    assert!(matches!(tab.picking, Some(Picking::Path(None))));
    assert!(!tab.path_open);
    click_card(&mut tab, &ctx, "Alpha", size, false);
    assert!(!tab.path_open);
    frame(
        &mut tab,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::F,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers: egui::Modifiers::ALT,
        }],
        size,
    );
    assert!(matches!(&tab.picking,Some(Picking::Path(Some(name))) if name=="Alpha"));
    click_card(&mut tab, &ctx, "Beta", size, false);
    assert!(tab.path_open && tab.picking.is_none());
    assert_eq!((&*tab.query.from, &*tab.query.to), ("Alpha", "Beta"));
    frame(&mut tab, &ctx, vec![key(egui::Key::Escape)], size);
    click_id(
        &mut tab,
        &ctx,
        egui::Id::new("squidy-find-path-open"),
        size,
        egui::Modifiers::NONE,
    );
    assert!(matches!(tab.picking, Some(Picking::Path(None))));
    frame(&mut tab, &ctx, vec![key(egui::Key::Escape)], size);
    assert!(tab.picking.is_none());
    assert!(tab.link.is_none() && !tab.path_open);
}
#[test]
fn selectors_offer_cards_and_distinct_edge_titles_without_closing_the_filter_dialog() {
    let (mut tab, ctx) = setup();
    let size = egui::vec2(980., 640.);
    tab.new_link("Alpha", "Alpha");
    click_id(
        &mut tab,
        &ctx,
        egui::Id::new(("squidy-link-card", "To")).with("choices"),
        size,
        egui::Modifiers::NONE,
    );
    let output = frame(&mut tab, &ctx, vec![], size);
    click(&mut tab, &ctx, &output, "Gamma", size);
    assert_eq!(tab.link.as_ref().unwrap().to, "Gamma");
    frame(&mut tab, &ctx, vec![key(egui::Key::Escape)], size);
    tab.path_open = true;
    click_id(
        &mut tab,
        &ctx,
        egui::Id::new("squidy-include-titles").with("choices"),
        size,
        egui::Modifiers::NONE,
    );
    let output = frame(&mut tab, &ctx, vec![], size);
    click(&mut tab, &ctx, &output, "supports", size);
    assert_eq!(tab.query.include, "supports");
    assert!(tab.path_open);
    let output = frame(&mut tab, &ctx, vec![], size);
    click(&mut tab, &ctx, &output, "contradicts", size);
    assert_eq!(tab.query.include.lines().count(), 2);
    frame(&mut tab, &ctx, vec![key(egui::Key::Escape)], size);
    assert!(tab.path_open);
}
#[test]
fn card_data_previews_edits_on_double_click_and_color_swatches_remain_drafts() {
    for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
        let (mut tab, ctx) = setup();
        let id = tab.board.as_ref().unwrap().cards[0].id.clone();
        tab.board.as_mut().unwrap().cards[0].data = "# Payload\n\n**Evidence**".into();
        let original = tab.board.clone();
        tab.edit_card(&id);
        let mut output = frame(&mut tab, &ctx, vec![], size);
        for _ in 0..3 {
            output = frame(&mut tab, &ctx, vec![], size);
        }
        assert!(!tab.editor.as_ref().unwrap().data_editor.editing);
        let pos = text_rect(&output, "Payload").center();
        for _ in 0..2 {
            for pressed in [true, false] {
                frame(
                    &mut tab,
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
                    size,
                );
            }
        }
        frame(&mut tab, &ctx, vec![], size);
        assert!(tab.editor.as_ref().unwrap().data_editor.editing);
        assert!(ctx.memory(|m| m.has_focus(egui::Id::new(("squidy-card-data", &id)))));
        frame(
            &mut tab,
            &ctx,
            vec![egui::Event::Text(" addition".into())],
            size,
        );
        let color_id = egui::Id::new(("squidy-card-color", &id));
        click_id(&mut tab, &ctx, color_id, size, egui::Modifiers::NONE);
        assert!(ctx.read_response(color_id).unwrap().rect.width() <= 28.);
        click_id(
            &mut tab,
            &ctx,
            color_id.with(("swatch", "Green")),
            size,
            egui::Modifiers::NONE,
        );
        assert_eq!(tab.editor.as_ref().unwrap().card.color, [52, 199, 89]);
        assert_eq!(tab.board, original);
        frame(&mut tab, &ctx, vec![key(egui::Key::Escape)], size);
        assert!(tab.editor.is_none());
        assert_eq!(tab.board, original);
        tab.edit_card(&id);
        assert!(!tab.editor.as_ref().unwrap().data_editor.editing);
    }
}
#[test]
fn board_tools_share_the_board_surface_with_creation_first_and_zoom_on_the_right() {
    for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
        let (mut tab, ctx) = setup();
        let mut output = frame(&mut tab, &ctx, vec![], size);
        for _ in 0..3 {
            output = frame(&mut tab, &ctx, vec![], size);
        }
        let route = ctx
            .read_response(egui::Id::new("squidy-find-path-open"))
            .unwrap()
            .rect;
        let plus = ctx
            .read_response(egui::Id::new("squidy-new-card"))
            .unwrap()
            .rect;
        let link = ctx
            .read_response(egui::Id::new("squidy-link-cards"))
            .unwrap()
            .rect;
        assert!(plus.bottom() < link.top() && link.bottom() < route.top());
        assert_eq!(plus.center().x, link.center().x);
        assert_eq!(link.center().x, route.center().x);
        assert!(route.right() < tab.viewport.left());
        assert!(plus.top() > tab.viewport.top());
        let background = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(shape)
                    if shape.rect.contains_rect(tab.viewport)
                        && shape.rect.contains_rect(plus)
                        && shape.rect.contains_rect(route) =>
                {
                    Some(shape)
                }
                _ => None,
            })
            .expect("The board and toolbar must share one background");
        assert!(background.rect.left() <= plus.left());
        assert!(text_rect(&output, "100%").center().x > tab.viewport.center().x);
        assert!(text_rect(&output, "100%").right() < size.x);
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape {
                assert!(
                    !["Arrange", "Fit", "Find Path", "New Card", "Link Cards"]
                        .contains(&text.galley.text())
                );
            }
        }
    }
}

#[test]
fn alt_f_handles_physical_keys_and_card_focus_but_preserves_dialog_input() {
    for (logical, physical) in [(egui::Key::F, None), (egui::Key::G, Some(egui::Key::F))] {
        let (mut tab, ctx) = setup();
        let size = egui::vec2(980., 640.);
        click_card(&mut tab, &ctx, "Alpha", size, false);
        assert!(tab.selected.is_some() && tab.picking.is_none());
        let shortcut = || egui::Event::Key {
            key: logical,
            physical_key: physical,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        };
        frame(&mut tab, &ctx, vec![shortcut()], size);
        assert!(matches!(tab.picking, Some(Picking::Path(None))));
        // A held shortcut must not erase the first endpoint.
        click_card(&mut tab, &ctx, "Alpha", size, false);
        frame(&mut tab, &ctx, vec![shortcut()], size);
        assert!(matches!(&tab.picking, Some(Picking::Path(Some(name))) if name == "Alpha"));
        click_card(&mut tab, &ctx, "Beta", size, false);
        assert!(tab.path_open && tab.picking.is_none());
        frame(
            &mut tab,
            &ctx,
            vec![egui::Event::Key {
                key: logical,
                physical_key: physical,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            size,
        );
        frame(&mut tab, &ctx, vec![shortcut()], size);
        assert!(tab.path_open && tab.picking.is_none());
    }
    let (mut tab, ctx) = setup();
    let size = egui::vec2(980., 640.);
    tab.new_card(None);
    frame(&mut tab, &ctx, vec![], size);
    frame(
        &mut tab,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::F,
            physical_key: Some(egui::Key::F),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        }],
        size,
    );
    assert!(tab.editor.is_some() && tab.picking.is_none());
}
#[test]
fn selection_highlights_eligible_cards_marks_the_source_and_keeps_the_canvas_stationary() {
    for dark in [false, true] {
        let (mut tab, ctx) = setup();
        ctx.set_theme(if dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        });
        let size = egui::vec2(1440., 900.);
        frame(&mut tab, &ctx, vec![], size);
        let viewport = tab.viewport;
        let original = tab.board.clone();
        click_id(
            &mut tab,
            &ctx,
            egui::Id::new("squidy-link-cards"),
            size,
            egui::Modifiers::NONE,
        );
        assert!(matches!(tab.picking, Some(Picking::Connection(None))));
        let output = frame(&mut tab, &ctx, vec![], size);
        assert_eq!(tab.viewport, viewport);
        let cards: Vec<_> = tab
            .board
            .as_ref()
            .unwrap()
            .cards
            .iter()
            .map(|c| c.id.clone())
            .collect();
        let highlighted = |output: &egui::FullOutput, id: &str| {
            let rect = ctx
                .read_response(egui::Id::new(("squidy-card", id)))
                .unwrap()
                .rect;
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(shape) if shape.rect == rect && shape.stroke.width > 0. => {
                        Some(shape.stroke)
                    }
                    _ => None,
                })
                .expect("Card outline")
        };
        let accent = highlighted(&output, &cards[0]).color;
        for id in &cards {
            let stroke = highlighted(&output, id);
            assert_eq!(stroke.color, accent);
            assert!(stroke.width >= 1.5);
        }
        click_card(&mut tab, &ctx, "Alpha", size, false);
        let output = frame(&mut tab, &ctx, vec![], size);
        assert_eq!(tab.viewport, viewport);
        assert_eq!(highlighted(&output, &cards[0]).width, 3.);
        assert_eq!(highlighted(&output, &cards[1]).width, 1.5);
        // Selection uses card outlines/checks, with no instruction strip.
        assert!(output.shapes.iter().all(|shape| match &shape.shape {
            egui::Shape::Text(text) =>
                !text.galley.text().starts_with("Select the starting")
                    && !text.galley.text().starts_with("Connect from Alpha:")
                    && text.galley.text() != "Cancel (Esc)",
            _ => true,
        }));
        click_card(&mut tab, &ctx, "Beta", size, false);
        let link = tab.link.as_ref().unwrap();
        assert_eq!((&*link.from, &*link.to), ("Alpha", "Beta"));
        assert_eq!(tab.board, original);
        frame(&mut tab, &ctx, vec![key(egui::Key::Escape)], size);
        click_id(
            &mut tab,
            &ctx,
            egui::Id::new("squidy-new-card"),
            size,
            egui::Modifiers::NONE,
        );
        assert!(tab.editor.is_some() && tab.picking.is_none());
    }
}

#[test]
fn connections_default_to_bidirectional_and_direction_and_swap_remain_drafts_until_saved() {
    for dark in [false, true] {
        for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
            let (mut tab, ctx) = setup();
            design::apply_theme(&ctx, dark);
            tab.board.as_mut().unwrap().links.truncate(1);
            let original = tab.board.clone();
            tab.new_link("Gamma", "Alpha");
            assert!(!tab.link.as_ref().unwrap().directed);
            tab.link.as_mut().unwrap().title = "New connection".into();
            let mut output = frame(&mut tab, &ctx, vec![], size);
            for _ in 0..12 {
                output = frame(&mut tab, &ctx, vec![], size);
            }
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let modal = ctx
                .memory(|m| m.area_rect(egui::Id::new("squidy-link-editor")))
                .unwrap();
            assert!(viewport.contains_rect(modal));
            for label in ["Bidirectional", "Directed", "From", "To", "Save Connection"] {
                assert!(modal.contains_rect(text_rect(&output, label)));
            }
            let swap = ctx
                .read_response(egui::Id::new("squidy-link-swap"))
                .unwrap()
                .rect;
            assert!(swap.left() > text_rect(&output, "Directed").right());
            click_id(
                &mut tab,
                &ctx,
                egui::Id::new("squidy-link-swap"),
                size,
                egui::Modifiers::NONE,
            );
            let link = tab.link.as_ref().unwrap();
            assert_eq!((&*link.from, &*link.to), ("Alpha", "Gamma"));
            let output = frame(&mut tab, &ctx, vec![], size);
            click(&mut tab, &ctx, &output, "Directed", size);
            assert!(tab.link.as_ref().unwrap().directed);
            assert_eq!(tab.board, original);
            frame(&mut tab, &ctx, vec![key(egui::Key::Escape)], size);
            assert!(tab.link.is_none());
            assert_eq!(tab.board, original);

            let id = tab.board.as_ref().unwrap().links[0].id.clone();
            click_id(
                &mut tab,
                &ctx,
                egui::Id::new(("squidy-edge", &id)),
                size,
                egui::Modifiers::NONE,
            );
            let output = frame(&mut tab, &ctx, vec![], size);
            click(&mut tab, &ctx, &output, "Directed", size);
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("squidy-link-swap")));
            frame(&mut tab, &ctx, vec![key(egui::Key::Enter)], size);
            let draft = tab.link.as_ref().unwrap();
            assert_eq!((&*draft.from, &*draft.to), ("Beta", "Alpha"));
            assert!(draft.directed);
            assert_eq!(tab.board, original);
            let output = frame(&mut tab, &ctx, vec![], size);
            click(&mut tab, &ctx, &output, "Save Connection", size);
            let board = tab.board.as_ref().unwrap();
            assert_eq!(board.links[0].id, id);
            assert!(board.links[0].directed);
            assert!(
                board
                    .path("Beta", "Alpha", &Default::default())
                    .unwrap()
                    .is_some()
            );
            assert!(
                board
                    .path("Alpha", "Beta", &Default::default())
                    .unwrap()
                    .is_none()
            );

            click_id(
                &mut tab,
                &ctx,
                egui::Id::new(("squidy-edge", &id)),
                size,
                egui::Modifiers::NONE,
            );
            let output = frame(&mut tab, &ctx, vec![], size);
            click(&mut tab, &ctx, &output, "Bidirectional", size);
            let output = frame(&mut tab, &ctx, vec![], size);
            click(&mut tab, &ctx, &output, "Save Connection", size);
            assert!(!tab.board.as_ref().unwrap().links[0].directed);
            assert!(
                tab.board
                    .as_ref()
                    .unwrap()
                    .path("Alpha", "Beta", &Default::default())
                    .unwrap()
                    .is_some()
            );
        }
    }
}

#[test]
fn canvas_draws_arrows_at_both_ends_only_for_bidirectional_connections() {
    for directed in [false, true] {
        for (from, to) in [("Alpha", "Beta"), ("Beta", "Alpha"), ("Alpha", "Alpha")] {
            let (mut tab, ctx) = setup();
            let board = tab.board.as_mut().unwrap();
            for (index, card) in board.cards.iter_mut().enumerate() {
                card.position = Some([80. + index as f32 * 340., 200.]);
            }
            let mut link = Link::new(from, to, "Direction test");
            link.directed = directed;
            link.color = [231, 64, 173];
            board.links = vec![link];
            let output = frame(&mut tab, &ctx, vec![], egui::vec2(1440., 900.));
            let arrows = output.shapes.iter().filter(|shape| {
                matches!(&shape.shape, egui::Shape::Path(path) if path.points.len() == 3 && path.fill == egui::Color32::from_rgb(231, 64, 173))
            }).count();
            assert_eq!(arrows, if directed { 1 } else { 2 }, "{from} to {to}");
        }
    }
}

#[test]
fn path_result_wraps_card_chips_keeps_direction_icons_and_clears_the_highlight() {
    for dark in [false, true] {
        for size in [egui::vec2(980., 640.), egui::vec2(1440., 900.)] {
            let (mut tab, ctx) = setup();
            design::apply_theme(&ctx, dark);
            let mut board = Board::new("Route presentation");
            let names = [
                "A very long starting card name that must stay inside the route summary instead of overflowing the window",
                "Working hypothesis",
                "Laboratory observations",
                "Next expedition",
            ];
            for (index, name) in names.iter().enumerate() {
                let mut card = Card::new(name);
                card.position = Some([80. + index as f32 * 300., 180.]);
                board.put_card(card).unwrap();
            }
            for (index, (from, to, title, directed)) in [
                (names[0], names[1], "supports", false),
                (names[1], names[2], "tests", true),
                (names[3], names[2], "informs", true),
            ]
            .into_iter()
            .enumerate()
            {
                let mut link = Link::new(from, to, title);
                link.directed = directed;
                link.color = [200, 80 + index as u8 * 30, 70];
                board.put_link(link).unwrap();
            }
            tab.path = board
                .path(
                    names[0],
                    names[3],
                    &amigo_core::squidy::Filter {
                        directed: false,
                        ..Default::default()
                    },
                )
                .unwrap();
            tab.board = Some(board);
            tab.path_message = "Obsolete plain text route".into();
            let mut output = frame(&mut tab, &ctx, vec![], size);
            for _ in 0..6 {
                output = frame(&mut tab, &ctx, vec![], size);
            }
            let result = ctx
                .read_response(egui::Id::new("squidy-path-result"))
                .unwrap()
                .rect;
            let clear = ctx
                .read_response(egui::Id::new("squidy-clear-path"))
                .unwrap()
                .rect;
            assert_eq!(clear.size(), egui::vec2(32., 32.));
            assert!(result.right() - clear.right() < 30.);
            assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(result));
            for name in names {
                let card = ctx
                    .read_response(egui::Id::new(("squidy-path-card", name)))
                    .unwrap()
                    .rect;
                assert!(result.contains_rect(card), "{name}: {card:?}, {result:?}");
            }
            let board = tab.board.as_ref().unwrap();
            for (index, edge) in board.links.iter().enumerate() {
                let connector = ctx
                    .read_response(egui::Id::new(("squidy-path-connection", &edge.id)))
                    .unwrap()
                    .rect;
                let card = ctx
                    .read_response(egui::Id::new(("squidy-path-card", names[index + 1])))
                    .unwrap()
                    .rect;
                assert!(result.contains_rect(connector));
                assert!(connector.right() < card.left());
                assert!((connector.center().y - card.center().y).abs() < 1.);
                let color = egui::Color32::from_rgb(edge.color[0], edge.color[1], edge.color[2]);
                let tips: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Path(path)
                            if path.points.len() == 3
                                && path.stroke.color == egui::epaint::ColorMode::Solid(color)
                                && path.fill == egui::Color32::TRANSPARENT =>
                        {
                            Some(path.points[1])
                        }
                        _ => None,
                    })
                    .filter(|tip| connector.contains(*tip))
                    .collect();
                assert_eq!(tips.len(), if edge.directed { 1 } else { 2 });
                assert!(tips.iter().all(|tip| (tip.y - card.center().y).abs() < 1.));
                if edge.directed {
                    let expected = if index == 1 {
                        connector.right() - 3.
                    } else {
                        connector.left() + 3.
                    };
                    assert!((tips[0].x - expected).abs() < 1.);
                }
            }
            assert!(tab.viewport.top() > result.bottom());
            assert!(output.shapes.iter().all(|shape| match &shape.shape {
                egui::Shape::Text(text) =>
                    !text.galley.text().contains("Shortest path")
                        && text.galley.text() != "Obsolete plain text route",
                _ => true,
            }));
            click_id(
                &mut tab,
                &ctx,
                egui::Id::new("squidy-clear-path"),
                size,
                egui::Modifiers::NONE,
            );
            assert!(tab.path.is_none() && tab.path_message.is_empty());
            for _ in 0..3 {
                frame(&mut tab, &ctx, vec![], size);
            }
            assert!(
                ctx.read_response(egui::Id::new("squidy-path-result"))
                    .is_none()
            );
        }
    }
}
