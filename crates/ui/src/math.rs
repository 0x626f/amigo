//! Cached LaTeX typesetting, using embedded STIX outlines rather than UI-font substitutions.
use egui::{Color32, FontId, TextFormat, TextureHandle, Vec2, text::LayoutJob};
use latex_rust::{Dim, MathFont, MathStyle, PngOptions};
use std::{
    ops::{Deref, DerefMut},
    sync::OnceLock,
};

const MAX_SOURCE: usize = 16_384;
const CACHE_SIZE: usize = 64;
const MAX_PIXELS: f32 = 4_000_000.;
const CACHE_BYTES: usize = 32 * 1024 * 1024;
fn font() -> Result<&'static MathFont, String> {
    static FONT: OnceLock<Result<MathFont, String>> = OnceLock::new();
    FONT.get_or_init(|| MathFont::stix_two_math().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(Clone::clone)
}
fn scalar(value: &Dim) -> f32 {
    f32::from_bits(value.to_ieee32_bits())
}
fn dim(value: f32) -> Dim {
    Dim::ratio((value * 1024.).round() as i64, 1024)
}

/// Bounded before raster allocation; the parser and layout also enforce nesting limits.
fn raster(
    source: &str,
    size: f32,
    ppp: f32,
    color: Color32,
    display: bool,
) -> Result<egui::ColorImage, String> {
    if source.len() > MAX_SOURCE {
        return Err("Formula exceeds the 16 KiB preview limit.".into());
    }
    let font = font()?;
    let ast = latex_rust::parse(source).map_err(|e| e.to_string())?;
    let tree = latex_rust::layout_with_em_size_pt(
        &ast,
        font,
        if display {
            MathStyle::Display
        } else {
            MathStyle::Text
        },
        &dim(size),
    )
    .map_err(|e| e.to_string())?;
    let width = scalar(&tree.width) * size * ppp;
    let height = scalar(&(&tree.height + &tree.depth)) * size * ppp;
    if !width.is_finite()
        || !height.is_finite()
        || width > 8192.
        || height > 8192.
        || width * height > MAX_PIXELS
    {
        return Err("Formula is too large to preview.".into());
    }
    let options = PngOptions {
        font_size_pt: dim(size),
        dpi: dim(72. * ppp),
        color: latex_rust::Color::rgb(color.r(), color.g(), color.b()),
        display,
        ..PngOptions::new()
    };
    let png = latex_rust::render_png(&tree, font, &options).map_err(|e| e.to_string())?;
    let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let mut result = egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    );
    if color.a() < 255 {
        for pixel in &mut result.pixels {
            *pixel = pixel.gamma_multiply(color.a() as f32 / 255.);
        }
    }
    Ok(result)
}
#[derive(Clone, PartialEq)]
struct Key {
    source: String,
    size: u32,
    ppp: u32,
    color: Color32,
    display: bool,
}
#[derive(Clone)]
struct Rendered {
    texture: TextureHandle,
    size: Vec2,
}
#[derive(Clone, Default)]
struct Cache {
    entries: Vec<(Key, Result<Rendered, String>)>,
}
fn cached(
    ui: &egui::Ui,
    source: &str,
    size: f32,
    color: Color32,
    display: bool,
) -> Result<Rendered, String> {
    if source.len() > MAX_SOURCE {
        return Err("Formula exceeds the 16 KiB preview limit.".into());
    }
    // Quarter-pixel resolution steps avoid rebuilding textures during tiny DPI changes.
    let ppp = (ui.ctx().pixels_per_point() * 4.).ceil().clamp(8., 16.) / 4.;
    let size = (size.clamp(8., 96.) * 4.).round() / 4.;
    let key = Key {
        source: source.into(),
        size: size.to_bits(),
        ppp: ppp.to_bits(),
        color,
        display,
    };
    let id = egui::Id::new("latex-formula-cache");
    if let Some(hit) = ui.ctx().data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<Cache>(id);
        let index = cache.entries.iter().position(|(k, _)| k == &key)?;
        let entry = cache.entries.remove(index);
        let result = entry.1.clone();
        cache.entries.push(entry);
        Some(result)
    }) {
        return hit;
    }
    let rendered = raster(source, size, ppp, color, display).map(|image| {
        let size = egui::vec2(image.size[0] as f32 / ppp, image.size[1] as f32 / ppp);
        Rendered {
            texture: ui
                .ctx()
                .load_texture("latex-formula", image, egui::TextureOptions::LINEAR),
            size,
        }
    });
    ui.ctx().data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<Cache>(id);
        let bytes = |r: &Result<Rendered, String>| {
            r.as_ref()
                .map_or(0, |r| r.texture.size()[0] * r.texture.size()[1] * 4)
        };
        let mut total = cache.entries.iter().map(|(_, r)| bytes(r)).sum::<usize>();
        while !cache.entries.is_empty()
            && (cache.entries.len() >= CACHE_SIZE || total + bytes(&rendered) > CACHE_BYTES)
        {
            let (_, removed) = cache.entries.remove(0);
            total -= bytes(&removed);
        }
        cache.entries.push((key, rendered.clone()));
    });
    rendered
}
fn formula(ui: &mut egui::Ui, source: &str, rendered: &Result<Rendered, String>) {
    match rendered {
        Ok(rendered) => {
            ui.add(egui::Image::new(&rendered.texture).fit_to_exact_size(rendered.size))
                .on_hover_text(source)
                .context_menu(|ui| {
                    if ui.button("Copy formula source").clicked() {
                        ui.ctx().copy_text(source.into());
                        ui.close();
                    }
                });
        }
        Err(error) => {
            ui.colored_label(crate::design::colors(ui).red, source)
                .on_hover_text(format!("Formula: {error}"));
        }
    }
}

/// Text runs wrap around formula widgets. Formula images stay indivisible and retain their full height.
#[derive(Default)]
pub struct RichJob {
    pub job: LayoutJob,
    parts: Vec<Part>,
}
enum Part {
    Text(LayoutJob),
    Formula {
        source: String,
        rendered: Result<Rendered, String>,
        display: bool,
    },
}
impl Deref for RichJob {
    type Target = LayoutJob;
    fn deref(&self) -> &LayoutJob {
        &self.job
    }
}
impl DerefMut for RichJob {
    fn deref_mut(&mut self) -> &mut LayoutJob {
        &mut self.job
    }
}
impl RichJob {
    pub fn is_empty(&self) -> bool {
        self.job.text.is_empty() && self.parts.is_empty()
    }
    pub fn append_formula(
        &mut self,
        ui: &egui::Ui,
        source: &str,
        format: TextFormat,
        display: bool,
    ) {
        if !self.job.text.is_empty() {
            self.parts.push(Part::Text(std::mem::take(&mut self.job)));
        }
        self.parts.push(Part::Formula {
            source: source.into(),
            rendered: cached(ui, source, format.font_id.size, format.color, display),
            display,
        });
    }
    pub fn show(self, ui: &mut egui::Ui) -> egui::Response {
        self.show_with_sense(ui, egui::Sense::hover())
    }
    pub fn show_with_sense(mut self, ui: &mut egui::Ui, sense: egui::Sense) -> egui::Response {
        if self.parts.is_empty() {
            return ui.add(egui::Label::new(self.job).sense(sense));
        }
        if !self.job.text.is_empty() {
            self.parts.push(Part::Text(self.job));
        }
        ui.scope_builder(egui::UiBuilder::new().sense(sense), |ui| {
            ui.style_mut().interaction.selectable_labels = false;
            if let [
                Part::Formula {
                    source,
                    rendered,
                    display: true,
                },
            ] = self.parts.as_slice()
            {
                egui::ScrollArea::horizontal()
                    .id_salt(ui.next_auto_id())
                    .show(ui, |ui| {
                        formula(ui, source, rendered);
                    });
            } else {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.;
                    let height = self
                        .parts
                        .iter()
                        .filter_map(|p| match p {
                            Part::Formula {
                                rendered: Ok(r), ..
                            } => Some(r.size.y),
                            _ => None,
                        })
                        .fold(ui.text_style_height(&egui::TextStyle::Body), f32::max);
                    ui.set_row_height(height);
                    for part in &self.parts {
                        match part {
                            Part::Text(job) => {
                                let mut job = job.clone();
                                for section in &mut job.sections {
                                    section.format.valign = egui::Align::Center;
                                }
                                ui.label(job);
                            }
                            Part::Formula {
                                source, rendered, ..
                            } => formula(ui, source, rendered),
                        }
                    }
                });
            }
        })
        .response
    }
}
pub fn show(ui: &mut egui::Ui, source: &str) {
    let mut job = RichJob::default();
    job.append_formula(
        ui,
        source,
        TextFormat {
            font_id: FontId::proportional(20.),
            color: crate::design::colors(ui).ink,
            ..Default::default()
        },
        true,
    );
    job.show(ui);
}

#[cfg(test)]
mod tests {
    use super::*;
    pub const BANDWIDTH: &str = r"B_{GB/s}=\frac{MT/s\times W_{bit}\times C}{8\times1000}";
    #[test]
    fn typesets_structural_formulas_and_unicode_units() {
        for source in [
            BANDWIDTH,
            r"\dfrac{1}{2}+\tfrac{3}{4}+\binom{n}{k}",
            r"\prod_{k=1}^{n} k=n!\quad\frac{\partial f}{\partial x}+\nabla\cdot\vec{v}",
            r"\log x+\ln y+\cos\theta+\operatorname{rank}(A)",
            r"\frac{1}{1+\frac{1}{x}}",
            r"\sqrt[3]{x^2+y^2}",
            r"\sum_{i=1}^{n} i=\frac{n(n+1)}{2}",
            r"\int_0^\infty e^{-x^2}\,dx=\frac{\sqrt{\pi}}{2}",
            r"\lim_{x\to0}\frac{\sin x}{x}=1",
            r"\left(\frac{a}{b}\right)^2",
            r"\begin{pmatrix}a&b\\c&d\end{pmatrix}",
            r"f(x)=\begin{cases}x^2&x\ge0\\-x&x<0\end{cases}",
            r"\begin{aligned}a&=b+c\\d&=e-f\end{aligned}",
            r"\vec{v}\cdot\hat{n}+\overline{AB}",
            r"\mathbf{A}\in\mathbb{R}^{n\times n}",
            r"P\propto f\times IPC",
            r"P_{wall}=1400/0.92\approx1522\text{ Вт}",
            r"E=1.522\times24\times30\approx1096\text{ кВт·ч}",
        ] {
            let image = raster(source, 24., 2., Color32::WHITE, true)
                .unwrap_or_else(|e| panic!("{source}: {e}"));
            assert!(image.size[0] > 0 && image.size[1] > 0);
            assert!(
                image.pixels.iter().any(|p| p.a() > 0),
                "Empty formula: {source}"
            );
        }
    }
    #[test]
    fn fraction_has_stacked_ink_and_horizontal_rule() {
        let image = raster(r"\frac{a}{b}", 32., 2., Color32::WHITE, true).unwrap();
        let counts: Vec<_> = image
            .pixels
            .chunks(image.size[0])
            .map(|r| r.iter().filter(|p| p.a() > 128).count())
            .collect();
        let bar = counts.iter().enumerate().max_by_key(|(_, n)| *n).unwrap().0;
        assert!(counts[bar] as f32 > image.size[0] as f32 * 0.7);
        assert!(counts[..bar.saturating_sub(2)].iter().any(|n| *n > 0));
        assert!(counts[(bar + 3).min(counts.len())..].iter().any(|n| *n > 0));
    }
    #[test]
    fn inline_formulas_preserve_surrounding_text_and_refresh_for_theme_and_size() {
        let ctx = egui::Context::default();
        crate::design::install(&ctx);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 300.),
                )),
                ..Default::default()
            },
            |ui| {
                let a = cached(ui, BANDWIDTH, 20., Color32::BLACK, true).unwrap();
                let same = cached(ui, BANDWIDTH, 20., Color32::BLACK, true).unwrap();
                let dark = cached(ui, BANDWIDTH, 20., Color32::WHITE, true).unwrap();
                let large = cached(ui, BANDWIDTH, 32., Color32::BLACK, true).unwrap();
                assert_eq!(a.texture.id(), same.texture.id());
                assert_ne!(a.texture.id(), dark.texture.id());
                assert!(large.size.y > a.size.y && large.size.x > a.size.x);
                let mut job = RichJob::default();
                job.append("Before ", 0., TextFormat::default());
                job.append_formula(
                    ui,
                    r"\frac{a}{b}",
                    TextFormat {
                        font_id: FontId::proportional(20.),
                        color: Color32::BLACK,
                        ..Default::default()
                    },
                    false,
                );
                job.append(" after", 0., TextFormat::default());
                job.show(ui);
            },
        );
        output.textures_delta.clear();
        let image = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.fill_texture_id() != egui::TextureId::default() => {
                    Some(r.rect)
                }
                _ => None,
            })
            .unwrap();
        for s in &output.shapes {
            if let egui::Shape::Text(t) = &s.shape {
                if t.galley.text().contains("Before") {
                    assert!(t.pos.x + t.galley.size().x <= image.left() + 1.);
                }
                if t.galley.text().contains("after") {
                    assert!(
                        t.pos.x + t.galley.rows[0].pos.x + t.galley.rows[0].glyphs[0].pos.x
                            >= image.right() - 1.,
                        "After text at {:?}, image at {:?}",
                        t,
                        image
                    );
                }
            }
        }
    }
    #[test]
    fn malformed_unknown_and_oversized_formulas_return_errors() {
        for source in [
            r"\frac{a}",
            r"\unknown{x}",
            r"\left(x",
            &"x".repeat(MAX_SOURCE + 1),
        ] {
            assert!(
                raster(source, 20., 1., Color32::WHITE, true).is_err(),
                "{source}"
            );
        }
    }
}
