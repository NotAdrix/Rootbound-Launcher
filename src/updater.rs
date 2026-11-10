//! Autoactualización del launcher desde las releases de GitHub.

use crate::legendary::hidden_command;
use crate::types::{BackendEvent, Events, UpdateStatus};
use anyhow::Result;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::os::windows::process::CommandExt;
use std::process::Command;

const RELEASES_URL: &str =
    "https://api.github.com/repos/NotAdrix/Rootbound-Launcher/releases/latest";

const LEGENDARY_RELEASES_URL: &str =
    "https://api.github.com/repos/legendary-gl/legendary/releases/latest";

const WEB_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn client() -> reqwest::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent("Rootbound-Launcher")
        // Una descarga grande puede superar el límite de 30 s por defecto.
        .timeout(None)
        .build()
}

/// Para webs de descarga que rechazan clientes que no parecen un navegador.
fn web_client() -> reqwest::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(WEB_USER_AGENT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

pub fn fetch_web_page(url: &str) -> Result<String> {
    Ok(web_client()?.get(url).send()?.error_for_status()?.text()?)
}

/// Envía un formulario y devuelve la respuesta junto con la redirección, si la hay.
pub fn post_web_form(url: &str, body: String) -> Result<(String, Option<String>)> {
    let response = web_client()?
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()?;
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    Ok((response.text()?, location))
}

pub fn fetch_text(url: &str) -> Result<String> {
    Ok(client()?.get(url).send()?.error_for_status()?.text()?)
}

/// Descarga a un fichero `.part` y solo lo renombra si ha llegado completo.
pub fn download(url: &str, destination: &Path) -> Result<()> {
    let mut response = client()?.get(url).send()?.error_for_status()?;
    let partial = destination.with_extension("part");
    let mut file = std::fs::File::create(&partial)?;
    std::io::copy(&mut response, &mut file)?;
    drop(file);
    std::fs::rename(&partial, destination)?;
    Ok(())
}

fn version_numbers(version: &str) -> Vec<u32> {
    version
        .trim_start_matches('v')
        .split('.')
        .map_while(|part| part.parse().ok())
        .collect()
}

fn is_newer(candidate: &str, current: &str) -> bool {
    version_numbers(candidate) > version_numbers(current)
}

/// Última versión publicada de legendary, si es más nueva que la indicada.
pub fn newer_legendary_release(current: &str) -> bool {
    let response = client().and_then(|c| c.get(LEGENDARY_RELEASES_URL).send());
    let Ok(release) = response.and_then(|r| r.json::<Release>()) else {
        return false;
    };
    is_newer(&release.tag_name, current)
}

fn latest_status() -> UpdateStatus {
    let response = match client().and_then(|c| c.get(RELEASES_URL).send()) {
        Ok(response) if response.status().is_success() => response,
        Ok(_) => return UpdateStatus::NoReleases,
        Err(_) => return UpdateStatus::NetworkError,
    };
    let Ok(release) = response.json::<Release>() else {
        return UpdateStatus::NoReleases;
    };
    if !is_newer(&release.tag_name, env!("CARGO_PKG_VERSION")) {
        return UpdateStatus::UpToDate;
    }
    match release.assets.into_iter().find(|a| a.name.ends_with(".exe")) {
        Some(asset) => UpdateStatus::Available {
            version: release.tag_name.trim_start_matches('v').to_string(),
            url: asset.browser_download_url,
        },
        None => UpdateStatus::NoExeAttached,
    }
}

pub fn check(events: &Events) {
    events.send(BackendEvent::Update(UpdateStatus::Checking));
    events.send(BackendEvent::Update(latest_status()));
}

fn previous_version_path(exe: &Path) -> PathBuf {
    exe.with_extension("old.exe")
}

pub fn remove_previous_version() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(previous_version_path(&exe));
    }
}

/// Windows permite renombrar un .exe en ejecución, pero no sobrescribirlo.
fn replace_running_exe(url: &str, events: &Events) -> Result<()> {
    let exe = std::env::current_exe()?;
    let new_exe = exe.with_extension("new.exe");
    download(url, &new_exe)?;

    events.send(BackendEvent::Update(UpdateStatus::Applying));
    crate::service::stop();
    let old_exe = previous_version_path(&exe);
    let _ = std::fs::remove_file(&old_exe);
    std::fs::rename(&exe, &old_exe)?;
    if let Err(e) = std::fs::rename(&new_exe, &exe) {
        let _ = std::fs::rename(&old_exe, &exe);
        return Err(e.into());
    }
    // RootboundBackground es una copia del mismo ejecutable.
    let _ = std::fs::copy(&exe, crate::paths::background_exe());
    Command::new(&exe).spawn()?;
    Ok(())
}

/// Un .exe en ejecución no puede borrarse a sí mismo: lo hace `cmd` cuando ya se ha cerrado.
fn schedule_deletion(path: &Path, delay_seconds: u32) -> std::io::Result<()> {
    hidden_command("cmd")
        .raw_arg(format!(
            "/C ping -n {} 127.0.0.1 >nul & del /f /q \"{}\"",
            delay_seconds + 1,
            path.display()
        ))
        .spawn()
        .map(drop)
}

pub fn delete_file_after_exit(path: &Path) {
    let _ = schedule_deletion(path, 8);
}

pub fn delete_own_exe_after_exit() {
    if let Ok(exe) = std::env::current_exe() {
        delete_file_after_exit(&exe);
    }
}

pub fn perform(url: &str, events: &Events) {
    events.send(BackendEvent::Update(UpdateStatus::Downloading));
    match replace_running_exe(url, events) {
        Ok(()) => std::process::exit(0),
        Err(_) => events.send(BackendEvent::Update(UpdateStatus::Failed)),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_newer, schedule_deletion};

    #[test]
    fn scheduled_deletion_handles_paths_with_spaces() {
        let folder = std::env::temp_dir().join(format!("rootbound test {}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("app (copia).exe");
        std::fs::write(&file, "x").unwrap();
        schedule_deletion(&file, 1).unwrap();
        for _ in 0..40 {
            if !file.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        let deleted = !file.exists();
        std::fs::remove_dir_all(folder).unwrap();
        assert!(deleted);
    }

    #[test]
    fn only_higher_versions_count_as_updates() {
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("v0.0.9", "0.1.0"));
    }
}
