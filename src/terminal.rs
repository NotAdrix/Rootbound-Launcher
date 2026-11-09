use crate::types::BackendEvent;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn new_command(cmd: &str) -> Command {
    let mut c = Command::new(cmd);
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

pub fn stream_command(cmd: &str, args: Vec<String>, tx: Sender<BackendEvent>) {
    let cmd_owned = cmd.to_string();
    let tx_clone = tx.clone();
    std::thread::spawn(move || {
        let mut child = match new_command(&cmd_owned)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx_clone.send(BackendEvent::Log(format!("Error: {}", e)));
                return;
            }
        };
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let tx_out = tx_clone.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx_out.send(BackendEvent::Log(line));
            }
        });
        let tx_err = tx_clone.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx_err.send(BackendEvent::Log(line));
            }
        });
        let _ = child.wait();
    });
}

pub fn blocking_stream_command(cmd: &str, args: Vec<String>, tx: Sender<BackendEvent>) {
    let cmd_owned = cmd.to_string();
    let mut child = match new_command(&cmd_owned)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(BackendEvent::Log(format!("Error: {}", e)));
            return;
        }
    };
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let tx_out = tx.clone();
    let h1 = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx_out.send(BackendEvent::Log(line));
        }
    });
    let tx_err = tx.clone();
    let h2 = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx_err.send(BackendEvent::Log(line));
        }
    });
    let _ = h1.join();
    let _ = h2.join();
    let _ = child.wait();
}
