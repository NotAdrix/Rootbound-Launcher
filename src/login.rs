//! Login de Epic en un navegador Chromium con perfil temporal: se lee el
//! `authorizationCode` de la página final y se le entrega a `legendary auth --code`.

use crate::legendary::{hidden_command, Legendary};
use crate::types::{Events, LogMsg, LoginFailure};
use anyhow::Result;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use std::process::Child;
use std::thread::sleep;
use std::time::Duration;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::Message;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

const LOGIN_URL: &str = "https://legendary.gl/epiclogin";
/// Página de Epic que muestra el JSON con el código tras iniciar sesión.
const CODE_PAGE: &str = "https://www.epicgames.com/id/api/redirect";
const EXTRACT_CODE_JS: &str =
    r#"var m = document.body.innerText.match(/"authorizationCode"\s*:\s*"([^"]+)"/); m ? m[1] : "";"#;
const BROWSERS: [&str; 4] = ["chrome.exe", "brave.exe", "thorium.exe", "msedge.exe"];

/// Debe ser un puerto explícito. Con `0` Chrome se declara automatizado
/// (`navigator.webdriver`) y Epic repite su verificación sin fin.
const DEBUG_PORT: u16 = 9222;
const CHROME_INSTALLER_URL: &str = "https://dl.google.com/chrome/install/latest/chrome_installer.exe";
const CHROME_UNINSTALL_KEYS: [&str; 2] = [
    r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Google Chrome",
    r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Google Chrome",
];
const LOGIN_ATTEMPTS: u32 = 150;
const POLL_INTERVAL: Duration = Duration::from_secs(2);

fn registered_browser() -> Option<String> {
    let hives = [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE];
    BROWSERS.iter().find_map(|exe| {
        let key = format!(r"Software\Microsoft\Windows\CurrentVersion\App Paths\{exe}");
        hives.iter().find_map(|hive| {
            RegKey::predef(*hive)
                .open_subkey(&key)
                .and_then(|k| k.get_value::<String, _>(""))
                .ok()
        })
    })
}

/// Cierra el navegador al terminar. El perfil solo se borra tras un login correcto,
/// para que las verificaciones superadas sigan valiendo en el siguiente intento.
struct Browser {
    child: Child,
    profile: PathBuf,
    login_completed: bool,
}

impl Browser {
    fn open(path: &str) -> std::io::Result<Self> {
        let profile = std::env::temp_dir().join("rootbound_launcher_profile");
        std::fs::create_dir_all(&profile)?;
        let child = hidden_command(path)
            .arg(format!("--remote-debugging-port={DEBUG_PORT}"))
            .arg(format!("--user-data-dir={}", profile.display()))
            .args(["--no-first-run", LOGIN_URL])
            .spawn()?;
        Ok(Self {
            child,
            profile,
            login_completed: false,
        })
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if self.login_completed {
            let _ = std::fs::remove_dir_all(&self.profile);
        }
    }
}

#[derive(Deserialize)]
struct Tab {
    url: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    ws_url: Option<String>,
}

fn evaluate(ws_url: &str, expression: &str) -> Result<Option<String>> {
    let (mut socket, _) = tungstenite::connect(ws_url)?;
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    }
    let request = json!({
        "id": 1,
        "method": "Runtime.evaluate",
        "params": { "expression": expression },
    });
    socket.send(Message::Text(request.to_string()))?;
    loop {
        let Message::Text(reply) = socket.read()? else {
            continue;
        };
        let reply: serde_json::Value = serde_json::from_str(&reply)?;
        if reply["id"] == 1 {
            let value = reply["result"]["result"]["value"].as_str();
            return Ok(value.filter(|v| !v.is_empty()).map(str::to_owned));
        }
    }
}

fn read_code(client: &reqwest::blocking::Client) -> Option<String> {
    let tabs: Vec<Tab> = client
        .get(format!("http://127.0.0.1:{DEBUG_PORT}/json"))
        .send()
        .ok()?
        .json()
        .ok()?;
    tabs.iter()
        .filter(|tab| is_code_page(&tab.url))
        .filter_map(|tab| tab.ws_url.as_deref())
        .find_map(|ws_url| evaluate(ws_url, EXTRACT_CODE_JS).ok().flatten())
}

/// La URL del login lleva la página final dentro de `redirectUrl`, así que hay que
/// comparar la ruta real: las páginas de login y verificación no se deben tocar.
fn is_code_page(url: &str) -> bool {
    url.split(['?', '#']).next() == Some(CODE_PAGE)
}

pub struct LoginBrowser {
    pub path: String,
    pub installed_by_launcher: bool,
}

/// Solo cuentan los navegadores Chromium: Internet Explorer no puede hacer este login.
pub fn find_or_install_browser(events: &Events) -> std::result::Result<LoginBrowser, LoginFailure> {
    if let Some(path) = registered_browser() {
        return Ok(LoginBrowser { path, installed_by_launcher: false });
    }
    events.log(LogMsg::InstallingBrowser);
    let installer = std::env::temp_dir().join("rootbound_chrome_installer.exe");
    crate::updater::download(CHROME_INSTALLER_URL, &installer).map_err(|_| LoginFailure::NoBrowser)?;
    let status = hidden_command(&installer).args(["/silent", "/install"]).status();
    let _ = std::fs::remove_file(&installer);
    status.map_err(|_| LoginFailure::NoBrowser)?;
    for _ in 0..60 {
        if let Some(path) = registered_browser() {
            return Ok(LoginBrowser { path, installed_by_launcher: true });
        }
        sleep(POLL_INTERVAL);
    }
    Err(LoginFailure::NoBrowser)
}

fn parse_uninstall_command(command: &str) -> Option<(String, Vec<String>)> {
    let command = command.trim();
    let (exe, rest) = match command.strip_prefix('"') {
        Some(quoted) => quoted.split_once('"')?,
        None => command.split_once(' ').unwrap_or((command, "")),
    };
    Some((exe.to_string(), rest.split_whitespace().map(str::to_owned).collect()))
}

pub fn uninstall_chrome() {
    let command = [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER].iter().find_map(|hive| {
        CHROME_UNINSTALL_KEYS.iter().find_map(|key| {
            RegKey::predef(*hive)
                .open_subkey(key)
                .and_then(|k| k.get_value::<String, _>("UninstallString"))
                .ok()
        })
    });
    if let Some((exe, args)) = command.as_deref().and_then(parse_uninstall_command) {
        let _ = hidden_command(exe).args(args).arg("--force-uninstall").status();
    }
}

pub fn wait_for_code(
    cli: &Legendary,
    events: &Events,
    browser_path: &str,
) -> std::result::Result<String, LoginFailure> {
    let mut browser = Browser::open(browser_path).map_err(|_| LoginFailure::CouldNotOpenBrowser)?;
    let client = reqwest::blocking::Client::new();
    events.log(LogMsg::WaitingLogin);
    for _ in 0..LOGIN_ATTEMPTS {
        if cli.is_shutting_down() {
            return Err(LoginFailure::Cancelled);
        }
        if let Some(code) = read_code(&client) {
            browser.login_completed = true;
            return Ok(code);
        }
        sleep(POLL_INTERVAL);
    }
    Err(LoginFailure::TimedOut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_chrome_uninstall_command() {
        let (exe, args) = parse_uninstall_command(
            r#""C:\Program Files\Google\Chrome\Application\1.0\Installer\setup.exe" --uninstall --system-level"#,
        )
        .unwrap();
        assert_eq!(exe, r"C:\Program Files\Google\Chrome\Application\1.0\Installer\setup.exe");
        assert_eq!(args, ["--uninstall", "--system-level"]);
    }

    #[test]
    fn only_the_final_page_counts_as_the_code_page() {
        assert!(is_code_page(
            "https://www.epicgames.com/id/api/redirect?clientId=abc&responseType=code"
        ));
        assert!(!is_code_page(
            "https://www.epicgames.com/id/login?redirectUrl=https%3A//www.epicgames.com/id/api/redirect%3FclientId%3Dabc"
        ));
        assert!(!is_code_page("https://accounts.google.com/signin?continue=/id/api/redirect"));
    }
}
