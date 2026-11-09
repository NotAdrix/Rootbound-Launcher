use crate::terminal;
use crate::types::BackendEvent;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

pub fn check_for_updates(tx: Sender<BackendEvent>) {
    let tx_clone = tx.clone();
    std::thread::spawn(move || {
        let _ = tx_clone.send(BackendEvent::UpdateStatus(
            "Buscando actualizaciones...".into(),
        ));
        let url = "https://api.github.com/repos/TU_USUARIO/Rootbound-Launcher/releases/latest";
        let client = reqwest::blocking::Client::builder()
            .user_agent("Rootbound-Launcher")
            .build()
            .unwrap();

        match client.get(url).send() {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<serde_json::Value>() {
                        if let Some(tag) = json["tag_name"].as_str() {
                            let current = env!("CARGO_PKG_VERSION");
                            let clean_tag = tag.trim_start_matches('v');
                            if clean_tag != current {
                                let _ = tx_clone.send(BackendEvent::UpdateStatus(format!(
                                    "¡Nueva versión disponible: {}!",
                                    tag
                                )));
                            } else {
                                let _ = tx_clone.send(BackendEvent::UpdateStatus(
                                    "Estás en la última versión.".into(),
                                ));
                            }
                        }
                    }
                } else {
                    let _ = tx_clone.send(BackendEvent::UpdateStatus(
                        "No se encontraron releases.".into(),
                    ));
                }
            }
            Err(_) => {
                let _ = tx_clone.send(BackendEvent::UpdateStatus(
                    "Error de red al buscar actualizaciones.".into(),
                ));
            }
        }
    });
}

pub fn factory_reset(
    runner: &str,
    delete_games: bool,
    tx: Sender<BackendEvent>,
    is_resetting: Arc<AtomicBool>,
) {
    is_resetting.store(true, Ordering::SeqCst);
    let _ = tx.send(BackendEvent::Status(
        "Iniciando restablecimiento de fábrica...".into(),
    ));
    let _ = terminal::new_command("taskkill")
        .args(&["/F", "/IM", "legendary.exe"])
        .output();

    if delete_games {
        if let Ok(out) = terminal::new_command(runner)
            .args(&["list-installed", "--json"])
            .output()
        {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                if let Some(arr) = json.as_array() {
                    for item in arr {
                        if let Some(a) = item["app_name"].as_str() {
                            let _ = tx.send(BackendEvent::Log(format!(
                                "Borrando metadata del juego: {}",
                                a
                            )));
                            let _ = terminal::new_command(runner)
                                .args(&["uninstall", a, "-y", "--keep-files"])
                                .output();
                        }
                    }
                }
            }
        }
    }

    if let Some(mut path) = dirs::home_dir() {
        path.push(".config");
        path.push("legendary");
        let _ = std::fs::remove_dir_all(path);
    }
    if let Some(mut path) = dirs::config_dir() {
        path.push("rootbound_launcher");
        let _ = std::fs::remove_dir_all(path);
    }
    let mut temp_dir = std::env::temp_dir();
    temp_dir.push("rootbound_launcher_profile");
    let _ = std::fs::remove_dir_all(temp_dir);

    if let Some(python) = crate::inicio::find_python() {
        let _ = tx.send(BackendEvent::Log("Desinstalando legendary-gl...".into()));
        let _ = terminal::new_command(&python)
            .args(&["-m", "pip", "uninstall", "-y", "legendary-gl"])
            .output();
    }

    std::thread::sleep(Duration::from_secs(2));
    let _ = tx.send(BackendEvent::ResetComplete);
    is_resetting.store(false, Ordering::SeqCst);
}
