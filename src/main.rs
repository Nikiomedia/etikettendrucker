#![windows_subsystem = "windows"]

#[macro_use]
mod i18n;
mod app;
mod icon;
mod model;
mod printer;
mod protocol;
mod render;
mod ui_kit;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Etikettendrucker")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([1040.0, 640.0])
            .with_icon(std::sync::Arc::new(egui::IconData { rgba: icon::draw_icon(64), width: 64, height: 64 }))
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("Etikettendrucker", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
