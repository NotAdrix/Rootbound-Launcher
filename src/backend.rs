use crate::ajustes;
use crate::inicio;
use crate::juegos;
use crate::libreria;
use crate::terminal;
use crate::types::{BackendEvent, UiCommand};
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub auto_launch_game: Option<String>,
    pub language: crate::types::Language,
    pub dark_mode: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auto_launch_game: None,
            language: crate::types::Language::Es,
            dark_mode: true,
        }
    }
}

pub fn load_config() -> Config {
    if let Some(mut path) = dirs::config_dir() {
        path.push("rootbound_launcher");
        path.push("config.json");
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(cfg) = serde_json::from_str(&data) {
                return cfg;
            }
        }
    }
    Config::default()
}

pub fn save_config(cfg: &Config) {
    if let Some(mut path) = dirs::config_dir() {
        path.push("rootbound_launcher");
        std::fs::create_dir_all(&path).ok();
        path.push("config.json");
        if let Ok(data) = serde_json::to_string_pretty(cfg) {
            let _ = std::fs::write(path, data);
        }
    }
}

pub fn start_backend(rx_cmd: Receiver<UiCommand>, tx_evt: Sender<BackendEvent>) {
    std::thread::spawn(move || {
        let mut system = System::new_with_specifics(
            RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
        );
        let mut running_game: Option<String> = None;
        let mut running_game_path: Option<String> = None;
        let mut launch_time: Option<std::time::Instant> = None;
        let is_resetting = Arc::new(AtomicBool::new(false));

        let runner = match inicio::get_legendary(&tx_evt) {
            Ok(r) => r,
            Err(e) => {
                let _ = tx_evt.send(BackendEvent::Error(format!("Error crítico: {}", e)));
                return;
            }
        };

        let tx_init = tx_evt.clone();
        let runner_init = runner.clone();
        let is_resetting_clone = is_resetting.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                if is_resetting_clone.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }

                tx_init
                    .send(BackendEvent::Status("Verificando sesión de Epic...".into()))
                    .unwrap();
                let is_logged_out = if let Ok(out) = terminal::new_command(&runner_init)
                    .args(&["status"])
                    .output()
                {
                    String::from_utf8_lossy(&out.stdout)
                        .to_lowercase()
                        .contains("not logged in")
                } else {
                    true
                };

                if is_logged_out {
                    tx_init
                        .send(BackendEvent::Status(
                            "No hay sesión. Abriendo navegador...".into(),
                        ))
                        .unwrap();
                    if let Ok(code) =
                        inicio::read_browser_json(&tx_init, is_resetting_clone.clone()).await
                    {
                        tx_init
                            .send(BackendEvent::Status(
                                "Código obtenido. Autenticando...".into(),
                            ))
                            .unwrap();
                        let runner_clone = runner_init.clone();
                        let tx_clone = tx_init.clone();
                        tokio::task::spawn_blocking(move || {
                            let _ = terminal::new_command(&runner_clone)
                                .args(&["auth", "--code", &code, "-y"])
                                .output();
                            let _ = tx_clone
                                .send(BackendEvent::Status("Autenticación finalizada.".into()));
                        })
                        .await
                        .unwrap();
                    }
                }
            });

            if is_resetting_clone.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }

            tx_init
                .send(BackendEvent::Status("Cargando biblioteca local...".into()))
                .unwrap();
            tx_init
                .send(BackendEvent::LibraryLoaded(inicio::fetch_library(
                    &runner_init,
                )))
                .unwrap();

            let tx_scan = tx_init.clone();
            let runner_scan = runner_init.clone();
            std::thread::spawn(move || {
                let _ = tx_scan.send(BackendEvent::Status("Buscando juegos nuevos...".into()));
                if inicio::auto_import_epic_games(&runner_scan, &tx_scan) {
                    let _ = tx_scan.send(BackendEvent::LibraryLoaded(inicio::fetch_library(
                        &runner_scan,
                    )));
                } else {
                    let _ = tx_scan.send(BackendEvent::Status("Biblioteca al día.".into()));
                }
            });
        });

        loop {
            if let Some(game) = running_game.clone() {
                if let Some(lt) = launch_time {
                    if lt.elapsed() > Duration::from_secs(15) {
                        system.refresh_processes();
                        let mut is_running = false;
                        if let Some(ref path) = running_game_path {
                            for (_, process) in system.processes() {
                                if let Some(exe) = process.exe() {
                                    if exe.to_string_lossy().contains(path) {
                                        is_running = true;
                                        break;
                                    }
                                }
                            }
                        } else {
                            let name_match = game.to_lowercase().replace(" ", "");
                            for (_, process) in system.processes() {
                                if process
                                    .name()
                                    .to_lowercase()
                                    .replace(" ", "")
                                    .contains(&name_match)
                                {
                                    is_running = true;
                                    break;
                                }
                            }
                        }
                        if !is_running {
                            running_game = None;
                            running_game_path = None;
                            launch_time = None;
                            tx_evt.send(BackendEvent::GameExited(game)).unwrap();
                        }
                    }
                }
            }

            if let Ok(cmd) = rx_cmd.try_recv() {
                match cmd {
                    UiCommand::LaunchGame(game) => {
                        if let Some(ref old) = running_game {
                            let runner_clone = runner.clone();
                            let old_clone = old.clone();
                            std::thread::spawn(move || {
                                juegos::kill_game(&runner_clone, &old_clone);
                            });
                        }
                        //...
                        running_game = Some(game.clone());
                        launch_time = Some(std::time::Instant::now());

                        running_game_path = None;
                        if let Ok(out) = terminal::new_command(&runner)
                            .args(&["list-installed", "--json"])
                            .output()
                        {
                            if let Ok(json) =
                                serde_json::from_slice::<serde_json::Value>(&out.stdout)
                            {
                                if let Some(arr) = json.as_array() {
                                    for item in arr {
                                        if item["app_name"].as_str() == Some(&game) {
                                            running_game_path = item["install_path"]
                                                .as_str()
                                                .map(|s| s.to_string());
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        juegos::launch_game(&runner, game, tx_evt.clone());
                    }
                    UiCommand::KillGame(game) => {
                        let runner_clone = runner.clone();
                        std::thread::spawn(move || {
                            juegos::kill_game(&runner_clone, &game);
                        });
                    }
                    UiCommand::UninstallGame(game) => {
                        juegos::uninstall_game(&runner, game, tx_evt.clone());
                    }
                    UiCommand::InstallGame {
                        app_name,
                        path,
                        dlcs,
                        lang,
                    } => {
                        libreria::install_game(&runner, app_name, path, dlcs, lang, tx_evt.clone());
                    }
                    UiCommand::FactoryReset(delete) => {
                        ajustes::factory_reset(
                            &runner,
                            delete,
                            tx_evt.clone(),
                            is_resetting.clone(),
                        );
                    }
                    UiCommand::CheckForUpdates => {
                        ajustes::check_for_updates(tx_evt.clone());
                    }
                    UiCommand::SetAutoLaunch(opt) => {
                        let mut cfg = load_config();
                        cfg.auto_launch_game = opt;
                        save_config(&cfg);
                    }
                    UiCommand::InstallDlc(app) => {
                        terminal::stream_command(
                            &runner,
                            vec!["install".into(), app, "--with-dlcs".into(), "-y".into()],
                            tx_evt.clone(),
                        );
                    }
                    UiCommand::VerifyGame(app) => {
                        terminal::stream_command(
                            &runner,
                            vec!["verify".into(), app, "-y".into()],
                            tx_evt.clone(),
                        );
                    }
                    UiCommand::VerifyUpdate(app) => {
                        terminal::stream_command(
                            &runner,
                            vec!["update".into(), app, "-y".into()],
                            tx_evt.clone(),
                        );
                    }
                    UiCommand::InstallLang(app, lang) => {
                        terminal::stream_command(
                            &runner,
                            vec![
                                "install".into(),
                                app,
                                "--language".into(),
                                lang,
                                "-y".into(),
                            ],
                            tx_evt.clone(),
                        );
                    }
                    UiCommand::PauseDownload => {
                        let _ = tx_evt.send(BackendEvent::Log("Pausando...".into()));
                        let _ = tx_evt.send(BackendEvent::DownloadPaused);
                    }
                    UiCommand::ResumeDownload => {
                        let _ = tx_evt.send(BackendEvent::Log("Reanudando...".into()));
                    }
                    UiCommand::CancelDownload => {
                        let _ = tx_evt.send(BackendEvent::Log("Cancelada.".into()));
                        let _ = terminal::new_command("taskkill")
                            .args(&["/F", "/IM", "legendary.exe"])
                            .output();
                        let _ = tx_evt.send(BackendEvent::DownloadFinished);
                    }
                    UiCommand::ReloadLibrary => {
                        let _ = tx_evt
                            .send(BackendEvent::LibraryLoaded(inicio::fetch_library(&runner)));
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}
