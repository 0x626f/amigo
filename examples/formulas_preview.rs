//! Capture native formula previews: cargo run --example formulas_preview -- /tmp/formulas.png [--dark]
use eframe::{App, egui};
struct Preview {
    output: std::path::PathBuf,
    frames: u32,
}
const SOURCE: &str = r"# Formula previews

Bandwidth with a stacked fraction

$$B_{GB/s}=\frac{MT/s\times W_{bit}\times C}{8\times1000}$$

Nested fractions, roots, and scripts

$$x_{n+1}=\frac{1}{1+\frac{1}{x_n}}+\sqrt[3]{a^2+b^2}$$

Sums and integrals

$$\sum_{i=1}^{n}i=\frac{n(n+1)}{2}\qquad\int_0^\infty e^{-x^2}\,dx=\frac{\sqrt{\pi}}{2}$$

Matrices and cases

$$A=\begin{pmatrix}a&b\\c&d\end{pmatrix}\qquad f(x)=\begin{cases}x^2&x\ge0\\-x&x<0\end{cases}$$

Inline math: $\frac{a}{b}$ followed by text and $\sqrt{x^2+y^2}$.

Cyrillic units

$$P_{wall}=1400/0.92\approx1522\text{ Вт}\qquad E\approx1096\text{ кВт·ч}$$
";
impl App for Preview {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        if let Some(image) = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        }) {
            let bytes: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            image::save_buffer(
                &self.output,
                &bytes,
                image.width() as u32,
                image.height() as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        egui::CentralPanel::default().show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(24)
                .show(ui, |ui| amigo_ui::markdown::show(ui, SOURCE, None));
        });
        self.frames += 1;
        if self.frames == 6 {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        ui.ctx().request_repaint();
    }
}
fn main() -> eframe::Result {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/amigo-formulas.png".into())
        .into();
    let dark = std::env::args().any(|a| a == "--dark");
    eframe::run_native(
        "Amigo formulas",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1000., 820.]),
            ..Default::default()
        },
        Box::new(move |cc| {
            amigo_ui::design::install(&cc.egui_ctx);
            amigo_ui::design::apply_theme(&cc.egui_ctx, dark);
            Ok(Box::new(Preview { output, frames: 0 }))
        }),
    )
}
