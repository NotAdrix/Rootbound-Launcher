use crate::terminal;
use crate::types::BackendEvent;
use std::sync::mpsc::Sender;

pub fn install_game(
    runner: &str,
    app_name: String,
    path: String,
    dlcs: bool,
    lang: String,
    tx: Sender<BackendEvent>,
) {
    let mut args = vec![
        "install".to_string(),
        app_name,
        "--base-path".into(),
        path,
        "--language".into(),
        lang,
        "-y".into(),
    ];
    if !dlcs {
        args.push("--skip-dlcs".into());
    }

    let tx_clone = tx.clone();
    let runner_clone = runner.to_string();
    std::thread::spawn(move || {
        terminal::blocking_stream_command(&runner_clone, args, tx_clone.clone());
        let _ = tx_clone.send(BackendEvent::LibraryLoaded(crate::inicio::fetch_library(
            &runner_clone,
        )));
    });
}
