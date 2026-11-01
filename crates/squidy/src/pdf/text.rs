//! Optional selectable text pages and their embedded Unicode font.
use amigo_core::squidy::Board;
use std::fmt::Write;
const FONT: &[u8] = include_bytes!("../../../ui/assets/fonts/AdwaitaSans-Regular.ttf");
pub(super) fn pages(board: &Board) -> Vec<String> {
    let font = Font::new();
    let mut pages = Vec::new();
    for card in &board.cards {
        let mut out = String::new();
        text(&mut out, &board.name, 40., 804., 10.);
        let mut y = 776.;
        for (content, size) in [(&card.name, 18.), (&card.brief, 12.), (&card.data, 12.)] {
            for line in wrap(content, 515., size, &font) {
                if y < 48. {
                    pages.push(std::mem::take(&mut out));
                    text(&mut out, &board.name, 40., 804., 10.);
                    y = 776.;
                }
                text(&mut out, &line, 40., y, size);
                y -= size * 1.4;
            }
            y -= 10.;
        }
        pages.push(out);
    }
    pages
}
pub(super) fn encoded(value: &str) -> String {
    value
        .chars()
        .map(|c| format!("{:04X}", if c as u32 <= 65535 { c as u32 } else { 0xfffd }))
        .collect()
}
fn text(out: &mut String, value: &str, x: f32, y: f32, size: f32) {
    writeln!(
        out,
        "0.1 0.12 0.15 rg BT /F1 {size} Tf 1 0 0 1 {x} {y} Tm <{}> Tj ET",
        encoded(value)
    )
    .unwrap();
}
fn wrap(text: &str, width: f32, size: f32, font: &Font) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut advance = 0.;
        for word in paragraph.split_whitespace() {
            let word_width: f32 = word
                .chars()
                .map(|c| font.width(c) as f32 * size / 1000.)
                .sum();
            if !line.is_empty() && advance + word_width + size * 0.25 > width {
                lines.push(std::mem::take(&mut line));
                advance = 0.;
            }
            if !line.is_empty() {
                line.push(' ');
                advance += size * 0.25;
            }
            for c in word.chars() {
                let w = font.width(c) as f32 * size / 1000.;
                if advance + w > width && !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                    advance = 0.;
                }
                line.push(c);
                advance += w;
            }
        }
        lines.push(line);
    }
    lines
}
/// These six objects occupy IDs 4–9 in the parent PDF.
pub(super) fn font_objects() -> Vec<Vec<u8>> {
    let font = Font::new();
    let mut widths = String::new();
    let mut map = Vec::with_capacity(131072);
    for code in 0..=65535u32 {
        let glyph = font.glyph(code as u16);
        map.extend_from_slice(&glyph.to_be_bytes());
        if glyph != 0 {
            write!(
                widths,
                "{code} [{}] ",
                font.width(char::from_u32(code).unwrap_or('?'))
            )
            .unwrap();
        }
    }
    vec![
        b"<< /Type /Font /Subtype /Type0 /BaseFont /AdwaitaSans /Encoding /Identity-H /DescendantFonts [5 0 R] /ToUnicode 9 0 R >>".to_vec(),
        format!("<< /Type /Font /Subtype /CIDFontType2 /BaseFont /AdwaitaSans /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 6 0 R /CIDToGIDMap 8 0 R /DW 600 /W [{widths}] >>").into_bytes(),
        b"<< /Type /FontDescriptor /FontName /AdwaitaSans /Flags 32 /FontBBox [-1000 -1000 3000 3000] /ItalicAngle 0 /Ascent 1000 /Descent -300 /CapHeight 750 /StemV 80 /FontFile2 7 0 R >>".to_vec(),
        super::stream(FONT, &format!("/Length1 {}",FONT.len())),
        super::stream(&map,""),
        super::stream(b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /Unicode def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> endcodespacerange 1 beginbfrange <0000> <FFFF> <0000> endbfrange endcmap CMapName currentdict /CMap defineresource pop end end",""),
    ]
}
// Parse only the trusted bundled font's BMP cmap and horizontal metrics.
struct Font {
    cmap: usize,
    hmtx: usize,
    metrics: usize,
    units: f32,
}
fn u16_at(offset: usize) -> u16 {
    u16::from_be_bytes([FONT[offset], FONT[offset + 1]])
}
fn u32_at(offset: usize) -> usize {
    u32::from_be_bytes(FONT[offset..offset + 4].try_into().unwrap()) as usize
}
impl Font {
    fn new() -> Self {
        let table = |name: &[u8]| {
            (0..u16_at(4) as usize)
                .map(|i| 12 + i * 16)
                .find(|i| &FONT[*i..*i + 4] == name)
                .map(|i| u32_at(i + 8))
                .unwrap()
        };
        let cmap = table(b"cmap");
        let cmap = (0..u16_at(cmap + 2) as usize)
            .map(|i| cmap + u32_at(cmap + 4 + i * 8 + 4))
            .find(|i| u16_at(*i) == 4)
            .unwrap();
        Self {
            cmap,
            hmtx: table(b"hmtx"),
            metrics: u16_at(table(b"hhea") + 34) as usize,
            units: u16_at(table(b"head") + 18) as f32,
        }
    }
    fn glyph(&self, code: u16) -> u16 {
        let count = u16_at(self.cmap + 6) as usize / 2;
        for i in 0..count {
            let end = u16_at(self.cmap + 14 + i * 2);
            let start = u16_at(self.cmap + 16 + count * 2 + i * 2);
            if start <= code && code <= end {
                let delta = u16_at(self.cmap + 16 + count * 4 + i * 2);
                let range = self.cmap + 16 + count * 6 + i * 2;
                let offset = u16_at(range) as usize;
                if offset == 0 {
                    return code.wrapping_add(delta);
                }
                let glyph = u16_at(range + offset + (code - start) as usize * 2);
                return if glyph == 0 {
                    0
                } else {
                    glyph.wrapping_add(delta)
                };
            }
        }
        0
    }
    fn width(&self, c: char) -> u16 {
        let glyph = self.glyph(if c as u32 <= 65535 { c as u16 } else { 0xfffd }) as usize;
        (u16_at(self.hmtx + glyph.min(self.metrics - 1) * 4) as f32 * 1000. / self.units).round()
            as u16
    }
}
