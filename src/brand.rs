//! Shared app identity for the native window and sidebar.
use eframe::egui;

#[cfg(target_os = "linux")]
mod desktop;
#[cfg(target_os = "linux")]
pub use desktop::register_desktop_icon;

pub fn window_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../packaging/amigo.png"))
        .expect("Bundled Amigo icon must be a valid PNG")
}

pub fn logo(ui: &mut egui::Ui, size: f32) -> egui::Response {
    let id = egui::Id::new("amigo-brand-icon");
    let texture = ui
        .ctx()
        .data(|data| data.get_temp::<egui::TextureHandle>(id));
    let texture = texture.unwrap_or_else(|| {
        let icon = window_icon();
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [icon.width as usize, icon.height as usize],
            &icon.rgba,
        );
        let texture = ui
            .ctx()
            .load_texture("Amigo logo", image, egui::TextureOptions::LINEAR);
        ui.ctx()
            .data_mut(|data| data.insert_temp(id, texture.clone()));
        texture
    });
    ui.add(egui::Image::new((texture.id(), egui::Vec2::splat(size))).alt_text("Amigo logo"))
}
