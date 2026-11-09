use crate::terminal;
use crate::types::{BackendEvent, GameInfo};
use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use winreg::enums::*;
use winreg::RegKey;

pub fn find_python() -> Option<String> {
    if let Ok(out) = terminal::new_command("py")
        .args(&["-c", "print('ok')"])
        .output()
    {
        if String::from_utf8_lossy(&out.stdout).trim() == "ok" {
            return Some("py".to_string());
        }
    }
    if let Ok(out) = terminal::new_command("python")
        .args(&["-c", "print('ok')"])
        .output()
    {
        if String::from_utf8_lossy(&out.stdout).trim() == "ok" {
            return Some("python".to_string());
        }
    }
    let local_appdata = dirs::data_local_dir()?;
    for v in &["314", "313", "312", "311", "310"] {
        let path = local_appdata.join(format!("Programs\\Python\\Python{}\\python.exe", v));
        if path.exists() {
            if let Ok(out) = terminal::new_command(&path.to_string_lossy())
                .args(&["-c", "print('ok')"])
                .output()
            {
                if String::from_utf8_lossy(&out.stdout).trim() == "ok" {
                    return Some(path.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

pub fn get_legendary(tx: &Sender<BackendEvent>) -> Result<String> {
    if let Ok(status) = terminal::new_command("legendary")
        .args(&["--version"])
        .status()
    {
        if status.success() {
            return Ok("legendary".to_string());
        }
    }

    let mut py = find_python();
    if py.is_none() {
        let _ = tx.send(BackendEvent::Log(
            "Python no está instalado. Instalando...".into(),
        ));
        let _ = terminal::new_command("winget")
            .args(&[
                "install",
                "-e",
                "--id",
                "Python.Python.3.13",
                "--silent",
                "--accept-package-agreements",
                "--accept-source-agreements",
            ])
            .status();
        py = find_python();
        if py.is_none() {
            let _ = tx.send(BackendEvent::Log(
                "Descargando instalador de Python (Plan B)...".into(),
            ));
            let ps_script = r#"
            $url = "https://www.python.org/ftp/python/3.13.0/python-3.13.0-amd64.exe"
            $out = "$env:TEMP\python_installer.exe"
            Invoke-WebRequest -Uri $url -OutFile $out
            Start-Process -FilePath $out -ArgumentList "/quiet InstallAllUsers=0 PrependPath=1 Include_test=0" -Wait
            "#;
            let _ = terminal::new_command("powershell")
                .args(&["-NoProfile", "-Command", ps_script])
                .status();
            py = find_python();
        }
    }

    if let Some(python_path) = py {
        if let Ok(out) = terminal::new_command(&python_path)
            .args(&[
                "-c",
                "import sysconfig;print(sysconfig.get_path('scripts','nt_user'))",
            ])
            .output()
        {
            let user_scripts = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !user_scripts.is_empty() {
                let lg_exe_user = Path::new(&user_scripts).join("legendary.exe");
                if lg_exe_user.exists() {
                    return Ok(lg_exe_user.to_string_lossy().to_string());
                }
            }
        }
        let parent = Path::new(&python_path).parent().unwrap();
        let lg_exe_base = parent.join("Scripts\\legendary.exe");
        if lg_exe_base.exists() {
            return Ok(lg_exe_base.to_string_lossy().to_string());
        }

        let _ = tx.send(BackendEvent::Log(
            "Instalando legendary-gl con pip...".into(),
        ));
        let _ = terminal::new_command("taskkill")
            .args(&["/F", "/IM", "legendary.exe"])
            .output();
        std::thread::sleep(Duration::from_secs(1));

        let _ = terminal::new_command(&python_path)
            .args(&[
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                "legendary-gl",
            ])
            .output();

        if let Ok(status) = terminal::new_command("legendary")
            .args(&["--version"])
            .status()
        {
            if status.success() {
                return Ok("legendary".to_string());
            }
        }
        if lg_exe_base.exists() {
            return Ok(lg_exe_base.to_string_lossy().to_string());
        }
        if let Ok(out) = terminal::new_command(&python_path)
            .args(&[
                "-c",
                "import sysconfig;print(sysconfig.get_path('scripts','nt_user'))",
            ])
            .output()
        {
            let user_scripts = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !user_scripts.is_empty() {
                let lg_exe_user = Path::new(&user_scripts).join("legendary.exe");
                if lg_exe_user.exists() {
                    return Ok(lg_exe_user.to_string_lossy().to_string());
                }
            }
        }
    }
    bail!("No se pudo instalar ni encontrar Legendary.")
}

fn get_chromium_browser_path() -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey(
        r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\http\UserChoice",
    ) {
        if let Ok(prog_id) = key.get_value::<String, _>("ProgId") {
            let exe_name = if prog_id.contains("Brave") {
                "brave.exe"
            } else if prog_id.contains("Chrome") {
                "chrome.exe"
            } else if prog_id.contains("MSEdge") || prog_id.contains("Edge") {
                "msedge.exe"
            } else if prog_id.contains("Thorium") {
                "thorium.exe"
            } else {
                ""
            };
            if !exe_name.is_empty() {
                let app_paths = format!(
                    r"Software\Microsoft\Windows\CurrentVersion\App Paths\{}",
                    exe_name
                );
                if let Ok(k) = hkcu.open_subkey(&app_paths) {
                    if let Ok(path) = k.get_value::<String, _>("") {
                        return Some(path);
                    }
                }
                let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
                if let Ok(k) = hklm.open_subkey(&app_paths) {
                    if let Ok(path) = k.get_value::<String, _>("") {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

pub async fn read_browser_json(
    tx: &Sender<BackendEvent>,
    is_resetting: Arc<AtomicBool>,
) -> Result<String> {
    tx.send(BackendEvent::Status("Abriendo navegador...".into()))
        .unwrap();
    let browser_path =
        get_chromium_browser_path().context("No se encontró navegador compatible")?;

    let mut temp_dir = std::env::temp_dir();
    temp_dir.push("rootbound_launcher_profile");
    let _ = std::fs::create_dir_all(&temp_dir);
    let user_data_arg = format!("--user-data-dir={}", temp_dir.to_string_lossy());

    let mut child = terminal::new_command(&browser_path)
        .args(&[
            "--remote-debugging-port=9222",
            "--remote-allow-origins=*",
            &user_data_arg,
            "--no-first-run",
            "https://legendary.gl/epiclogin",
        ])
        .spawn()?;

    let client = reqwest::Client::new();
    let mut ready = false;
    for _ in 0..40 {
        if is_resetting.load(Ordering::SeqCst) {
            child.kill().ok();
            let _ = std::fs::remove_dir_all(&temp_dir);
            bail!("Reinicio canceló el login.");
        }
        if let Ok(resp) = client.get("http://127.0.0.1:9222/json").send().await {
            if resp.status().is_success() {
                ready = true;
                break;
            }
        }
        sleep(Duration::from_millis(500)).await;
    }
    if !ready {
        child.kill().ok();
        bail!("No se pudo conectar al puerto de depuración del navegador.");
    }

    tx.send(BackendEvent::Status("Esperando login...".into()))
        .unwrap();

    for _ in 0..150 {
        if is_resetting.load(Ordering::SeqCst) {
            child.kill().ok();
            let _ = std::fs::remove_dir_all(&temp_dir);
            bail!("Reinicio canceló el login.");
        }
        sleep(Duration::from_secs(2)).await;
        if let Ok(resp) = client.get("http://127.0.0.1:9222/json").send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(tabs) = json.as_array() {
                    for tab in tabs {
                        let url = tab["url"].as_str().unwrap_or("");
                        if url.starts_with("http") {
                            if let Some(ws_url) = tab["webSocketDebuggerUrl"].as_str() {
                                if let Ok((mut ws_stream, _)) =
                                    tokio_tungstenite::connect_async(ws_url).await
                                {
                                    let js_code = r#"var m = document.body.innerText.match(/"authorizationCode"\s*:\s*"([^"]+)"/); m ? m[1] : "";"#;
                                    let evaluate_cmd = json!({ "id": 1, "method": "Runtime.evaluate", "params": { "expression": js_code } });
                                    let _ = ws_stream
                                        .send(tokio_tungstenite::tungstenite::Message::Text(
                                            evaluate_cmd.to_string(),
                                        ))
                                        .await;

                                    let mut found_code = None;
                                    let timeout =
                                        tokio::time::timeout(Duration::from_secs(1), async {
                                            while let Some(Ok(
                                                tokio_tungstenite::tungstenite::Message::Text(msg),
                                            )) = ws_stream.next().await
                                            {
                                                if let Ok(parsed) =
                                                    serde_json::from_str::<serde_json::Value>(&msg)
                                                {
                                                    if parsed["id"] == 1 {
                                                        if let Some(text) = parsed["result"]
                                                            ["result"]["value"]
                                                            .as_str()
                                                        {
                                                            if !text.is_empty() {
                                                                found_code = Some(text.to_string());
                                                            }
                                                        }
                                                        break;
                                                    }
                                                }
                                            }
                                        })
                                        .await;

                                    if let Ok(_) = timeout {
                                        if let Some(code) = found_code {
                                            child.kill().ok();
                                            let _ = std::fs::remove_dir_all(&temp_dir);
                                            return Ok(code);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    child.kill().ok();
    bail!("Tiempo agotado esperando el login.")
}

pub fn auto_import_epic_games(runner: &str, tx: &Sender<BackendEvent>) -> bool {
    let mut installed_app_names = std::collections::HashSet::new();
    if let Ok(out) = terminal::new_command(runner)
        .args(&["list-installed", "--json"])
        .output()
    {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
            if let Some(arr) = json.as_array() {
                for item in arr {
                    if let Some(a) = item["app_name"].as_str() {
                        installed_app_names.insert(a.to_string());
                    }
                }
            }
        }
    }

    let mut imported_something = false;

    let mut dat_paths = Vec::new();
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:\\", letter as char);
        if Path::new(&drive).exists() {
            dat_paths.push(format!(
                "{}ProgramData\\Epic\\UnrealEngineLauncher\\LauncherInstalled.dat",
                drive
            ));
        }
    }

    for dat_path in dat_paths {
        if let Ok(data) = std::fs::read(&dat_path) {
            let json_str = String::from_utf8_lossy(&data);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_str) {
                if let Some(list) = json["InstallationList"].as_array() {
                    for item in list {
                        if let (Some(app_name), Some(install_location)) =
                            (item["AppName"].as_str(), item["InstallLocation"].as_str())
                        {
                            if !installed_app_names.contains(app_name)
                                && Path::new(install_location).exists()
                            {
                                let _ = tx.send(BackendEvent::Log(format!(
                                    "[ACCION] Importando {} desde {}",
                                    app_name, install_location
                                )));
                                let _ = terminal::new_command(runner)
                                    .args(&["import", app_name, install_location])
                                    .output();
                                imported_something = true;
                            }
                        }
                    }
                }
            }
        }
    }

    let mut base_paths = Vec::new();
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:\\", letter as char);
        if Path::new(&drive).exists() {
            base_paths.push(format!("{}Program Files\\Epic Games", drive));
            base_paths.push(format!("{}Program Files (x86)\\Epic Games", drive));
            base_paths.push(format!("{}Epic Games", drive));
        }
    }

    for base_path in base_paths {
        let path = Path::new(&base_path);
        if path.exists() {
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                        let folder_name = entry.file_name().to_string_lossy().to_string();
                        let blacklist = [
                            "directx",
                            "gameinput",
                            "launcher",
                            "epic online services",
                            "unrealengine",
                            "ue4",
                            "ue5",
                        ];
                        if folder_name.starts_with('.')
                            || blacklist
                                .iter()
                                .any(|&b| folder_name.to_lowercase().contains(b))
                        {
                            continue;
                        }
                        if installed_app_names.contains(&folder_name) {
                            continue;
                        }

                        let clean_folder = folder_name
                            .trim_end_matches(|c: char| {
                                c.is_ascii_digit()
                                    || c.is_ascii_uppercase() && folder_name.len() > 5
                            })
                            .to_lowercase();
                        let mut already_installed = false;
                        for installed in &installed_app_names {
                            if installed.to_lowercase().contains(&clean_folder)
                                || installed.to_lowercase() == clean_folder
                            {
                                already_installed = true;
                                break;
                            }
                        }
                        if already_installed {
                            continue;
                        }

                        let game_path = entry.path().to_string_lossy().to_string();
                        let _ = tx.send(BackendEvent::Log(format!(
                            "[ACCION] Importando {} desde {}",
                            folder_name, game_path
                        )));
                        let out = terminal::new_command(runner)
                            .args(&["import", &folder_name, &game_path])
                            .output();
                        if let Ok(out) = out {
                            if out.status.success() {
                                imported_something = true;
                            }
                        }
                    }
                }
            }
        }
    }
    imported_something
}

pub fn fetch_library(runner: &str) -> Vec<GameInfo> {
    let mut games = Vec::new();
    let mut available = std::collections::HashMap::new();

    if let Ok(out) = terminal::new_command(runner)
        .args(&["list", "--json"])
        .output()
    {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
            if let Some(arr) = json.as_array() {
                for item in arr {
                    if let (Some(t), Some(a)) =
                        (item["app_title"].as_str(), item["app_name"].as_str())
                    {
                        available.insert(a.to_string(), t.to_string());
                    }
                }
            }
        }
    }

    let mut installed = std::collections::HashSet::new();
    if let Ok(out) = terminal::new_command(runner)
        .args(&["list-installed", "--json"])
        .output()
    {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
            if let Some(arr) = json.as_array() {
                for item in arr {
                    if let Some(a) = item["app_name"].as_str() {
                        installed.insert(a.to_string());
                    }
                }
            }
        }
    }

    for (app_name, title) in available {
        games.push(GameInfo {
            app_name: app_name.clone(),
            title,
            is_installed: installed.contains(&app_name),
        });
    }
    games.sort_by(|a, b| a.title.cmp(&b.title));
    games
}
