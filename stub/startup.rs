//! Entrada de "Aplicaciones de inicio" de Windows. Es un ejecutable aparte porque
//! Windows no arranca al iniciar sesión un programa que pide permisos de administrador:
//! este solo pide a la tarea programada que abra el launcher, sin aviso de UAC.

#![windows_subsystem = "windows"]

use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn main() {
    let _ = Command::new("schtasks")
        .args(["/Run", "/TN", "RootboundLauncher"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}
