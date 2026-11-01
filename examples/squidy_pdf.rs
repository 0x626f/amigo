//! Generate a PDF fixture for visual inspection with a desktop PDF viewer.
use amigo_core::squidy::{Board, Card, Link};
fn main() {
    let mut board = Board::new("Research · español · Привет");
    for (index, name) in ["Question", "Evidence", "Conclusion"]
        .into_iter()
        .enumerate()
    {
        let mut card = Card::new(name);
        card.position = Some([index as f32 * 340., 0.]);
        card.size = [300., 180.];
        card.brief = "A card summary with accents: café, mañana.\nDrag corners to resize.".into();
        card.data = "# Research notes\n\nDetailed research notes for this card.\n".repeat(25);
        board.cards.push(card);
    }
    board.backgrounds.push(amigo_core::squidy::Background::new(
        "Research sources",
        [-24., -48.],
        [688., 252.],
    ));
    board
        .links
        .push(Link::new("Question", "Evidence", "supports"));
    let path = std::env::args().nth(1).expect("pass output PDF path");
    std::fs::write(
        path,
        amigo_squidy::pdf::export(
            &board,
            amigo_squidy::pdf::board_area(&board),
            std::env::args().any(|arg| arg == "--text"),
        )
        .unwrap(),
    )
    .unwrap();
}
