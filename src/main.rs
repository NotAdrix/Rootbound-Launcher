#![windows_subsystem = "windows"]

mod ajustes;
mod backend;
mod frontend;
mod inicio;
mod juegos;
mod libreria;
mod terminal;
mod types;

use eframe::egui;
use std::sync::mpsc::channel;

fn main() -> Result<(), eframe::Error> {
    let (tx_cmd, rx_cmd) = channel();
    let (tx_evt, rx_evt) = channel();

    let config = backend::load_config();
    backend::start_backend(rx_cmd, tx_evt);

    let icon_bytes = include_bytes!("../icon.png");
    let icon = match image::load_from_memory(icon_bytes) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            egui::IconData {
                rgba: rgba.into_raw(),
                width,
                height,
            }
        }
        Err(_) => egui::IconData::default(),
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_min_inner_size([700.0, 400.0])
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "Rootbound Launcher",
        options,
        Box::new(move |cc| Box::new(frontend::LauncherApp::new(cc, tx_cmd, rx_evt, config))),
    )
}
