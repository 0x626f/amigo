use super::*;
struct Harness {
    workspace: Workspace,
    state: TasksTab,
    action: Option<TasksAction>,
}
fn harness() -> (Harness, egui::Context) {
    let ctx = egui::Context::default();
    design::install(&ctx);
    (
        Harness {
            workspace: Workspace::default(),
            state: TasksTab::default(),
            action: None,
        },
        ctx,
    )
}
impl Harness {
    fn render(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("navigation")
            .exact_size(224.0)
            .show(ui, |_| {});
        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(28))
            .show(ui, |ui| {
                self.state.show(&mut self.workspace, "Personal", ui);
            });
        if let Some(action) = self.state.dialogs(&mut self.workspace, ui.ctx()) {
            self.action = Some(action);
        }
    }
    fn new_task(&mut self, status: &str) {
        self.state.new_task(&self.workspace, status);
    }
    fn manage_board(&mut self) {
        self.state.manage_board(&self.workspace);
    }
    fn task_matches_filters(&self, task: &Task) -> bool {
        self.state.task_matches_filters(&self.workspace, task)
    }
}
fn render(app: &mut Harness, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    render_at(app, ctx, events, Vec2::new(1440.0, 900.0))
}

fn keyboard(app: &mut Harness, ctx: &egui::Context, key: egui::Key, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        let _ = render(
            app,
            ctx,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            }],
        );
    }
}

#[test]
fn task_attachments_cancel_without_copying_and_save_binary_files_with_retry_after_errors() {
    let root = std::env::temp_dir().join(format!("amigo-task-files-{}", id()));
    let board = root.join("board");
    std::fs::create_dir_all(&board).unwrap();
    let source = root.join("any file.bin");
    let content = [0, 255, 100, 0, 8];
    std::fs::write(&source, content).unwrap();
    let (mut app, ctx) = harness();
    app.state.set_board_folder(Some(board.clone()));
    app.new_task("todo");
    let editor = app.state.editor.as_mut().unwrap();
    editor.draft.title = "Cancel Files".into();
    editor.pending_attachments.push(source.clone());
    for _ in 0..3 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    keyboard(&mut app, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!board.join("attachments").exists());
    assert!(app.workspace.tasks.is_empty());
    app.new_task("todo");
    let editor = app.state.editor.as_mut().unwrap();
    editor.draft.title = "Save Files".into();
    editor.pending_attachments = vec![source.clone(), root.join("missing.pdf")];
    for _ in 0..3 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    keyboard(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
    settle_attachment_job(&mut app, &ctx);
    assert!(app.workspace.tasks.is_empty());
    let editor = app.state.editor.as_mut().unwrap();
    assert!(editor.attachment_error.contains("Cannot read attachment"));
    assert!(editor.draft.attachments.is_empty());
    assert_eq!(
        std::fs::read_dir(board.join("attachments"))
            .unwrap()
            .count(),
        0
    );
    editor.pending_attachments.pop();
    keyboard(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
    settle_attachment_job(&mut app, &ctx);
    assert!(app.state.editor.is_none());
    let task = &app.workspace.tasks[0];
    assert_eq!(task.attachments.len(), 1);
    let attached = amigo_core::attachments::resolve(&board, &task.attachments[0]).unwrap();
    std::fs::remove_file(source).unwrap();
    assert_eq!(std::fs::read(attached).unwrap(), content);
    std::fs::remove_dir_all(root).unwrap();
}

fn settle_attachment_job(app: &mut Harness, ctx: &egui::Context) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.state.attachment_job.is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "Attachment worker did not finish"
        );
        let _ = render(app, ctx, vec![]);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn task_keyboard_confirms_valid_edits_cancels_and_preserves_multiline_entry() {
    let (mut app, ctx) = harness();
    app.new_task("todo");
    app.state.editor.as_mut().unwrap().draft.title = "Created with Enter".into();
    for _ in 0..3 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    keyboard(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
    assert!(app.state.editor.is_none());
    assert_eq!(app.workspace.tasks[0].title, "Created with Enter");

    app.new_task("todo");
    for _ in 0..3 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    keyboard(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
    assert!(app.state.editor.is_some()); // Required summary still validates.
    keyboard(&mut app, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(app.state.editor.is_none());
    assert_eq!(app.workspace.tasks.len(), 1);

    app.new_task("todo");
    app.state.editor.as_mut().unwrap().draft.title = "Multiline".into();
    for _ in 0..3 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("task-description")));
    let _ = render(&mut app, &ctx, vec![]);
    keyboard(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
    assert!(
        app.state
            .editor
            .as_ref()
            .unwrap()
            .draft
            .description
            .contains('\n')
    );
    keyboard(
        &mut app,
        &ctx,
        egui::Key::Enter,
        egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        },
    );
    assert!(app.state.editor.is_none());
    assert_eq!(app.workspace.tasks.len(), 2);
}

#[test]
fn task_keyboard_handles_delete_confirmation_and_popup_priority() {
    let (mut app, ctx) = harness();
    let mut task = Task::new(1, "todo");
    task.title = "Existing".into();
    app.workspace.tasks.push(task.clone());
    app.state.edit_task(task);
    for _ in 0..5 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    let calendar = ctx
        .read_response(egui::Id::new(("task-due-date", "calendar")))
        .unwrap()
        .rect
        .center();
    click(&mut app, &ctx, calendar);
    assert!(egui::Popup::is_any_open(&ctx));
    keyboard(&mut app, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(app.state.editor.is_some());
    assert!(!egui::Popup::is_any_open(&ctx));
    let delete = ctx
        .read_response(egui::Id::new("task-delete"))
        .unwrap()
        .rect
        .center();
    click(&mut app, &ctx, delete);
    assert!(app.state.editor.as_ref().unwrap().delete_confirm);
    keyboard(&mut app, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!app.state.editor.as_ref().unwrap().delete_confirm);
    assert_eq!(app.workspace.tasks.len(), 1);
    click(&mut app, &ctx, delete);
    keyboard(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
    assert!(app.state.editor.is_none());
    assert!(app.workspace.tasks.is_empty());
}
fn render_at(
    app: &mut Harness,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    size: Vec2,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| app.render(ui),
    );
    output.textures_delta.clear();
    output
}
fn text_rect_optional(output: &egui::FullOutput, text: &str) -> Option<egui::Rect> {
    fn find(shape: &egui::Shape, text: &str) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == text => {
                Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, text)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .rev()
        .find_map(|s| find(&s.shape, text))
}
fn text_rect(output: &egui::FullOutput, text: &str) -> egui::Rect {
    text_rect_optional(output, text).unwrap_or_else(|| panic!("Text not rendered: {text}"))
}
fn pointer_button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}
fn click(app: &mut Harness, ctx: &egui::Context, pos: egui::Pos2) {
    let _ = render(
        app,
        ctx,
        vec![egui::Event::PointerMoved(pos), pointer_button(pos, true)],
    );
    let _ = render(app, ctx, vec![pointer_button(pos, false)]);
}

#[test]
fn columns_grow_with_tasks_and_scroll_together() {
    for size in [Vec2::new(980.0, 640.0), Vec2::new(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        for number in 1..=20 {
            let mut task = Task::new(number, "todo");
            task.title = format!("Task {number}");
            app.workspace.tasks.push(task);
        }
        let mut short = Task::new(21, "backlog");
        short.title = "Short column".into();
        app.workspace.tasks.push(short);
        for _ in 0..4 {
            let _ = render_at(&mut app, &ctx, vec![], size);
        }
        let column = |status| {
            ctx.read_response(egui::Id::new(("task-column", status)))
                .unwrap()
                .rect
        };
        let tall = column("todo");
        let short = column("backlog");
        assert!(tall.height() > size.y);
        assert!(short.height() < 320.0);
        assert_eq!(tall.top(), short.top());
        let mut extra = Task::new(22, "todo");
        extra.title = "Added task".into();
        let last = extra.id.clone();
        app.workspace.tasks.push(extra);
        for _ in 0..2 {
            let _ = render_at(&mut app, &ctx, vec![], size);
        }
        assert!(column("todo").height() > tall.height());
        let _ = render_at(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(egui::pos2(tall.center().x, 300.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    delta: Vec2::new(0.0, -10000.0),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            size,
        );
        for _ in 0..30 {
            let _ = render_at(&mut app, &ctx, vec![], size);
        }
        let final_card = ctx.read_response(egui::Id::new(last)).unwrap().rect;
        assert!(final_card.top() > 100.0 && final_card.bottom() < size.y);
        assert!(column("backlog").top() < short.top() - 500.0);
        assert_eq!(column("backlog").top(), column("todo").top());
    }
}

#[test]
fn priorities_sort_cards_and_constrain_the_drop_indicator() {
    for size in [Vec2::new(980.0, 900.0), Vec2::new(1440.0, 900.0)] {
        let (mut app, ctx) = harness();
        for (number, status, priority) in [
            (1, "backlog", "High"),
            (2, "todo", "Low"),
            (3, "todo", "High"),
            (4, "todo", "Medium"),
        ] {
            let mut task = Task::new(number, status);
            task.title = format!("Task {number}");
            task.priority = priority.into();
            app.workspace.tasks.push(task);
        }
        for _ in 0..4 {
            let _ = render_at(&mut app, &ctx, vec![], size);
        }
        let rect = |index: usize| {
            ctx.read_response(egui::Id::new(&app.workspace.tasks[index].id))
                .unwrap()
                .rect
        };
        let start = rect(0).center();
        let target = rect(1).center();
        assert!(rect(2).bottom() < rect(3).top());
        assert!(rect(3).bottom() < rect(1).top());
        for events in [
            vec![
                egui::Event::PointerMoved(start),
                pointer_button(start, true),
            ],
            vec![pointer_button(start, false)],
            vec![pointer_button(start, true)],
            vec![egui::Event::PointerMoved(start + Vec2::new(20.0, 0.0))],
            vec![egui::Event::PointerMoved(target)],
            vec![],
        ] {
            let _ = render_at(&mut app, &ctx, events, size);
        }
        let marker = ctx
            .read_response(egui::Id::new(("task-drop-position", "todo")))
            .unwrap()
            .rect;
        let high = ctx
            .read_response(egui::Id::new(&app.workspace.tasks[2].id))
            .unwrap()
            .rect;
        let medium = ctx
            .read_response(egui::Id::new(&app.workspace.tasks[3].id))
            .unwrap()
            .rect;
        assert!(marker.width() > 150.0 && marker.height() <= 3.0);
        assert!(marker.top() >= high.bottom() && marker.bottom() <= medium.top());
        let _ = render_at(&mut app, &ctx, vec![pointer_button(target, false)], size);
        for _ in 0..2 {
            let _ = render_at(&mut app, &ctx, vec![], size);
        }
        let moved = app
            .workspace
            .tasks
            .iter()
            .find(|task| task.number == 1)
            .unwrap();
        assert_eq!(moved.status, "todo");
        let mut positions: Vec<_> = app
            .workspace
            .tasks
            .iter()
            .map(|task| {
                (
                    ctx.read_response(egui::Id::new(&task.id))
                        .unwrap()
                        .rect
                        .top(),
                    task.number,
                )
            })
            .collect();
        positions.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(
            positions
                .iter()
                .map(|(_, number)| *number)
                .collect::<Vec<_>>(),
            [3, 1, 4, 2]
        );
    }
}
#[test]
fn kanban_columns_fit_and_double_click_drag_still_moves_a_card() {
    let (mut app, ctx) = harness();
    app.workspace.task_statuses[2].id = "review".into();
    app.workspace.task_statuses[2].name = "In review".into();
    for status in 0..4 {
        let mut t = Task::new(status as u32 + 1, &app.workspace.task_statuses[status].id);
        t.title = format!("Card {status}");
        app.workspace.tasks.push(t);
    }
    let _ = render(&mut app, &ctx, vec![]);
    let output = render(&mut app, &ctx, vec![]);
    let cards: Vec<_> = (0..4)
        .map(|i| text_rect(&output, &format!("Card {i}")))
        .collect();
    for pair in cards.windows(2) {
        assert!((pair[0].top() - pair[1].top()).abs() < 5.0);
        assert!(pair[1].left() > pair[0].right());
    }
    assert!(cards[3].right() < 1440.0);
    let start = cards[0].center();
    let target = egui::pos2(cards[2].center().x, cards[2].bottom() + 100.0);
    // Click once, then hold the second press to start moving the card.
    click(&mut app, &ctx, start);
    let _ = render(&mut app, &ctx, vec![pointer_button(start, true)]);
    let _ = render(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(start + Vec2::new(20.0, 0.0))],
    );
    assert!(egui::DragAndDrop::has_payload_of_type::<cards::TaskDrag>(
        &ctx
    ));
    let _ = render(&mut app, &ctx, vec![egui::Event::PointerMoved(target)]);
    let _ = render(&mut app, &ctx, vec![]);
    let _ = render(&mut app, &ctx, vec![pointer_button(target, false)]);
    assert_eq!(
        app.workspace
            .tasks
            .iter()
            .find(|t| t.number == 1)
            .unwrap()
            .status,
        "review"
    );
    for _ in 0..25 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    assert!(app.state.editor.is_none());
}
#[test]
fn task_card_hover_keeps_arrow_and_single_click_opens_the_dialog() {
    let (mut app, ctx) = harness();
    let mut task = Task::new(1, "todo");
    task.title = "Open by clicking anywhere on the card".into();
    let id = task.id.clone();
    app.workspace.tasks.push(task);
    for _ in 0..4 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    let rect = ctx.read_response(egui::Id::new(&id)).unwrap().rect;
    for pos in [
        rect.left_top() + Vec2::splat(6.0),
        rect.center(),
        rect.right_bottom() - Vec2::splat(6.0),
    ] {
        let output = render(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)]);
        assert_eq!(
            output.platform_output.cursor_icon,
            egui::CursorIcon::Default
        );
    }
    click(&mut app, &ctx, rect.right_bottom() - Vec2::splat(6.0));
    assert!(app.state.editor.is_none());
    for _ in 0..25 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    assert_eq!(app.state.editor.as_ref().unwrap().draft.id, id);
}
#[test]
fn double_click_arms_the_next_drag_and_escape_cancels_without_opening_a_dialog() {
    let (mut app, ctx) = harness();
    let mut task = Task::new(1, "todo");
    task.title = "Move deliberately".into();
    let id = task.id.clone();
    app.workspace.tasks.push(task);
    for _ in 0..4 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    let pos = ctx.read_response(egui::Id::new(&id)).unwrap().rect.center();
    click(&mut app, &ctx, pos);
    click(&mut app, &ctx, pos);
    for _ in 0..25 {
        let output = render(&mut app, &ctx, vec![]);
        assert_eq!(
            output.platform_output.cursor_icon,
            egui::CursorIcon::Default
        );
    }
    assert!(app.state.editor.is_none());
    let _ = render(&mut app, &ctx, vec![pointer_button(pos, true)]);
    let target = pos + Vec2::new(60.0, 70.0);
    let _ = render(&mut app, &ctx, vec![egui::Event::PointerMoved(target)]);
    assert!(egui::DragAndDrop::has_payload_of_type::<cards::TaskDrag>(
        &ctx
    ));
    let _ = render(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let _ = render(&mut app, &ctx, vec![pointer_button(target, false)]);
    assert!(!egui::DragAndDrop::has_any_payload(&ctx));
    assert!(app.state.editor.is_none());
    assert_eq!(app.workspace.tasks[0].status, "todo");
}
#[test]
fn double_click_drag_reorders_cards_within_a_column() {
    let (mut app, ctx) = harness();
    for number in 1..=3 {
        let mut task = Task::new(number, "todo");
        task.title = format!("Task {number}");
        app.workspace.tasks.push(task);
    }
    for _ in 0..4 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    let start = ctx
        .read_response(egui::Id::new(&app.workspace.tasks[2].id))
        .unwrap()
        .rect
        .center();
    let target = ctx
        .read_response(egui::Id::new(&app.workspace.tasks[0].id))
        .unwrap()
        .rect
        .center();
    click(&mut app, &ctx, start);
    click(&mut app, &ctx, start);
    let _ = render(&mut app, &ctx, vec![pointer_button(start, true)]);
    let _ = render(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(start + Vec2::new(20.0, 0.0))],
    );
    let _ = render(&mut app, &ctx, vec![egui::Event::PointerMoved(target)]);
    let _ = render(&mut app, &ctx, vec![pointer_button(target, false)]);
    assert_eq!(
        app.workspace
            .tasks
            .iter()
            .map(|t| t.number)
            .collect::<Vec<_>>(),
        [3, 1, 2]
    );
    assert!(app.state.editor.is_none());
}
#[test]
fn fast_double_click_drag_handles_multiple_pointer_events_in_one_frame() {
    for gesture in 0..6 {
        let (mut app, ctx) = harness();
        let (source_status, target_status) = if gesture < 3 {
            ("backlog", "in-progress")
        } else {
            ("in-progress", "backlog")
        };
        for (number, status) in [(1, source_status), (2, target_status)] {
            let mut task = Task::new(number, status);
            task.title = format!("Task {number}");
            app.workspace.tasks.push(task);
        }
        for _ in 0..4 {
            let _ = render(&mut app, &ctx, vec![]);
        }
        let start = ctx
            .read_response(egui::Id::new(&app.workspace.tasks[0].id))
            .unwrap()
            .rect
            .center();
        let target = ctx
            .read_response(egui::Id::new(&app.workspace.tasks[1].id))
            .unwrap()
            .rect
            .center();
        let _ = render(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                pointer_button(start, true),
            ],
        );
        match gesture % 3 {
            0 => {
                // First release, second press, and movement arrive together.
                let _ = render(
                    &mut app,
                    &ctx,
                    vec![
                        pointer_button(start, false),
                        pointer_button(start, true),
                        egui::Event::PointerMoved(target),
                    ],
                );
            }
            1 => {
                let _ = render(&mut app, &ctx, vec![pointer_button(start, false)]);
                // A complete second click is followed by a fast drag press.
                let _ = render(
                    &mut app,
                    &ctx,
                    vec![pointer_button(start, true), pointer_button(start, false)],
                );
                let _ = render(
                    &mut app,
                    &ctx,
                    vec![
                        pointer_button(start, true),
                        egui::Event::PointerMoved(target),
                    ],
                );
            }
            _ => {
                let _ = render(&mut app, &ctx, vec![pointer_button(start, false)]);
                // Both events in the second click share a frame.
                let _ = render(
                    &mut app,
                    &ctx,
                    vec![pointer_button(start, true), pointer_button(start, false)],
                );
                let _ = render(&mut app, &ctx, vec![pointer_button(start, true)]);
                let _ = render(&mut app, &ctx, vec![egui::Event::PointerMoved(target)]);
            }
        }
        let _ = render(&mut app, &ctx, vec![]);
        assert_eq!(
            app.workspace
                .tasks
                .iter()
                .find(|task| task.number == 1)
                .unwrap()
                .status,
            source_status,
            "gesture {gesture} must not drop while the second press is held"
        );
        assert!(
            egui::DragAndDrop::has_payload_of_type::<cards::TaskDrag>(&ctx),
            "gesture {gesture} must start dragging"
        );
        let _ = render(&mut app, &ctx, vec![pointer_button(target, false)]);
        assert_eq!(
            app.workspace
                .tasks
                .iter()
                .find(|t| t.number == 1)
                .unwrap()
                .status,
            target_status,
            "gesture {gesture}"
        );
        assert!(app.state.editor.is_none());
    }
}
#[test]
fn a_complete_click_in_one_frame_still_opens_the_task_dialog() {
    let (mut app, ctx) = harness();
    let mut task = Task::new(1, "todo");
    task.title = "Fast click".into();
    let id = task.id.clone();
    app.workspace.tasks.push(task);
    for _ in 0..4 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    let pos = ctx.read_response(egui::Id::new(&id)).unwrap().rect.center();
    let _ = render(
        &mut app,
        &ctx,
        vec![
            egui::Event::PointerMoved(pos),
            pointer_button(pos, true),
            pointer_button(pos, false),
        ],
    );
    for _ in 0..25 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    assert_eq!(app.state.editor.as_ref().unwrap().draft.id, id);
}
#[test]
fn touchpad_tap_and_drag_accepts_one_synthesized_button_hold() {
    let (mut app, ctx) = harness();
    for (number, status) in [(1, "backlog"), (2, "todo")] {
        let mut task = Task::new(number, status);
        task.title = format!("Task {number}");
        app.workspace.tasks.push(task);
    }
    for _ in 0..4 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    let start = ctx
        .read_response(egui::Id::new(&app.workspace.tasks[0].id))
        .unwrap()
        .rect
        .center();
    let target = ctx
        .read_response(egui::Id::new(&app.workspace.tasks[1].id))
        .unwrap()
        .rect
        .center();
    // libinput may represent the initial tap and second held touch as one
    // logical button down, movement, and one button up on finger release.
    let _ = render(
        &mut app,
        &ctx,
        vec![
            egui::Event::PointerMoved(start),
            pointer_button(start, true),
        ],
    );
    let _ = render(&mut app, &ctx, vec![egui::Event::PointerMoved(target)]);
    let _ = render(&mut app, &ctx, vec![]);
    assert!(egui::DragAndDrop::has_payload_of_type::<cards::TaskDrag>(
        &ctx
    ));
    assert_eq!(
        app.workspace
            .tasks
            .iter()
            .find(|t| t.number == 1)
            .unwrap()
            .status,
        "backlog"
    );
    let _ = render(&mut app, &ctx, vec![pointer_button(target, false)]);
    assert_eq!(
        app.workspace
            .tasks
            .iter()
            .find(|t| t.number == 1)
            .unwrap()
            .status,
        "todo"
    );
    assert!(app.state.editor.is_none());
}
#[test]
fn task_ids_are_hidden_and_trash_requires_a_separate_confirmation() {
    for size in [Vec2::new(1440.0, 900.0), Vec2::new(980.0, 640.0)] {
        let (mut app, ctx) = harness();
        let mut task = Task::new(9876, "todo");
        task.title = "Existing task".into();
        app.workspace.tasks.push(task.clone());
        let frame = |app: &mut Harness| render_at(app, &ctx, vec![], size);
        let press = |app: &mut Harness, pos| {
            for pressed in [true, false] {
                let _ = render_at(
                    app,
                    &ctx,
                    vec![egui::Event::PointerMoved(pos), pointer_button(pos, pressed)],
                    size,
                );
            }
        };
        let _ = frame(&mut app);
        let output = frame(&mut app);
        assert!(text_rect_optional(&output, "Existing task").is_some());
        assert!(text_rect_optional(&output, "TASK-9876").is_none());
        assert!(text_rect_optional(&output, &task.id).is_none());
        app.state.edit_task(task.clone());
        app.state.editor.as_mut().unwrap().draft.title = "Unsaved draft".into();
        for _ in 0..4 {
            let _ = frame(&mut app);
        }
        let output = frame(&mut app);
        assert!(text_rect_optional(&output, "Edit Task").is_some());
        assert!(text_rect_optional(&output, "Edit Task · TASK-9876").is_none());
        assert!(text_rect_optional(&output, "Delete…").is_none());
        let delete = ctx
            .read_response(egui::Id::new("task-delete"))
            .unwrap()
            .rect
            .center();
        press(&mut app, delete);
        let output = frame(&mut app);
        assert!(text_rect_optional(&output, "Delete Task?").is_some());
        assert_eq!(app.workspace.tasks, vec![task.clone()]);
        let confirm = text_rect(&output, "Delete Task");
        assert!(confirm.bottom() < size.y && confirm.right() < size.x);
        // Saving the editor behind the confirmation must do nothing.
        press(&mut app, text_rect(&output, "Save Task").center());
        assert!(app.state.editor.as_ref().unwrap().delete_confirm);
        assert_eq!(app.workspace.tasks, vec![task.clone()]);
        let output = frame(&mut app);
        press(&mut app, text_rect(&output, "Cancel").center());
        let editor = app.state.editor.as_ref().unwrap();
        assert!(!editor.delete_confirm);
        assert_eq!(editor.draft.title, "Unsaved draft");
        assert_eq!(editor.draft.id, task.id);
        assert_eq!(editor.draft.number, task.number);
        assert_eq!(app.workspace.tasks, vec![task]);
        let _ = frame(&mut app);
        press(&mut app, delete);
        let output = frame(&mut app);
        press(&mut app, text_rect(&output, "Delete Task").center());
        assert!(app.workspace.tasks.is_empty());
        assert!(app.state.editor.is_none());
    }
}

#[test]
fn keyboard_activation_of_trash_only_opens_confirmation() {
    let (mut app, ctx) = harness();
    let mut task = Task::new(1, "todo");
    task.title = "Keep until confirmed".into();
    app.workspace.tasks.push(task.clone());
    app.state.edit_task(task);
    for _ in 0..3 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("task-delete")));
    keyboard(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::NONE);
    assert!(app.state.editor.as_ref().unwrap().delete_confirm);
    assert_eq!(app.workspace.tasks.len(), 1);
    keyboard(&mut app, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!app.state.editor.as_ref().unwrap().delete_confirm);
    assert_eq!(app.workspace.tasks.len(), 1);
}

#[test]
fn task_editor_save_and_cancel_through_pointer_input() {
    let (mut app, ctx) = harness();
    app.workspace.task_categories.push(StoredValue {
        id: "work".into(),
        name: "Work".into(),
    });
    app.state.categories = BTreeSet::from(["work".into()]);
    app.new_task("todo");
    app.state.editor.as_mut().unwrap().draft.title = "Saved task".into();
    let _ = render(&mut app, &ctx, vec![]);
    let output = render(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_rect(&output, "Save Task").center());
    assert_eq!(app.workspace.tasks.len(), 1);
    assert_eq!(app.workspace.tasks[0].title, "Saved task");
    assert_eq!(app.workspace.tasks[0].category_id, "work");
    assert!(app.state.editor.is_none());
    app.new_task("backlog");
    app.state.editor.as_mut().unwrap().draft.title = "Discarded task".into();
    let output = render(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_rect(&output, "Cancel").center());
    assert_eq!(app.workspace.tasks.len(), 1);
    assert!(app.state.editor.is_none());
}
#[test]
fn board_category_filter_combines_with_search_and_priority() {
    let (mut app, ctx) = harness();
    app.workspace.task_categories = vec![
        StoredValue {
            id: "work".into(),
            name: "Work".into(),
        },
        StoredValue {
            id: "personal".into(),
            name: "Personal".into(),
        },
    ];
    for (number, title, category, priority) in [
        (1, "Quarterly report", "work", "High"),
        (2, "Personal planning", "personal", "Medium"),
        (3, "Loose task", "", "High"),
    ] {
        let mut task = Task::new(number, "todo");
        task.title = title.into();
        task.category_id = category.into();
        task.priority = priority.into();
        app.workspace.tasks.push(task);
    }
    app.state.categories = BTreeSet::from(["work".into(), "personal".into()]);
    app.state.priorities = BTreeSet::from(["High".into(), "Medium".into()]);
    assert_eq!(
        app.workspace
            .tasks
            .iter()
            .filter(|t| app.task_matches_filters(t))
            .count(),
        2
    );
    app.state.priorities = BTreeSet::from(["High".into()]);
    assert_eq!(
        app.workspace
            .tasks
            .iter()
            .filter(|t| app.task_matches_filters(t))
            .count(),
        1
    );
    app.new_task("todo");
    assert!(
        app.state
            .editor
            .as_ref()
            .unwrap()
            .draft
            .category_id
            .is_empty()
    );
    app.state.editor = None;
    app.state.priorities.clear();
    app.state.categories = BTreeSet::from(["work".into()]);
    let _ = render(&mut app, &ctx, vec![]);
    let output = render(&mut app, &ctx, vec![]);
    assert!(text_rect_optional(&output, "Quarterly report").is_some());
    assert!(text_rect_optional(&output, "Personal planning").is_none());
    assert!(text_rect_optional(&output, "Loose task").is_none());
    app.state.search = "report".into();
    app.state.priorities = BTreeSet::from(["High".into()]);
    assert_eq!(
        app.workspace
            .tasks
            .iter()
            .filter(|t| app.task_matches_filters(t))
            .count(),
        1
    );
    app.state.priorities = BTreeSet::from(["Low".into()]);
    assert!(
        !app.workspace
            .tasks
            .iter()
            .any(|t| app.task_matches_filters(t))
    );
    app.state.search.clear();
    app.state.priorities.clear();
    app.state.categories = BTreeSet::from([String::new()]);
    let output = render(&mut app, &ctx, vec![]);
    assert!(text_rect_optional(&output, "Loose task").is_some());
    assert!(text_rect_optional(&output, "Quarterly report").is_none());
    app.state.categories.clear();
    assert_eq!(
        app.workspace
            .tasks
            .iter()
            .filter(|t| app.task_matches_filters(t))
            .count(),
        3
    );
}
#[test]
fn board_settings_can_be_opened_saved_and_cancelled() {
    let (mut app, ctx) = harness();
    let original = app.workspace.clone();
    let _ = render(&mut app, &ctx, vec![]);
    let _ = render(&mut app, &ctx, vec![]);
    app.manage_board();
    let _ = render(&mut app, &ctx, vec![]);
    let output = render(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_rect(&output, "+ Add Status").center());
    let output = render(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_rect(&output, "Save Board").center());
    assert!(app.state.board_editor.is_some());
    assert_eq!(app.workspace, original);
    let output = render(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_rect(&output, "Cancel").center());
    assert!(app.state.board_editor.is_none());
    assert_eq!(app.workspace, original);
    app.manage_board();
    let _ = render(&mut app, &ctx, vec![]);
    let output = render(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_rect(&output, "Save Board").center());
    assert!(app.state.board_editor.is_none());
    app.manage_board();
    let _ = render(&mut app, &ctx, vec![]);
    let output = render(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_rect(&output, "Categories").center());
    let output = render(&mut app, &ctx, vec![]);
    click(
        &mut app,
        &ctx,
        text_rect(&output, "+ Add Category").center(),
    );
    let _ = render(&mut app, &ctx, vec![]);
    let _ = render(&mut app, &ctx, vec![]);
    let output = render(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_rect(&output, "Category Name").center());
    let _ = render(&mut app, &ctx, vec![egui::Event::Text("Work".into())]);
    let output = render(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_rect(&output, "Save Board").center());
    assert!(
        app.state.board_editor.is_none(),
        "{:?}",
        app.state.board_editor
    );
    assert_eq!(app.workspace.task_categories[0].name, "Work");
    let _ = render(&mut app, &ctx, vec![]);
    click(
        &mut app,
        &ctx,
        ctx.read_response(egui::Id::new("task-filters"))
            .unwrap()
            .rect
            .center(),
    );
    let output = render(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_rect(&output, "Work").center());
    assert_eq!(
        app.state.categories,
        BTreeSet::from([app.workspace.task_categories[0].id.clone()])
    );
}

#[test]
fn checkbox_filter_menu_supports_multiple_selections_and_overflow_at_minimum_size() {
    let (mut app, ctx) = harness();
    app.workspace.task_categories = (0..30)
        .map(|i| StoredValue {
            id: format!("category-{i}"),
            name: format!("Category {i}"),
        })
        .collect();
    let narrow_frame = |app: &mut Harness, events| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(980.0, 640.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.render(ui),
        );
        output.textures_delta.clear();
        output
    };
    let narrow_click = |app: &mut Harness, pos| {
        let _ = narrow_frame(
            app,
            vec![egui::Event::PointerMoved(pos), pointer_button(pos, true)],
        );
        let _ = narrow_frame(app, vec![pointer_button(pos, false)]);
    };
    let _ = narrow_frame(&mut app, vec![]);
    let output = narrow_frame(&mut app, vec![]);
    let create = text_rect(&output, "+ Create Task");
    let filters = ctx
        .read_response(egui::Id::new("task-filters"))
        .unwrap()
        .rect;
    let settings = ctx
        .read_response(egui::Id::new("board-import"))
        .unwrap()
        .rect;
    let search = ctx
        .read_response(egui::Id::new("task-search"))
        .unwrap()
        .rect;
    assert!(
        search.right() < settings.left()
            && settings.right() < filters.left()
            && filters.right() < create.left()
    );
    assert!(create.right() <= 952.0);
    assert!((create.center().y - filters.center().y).abs() < 2.0);
    assert!(text_rect_optional(&output, "Search Tasks…").is_none());
    assert!(text_rect(&output, "Backlog").top() > filters.bottom());
    narrow_click(&mut app, filters.center());
    let output = narrow_frame(&mut app, vec![]);
    let first = text_rect(&output, "Category 0");
    narrow_click(&mut app, first.center());
    assert!(app.state.categories.contains("category-0"));
    let output = narrow_frame(&mut app, vec![]);
    let second = text_rect(&output, "Category 1");
    narrow_click(&mut app, second.center());
    assert_eq!(app.state.categories.len(), 2);
    // Checkboxes keep the menu open; scrolling reaches every category.
    let output = narrow_frame(&mut app, vec![]);
    assert!(text_rect_optional(&output, "Priorities").is_some());
    let _ = narrow_frame(
        &mut app,
        vec![
            egui::Event::PointerMoved(first.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                delta: Vec2::new(0.0, -10000.0),
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    let mut output = narrow_frame(&mut app, vec![]);
    for _ in 0..40 {
        output = narrow_frame(&mut app, vec![]);
    }
    let last = text_rect(&output, "Category 29");
    assert!(last.left() > 224.0 && last.right() < 980.0 && last.bottom() < 640.0);
    narrow_click(&mut app, last.center());
    assert!(app.state.categories.contains("category-29"));
    let output = narrow_frame(&mut app, vec![]);
    narrow_click(&mut app, text_rect(&output, "High").center());
    let output = narrow_frame(&mut app, vec![]);
    narrow_click(&mut app, text_rect(&output, "Low").center());
    assert_eq!(
        app.state.priorities,
        BTreeSet::from(["High".into(), "Low".into()])
    );
    let output = narrow_frame(&mut app, vec![]);
    narrow_click(&mut app, text_rect(&output, "High").center());
    assert_eq!(app.state.priorities, BTreeSet::from(["Low".into()]));
    let output = narrow_frame(&mut app, vec![]);
    narrow_click(&mut app, text_rect(&output, "Clear Filters").center());
    assert!(app.state.categories.is_empty() && app.state.priorities.is_empty());
    narrow_click(&mut app, egui::pos2(270.0, 30.0));
    let output = narrow_frame(&mut app, vec![]);
    assert!(text_rect_optional(&output, "Priorities").is_none());
    narrow_click(&mut app, create.center());
    assert!(app.state.editor.is_some());
}

#[test]
fn configuration_dialog_edits_only_the_board_name_in_its_heading() {
    let (mut app, ctx) = harness();
    for _ in 0..3 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    app.manage_board();
    for _ in 0..3 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    let output = render(&mut app, &ctx, vec![]);
    assert!(text_rect_optional(&output, "Rename Board").is_none());
    let rename = ctx
        .read_response(egui::Id::new("board-settings-rename"))
        .unwrap()
        .rect;
    click(&mut app, &ctx, rename.center());
    let _ = render(&mut app, &ctx, vec![]);
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(egui::Id::new("board-settings-name"))
    );
    let _ = render(&mut app, &ctx, vec![egui::Event::Text("Work".into())]);
    let output = render(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, text_rect(&output, "Save Board").center());
    assert_eq!(app.action, Some(TasksAction::RenameBoard("Work".into())));
    assert!(app.state.board_editor.is_none());
}

#[test]
fn search_expands_focuses_and_collapses_on_blur_or_escape_without_losing_the_query() {
    for size in [Vec2::new(1440.0, 900.0), Vec2::new(980.0, 640.0)] {
        let (mut app, ctx) = harness();
        for (number, title) in [(1, "Review report"), (2, "Plan holiday")] {
            let mut task = Task::new(number, "todo");
            task.title = title.into();
            app.workspace.tasks.push(task);
        }
        let frame = |app: &mut Harness, events| render_at(app, &ctx, events, size);
        let press = |app: &mut Harness, pos| {
            let _ = frame(
                app,
                vec![egui::Event::PointerMoved(pos), pointer_button(pos, true)],
            );
            let _ = frame(app, vec![pointer_button(pos, false)]);
        };
        let _ = frame(&mut app, vec![]);
        let output = frame(&mut app, vec![]);
        assert!(text_rect_optional(&output, "Search Tasks…").is_none());
        let search = ctx
            .read_response(egui::Id::new("task-search"))
            .unwrap()
            .rect;
        let create = text_rect(&output, "+ Create Task");
        press(&mut app, search.center());
        let _ = frame(&mut app, vec![]);
        assert!(app.state.search_control.open);
        assert!(ctx.memory(|m| m.has_focus(egui::Id::new("task-search-input"))));
        for _ in 0..20 {
            let _ = frame(&mut app, vec![]);
        }
        let input = ctx
            .read_response(egui::Id::new("task-search-input"))
            .unwrap()
            .rect;
        assert!(input.width() >= 180.0);
        let settings = ctx
            .read_response(egui::Id::new("board-import"))
            .unwrap()
            .rect;
        assert!(input.right() < settings.left() && input.left() > 252.0);
        let output = frame(&mut app, vec![egui::Event::Text("report".into())]);
        assert_eq!(app.state.search, "report");
        assert!(text_rect_optional(&output, "Review report").is_some());
        assert!(text_rect_optional(&output, "Plan holiday").is_none());
        assert_eq!(
            text_rect(&output, "+ Create Task"),
            create,
            "expanding search must keep the other buttons stationary"
        );
        let filter = ctx
            .read_response(egui::Id::new("task-filters"))
            .unwrap()
            .rect;
        press(&mut app, filter.center());
        for _ in 0..20 {
            let _ = frame(&mut app, vec![]);
        }
        let output = frame(&mut app, vec![]);
        assert!(!app.state.search_control.open);
        assert_eq!(app.state.search, "report");
        assert!(text_rect_optional(&output, "report").is_none());
        assert!(text_rect_optional(&output, "Priorities").is_some());
        assert!(text_rect_optional(&output, "Review report").is_some());
        // Reopening restores the query and keyboard focus.
        press(
            &mut app,
            ctx.read_response(egui::Id::new("task-search"))
                .unwrap()
                .rect
                .center(),
        );
        let _ = frame(&mut app, vec![]);
        assert!(app.state.search_control.open);
        assert!(ctx.memory(|m| m.has_focus(egui::Id::new("task-search-input"))));
        let _ = frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(!app.state.search_control.open);
        assert_eq!(app.state.search, "report");
    }
}

#[test]
fn task_modal_groups_properties_and_blocks_background_without_losing_drafts() {
    for size in [Vec2::new(1440.0, 900.0), Vec2::new(980.0, 640.0)] {
        let (mut app, ctx) = harness();
        app.new_task("todo");
        app.state.editor.as_mut().unwrap().draft.title = "Unsaved draft".into();
        let _ = render_at(&mut app, &ctx, vec![], size);
        let _ = render_at(&mut app, &ctx, vec![], size);
        let output = render_at(&mut app, &ctx, vec![], size);
        assert!(
            ctx.memory(|m| m.has_focus(egui::Id::new("task-summary"))),
            "the summary should take keyboard focus on opening"
        );
        let summary = text_rect(&output, "Summary *");
        let description = text_rect(&output, "Description");
        let details = text_rect(&output, "Details");
        let due = text_rect(&output, "Due Date");
        let save = text_rect(&output, "Save Task");
        assert!(summary.left() < details.left() && description.left() < details.left());
        assert!(save.bottom() < size.y - 20.0 && save.right() < size.x - 20.0);
        assert!(due.bottom() < save.top() && due.right() < size.x - 20.0);
        let settings = ctx
            .read_response(egui::Id::new("board-import"))
            .unwrap()
            .rect
            .center();
        for pos in [settings, egui::pos2(5.0, 5.0)] {
            for pressed in [true, false] {
                let _ = render_at(
                    &mut app,
                    &ctx,
                    vec![egui::Event::PointerMoved(pos), pointer_button(pos, pressed)],
                    size,
                );
            }
            assert!(app.state.board_editor.is_none());
            assert_eq!(
                app.state.editor.as_ref().unwrap().draft.title,
                "Unsaved draft"
            );
        }
        let _ = render_at(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            size,
        );
        assert!(app.state.editor.is_none());
        assert!(app.workspace.tasks.is_empty());
    }
}

#[test]
fn date_picker_selects_leap_day_and_clears_an_optional_due_date_without_closing_task() {
    let (mut app, ctx) = harness();
    app.new_task("todo");
    app.state.editor.as_mut().unwrap().draft.due_date = "2024-02-10".into();
    for _ in 0..5 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    let open = |app: &mut Harness| {
        let rect = ctx
            .read_response(egui::Id::new(("task-due-date", "calendar")))
            .unwrap()
            .rect;
        click(app, &ctx, rect.center());
        let _ = render(app, &ctx, vec![]);
        render(app, &ctx, vec![])
    };
    let output = open(&mut app);
    assert!(text_rect_optional(&output, "February 2024").is_some());
    click(&mut app, &ctx, text_rect(&output, "29").center());
    assert_eq!(
        app.state.editor.as_ref().unwrap().draft.due_date,
        "2024-02-29"
    );
    let _ = render(&mut app, &ctx, vec![]);
    let output = open(&mut app);
    click(&mut app, &ctx, text_rect(&output, "Clear Date").center());
    assert!(app.state.editor.as_ref().unwrap().draft.due_date.is_empty());
    assert!(app.workspace.tasks.is_empty());
}

#[test]
fn filter_keyboard_discards_changes_with_escape_and_accepts_with_enter() {
    let (mut app, ctx) = harness();
    for _ in 0..3 {
        let _ = render(&mut app, &ctx, vec![]);
    }
    for key in [egui::Key::Escape, egui::Key::Enter] {
        let filters = ctx
            .read_response(egui::Id::new("task-filters"))
            .unwrap()
            .rect;
        click(&mut app, &ctx, filters.center());
        let output = render(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, text_rect(&output, "High").center());
        assert!(app.state.priorities.contains("High"));
        keyboard(&mut app, &ctx, key, egui::Modifiers::NONE);
        assert!(!egui::Popup::is_any_open(&ctx));
        assert_eq!(
            app.state.priorities.contains("High"),
            key == egui::Key::Enter
        );
    }
}

#[test]
fn search_keyboard_discards_the_query_or_confirms_it() {
    let (mut app, ctx) = harness();
    app.state.search = "Original".into();
    for key in [egui::Key::Escape, egui::Key::Enter] {
        for _ in 0..25 {
            let _ = render(&mut app, &ctx, vec![]);
        }
        let search = ctx
            .read_response(egui::Id::new("task-search"))
            .unwrap()
            .rect;
        click(&mut app, &ctx, search.center());
        let _ = render(&mut app, &ctx, vec![]);
        let _ = render(&mut app, &ctx, vec![egui::Event::Text(" changed".into())]);
        assert_ne!(app.state.search, "Original");
        keyboard(&mut app, &ctx, key, egui::Modifiers::NONE);
        assert!(!app.state.search_control.open);
        assert_eq!(app.state.search == "Original", key == egui::Key::Escape);
    }
}
