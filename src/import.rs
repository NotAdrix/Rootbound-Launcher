//! Detecta juegos de Epic ya presentes en disco para importarlos en legendary.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use sysinfo::Disks;

const MAX_SCAN_DEPTH: usize = 7;

#[derive(Deserialize)]
struct LauncherInstalled {
    #[serde(rename = "InstallationList")]
    installation_list: Vec<LauncherEntry>,
}

#[derive(Deserialize)]
struct LauncherEntry {
    #[serde(rename = "AppName")]
    app_name: String,
    #[serde(rename = "InstallLocation")]
    install_location: String,
}

#[derive(Deserialize)]
struct Mancpn {
    #[serde(rename = "AppName")]
    app_name: String,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

fn from_epic_launcher(found: &mut BTreeMap<String, String>) {
    let Ok(program_data) = std::env::var("ProgramData") else {
        return;
    };
    let dat = Path::new(&program_data).join(r"Epic\UnrealEngineLauncher\LauncherInstalled.dat");
    let Some(installed) = read_json::<LauncherInstalled>(&dat) else {
        return;
    };
    for entry in installed.installation_list {
        found.entry(entry.app_name).or_insert(entry.install_location);
    }
}

fn is_system_dir(name: &str) -> bool {
    name.starts_with('$')
        || ["windows", "system volume information", "programdata"]
            .iter()
            .any(|skip| name.eq_ignore_ascii_case(skip))
}

/// Lee el `.mancpn` de una carpeta `.egstore`, que marca la raíz de un juego de Epic.
fn app_name_in_egstore(egstore: &Path) -> Option<String> {
    let manifest = std::fs::read_dir(egstore)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "mancpn"))?;
    read_json::<Mancpn>(&manifest).map(|m| m.app_name)
}

fn scan(dir: &Path, depth: usize, found: &mut BTreeMap<String, String>) {
    let egstore = dir.join(".egstore");
    if egstore.is_dir() {
        if let Some(app_name) = app_name_in_egstore(&egstore) {
            found
                .entry(app_name)
                .or_insert_with(|| dir.to_string_lossy().into_owned());
        }
        return; // Dentro de un juego no hay más juegos: así no se recorre entero.
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let is_real_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        if is_real_dir && !is_system_dir(&entry.file_name().to_string_lossy()) {
            scan(&entry.path(), depth - 1, found);
        }
    }
}

/// `(app_name, carpeta)` de cada juego encontrado que siga existiendo en disco.
pub fn find_epic_games() -> Vec<(String, String)> {
    let mut found = BTreeMap::new();
    from_epic_launcher(&mut found);
    for disk in Disks::new_with_refreshed_list().list() {
        if !disk.is_removable() {
            scan(disk.mount_point(), MAX_SCAN_DEPTH, &mut found);
        }
    }
    found
        .into_iter()
        .filter(|(_, path)| Path::new(path).exists())
        .collect()
}
