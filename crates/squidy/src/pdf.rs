//! Image-first PDF output with optional card text pages.
mod text;
use amigo_core::squidy::Board;
use egui::Rect;

pub fn board_area(board: &Board) -> [f32; 4] {
    let area = super::capture::Capture::new(board, &Default::default()).area;
    [area.min.x, area.min.y, area.max.x, area.max.y]
}
/// Export the captured diagram, optionally followed by card text pages.
pub fn export(
    board: &Board,
    [left, top, right, bottom]: [f32; 4],
    include_text: bool,
) -> Result<Vec<u8>, String> {
    board.validate()?;
    if ![left, top, right, bottom].iter().all(|n| n.is_finite())
        || right <= left
        || bottom <= top
        || right - left > 30_000_000.
        || bottom - top > 30_000_000.
    {
        return Err("Choose a nonempty capture frame.".into());
    }
    let area = Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom));
    let captured = super::capture::captured_board(board, area);
    if captured.cards.is_empty() {
        return Err("The capture frame must contain at least one whole card.".into());
    }
    let image = super::capture::raster(&captured, area);
    use std::io::Write as _;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&image.rgb).map_err(|e| e.to_string())?;
    let compressed = encoder.finish().map_err(|e| e.to_string())?;
    let text_pages = if include_text {
        text::pages(&captured)
    } else {
        Vec::new()
    };
    Ok(pdf(&compressed, image.width, image.height, text_pages))
}

fn stream(bytes: &[u8], extra: &str) -> Vec<u8> {
    let mut out = format!("<< /Length {} {extra} >>\nstream\n", bytes.len()).into_bytes();
    out.extend_from_slice(bytes);
    out.extend_from_slice(b"\nendstream");
    out
}
fn pdf(image: &[u8], image_width: usize, image_height: usize, text_pages: Vec<String>) -> Vec<u8> {
    let scale = (786. / image_width as f32).min(539. / image_height as f32);
    let width = image_width as f32 * scale;
    let height = image_height as f32 * scale;
    let content = format!(
        "q {width} 0 0 {height} {} {} cm /Capture Do Q",
        (842. - width) / 2.,
        (595. - height) / 2.
    );
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        Vec::new(),
        stream(
            image,
            &format!(
                "/Type /XObject /Subtype /Image /Width {image_width} /Height {image_height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode"
            ),
        ),
    ];
    let with_text = !text_pages.is_empty();
    if with_text {
        objects.extend(text::font_objects());
    }
    let mut pages = vec![(842., 595., content)];
    pages.extend(text_pages.into_iter().map(|content| (595., 842., content)));
    let count = pages.len();
    let mut kids = String::new();
    for (width, height, content) in pages {
        let id = objects.len() + 1;
        kids.push_str(&format!("{id} 0 R "));
        let font = if with_text {
            "/Font << /F1 4 0 R >>"
        } else {
            ""
        };
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Resources << /XObject << /Capture 3 0 R >> {font} >> /Contents {} 0 R >>",id+1).into_bytes());
        objects.push(stream(content.as_bytes(), ""));
    }
    objects[1] = format!("<< /Type /Pages /Count {count} /Kids [{kids}] >>").into_bytes();
    let mut out = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(object);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optional_text_follows_image_and_contains_only_captured_cards() {
        let mut board = Board::new("Notes español Привет");
        for name in ["Outside", "Captured"] {
            let mut card = amigo_core::squidy::Card::new(name);
            card.brief = "Brief content".into();
            card.data = "Description content\n".repeat(60);
            board.cards.push(card);
        }
        let rect = crate::capture::card_rects(&board)[&board.cards[1].id].expand(10.);
        let bytes = export(
            &board,
            [rect.left(), rect.top(), rect.right(), rect.bottom()],
            true,
        )
        .unwrap();
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("/Count 3 /Kids"));
        assert!(content.contains("/FontFile2"));
        let image = content.find("/Capture Do").unwrap();
        let title = content.find(&text::encoded("Captured")).unwrap();
        let brief = content.find(&text::encoded("Brief content")).unwrap();
        let description = content.find(&text::encoded("Description content")).unwrap();
        assert!(image < title && title < brief && brief < description);
        assert!(!content.contains(&text::encoded("Outside")));
    }
    #[test]
    fn capture_exports_one_image_page_without_text_or_fonts() {
        let mut board = Board::new("Notes español Привет");
        for name in ["Outside", "Captured"] {
            let mut card = amigo_core::squidy::Card::new(name);
            card.brief = "Brief content".into();
            card.data = "Description content\n".repeat(60);
            board.cards.push(card);
        }
        let bounds = crate::capture::card_rects(&board);
        let rect = bounds[&board.cards[1].id].expand(10.);
        let bytes = export(
            &board,
            [rect.left(), rect.top(), rect.right(), rect.bottom()],
            false,
        )
        .unwrap();
        assert!(bytes.starts_with(b"%PDF-1.7"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        let contents = String::from_utf8_lossy(&bytes);
        assert!(contents.contains("/Subtype /Image"));
        assert!(contents.contains("/Capture Do"));
        assert!(contents.contains("/Count 1 /Kids"));
        assert!(!contents.contains("/Font"));
        assert!(!contents.contains("Description content"));
        let xref = contents
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse::<usize>()
            .unwrap();
        assert_eq!(&bytes[xref..xref + 4], b"xref");
        assert!(
            export(
                &board,
                [rect.left(), rect.top(), rect.left() + 10., rect.top() + 10.],
                false,
            )
            .is_err()
        );
    }
}
