use super::*;
use crate::app::tests::harness;
fn settle(app: &mut Amigo, ctx: &egui::Context) {
    for _ in 0..3000 {
        app.tick_squidy(ctx);
        if app.squidy_io.initialized && !app.squidy_busy() && !app.squidy_dirty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("Squidy did not settle: {}", app.squidy.error);
}
#[test]
fn boards_save_switch_and_reopen_with_cards_parallel_edges_and_camera() {
    let (mut app, ctx) = harness();
    app.section = Section::Squidy;
    settle(&mut app, &ctx);
    let id = app.squidy.board.as_ref().unwrap().id.clone();
    {
        let board = app.squidy.board.as_mut().unwrap();
        let mut c = amigo_core::squidy::Card::new("Evidence");
        c.brief = "Visible brief".into();
        c.data = "Private payload in dialog".into();
        c.position = Some([120., -40.]);
        board.put_card(c).unwrap();
        board
            .put_card(amigo_core::squidy::Card::new("Hypothesis"))
            .unwrap();
        board
            .put_link(amigo_core::squidy::Link::new(
                "Evidence",
                "Hypothesis",
                "supports",
            ))
            .unwrap();
        board
            .put_link(amigo_core::squidy::Link::new(
                "Evidence",
                "Hypothesis",
                "questions",
            ))
            .unwrap();
        board.links[0].directed = true;
        assert!(!board.links[1].directed);
        board.zoom = 0.7;
        board.pan = [50., -10.];
    }
    app.squidy.changed_at = Instant::now() - Duration::from_secs(1);
    settle(&mut app, &ctx);
    app.squidy_action(Action::Create("Other research".into()), &ctx);
    settle(&mut app, &ctx);
    assert_ne!(app.squidy.board.as_ref().unwrap().id, id);
    app.squidy_action(Action::Select(id.clone()), &ctx);
    settle(&mut app, &ctx);
    let expected = app.squidy.board.clone();
    let mut reopened = Amigo::load(app.data_path.clone());
    reopened.section = Section::Squidy;
    settle(&mut reopened, &ctx);
    assert_eq!(reopened.squidy.board, expected);
    assert_eq!(reopened.library.settings.squidy_board, id);
}
#[test]
fn external_conflict_keeps_local_graph_and_prevents_navigation_until_explicit_reload() {
    let (mut app, ctx) = harness();
    app.section = Section::Squidy;
    settle(&mut app, &ctx);
    let original = app.squidy.board.clone().unwrap();
    let store = Store::open(&app.data_path).unwrap();
    let mut external = original.clone();
    external
        .put_card(amigo_core::squidy::Card::new("External"))
        .unwrap();
    store.save(&external, None).unwrap();
    app.squidy
        .board
        .as_mut()
        .unwrap()
        .put_card(amigo_core::squidy::Card::new("Local"))
        .unwrap();
    app.squidy.changed_at = Instant::now() - Duration::from_secs(1);
    for _ in 0..1000 {
        app.tick_squidy(&ctx);
        if app.squidy_io.blocked {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(app.squidy_dirty());
    assert!(app.squidy.board.as_ref().unwrap().card("Local").is_some());
    app.squidy_action(Action::Create("Another".into()), &ctx);
    assert!(app.squidy_io.pending.is_empty());
    assert_eq!(app.squidy.board.as_ref().unwrap().id, original.id);
    app.squidy_action(Action::Refresh, &ctx);
    settle(&mut app, &ctx);
    assert_eq!(app.squidy.board.as_ref().unwrap(), &external);
}
#[test]
fn cancelled_card_attachment_copies_are_removed_and_accepted_files_survive_board_save() {
    let (mut app, ctx) = harness();
    app.section = Section::Squidy;
    settle(&mut app, &ctx);
    let source = app.data_path.join("fixture.txt");
    std::fs::write(&source, "Attached payload").unwrap();
    let folder = app.squidy.root.clone().unwrap();
    let batch = Batch::import(folder.clone(), std::slice::from_ref(&source)).unwrap();
    let cancelled = batch.files[0].clone();
    app.squidy.attachment_result("closed editor", batch);
    assert!(amigo_core::attachments::resolve(&folder, &cancelled).is_err());
    app.squidy.new_card(None);
    let card_id = app.squidy.editor.as_ref().unwrap().card.id.clone();
    let batch = Batch::import(folder.clone(), &[source]).unwrap();
    let file = batch.files[0].clone();
    app.squidy.attachment_result(&card_id, batch);
    let mut editor = app.squidy.editor.take().unwrap();
    editor.card.name = "With file".into();
    app.squidy
        .board
        .as_mut()
        .unwrap()
        .put_card(editor.card)
        .unwrap();
    for batch in &mut editor.batches {
        batch.keep();
    }
    app.squidy.changed_at = Instant::now() - Duration::from_secs(1);
    settle(&mut app, &ctx);
    assert_eq!(
        std::fs::read_to_string(amigo_core::attachments::resolve(&folder, &file).unwrap()).unwrap(),
        "Attached payload"
    );
    let archive =
        amigo_core::backup::Archive::capture(&app.library, amigo_core::backup::Scope::Setup)
            .unwrap();
    assert!(archive.review(&app.library).is_ok());
    let restored_root =
        std::env::temp_dir().join(format!("amigo-squidy-restored-{}", amigo_core::model::id()));
    let destination = amigo_core::library::Library::open(restored_root.clone(), None).unwrap();
    archive.import(&destination).unwrap();
    let restored = Store::open(&restored_root).unwrap();
    let expected = app.squidy.board.as_ref().unwrap();
    assert_eq!(restored.load(&expected.id).unwrap().0, *expected);
    assert_eq!(
        std::fs::read_to_string(
            amigo_core::attachments::resolve(&restored.folder(&expected.id).unwrap(), &file)
                .unwrap()
        )
        .unwrap(),
        "Attached payload"
    );
    std::fs::remove_dir_all(restored_root).unwrap();
    std::fs::remove_dir_all(&app.data_path).unwrap();
}
