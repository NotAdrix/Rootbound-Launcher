//! Proceso en segundo plano: icono en la bandeja del sistema, tiempo jugado y
//! actualización de los juegos marcados. Es una tarea programada y no un servicio de
//! Windows, porque estos corren como SYSTEM y no verían la sesión de legendary.

use crate::config;
use crate::juegos::logica::{self, PlaytimeTracker};
use crate::legendary::{self, hidden_command, Legendary};
use crate::process::{self, NamedLock};
use crate::types::{Events, Language};
use std::path::Path;
use std::thread::{self, sleep};
use std::time::Duration;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, FindWindowW, GetMessageW, SetForegroundWindow, ShowWindow, TranslateMessage,
    MSG, SW_RESTORE,
};
use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
use winreg::RegKey;

pub const BACKGROUND_FLAG: &str = "--background";
pub const WINDOW_TITLE: &str = "Rootbound Launcher";
const TASK_NAME: &str = "RootboundLauncher";
const STARTUP_ENTRY: &str = "Rootbound Launcher";
const STARTUP_STUB_NAME: &str = "Rootbound Launcher.exe";
const STARTUP_STUB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/startup_stub.exe"));
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const SERVICE_LOCK: &str = "Local\\RootboundLauncherService";
const GUI_LOCK: &str = "Local\\RootboundLauncherGui";
const TICK: Duration = Duration::from_secs(60);
const TICKS_BETWEEN_UPDATES: u32 = 30;

pub fn run() {
    let Some(_lock) = NamedLock::acquire(SERVICE_LOCK) else {
        return;
    };
    thread::spawn(background_work);
    run_tray();
}

fn background_work() {
    let events = Events::detached();
    let Ok(cli) = Legendary::locate(&events) else {
        return;
    };
    let mut tracker = PlaytimeTracker::new(TICK);
    let mut ticks = 0;
    loop {
        tracker.tick(&cli);
        // Con la interfaz abierta actualiza ella, para mostrar el progreso.
        if ticks % TICKS_BETWEEN_UPDATES == 0 && !NamedLock::exists(GUI_LOCK) {
            logica::auto_update(&cli, &events);
        }
        ticks += 1;
        sleep(TICK);
    }
}

fn tray_icon_image() -> Option<Icon> {
    let image = image::load_from_memory(crate::WINDOW_ICON_PNG).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).ok()
}

fn run_tray() {
    let (open_label, close_label) = match config::load().language {
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
        .with_tooltip(WINDOW_TITLE);
    if let Some(icon) = tray_icon_image() {
        builder = builder.with_icon(icon);
    }
    let Ok(_tray) = builder.build() else {
        return;
    };

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
                open_interface();
            }
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == *open.id() {
                open_interface();
            } else if event.id == *close.id() {
                process::kill_other_instances();
                std::process::exit(0);
            }
        }
    }
}

fn open_interface() {
    if NamedLock::exists(GUI_LOCK) {
        focus_existing_window();
    } else if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
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

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Tarea sin disparador: solo se ejecuta cuando la pide la entrada de inicio. Así,
/// desactivarla en la configuración de Windows la desactiva de verdad.
fn task_definition(exe: &Path, user: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{command}</Command>
      <Arguments>{BACKGROUND_FLAG}</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        user = xml_escape(user),
        command = xml_escape(&exe.display().to_string()),
    )
}

fn register_task(exe: &Path) {
    let (Ok(domain), Ok(name)) = (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) else {
        return;
    };
    let xml = task_definition(exe, &format!(r"{domain}\{name}"));
    let path = std::env::temp_dir().join("rootbound_task.xml");
    let bytes: Vec<u8> = std::iter::once(0xFEFF_u16)
        .chain(xml.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect();
    if std::fs::write(&path, bytes).is_ok() {
        let _ = hidden_command("schtasks")
            .args(["/Create", "/TN", TASK_NAME, "/F", "/XML"])
            .arg(&path)
            .output();
        let _ = std::fs::remove_file(path);
    }
}

fn register_startup_entry() {
    let Some(dir) = legendary::install_dir() else {
        return;
    };
    let stub = dir.join(STARTUP_STUB_NAME);
    let _ = std::fs::create_dir_all(&dir);
    if std::fs::write(&stub, STARTUP_STUB).is_err() && !stub.exists() {
        return;
    }
    if let Ok((key, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY) {
        let _ = key.set_value(STARTUP_ENTRY, &format!("\"{}\"", stub.display()));
    }
}

/// Registra el inicio con Windows y arranca el proceso si todavía no está en marcha.
pub fn install_and_start() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    register_task(&exe);
    register_startup_entry();
    if !NamedLock::exists(SERVICE_LOCK) {
        let _ = std::process::Command::new(exe).arg(BACKGROUND_FLAG).spawn();
    }
}

pub fn stop() {
    process::kill_other_instances();
    sleep(Duration::from_millis(500));
}

pub fn unregister() {
    let _ = hidden_command("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .output();
    if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE) {
        let _ = key.delete_value(STARTUP_ENTRY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_task_definition_escapes_special_characters() {
        let xml = task_definition(Path::new(r"C:\Juegos & Apps\app.exe"), r"PC\Ana");
        assert!(xml.contains(r"<Command>C:\Juegos &amp; Apps\app.exe</Command>"));
        assert!(xml.contains(r"<UserId>PC\Ana</UserId>"));
        assert!(!xml.contains("Trigger"));
    }
}
