//! RootboundBackground: programa aparte, silencioso y sin ventana ni icono. Arranca
//! con Windows desde la carpeta de Inicio, cuenta el tiempo jugado y actualiza el
//! último juego ejecutado si tiene "Actualizar Automáticamente". Si no hace falta
//! ninguna de las dos cosas, ni se ejecuta. El icono de la bandeja es del launcher.

use crate::config::{self, Config};
use crate::juegos::logica::{self, PlaytimeTracker};
use crate::legendary::{hidden_command, Legendary};
use crate::process::{self, NamedLock};
use crate::types::Events;
use crate::{installer, paths};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
};
use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
use winreg::RegKey;

pub const WINDOW_TITLE: &str = "Rootbound Launcher";
const STARTUP_SHORTCUT: &str = "RootboundBackground.lnk";
const SERVICE_LOCK: &str = "Local\\RootboundLauncherService";
const GUI_LOCK: &str = "Local\\RootboundLauncherGui";
const TICK: Duration = Duration::from_secs(60);
const TICKS_BETWEEN_UPDATES: u32 = 30;
const LEGACY_TASK: &str = "RootboundLauncher";
const LEGACY_RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const LEGACY_RUN_VALUE: &str = "Rootbound Launcher";

/// El mismo ejecutable hace de RootboundBackground cuando se llama así.
pub fn is_background_process() -> bool {
    std::env::current_exe().is_ok_and(|exe| {
        exe.file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(paths::BACKGROUND_EXE))
    })
}

fn is_needed(config: &Config) -> bool {
    !config.tracking_disabled || !config.auto_update.is_empty()
}

/// Bucle silencioso: sin ventana, sin icono, sin consola.
pub fn run() {
    if !is_needed(&config::load()) {
        return;
    }
    let Some(_lock) = NamedLock::acquire(SERVICE_LOCK) else {
        return;
    };
    let events = Events::detached();
    let Ok(cli) = Legendary::locate(&events) else {
        return;
    };
    let mut tracker = PlaytimeTracker::new(TICK);
    let mut ticks = 0;
    loop {
        let config = config::load();
        if !is_needed(&config) {
            return;
        }
        if !config.tracking_disabled {
            tracker.tick(&cli);
        }
        // Con la interfaz abierta actualiza ella, para mostrar el progreso.
        if ticks % TICKS_BETWEEN_UPDATES == 0 && !interface_is_open() {
            logica::update_last_played(&cli, &config, &events);
        }
        ticks += 1;
        sleep(TICK);
    }
}

pub fn interface_is_open() -> bool {
    NamedLock::exists(GUI_LOCK)
}

pub fn focus_existing_window() {
    let title: Vec<u16> = WINDOW_TITLE.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `title` termina en cero; se comprueba el handle antes de usarlo.
    unsafe {
        let window = FindWindowW(std::ptr::null(), title.as_ptr());
        if !window.is_null() {
            ShowWindow(window, SW_RESTORE);
            SetForegroundWindow(window);
        }
    }
}

/// Se mantiene mientras la interfaz siga abierta. `None` si ya hay otra abierta.
pub fn hold_gui_lock() -> Option<NamedLock> {
    NamedLock::acquire(GUI_LOCK)
}

fn startup_shortcut() -> Option<PathBuf> {
    Some(
        dirs::config_dir()?
            .join(r"Microsoft\Windows\Start Menu\Programs\Startup")
            .join(STARTUP_SHORTCUT),
    )
}

/// Activa o detiene RootboundBackground según los ajustes: acceso directo en la
/// carpeta de Inicio y proceso en marcha solo si hace falta.
pub fn sync() {
    let Some(shortcut) = startup_shortcut() else {
        return;
    };
    let exe = paths::background_exe();
    if is_needed(&config::load()) && exe.exists() {
        if !shortcut.exists() {
            installer::create_shortcut(&shortcut, &exe);
        }
        if !NamedLock::exists(SERVICE_LOCK) {
            let _ = std::process::Command::new(exe).spawn();
        }
    } else {
        let _ = std::fs::remove_file(shortcut);
        stop();
    }
}

pub fn stop() {
    process::kill_exe(&paths::background_exe());
    sleep(Duration::from_millis(500));
}

/// Quita lo que usaban versiones anteriores para arrancar con Windows.
pub fn remove_legacy_startup() {
    let _ = hidden_command("schtasks")
        .args(["/Delete", "/TN", LEGACY_TASK, "/F"])
        .output();
    if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(LEGACY_RUN_KEY, KEY_SET_VALUE) {
        let _ = key.delete_value(LEGACY_RUN_VALUE);
    }
}

/// Elimina los accesos directos al desinstalar.
pub fn unregister() {
    remove_legacy_startup();
    for shortcut in [startup_shortcut(), installer::desktop_shortcut()].into_iter().flatten() {
        let _ = std::fs::remove_file(shortcut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_background_runs_only_when_something_needs_it() {
        let mut config = Config::default();
        assert!(is_needed(&config));
        config.tracking_disabled = true;
        assert!(!is_needed(&config));
        config.auto_update.insert("Fortnite".to_string());
        assert!(is_needed(&config));
    }
}
