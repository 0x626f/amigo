#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() -> eframe::Result {
    #[cfg(target_os = "linux")]
    if let Err(error) = amigo::brand::register_desktop_icon() {
        eprintln!("Could not register the Amigo desktop icon: {error}");
    }
    let icon = amigo::brand::window_icon();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Amigo")
            .with_app_id("amigo")
            .with_icon(icon)
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([980.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Amigo",
        options,
        Box::new(|cc| Ok(Box::new(amigo::Amigo::new(cc)))),
    )
}
