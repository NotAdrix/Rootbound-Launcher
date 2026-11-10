#![windows_subsystem = "windows"]

mod backend;
mod config;
mod drivers;
mod frontend;
mod i18n;
mod import;
mod juegos;
mod legendary;
mod login;
mod process;
mod service;
mod types;
mod updater;

use eframe::egui;
use std::sync::mpsc::channel;
use types::Events;

pub const VERSION_LABEL: &str = "v1.0 Alpha";
const WINDOW_ICON_PNG: &[u8] = include_bytes!("../window_icon.png");
const LOGO_PNG: &[u8] = include_bytes!("../logo.png");

fn window_icon() -> egui::IconData {
    match image::load_from_memory(WINDOW_ICON_PNG) {
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
    }
}

fn main() -> Result<(), eframe::Error> {
    updater::remove_previous_version();
    if std::env::args().any(|arg| arg == service::BACKGROUND_FLAG) {
        service::run();
        return Ok(());
    }
    let Some(_gui_lock) = service::hold_gui_lock() else {
        service::focus_existing_window();
        return Ok(());
    };
    std::thread::spawn(service::install_and_start);
    let config = config::load();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1300.0, 720.0])
            .with_min_inner_size([900.0, 520.0])
            .with_icon(window_icon()),
        ..Default::default()
    };

    eframe::run_native(
        service::WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            let (tx_cmd, rx_cmd) = channel();
            let (tx_evt, rx_evt) = channel();
            backend::start(rx_cmd, Events::new(tx_evt, cc.egui_ctx.clone()));
            Box::new(frontend::LauncherApp::new(cc, tx_cmd, rx_evt, config))
        }),
    )
}
