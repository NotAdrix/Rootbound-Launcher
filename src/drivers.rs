//! Limpieza de drivers de vídeo con DDU en modo seguro, sin intervención del usuario.
//!
//! Se prepara todo antes de reiniciar, de modo que cualquier fallo (descarga, hash,
//! instalación) ocurre sin haber tocado el sistema. El modo seguro se pide con una
//! entrada de arranque de un solo uso: si algo sale mal, basta reiniciar.

use crate::config;
use crate::legendary::hidden_command;
use crate::types::{DriversFailure, Events, Language, LogMsg};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_SET_VALUE};
use winreg::RegKey;

const WINGET_API: &str = "https://api.github.com/repos/microsoft/winget-pkgs/contents/manifests";
const WINGET_RAW: &str = "https://raw.githubusercontent.com/microsoft/winget-pkgs/master/manifests";
const DDU: Package = Package {
    path: "w/Wagnardsoft/DisplayDriverUninstaller",
    id: "Wagnardsoft.DisplayDriverUninstaller",
};
const NVCLEANSTALL: Package = Package {
    path: "t/TechPowerUp/NVCleanstall",
    id: "TechPowerUp.NVCleanstall",
};
const NVCLEANSTALL_PAGE: &str = "https://www.techpowerup.com/download/techpowerup-nvcleanstall/";
const DDU_EXE: &str = "Display Driver Uninstaller.exe";
const DDU_UNINSTALL_KEYS: [&str; 2] = [
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Display Driver Uninstaller",
    r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Display Driver Uninstaller",
];
const DISPLAY_CLASS_GUID: &str = "{4d36e968-e325-11ce-bfc1-08002be10318}";
const PCI_ENUM_KEY: &str = r"SYSTEM\CurrentControlSet\Enum\PCI";
const GPU_CLASS_KEY: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
const RUN_ONCE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";
/// El asterisco hace que Windows ejecute la entrada también en modo seguro.
const SAFE_MODE_ENTRY: &str = "*RootboundDriversCleanup";
const NVIDIA_DRIVERS_URL: &str = "https://www.nvidia.com/Download/index.aspx";
const AMD_DRIVERS_URL: &str = "https://www.amd.com/en/support/download/drivers.html";

struct Package {
    path: &'static str,
    id: &'static str,
}

#[derive(Debug, PartialEq)]
struct Installer {
    url: String,
    sha256: String,
}

#[derive(Deserialize)]
struct CatalogEntry {
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Default, Debug, PartialEq, Clone, Copy)]
struct GpuVendors {
    nvidia: bool,
    amd: bool,
}

/// Pasos que se ejecutan tras reiniciar, ya con todo descargado.
struct Finish {
    urls: Vec<&'static str>,
    program: Option<PathBuf>,
}

fn version_key(version: &str) -> Vec<u32> {
    version.split('.').map(|part| part.parse().unwrap_or(0)).collect()
}

fn latest_version(entries: Vec<CatalogEntry>) -> Option<String> {
    entries
        .into_iter()
        .filter(|entry| entry.kind == "dir")
        .map(|entry| entry.name)
        .max_by_key(|name| version_key(name))
}

fn yaml_value<'a>(yaml: &'a str, key: &str) -> Option<&'a str> {
    yaml.lines().find_map(|line| {
        let line = line.trim().trim_start_matches("- ");
        line.strip_prefix(key)?.strip_prefix(':').map(str::trim)
    })
}

fn parse_installer(yaml: &str) -> Option<Installer> {
    Some(Installer {
        url: yaml_value(yaml, "InstallerUrl")?.to_string(),
        sha256: yaml_value(yaml, "InstallerSha256")?.to_ascii_lowercase(),
    })
}

fn find_installer(package: &Package) -> Option<Installer> {
    let listing = crate::updater::fetch_text(&format!("{WINGET_API}/{}", package.path)).ok()?;
    let version = latest_version(serde_json::from_str(&listing).ok()?)?;
    let manifest = format!("{WINGET_RAW}/{}/{version}/{}.installer.yaml", package.path, package.id);
    parse_installer(&crate::updater::fetch_text(&manifest).ok()?)
}

fn sha256_of(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Descarta el archivo si su hash no coincide con el publicado en el catálogo.
fn check_hash(path: &Path, expected: &str) -> Result<(), DriversFailure> {
    if sha256_of(path).as_deref() == Some(expected) {
        return Ok(());
    }
    let _ = std::fs::remove_file(path);
    Err(DriversFailure::InvalidHash)
}

fn download_verified(package: &Package, destination: &Path) -> Result<(), DriversFailure> {
    let installer = find_installer(package).ok_or(DriversFailure::Download)?;
    crate::updater::download(&installer.url, destination).map_err(|_| DriversFailure::Download)?;
    check_hash(destination, &installer.sha256)
}

fn form_values(html: &str, field: &str) -> Vec<String> {
    let marker = format!("name=\"{field}\" value=\"");
    html.split(&marker)
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_owned)
        .collect()
}

/// El enlace del catálogo no funciona: TechPowerUp solo entrega el archivo tras su
/// formulario, con una URL firmada. El hash sigue viniendo del catálogo.
fn download_nvcleanstall(destination: &Path) -> Result<(), DriversFailure> {
    let installer = find_installer(&NVCLEANSTALL).ok_or(DriversFailure::Download)?;
    let page = crate::updater::fetch_web_page(NVCLEANSTALL_PAGE).map_err(|_| DriversFailure::Download)?;
    let id = form_values(&page, "id").into_iter().next().ok_or(DriversFailure::Download)?;
    let (mirrors, _) = crate::updater::post_web_form(NVCLEANSTALL_PAGE, format!("id={id}"))
        .map_err(|_| DriversFailure::Download)?;
    for server in form_values(&mirrors, "server_id") {
        let body = format!("id={id}&server_id={server}");
        let Ok((_, Some(url))) = crate::updater::post_web_form(NVCLEANSTALL_PAGE, body) else {
            continue;
        };
        if crate::updater::download(&url, destination).is_ok() {
            return check_hash(destination, &installer.sha256);
        }
    }
    Err(DriversFailure::Download)
}

fn detect_gpu_vendors() -> GpuVendors {
    let mut vendors = pci_display_vendors();
    let Ok(class) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(GPU_CLASS_KEY) else {
        return vendors;
    };
    for name in class.enum_keys().flatten() {
        let Ok(adapter) = class.open_subkey(&name) else {
            continue;
        };
        let description = adapter.get_value::<String, _>("DriverDesc").unwrap_or_default();
        let found = vendors_in(&description);
        vendors.nvidia |= found.nvidia;
        vendors.amd |= found.amd;
    }
    vendors
}

/// Identifica la tarjeta por su fabricante en el bus PCI: sigue funcionando después
/// de borrar los drivers, cuando Windows ya no conoce el nombre de la gráfica.
fn pci_display_vendors() -> GpuVendors {
    let mut vendors = GpuVendors::default();
    let Ok(pci) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(PCI_ENUM_KEY) else {
        return vendors;
    };
    for name in pci.enum_keys().flatten() {
        let found = vendors_in_pci_id(&name);
        if found == GpuVendors::default() {
            continue;
        }
        let Ok(device) = pci.open_subkey(&name) else {
            continue;
        };
        let is_display = device.enum_keys().flatten().any(|instance| {
            device
                .open_subkey(&instance)
                .and_then(|k| k.get_value::<String, _>("ClassGUID"))
                .is_ok_and(|class| class.eq_ignore_ascii_case(DISPLAY_CLASS_GUID))
        });
        if is_display {
            vendors.nvidia |= found.nvidia;
            vendors.amd |= found.amd;
        }
    }
    vendors
}

fn vendors_in_pci_id(id: &str) -> GpuVendors {
    let id = id.to_ascii_uppercase();
    GpuVendors {
        nvidia: id.starts_with("VEN_10DE"),
        amd: id.starts_with("VEN_1002"),
    }
}

pub fn has_nvidia_gpu() -> bool {
    detect_gpu_vendors().nvidia
}

fn vendors_in(description: &str) -> GpuVendors {
    let text = description.to_lowercase();
    GpuVendors {
        nvidia: text.contains("nvidia"),
        amd: text.contains("amd") || text.contains("radeon") || text.contains("advanced micro"),
    }
}

/// Cambiar el arranque con BitLocker activo puede exigir la clave de recuperación.
fn bitlocker_is_active() -> bool {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let query = format!(
        "(Get-CimInstance -Namespace root/cimv2/Security/MicrosoftVolumeEncryption \
         -ClassName Win32_EncryptableVolume -Filter \"DriveLetter='{drive}'\").ProtectionStatus"
    );
    hidden_command("powershell")
        .args(["-NoProfile", "-Command", &query])
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).trim() == "1")
}

fn work_dir() -> Result<PathBuf, DriversFailure> {
    let dir = crate::paths::drivers_dir();
    std::fs::create_dir_all(&dir).map_err(|_| DriversFailure::Prepare)?;
    Ok(dir)
}

fn installed_ddu() -> Option<PathBuf> {
    let machine = RegKey::predef(HKEY_LOCAL_MACHINE);
    let registered = DDU_UNINSTALL_KEYS.iter().find_map(|key| {
        machine.open_subkey(key).and_then(|k| k.get_value::<String, _>("InstallLocation")).ok()
    });
    let default_folders = ["ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|base| PathBuf::from(base).join("Display Driver Uninstaller"));
    registered
        .map(PathBuf::from)
        .into_iter()
        .chain(default_folders)
        .map(|folder| folder.join(DDU_EXE))
        .find(|exe| exe.exists())
}

fn install_ddu(dir: &Path) -> Result<PathBuf, DriversFailure> {
    let setup = dir.join("ddu_setup.exe");
    download_verified(&DDU, &setup)?;
    hidden_command(&setup)
        .arg("/S")
        .status()
        .map_err(|_| DriversFailure::Install)?;
    let _ = std::fs::remove_file(setup);
    installed_ddu().ok_or(DriversFailure::Install)
}

fn prepare_finish(
    vendors: GpuVendors,
    use_nvcleanstall: bool,
    dir: &Path,
) -> Result<Finish, DriversFailure> {
    let mut urls = Vec::new();
    let mut program = None;
    if vendors.nvidia {
        if use_nvcleanstall {
            let path = dir.join("NVCleanstall.exe");
            download_nvcleanstall(&path)?;
            program = Some(path);
        } else {
            urls.push(NVIDIA_DRIVERS_URL);
        }
    }
    if vendors.amd {
        urls.push(AMD_DRIVERS_URL);
    }
    Ok(Finish { urls, program })
}

fn finish_script(finish: &Finish) -> String {
    let mut script = String::from("@echo off\r\n");
    for url in &finish.urls {
        script.push_str(&format!("start \"\" \"{url}\"\r\n"));
    }
    if let Some(program) = &finish.program {
        script.push_str(&format!("start \"\" \"{}\"\r\n", program.display()));
    }
    script
}

/// Si por lo que sea arranca en modo normal, no hace nada: DDU solo debe correr en modo seguro.
fn safe_mode_script(
    language: Language,
    ddu: &Path,
    boot_entry: &str,
    finish: &Path,
    log: &Path,
) -> String {
    let message = match language {
        Language::Es => "Eliminando drivers de GPU, espere...",
        Language::En => "Removing GPU drivers, please wait...",
    };
    format!(
        "@echo off\r\n\
         title Rootbound Launcher\r\n\
         if not defined SAFEBOOT_OPTION goto not_safe_mode\r\n\
         echo {message}\r\n\
         reg add \"HKLM\\{RUN_ONCE_KEY}\" /v RootboundDriversFinish /t REG_SZ /d \"cmd.exe /c \\\"{finish}\\\"\" /f >nul\r\n\
         start \"\" /wait \"{ddu}\" -silent -cleanallgpus\r\n\
         >>\"{log}\" echo DDU exit code: %errorlevel%\r\n\
         bcdedit /delete {boot_entry} /f >nul\r\n\
         shutdown /r /t 3\r\n\
         exit /b\r\n\
         :not_safe_mode\r\n\
         bcdedit /delete {boot_entry} /f >nul\r\n\
         exit /b\r\n",
        finish = finish.display(),
        ddu = ddu.display(),
        log = log.display(),
    )
}

fn bcdedit(args: &[&str]) -> Option<String> {
    let out = hidden_command("bcdedit").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn guid_in(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = start + text[start..].find('}')?;
    Some(text[start..=end].to_string())
}

/// Crea una entrada de arranque en modo seguro que Windows usa una sola vez.
fn schedule_safe_boot() -> Result<String, DriversFailure> {
    let copied = bcdedit(&["/copy", "{current}", "/d", "Rootbound Safe Mode"]).ok_or(DriversFailure::Boot)?;
    let entry = guid_in(&copied).ok_or(DriversFailure::Boot)?;
    let configured = bcdedit(&["/set", &entry, "safeboot", "minimal"]).is_some()
        && bcdedit(&["/bootsequence", &entry]).is_some();
    if !configured {
        bcdedit(&["/delete", &entry, "/f"]);
        return Err(DriversFailure::Boot);
    }
    Ok(entry)
}

fn prepare_and_schedule(language: Language, use_nvcleanstall: bool, events: &Events) -> Result<(), DriversFailure> {
    if bitlocker_is_active() {
        return Err(DriversFailure::BitLocker);
    }
    let dir = work_dir()?;
    events.log(LogMsg::DriversDownloading);
    let ddu = install_ddu(&dir)?;
    let finish = prepare_finish(detect_gpu_vendors(), use_nvcleanstall, &dir)?;

    let finish_path = dir.join("finish.cmd");
    std::fs::write(&finish_path, finish_script(&finish)).map_err(|_| DriversFailure::Prepare)?;

    let entry = schedule_safe_boot()?;
    let script_path = dir.join("safe_mode.cmd");
    let script = safe_mode_script(language, &ddu, &entry, &finish_path, &dir.join("drivers.log"));
    let registered = std::fs::write(&script_path, script).is_ok()
        && RegKey::predef(HKEY_LOCAL_MACHINE)
            .create_subkey(RUN_ONCE_KEY)
            .and_then(|(key, _)| {
                key.set_value(SAFE_MODE_ENTRY, &format!("cmd.exe /c \"{}\"", script_path.display()))
            })
            .is_ok();
    if !registered {
        bcdedit(&["/delete", &entry, "/f"]);
        return Err(DriversFailure::Prepare);
    }

    events.log(LogMsg::DriversRestarting);
    let restarted = hidden_command("shutdown").args(["/r", "/t", "10"]).status();
    if restarted.is_err() {
        cancel_safe_boot(&entry);
        return Err(DriversFailure::Boot);
    }
    Ok(())
}

fn cancel_safe_boot(entry: &str) {
    bcdedit(&["/delete", entry, "/f"]);
    if let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(RUN_ONCE_KEY, KEY_SET_VALUE) {
        let _ = key.delete_value(SAFE_MODE_ENTRY);
    }
}

/// Abre NVCleanstall, descargándolo y verificándolo solo la primera vez.
pub fn open_nvcleanstall(events: &Events) {
    let opened = work_dir().and_then(|dir| {
        let path = dir.join("NVCleanstall.exe");
        if !path.exists() {
            events.log(LogMsg::NvcleanstallDownloading);
            download_nvcleanstall(&path)?;
        }
        // El instalador pide administrador: así Windows muestra el aviso en vez de fallar.
        crate::process::shell_open(&path, "", false)
            .then_some(())
            .ok_or(DriversFailure::Install)
    });
    if let Err(failure) = opened {
        events.log(LogMsg::DriversFailed(failure));
    }
}

pub const ELEVATED_FLAG: &str = "--clean-drivers";

/// Cambiar el arranque necesita administrador: se pide solo para esto, relanzando el
/// launcher con permisos. Si ya los tiene, se hace directamente.
pub fn clean_drivers(events: &Events) {
    if crate::process::is_elevated() {
        let config = config::load();
        if let Err(failure) = prepare_and_schedule(config.language, config.use_nvcleanstall, events) {
            events.log(LogMsg::DriversFailed(failure));
        }
        return;
    }
    events.log(LogMsg::DriversElevating);
    let launched = std::env::current_exe()
        .is_ok_and(|exe| crate::process::shell_open(&exe, ELEVATED_FLAG, true));
    if !launched {
        events.log(LogMsg::DriversFailed(DriversFailure::Cancelled));
    }
}

/// Proceso con permisos de administrador y sin interfaz: si falla, avisa con un mensaje.
pub fn clean_drivers_elevated() {
    let config = config::load();
    let result = prepare_and_schedule(config.language, config.use_nvcleanstall, &Events::detached());
    if let Err(failure) = result {
        let text = crate::i18n::render_log(config.language, &LogMsg::DriversFailed(failure));
        show_message(&text);
    }
}

fn show_message(text: &str) {
    let text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let title: Vec<u16> = "Rootbound Launcher".encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: ambas cadenas terminan en cero y viven durante la llamada.
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_highest_catalog_version() {
        let entries = ["18.1.5.3", "18.1.10.0", "18.1.9.9"]
            .iter()
            .map(|name| CatalogEntry { name: name.to_string(), kind: "dir".to_string() })
            .chain([CatalogEntry { name: "99.0".to_string(), kind: "file".to_string() }])
            .collect();
        assert_eq!(latest_version(entries).as_deref(), Some("18.1.10.0"));
    }

    #[test]
    fn reads_the_installer_from_a_winget_manifest() {
        let yaml = "PackageVersion: 18.1.5.3\nInstallers:\n- Architecture: x64\n  InstallerUrl: https://x/DDU%20v1_setup.exe\n  InstallerSha256: ABC123\nManifestType: installer\n";
        assert_eq!(
            parse_installer(yaml),
            Some(Installer { url: "https://x/DDU%20v1_setup.exe".to_string(), sha256: "abc123".to_string() })
        );
    }

    #[test]
    fn reads_values_from_download_forms() {
        let html = r#"<input type="hidden" name="id" value="2849" /><button name="server_id" value="26"></button><input name="id" value="2779">"#;
        assert_eq!(form_values(html, "id"), ["2849", "2779"]);
        assert_eq!(form_values(html, "server_id"), ["26"]);
    }

    #[test]
    fn recognises_vendors_by_pci_id() {
        assert_eq!(vendors_in_pci_id("VEN_10DE&DEV_2484"), GpuVendors { nvidia: true, amd: false });
        assert_eq!(vendors_in_pci_id("ven_1002&dev_73ff"), GpuVendors { nvidia: false, amd: true });
        assert_eq!(vendors_in_pci_id("VEN_8086&DEV_4680"), GpuVendors::default());
    }

    #[test]
    fn recognises_gpu_vendors() {
        assert_eq!(vendors_in("NVIDIA GeForce RTX 3070"), GpuVendors { nvidia: true, amd: false });
        assert_eq!(vendors_in("AMD Radeon RX 6600"), GpuVendors { nvidia: false, amd: true });
        assert_eq!(vendors_in("Intel(R) UHD Graphics"), GpuVendors::default());
    }

    #[test]
    fn reads_the_boot_entry_id_in_any_language() {
        assert_eq!(
            guid_in("La entrada se copió correctamente en {ab-12}.\r\n").as_deref(),
            Some("{ab-12}")
        );
    }

    #[test]
    fn the_cleanup_only_runs_in_safe_mode_and_removes_the_boot_entry() {
        let script = safe_mode_script(
            Language::Es,
            Path::new(r"C:\DDU\Display Driver Uninstaller.exe"),
            "{ab-12}",
            Path::new(r"C:\d\finish.cmd"),
            Path::new(r"C:\d\drivers.log"),
        );
        let guard = script.find("if not defined SAFEBOOT_OPTION").unwrap();
        let ddu = script.find("-silent -cleanallgpus").unwrap();
        let delete = script.find("bcdedit /delete {ab-12}").unwrap();
        assert!(guard < ddu && ddu < delete);
        assert!(script.contains("Eliminando drivers de GPU, espere..."));
    }

    #[test]
    fn finishing_opens_the_page_or_runs_nvcleanstall() {
        let pages = Finish { urls: vec![NVIDIA_DRIVERS_URL], program: None };
        assert!(finish_script(&pages).contains(NVIDIA_DRIVERS_URL));
        let tool = Finish { urls: vec![], program: Some(PathBuf::from(r"C:\d\NVCleanstall.exe")) };
        let script = finish_script(&tool);
        assert!(script.contains("NVCleanstall.exe") && !script.contains(NVIDIA_DRIVERS_URL));
    }
}
