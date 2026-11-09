use crate::terminal;
use crate::types::BackendEvent;
use std::sync::mpsc::Sender;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

pub fn launch_game(runner: &str, game: String, tx: Sender<BackendEvent>) {
    let tx_clone = tx.clone();
    let runner_clone = runner.to_string();
    let g_clone = game.clone();

    // Aseguramos que no haya procesos colgados antes de abrir
    kill_game(&runner_clone, &g_clone);

    terminal::stream_command(
        runner,
        vec![
            "update".into(),
            g_clone.clone(),
            "--skip-dlcs".into(),
            "-y".into(),
        ],
        tx_clone,
    );
    terminal::stream_command(
        &runner_clone,
        vec!["launch".into(), g_clone, "-y".into()],
        tx.clone(),
    );

    tx.send(BackendEvent::GameStarted(game)).unwrap();
}

pub fn kill_game(runner: &str, game: &str) {
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );

    // 1. Obtener la ruta de instalación del juego
    let mut install_path = None;
    if let Ok(out) = terminal::new_command(runner)
        .args(&["list-installed", "--json"])
        .output()
    {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
            if let Some(arr) = json.as_array() {
                for item in arr {
                    if item["app_name"].as_str() == Some(game) {
                        install_path = item["install_path"].as_str().map(|s| s.to_lowercase());
                        break;
                    }
                }
            }
        }
    }

    // 2. Bucle de eliminación forzada (hasta 3 segundos)
    for _ in 0..30 {
        sys.refresh_processes();
        let mut found_any = false;

        for (pid, process) in sys.processes() {
            let mut should_kill = false;

            // Buscar por ruta de instalación
            if let Some(ref path) = install_path {
                if let Some(exe) = process.exe() {
                    let exe_str = exe.to_string_lossy().to_lowercase();
                    if exe_str.contains(path) && !exe_str.contains("launcher") {
                        should_kill = true;
                    }
                }
            }

            // Fallback: buscar por nombre (por si falla la ruta)
            if !should_kill {
                let p_name = process.name().to_lowercase();
                let g_name = game.to_lowercase();
                // Caso especial para Fortnite que usa "fortniteclient"
                if p_name.contains(&g_name) || (g_name == "fortnite" && p_name.contains("fortnite"))
                {
                    should_kill = true;
                }
            }

            if should_kill {
                // Intentar matar el proceso nativamente
                let _ = process.kill();
                // Y usar taskkill con /T para eliminar el árbol de subprocesos
                let _ = terminal::new_command("taskkill")
                    .args(&["/F", "/T", "/PID", &pid.to_string()])
                    .output();
                found_any = true;
            }
        }

        // Si no encontramos nada, verificamos si de verdad ya no queda nada vivo
        if !found_any {
            std::thread::sleep(Duration::from_millis(100));
            sys.refresh_processes();
            let mut still_any = false;
            for (_, process) in sys.processes() {
                if let Some(ref path) = install_path {
                    if let Some(exe) = process.exe() {
                        let exe_str = exe.to_string_lossy().to_lowercase();
                        if exe_str.contains(path) && !exe_str.contains("launcher") {
                            still_any = true;
                            break;
                        }
                    }
                }
                let p_name = process.name().to_lowercase();
                let g_name = game.to_lowercase();
                if p_name.contains(&g_name) || (g_name == "fortnite" && p_name.contains("fortnite"))
                {
                    still_any = true;
                    break;
                }
            }
            // Si de verdad ya no hay nada, terminamos el bucle
            if !still_any {
                break;
            }
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn uninstall_game(runner: &str, game: String, tx: Sender<BackendEvent>) {
    let runner_clone = runner.to_string();
    let game_clone = game.clone();
    let tx_clone = tx.clone();

    std::thread::spawn(move || {
        // Primero cerramos el juego por si acaso
        kill_game(&runner_clone, &game_clone);

        let args = vec!["uninstall".to_string(), game_clone, "-y".to_string()];
        terminal::blocking_stream_command(&runner_clone, args, tx_clone.clone());
        let _ = tx_clone.send(BackendEvent::LibraryLoaded(crate::inicio::fetch_library(
            &runner_clone,
        )));
    });
}
