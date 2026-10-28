use super::*;
#[test]
fn empty_and_populated_boards_survive_setup_restore_and_remain_editable() {
    use crate::{
        backup::{Archive, Scope},
        library::Library,
    };
    let source = Temp::new();
    let library = Library::open(source.0.clone(), None).unwrap();
    let store = Store::open(&library.root).unwrap();
    let empty = store.create("Empty research").unwrap();
    let populated = graph();
    store.save(&populated, None).unwrap();
    let archive = Archive::capture(&library, Scope::Setup).unwrap();
    assert!(archive.review(&library).is_ok());
    let destination = Temp::new();
    let target = Library::open(destination.0.clone(), None).unwrap();
    archive.import(&target).unwrap();
    let restored = Store::open(&target.root).unwrap();
    let (mut board, fingerprint) = restored.load(&empty.id).unwrap();
    assert_eq!(board, empty);
    assert_eq!(restored.load(&populated.id).unwrap().0, populated);
    board.pan = [30., 70.];
    restored.save(&board, Some(&fingerprint)).unwrap();
    assert_eq!(restored.load(&empty.id).unwrap().0, board);
    fs::remove_dir_all(
        restored
            .folder(&populated.id)
            .unwrap()
            .join("content/cards"),
    )
    .unwrap();
    assert!(restored.load(&populated.id).is_err());
}
fn graph() -> Board {
    let mut b = Board::new("Research");
    for name in ["Alpha", "Beta", "Gamma", "Delta"] {
        b.put_card(Card::new(name)).unwrap();
    }
    for (a, bn, t) in [
        ("Alpha", "Beta", "supports"),
        ("Alpha", "Beta", "opposes"),
        ("Beta", "Gamma", "supports"),
        ("Gamma", "Alpha", "cycle"),
        ("Delta", "Gamma", "opposes"),
    ] {
        let mut link = Link::new(a, bn, t);
        link.directed = true;
        b.put_link(link).unwrap();
    }
    b
}
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("amigo-squidy-{}", id()));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn paths_respect_direction_parallel_edges_filters_and_cycles() {
    let b = graph();
    let mut f = Filter::default();
    let path = b.path("alpha", "Gamma", &f).unwrap().unwrap();
    assert_eq!(path.names, ["Alpha", "Beta", "Gamma"]);
    assert_eq!(path.links.len(), 2);
    assert!(b.path("Alpha", "Delta", &f).unwrap().is_none());
    f.directed = false;
    assert!(b.path("Alpha", "Delta", &f).unwrap().is_some());
    f.directed = true;
    f.include = vec!["SUPPORTS".into()];
    assert!(b.path("Alpha", "Gamma", &f).unwrap().is_some());
    f.exclude = vec!["supports".into()];
    assert!(b.path("Alpha", "Gamma", &f).unwrap().is_none());
    f.include.clear();
    f.exclude.clear();
    assert_eq!(
        b.path("Alpha", "Alpha", &f).unwrap().unwrap().links.len(),
        0
    );
    assert!(b.path("Missing", "Alpha", &f).is_err());
}
#[test]
fn names_are_unique_and_renaming_preserves_all_edge_relationships() {
    let mut b = graph();
    assert!(b.put_card(Card::new(" alpha ")).is_err());
    let mut card = b.cards[0].clone();
    let id = card.id.clone();
    card.name = "New ✓".into();
    card.data = "payload".into();
    b.put_card(card).unwrap();
    b.validate().unwrap();
    assert_eq!(b.cards[0].id, id);
    assert_eq!(b.links.iter().filter(|e| e.from == "New ✓").count(), 2);
    assert_eq!(b.links.iter().filter(|e| e.to == "New ✓").count(), 1);
    b.remove_card(&id);
    b.validate().unwrap();
    assert!(b.links.iter().all(|e| e.from != "New ✓" && e.to != "New ✓"));
}
#[test]
fn markdown_and_links_roundtrip_colors_positions_payload_and_parallel_edges() {
    let temp = Temp::new();
    let store = Store::open(&temp.0).unwrap();
    let mut b = graph();
    b.cards[0].data = "# Hello ✓\r\n\r\n```rust\r\nlet x = 1;\r\n```\r\n".into();
    b.cards[0].position = Some([-250., 125.]);
    b.cards[0].color = [255, 100, 10];
    b.links[0].color = [1, 2, 3];
    b.links[1].directed = false;
    let fp = store.save(&b, None).unwrap();
    let (loaded, saved) = store.load(&b.id).unwrap();
    assert_eq!(loaded, b);
    assert_eq!(fp, saved);
    let root = store.folder(&b.id).unwrap().join("content");
    assert!(root.join("connections.link").is_file());
    assert!(
        root.join("cards")
            .join(format!("{}.md", b.cards[0].id))
            .is_file()
    );
    assert_eq!(store.entries().unwrap()[0].name, "Research");
}
#[test]
fn stale_saves_preserve_external_edits_and_recovery_restores_old_snapshot() {
    let temp = Temp::new();
    let store = Store::open(&temp.0).unwrap();
    let b = graph();
    let fp = store.save(&b, None).unwrap();
    let folder = store.folder(&b.id).unwrap();
    let file = folder
        .join("content/cards")
        .join(format!("{}.md", b.cards[0].id));
    let mut data = fs::read_to_string(&file).unwrap();
    data.push_str("External");
    fs::write(&file, &data).unwrap();
    assert!(store.save(&b, Some(&fp)).is_err());
    assert_eq!(fs::read_to_string(file).unwrap(), data);
    fs::rename(folder.join("content"), folder.join("old-test")).unwrap();
    write(&folder.join("commit.json"), &"old-test").unwrap();
    assert!(
        store.load(&b.id).unwrap().0.cards[0]
            .data
            .ends_with("External")
    );
    assert!(!folder.join("commit.json").exists());
}
#[test]
fn automatic_positions_avoid_overlaps_and_never_move_pinned_cards() {
    let mut b = graph();
    b.cards[0].position = Some([330., 180.]);
    let positions = b.positions();
    assert_eq!(positions[&b.cards[0].id], [330., 180.]);
    for (a, p) in &positions {
        for (bn, q) in &positions {
            if a != bn {
                assert!((p[0] - q[0]).abs() >= 290. || (p[1] - q[1]).abs() >= 150.);
            }
        }
    }
    assert_eq!(positions, b.positions());
    b.arrange();
    assert!(b.cards.iter().all(|c| c.position.is_some()));
}
#[test]
fn invalid_connections_and_malformed_files_do_not_create_a_default_replacement() {
    let temp = Temp::new();
    let store = Store::open(&temp.0).unwrap();
    let mut b = graph();
    assert!(b.put_link(Link::new("Missing", "Alpha", "x")).is_err());
    assert!(b.put_link(Link::new("Alpha", "Beta", "")).is_err());
    store.save(&b, None).unwrap();
    let links = store
        .folder(&b.id)
        .unwrap()
        .join("content/connections.link");
    fs::write(&links, b"invalid").unwrap();
    assert!(store.load(&b.id).is_err());
    assert_eq!(fs::read(links).unwrap(), b"invalid");
    assert!(store.folder("../escape").is_err());
}

#[test]
fn mixed_connections_respect_each_edges_direction_and_filters() {
    let mut board = Board::new("Mixed directions");
    for name in ["A", "B", "C", "D"] {
        board.put_card(Card::new(name)).unwrap();
    }
    let bidirectional = Link::new("A", "B", "shared");
    assert!(!bidirectional.directed);
    board.put_link(bidirectional).unwrap();
    let mut directed = Link::new("B", "C", "forward");
    directed.directed = true;
    board.put_link(directed).unwrap();
    board.put_link(Link::new("D", "C", "shared")).unwrap();
    board.put_link(Link::new("C", "C", "loop")).unwrap();
    let mut filter = Filter::default();
    assert_eq!(
        board.path("A", "D", &filter).unwrap().unwrap().names,
        ["A", "B", "C", "D"]
    );
    assert!(board.path("D", "A", &filter).unwrap().is_none());
    assert_eq!(
        board.path("B", "A", &filter).unwrap().unwrap().names,
        ["B", "A"]
    );
    assert_eq!(
        board.path("C", "C", &filter).unwrap().unwrap().links.len(),
        0
    );
    filter.directed = false;
    assert_eq!(
        board.path("D", "A", &filter).unwrap().unwrap().names,
        ["D", "C", "B", "A"]
    );
    filter.exclude = vec!["forward".into()];
    assert!(board.path("A", "D", &filter).unwrap().is_none());
    filter.include = vec!["shared".into()];
    assert!(board.path("B", "A", &filter).unwrap().is_some());
}

#[test]
fn legacy_connections_keep_their_direction_and_save_an_explicit_setting() {
    let temp = Temp::new();
    let store = Store::open(&temp.0).unwrap();
    let board = graph();
    store.save(&board, None).unwrap();
    let file = store
        .folder(&board.id)
        .unwrap()
        .join("content/connections.link");
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    for link in document["links"].as_array_mut().unwrap() {
        link.as_object_mut().unwrap().remove("directed");
    }
    fs::write(&file, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
    let (mut loaded, fingerprint) = store.load(&board.id).unwrap();
    assert_eq!(loaded, board);
    assert!(loaded.links.iter().all(|link| link.directed));
    assert!(
        loaded
            .path("Alpha", "Delta", &Filter::default())
            .unwrap()
            .is_none()
    );
    loaded.links[0].directed = false;
    let new = Link::new("Gamma", "Delta", "new bidirectional connection");
    assert!(!new.directed);
    loaded.put_link(new).unwrap();
    store.save(&loaded, Some(&fingerprint)).unwrap();
    let (reopened, _) = store.load(&loaded.id).unwrap();
    assert_eq!(reopened, loaded);
    let document: serde_json::Value = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
    assert!(
        document["links"]
            .as_array()
            .unwrap()
            .iter()
            .all(|link| link["directed"].is_boolean())
    );
}
