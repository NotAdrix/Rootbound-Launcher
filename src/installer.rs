//! Instalación en `C:\Rootbound`: el launcher se copia allí, crea su acceso directo y
//! se abre desde esa copia. También trae los datos de versiones anteriores.

use crate::legendary::hidden_command;
use crate::{paths, process, service, updater};
use std::path::{Component, Path, PathBuf};

const DESKTOP_SHORTCUT: &str = "Rootbound Launcher.lnk";

fn same_file(a: &Path, b: &Path) -> bool {
    let normalize = |p: &Path| p.to_string_lossy().to_lowercase().replace('/', "\\");
    normalize(a) == normalize(b)
}

/// Un `.exe` recién compilado con cargo no se borra al instalar.
fn is_build_output(exe: &Path) -> bool {
    let parts: Vec<String> = exe
        .components()
        .filter_map(|part| match part {
            Component::Normal(name) => Some(name.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect();
    parts
        .windows(2)
        .any(|pair| pair[0] == "target" && (pair[1] == "debug" || pair[1] == "release"))
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Las versiones anteriores guardaban datos en `%APPDATA%`, `%LOCALAPPDATA%` y
/// `~/.config/legendary`. Se copian una sola vez, para no perder la sesión ni nada más.
fn migrate_old_data() {
    let config = paths::config_dir();
    if config.exists() {
        return;
    }
    let old_config = dirs::config_dir().map(|dir| dir.join("rootbound_launcher"));
    let old_local = dirs::data_local_dir().map(|dir| dir.join("rootbound_launcher"));
    let old_legendary = dirs::home_dir().map(|dir| dir.join(".config").join("legendary"));

    // Los ajustes y el tiempo jugado no se pueden recuperar: si la copia falla, se deja
    // todo como estaba y se reintenta en la próxima apertura.
    if let Some(old) = old_config.as_ref().filter(|dir| dir.exists()) {
        if copy_dir(old, &config).is_err() {
            let _ = std::fs::remove_dir_all(&config);
            return;
        }
    }
    if let Some(old) = old_legendary.filter(|dir| dir.exists()) {
        if !paths::legendary_data().exists() && copy_dir(&old, &paths::legendary_data()).is_err() {
            let _ = std::fs::remove_dir_all(paths::legendary_data());
            let _ = std::fs::remove_dir_all(&config);
            return;
        }
    }
    // Legendary y las portadas se pueden volver a descargar: se copian si se puede.
    if let Some(old) = old_local.as_ref().filter(|dir| dir.exists()) {
        let _ = std::fs::copy(old.join("legendary.exe"), paths::root().join("legendary.exe"));
        let _ = copy_dir(&old.join("art"), &paths::root().join("art"));
    }
    let _ = std::fs::create_dir_all(&config);
    // La carpeta original de legendary se deja: la puede usar otro programa.
    for old in [old_config, old_local].into_iter().flatten() {
        let _ = std::fs::remove_dir_all(old);
    }
}

/// Crea un acceso directo `.lnk`. Las rutas van por variables de entorno para no
/// tener que escaparlas dentro del comando.
pub fn create_shortcut(shortcut: &Path, target: &Path) -> bool {
    let script = "$s = (New-Object -ComObject WScript.Shell).CreateShortcut($env:RB_LINK); \
                  $s.TargetPath = $env:RB_TARGET; $s.WorkingDirectory = $env:RB_DIR; $s.Save()";
    let folder = target.parent().unwrap_or(target);
    hidden_command("powershell")
        .args(["-NoProfile", "-Command", script])
        .env("RB_LINK", shortcut)
        .env("RB_TARGET", target)
        .env("RB_DIR", folder)
        .output()
        .is_ok_and(|out| out.status.success())
}

pub fn desktop_shortcut() -> Option<PathBuf> {
    Some(dirs::desktop_dir()?.join(DESKTOP_SHORTCUT))
}

fn install(current: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(paths::root())?;
    process::kill_exe(&paths::background_exe());
    std::fs::copy(current, paths::launcher_exe())?;
    std::fs::copy(current, paths::background_exe())?;
    migrate_old_data();
    if let Some(shortcut) = desktop_shortcut() {
        create_shortcut(&shortcut, &paths::launcher_exe());
    }
    service::remove_legacy_startup();
    Ok(())
}

/// Devuelve `true` si este proceso debe terminar porque ya se abrió la copia instalada.
pub fn ensure_installed() -> bool {
    let Ok(current) = std::env::current_exe() else {
        return false;
    };
    if same_file(&current, &paths::launcher_exe()) {
        return false;
    }
    if service::interface_is_open() {
        service::focus_existing_window();
        return true;
    }
    if install(&current).is_err() {
        return false;
    }
    if std::process::Command::new(paths::launcher_exe()).spawn().is_err() {
        return false;
    }
    if !is_build_output(&current) {
        updater::delete_file_after_exit(&current);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_cargo_build_output() {
        assert!(is_build_output(Path::new(r"C:\p\target\release\a.exe")));
        assert!(is_build_output(Path::new(r"C:\p\Target\Debug\a.exe")));
        assert!(!is_build_output(Path::new(r"C:\Users\x\Downloads\a.exe")));
    }

    #[test]
    fn creates_windows_shortcuts() {
        let folder = std::env::temp_dir().join(format!("rootbound_lnk_{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let link = folder.join("Prueba Rootbound.lnk");
        let created = create_shortcut(&link, Path::new(r"C:\Windows\notepad.exe"));
        let exists = link.exists();
        std::fs::remove_dir_all(&folder).unwrap();
        assert!(created && exists);
    }

    #[test]
    fn copies_folders_recursively() {
        let base = std::env::temp_dir().join(format!("rootbound_copy_{}", std::process::id()));
        std::fs::create_dir_all(base.join("from").join("sub")).unwrap();
        std::fs::write(base.join("from").join("sub").join("a.txt"), "x").unwrap();
        copy_dir(&base.join("from"), &base.join("to")).unwrap();
        let copied = base.join("to").join("sub").join("a.txt").exists();
        std::fs::remove_dir_all(&base).unwrap();
        assert!(copied);
    }
}
