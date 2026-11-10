//! Icono en la bandeja del sistema. Pertenece al launcher: al cerrar la ventana esta
//! se oculta y el launcher sigue vivo aquí. RootboundBackground no tiene icono.

use crate::types::Language;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG,
};

/// Peticiones de la bandeja que la interfaz atiende en su propio hilo.
#[derive(Default)]
pub struct TrayState {
    show: AtomicBool,
    quit: AtomicBool,
}

impl TrayState {
    pub fn take_show(&self) -> bool {
        self.show.swap(false, Ordering::SeqCst)
    }

    pub fn should_quit(&self) -> bool {
        self.quit.load(Ordering::SeqCst)
    }
}

fn tray_icon_image() -> Option<Icon> {
    let image = image::load_from_memory(crate::WINDOW_ICON_PNG).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).ok()
}

/// Arranca la bandeja en su propio hilo con su cola de mensajes. Al pulsar algo,
/// marca el estado y despierta a la interfaz con `request_repaint`.
pub fn spawn(ctx: egui::Context, language: Language) -> Arc<TrayState> {
    let state = Arc::new(TrayState::default());
    let worker = state.clone();
    thread::spawn(move || run(&ctx, &worker, language));
    state
}

fn run(ctx: &egui::Context, state: &TrayState, language: Language) {
    let (open_label, close_label) = match language {
        Language::Es => ("Abrir", "Cerrar"),
        Language::En => ("Open", "Close"),
    };
    let open = MenuItem::new(open_label, true, None);
    let close = MenuItem::new(close_label, true, None);
    let menu = Menu::new();
    if menu.append(&open).is_err() || menu.append(&close).is_err() {
        return;
    }
    let mut builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip(crate::service::WINDOW_TITLE);
    if let Some(icon) = tray_icon_image() {
        builder = builder.with_icon(icon);
    }
    let Ok(_tray) = builder.build() else {
        return;
    };

    let wake = |ctx: &egui::Context| ctx.request_repaint();
    // SAFETY: MSG admite el patrón de ceros y GetMessageW lo rellena antes de usarlo.
    let mut message: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: `message` es válido durante todo el bucle.
    while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
        // SAFETY: `message` lo acaba de rellenar GetMessageW.
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            let left_click = matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            );
            if left_click {
                state.show.store(true, Ordering::SeqCst);
                wake(ctx);
            }
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == *open.id() {
                state.show.store(true, Ordering::SeqCst);
            } else if event.id == *close.id() {
                state.quit.store(true, Ordering::SeqCst);
            }
            wake(ctx);
        }
    }
}
