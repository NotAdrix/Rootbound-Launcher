//! Único módulo que conoce la CLI de legendary (https://github.com/legendary-gl/legendary).

use crate::juegos::logica::{self, epic_art, DlcEntry, GameInfo, GameSource, LanguageOption, Profile};
use crate::process::KillOnExitJob;
use crate::types::{BackendEvent, Events, LogMsg, Progress};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{BufReader, Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const CREATE_NO_WINDOW: u32 = 0x08000000;
const DOWNLOAD_URL: &str = if cfg!(target_arch = "aarch64") {
    "https://github.com/legendary-gl/legendary/releases/latest/download/legendary_windows_arm64.exe"
} else {
    "https://github.com/legendary-gl/legendary/releases/latest/download/legendary_windows_x64.exe"
};

/// Ejecuta `program` sin abrir una consola.
pub fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command.stdin(Stdio::null());
    command
}

/// Carpeta donde el launcher guarda su propia copia de legendary.
fn ensure_installed(events: &Events) -> Result<PathBuf> {
    let dir = crate::paths::root();
    let exe = dir.join("legendary.exe");
    let _ = std::fs::remove_file(exe.with_extension("old.exe"));
    if !exe.exists() {
        events.log(LogMsg::DownloadingLegendary);
        std::fs::create_dir_all(&dir)?;
        crate::updater::download(DOWNLOAD_URL, &exe).context("No se pudo descargar Legendary.")?;
    }
    Ok(exe)
}

pub enum ImportOutcome {
    Finished,
    /// La carpeta existe pero no contiene ningún archivo del juego.
    NoGameFiles,
}

#[derive(Deserialize)]
struct KeyImage {
    #[serde(rename = "type")]
    kind: String,
    url: String,
}

#[derive(Deserialize)]
struct Metadata {
    #[serde(default, rename = "keyImages")]
    key_images: Vec<KeyImage>,
}

#[derive(Deserialize)]
struct Dlc {
    app_name: String,
    app_title: String,
}

#[derive(Deserialize)]
struct ManifestDetails {
    install_tags: Vec<String>,
}

#[derive(Deserialize)]
struct GameDetails {
    manifest: ManifestDetails,
}

#[derive(Deserialize)]
struct ListedGame {
    app_name: String,
    app_title: String,
    metadata: Option<Metadata>,
    #[serde(default)]
    dlcs: Vec<Dlc>,
}

#[derive(Deserialize)]
pub struct InstalledGame {
    pub app_name: String,
    pub title: String,
    pub install_path: PathBuf,
    #[serde(default)]
    is_dlc: bool,
    #[serde(default)]
    pub install_tags: Vec<String>,
}

#[derive(Deserialize)]
struct Status {
    account: String,
}

#[derive(Deserialize)]
struct BearerToken {
    access_token: String,
    account_id: String,
}

#[derive(Deserialize)]
struct AccountInfo {
    email: Option<String>,
}

/// Proceso de legendary en curso que el usuario puede pausar o cancelar.
struct Tracked {
    pid: u32,
    args: Vec<String>,
}

#[derive(Clone)]
pub struct Legendary {
    exe: PathBuf,
    tracked: Arc<Mutex<Option<Tracked>>>,
    shutting_down: Arc<AtomicBool>,
    offline: Arc<AtomicBool>,
    job: Arc<Option<KillOnExitJob>>,
    /// Juegos con actualización pendiente, para no consultarlo antes de cada lanzamiento.
    pending_updates: Arc<Mutex<Option<Vec<String>>>>,
}

impl Legendary {
    pub fn locate(events: &Events) -> Result<Self> {
        Ok(Self {
            exe: ensure_installed(events)?,
            tracked: Arc::default(),
            shutting_down: Arc::default(),
            offline: Arc::new(AtomicBool::new(crate::config::load().offline_mode)),
            job: Arc::new(KillOnExitJob::new()),
            pending_updates: Arc::default(),
        })
    }

    fn command(&self) -> Command {
        let mut command = hidden_command(&self.exe);
        command.env("LEGENDARY_CONFIG_PATH", crate::paths::legendary_data());
        command
    }

    fn json<T: serde::de::DeserializeOwned>(&self, args: &[&str]) -> Option<T> {
        let out = self.command().args(args).output().ok()?;
        serde_json::from_slice(&out.stdout).ok()
    }

    /// Lee directamente el registro de legendary: tarda milisegundos, y lanzar
    /// `list-installed` casi un segundo. Solo se lee; lo sigue gestionando legendary.
    pub fn installed(&self) -> Vec<InstalledGame> {
        let Some(path) = data_dir().map(|dir| dir.join("installed.json")) else {
            return Vec::new();
        };
        if !path.exists() {
            return Vec::new();
        }
        // Legendary reescribe el archivo entero: se puede pillar a medias.
        for _ in 0..5 {
            if let Some(games) = read_installed_file(&path) {
                return games;
            }
            thread::sleep(Duration::from_millis(50));
        }
        Vec::new()
    }

    pub fn install_path(&self, app_name: &str) -> Option<PathBuf> {
        self.installed()
            .into_iter()
            .find(|game| game.app_name == app_name)
            .map(|game| game.install_path)
    }

    /// Une lo que hay en la cuenta con lo instalado, ordenado por título.
    pub fn library(&self) -> Vec<GameInfo> {
        // Sin conexión solo se conoce lo instalado: `list` necesita iniciar sesión.
        let listed = if self.is_offline() {
            Vec::new()
        } else {
            self.json(&["list", "--json"]).unwrap_or_default()
        };
        self.merge_library(listed)
    }

    /// Carga la biblioteca comprobando de paso la sesión, que `list` ya valida.
    /// Devuelve `None` si hay que iniciar sesión. Un fallo de red no cuenta.
    pub fn library_checking_session(&self) -> Option<Vec<GameInfo>> {
        data_dir().filter(|dir| dir.join("user.json").exists())?;
        let Ok(out) = self.command().args(["list", "--json"]).output() else {
            return Some(self.merge_library(Vec::new()));
        };
        if let Ok(listed) = serde_json::from_slice(&out.stdout) {
            return Some(self.merge_library(listed));
        }
        let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
        let session_rejected = ["invalidcredentials", "login failed", "log in failed"]
            .iter()
            .any(|error| stderr.contains(error));
        (!session_rejected).then(|| self.merge_library(Vec::new()))
    }

    fn merge_library(&self, listed: Vec<ListedGame>) -> Vec<GameInfo> {
        let mut games: BTreeMap<String, GameInfo> = listed
            .into_iter()
            .map(|game| {
                let images: Vec<(String, String)> = game
                    .metadata
                    .into_iter()
                    .flat_map(|metadata| metadata.key_images)
                    .map(|image| (image.kind, image.url))
                    .collect();
                let info = GameInfo {
                    title: game.app_title,
                    app_name: game.app_name.clone(),
                    is_installed: false,
                    source: GameSource::Epic,
                    art: epic_art(&images),
                };
                (game.app_name, info)
            })
            .collect();
        for game in self.installed().into_iter().filter(|game| !game.is_dlc) {
            let app_name = game.app_name.clone();
            games
                .entry(game.app_name.clone())
                .or_insert(GameInfo {
                    title: game.title,
                    app_name: game.app_name,
                    is_installed: true,
                    source: GameSource::Epic,
                    art: logica::cached_art(&app_name),
                })
                .is_installed = true;
        }
        let mut games: Vec<_> = games.into_values().collect();
        games.sort_by(|a, b| a.title.cmp(&b.title));
        games
    }

    pub fn import_game(&self, app_name: &str, path: &str, events: &Events) -> ImportOutcome {
        let Ok(out) = self.command().args(["import", app_name, path, "-y"]).output() else {
            events.log(LogMsg::CommandCouldNotStart);
            return ImportOutcome::Finished;
        };
        let stderr = String::from_utf8_lossy(&out.stderr);
        if stderr.contains("No files belonging to") {
            return ImportOutcome::NoGameFiles;
        }
        stderr.lines().for_each(|line| report_line(line, events));
        ImportOutcome::Finished
    }

    pub fn is_offline(&self) -> bool {
        self.offline.load(Ordering::SeqCst)
    }

    pub fn set_offline(&self, offline: bool) {
        self.offline.store(offline, Ordering::SeqCst);
    }

    /// Juegos instalados con una versión más nueva disponible.
    pub fn games_with_updates(&self) -> Vec<String> {
        let Ok(out) = self.command().args(["list-installed", "--check-updates", "--tsv"]).output() else {
            return Vec::new();
        };
        logica::parse_pending_updates(&String::from_utf8_lossy(&out.stdout))
    }

    /// Hay que comprobarlo antes de `update`: aunque la versión sea la misma, ese comando
    /// descarga los archivos que falten, y Fortnite no guarda en disco todo su contenido.
    pub fn has_update(&self, app_name: &str) -> bool {
        !self.is_offline() && self.pending_updates().iter().any(|game| game == app_name)
    }

    /// Resultado de la última consulta; solo pregunta a Epic si aún no hay ninguno.
    pub fn pending_updates(&self) -> Vec<String> {
        if let Some(pending) = crate::process::lock(&self.pending_updates).clone() {
            return pending;
        }
        self.refresh_pending_updates()
    }

    pub fn refresh_pending_updates(&self) -> Vec<String> {
        let pending = self.games_with_updates();
        *crate::process::lock(&self.pending_updates) = Some(pending.clone());
        pending
    }

    pub fn forget_update(&self, app_name: &str) {
        if let Some(pending) = crate::process::lock(&self.pending_updates).as_mut() {
            pending.retain(|game| game != app_name);
        }
    }

    pub fn version(&self) -> Option<String> {
        let out = self.command().arg("--version").output().ok()?;
        between(&String::from_utf8_lossy(&out.stdout), "version \"", "\"").map(str::to_owned)
    }

    /// Windows permite renombrar el .exe aunque haya un juego lanzado con él.
    pub fn replace_with_latest(&self) -> Result<()> {
        let new_exe = self.exe.with_extension("new.exe");
        crate::updater::download(DOWNLOAD_URL, &new_exe)?;
        let old_exe = self.exe.with_extension("old.exe");
        let _ = std::fs::remove_file(&old_exe);
        std::fs::rename(&self.exe, &old_exe)?;
        if let Err(e) = std::fs::rename(&new_exe, &self.exe) {
            let _ = std::fs::rename(&old_exe, &self.exe);
            return Err(e.into());
        }
        let _ = std::fs::remove_file(old_exe);
        Ok(())
    }

    /// El nombre sale de los datos locales; el correo requiere consultar a Epic.
    pub fn profile(&self) -> Profile {
        let status: Option<Status> = self.json(&["status", "--offline", "--json"]);
        Profile {
            name: status.map(|status| status.account),
            email: if self.is_offline() { None } else { self.account_email() },
        }
    }

    fn account_email(&self) -> Option<String> {
        let token: BearerToken = self.json(&["get-token", "--bearer", "--json"])?;
        let url = format!(
            "https://account-public-service-prod03.ol.epicgames.com/account/api/public/account/{}",
            token.account_id
        );
        let info: AccountInfo = reqwest::blocking::Client::new()
            .get(url)
            .bearer_auth(token.access_token)
            .send()
            .ok()?
            .error_for_status()
            .ok()?
            .json()
            .ok()?;
        info.email
    }

    pub fn dlcs(&self, app_name: &str) -> Vec<DlcEntry> {
        let listed: Vec<ListedGame> = self.json(&["list", "--json"]).unwrap_or_default();
        let installed = self.installed();
        listed
            .into_iter()
            .find(|game| game.app_name == app_name)
            .map_or_else(Vec::new, |game| game.dlcs)
            .into_iter()
            .map(|dlc| DlcEntry {
                installed: installed.iter().any(|game| game.app_name == dlc.app_name),
                app_name: dlc.app_name,
                title: dlc.app_title,
            })
            .collect()
    }

    /// Idiomas que el propio juego separa con etiquetas en su manifiesto.
    pub fn language_options(&self, app_name: &str) -> Vec<LanguageOption> {
        let details: Option<GameDetails> = self.json(&["info", app_name, "--offline", "--json"]);
        let installed_tags = self.installed_tags(app_name);
        details.map_or_else(Vec::new, |details| {
            logica::language_options(&details.manifest.install_tags, &installed_tags)
        })
    }

    /// Mantiene lo que el juego tiene instalado y evita la pregunta interactiva de
    /// descargas selectivas, que sin consola termina en un error.
    pub fn selection_args(&self, app_name: &str) -> Vec<String> {
        logica::selection_args(&self.installed_tags(app_name))
    }

    pub fn installed_tags(&self, app_name: &str) -> Vec<String> {
        self.installed()
            .into_iter()
            .find(|game| game.app_name == app_name)
            .map_or_else(Vec::new, |game| game.install_tags)
    }

    pub fn authenticate(&self, code: &str) -> bool {
        self.command()
            .args(["auth", "--code", code, "-y"])
            .output()
            .is_ok_and(|out| out.status.success())
    }

    /// Ejecuta legendary volcando su salida al log. Devuelve `false` solo si se detuvo
    /// con `stop`.
    pub fn run(&self, args: &[String], events: &Events) -> bool {
        self.run_streamed(args, events, true)
    }

    /// Como `run`, pero `stop` no lo afecta: cancelar una descarga no debe cerrar un juego.
    pub fn run_untracked(&self, args: &[String], events: &Events) {
        self.run_streamed(args, events, false);
    }

    /// Ejecuta el comando elevándolo si `target` está en una carpeta protegida (p. ej.
    /// `C:\Program Files`). Si no, se ejecuta normal, con pausa y progreso en directo.
    pub fn run_in(&self, args: &[String], target: Option<&Path>, events: &Events) -> bool {
        if target.is_some_and(crate::process::needs_elevation) {
            self.run_elevated(args, events)
        } else {
            self.run(args, events)
        }
    }

    /// Relanza legendary con permisos de administrador. Una sola petición de UAC por
    /// operación; su salida se sigue mostrando en el terminal.
    fn run_elevated(&self, args: &[String], events: &Events) -> bool {
        let work = crate::paths::config_dir();
        let _ = std::fs::create_dir_all(&work);
        let log = work.join("elevated.log");
        let script = work.join("elevated.cmd");
        let _ = std::fs::remove_file(&log);

        let arguments = args.iter().map(|arg| quote_arg(arg)).collect::<Vec<_>>().join(" ");
        let content = format!(
            "@echo off\r\nchcp 65001 >nul\r\n\
             set \"LEGENDARY_CONFIG_PATH={cfg}\"\r\n\
             set \"PYTHONUNBUFFERED=1\"\r\nset \"PYTHONIOENCODING=utf-8\"\r\n\
             {exe} {arguments} > {log} 2>&1\r\n",
            cfg = crate::paths::legendary_data().display(),
            exe = quote_arg(&self.exe.display().to_string()),
            log = quote_arg(&log.display().to_string()),
        );
        if std::fs::write(&script, content).is_err() {
            events.log(LogMsg::CommandCouldNotStart);
            return true;
        }

        events.log(LogMsg::ElevatingGame);
        let done = Arc::new(AtomicBool::new(false));
        let tail = {
            let (done, events, log) = (done.clone(), events.clone(), log.clone());
            thread::spawn(move || tail_log(&log, &done, &events))
        };
        let params = format!("/c {}", quote_arg(&script.display().to_string()));
        let exit = crate::process::shell_execute_wait(Path::new("cmd.exe"), &params);
        done.store(true, Ordering::SeqCst);
        let _ = tail.join();
        let _ = std::fs::remove_file(&script);
        let _ = std::fs::remove_file(&log);

        match exit {
            Some(0) => {}
            Some(_) => events.log(LogMsg::CommandFailed),
            None => events.log(LogMsg::ElevationDenied),
        }
        true
    }

    fn run_streamed(&self, args: &[String], events: &Events, track: bool) -> bool {
        let spawned = self
            .command()
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(_) => {
                events.log(LogMsg::CommandCouldNotStart);
                return true;
            }
        };
        if track {
            if let Some(job) = self.job.as_ref() {
                job.assign(&child);
            }
            *crate::process::lock(&self.tracked) = Some(Tracked {
                pid: child.id(),
                args: args.to_vec(),
            });
        }

        let stdout = child.stdout.take().map(|s| forward_output(s, events.clone()));
        let stderr = child.stderr.take().map(|s| forward_output(s, events.clone()));
        for reader in [stdout, stderr].into_iter().flatten() {
            let _ = reader.join();
        }
        let succeeded = child.wait().is_ok_and(|status| status.success());

        // `stop` vacía la ranura: si ya no está, nos han detenido.
        let completed = !track || crate::process::lock(&self.tracked).take().is_some();
        if completed && !succeeded {
            events.log(LogMsg::CommandFailed);
        }
        completed
    }

    pub fn is_busy(&self) -> bool {
        crate::process::lock(&self.tracked).is_some()
    }

    /// Mata el proceso en curso con todo su árbol y devuelve sus argumentos para
    /// poder relanzarlo: legendary retoma las descargas desde su fichero de resume.
    pub fn stop(&self) -> Option<Vec<String>> {
        let tracked = crate::process::lock(&self.tracked).take()?;
        crate::process::kill_tree(tracked.pid);
        Some(tracked.args)
    }

    pub fn begin_shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        self.stop();
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }
}

/// Misma ubicación que usa legendary para sus datos.
/// Los datos de legendary también viven en `C:\Rootbound`.
fn data_dir() -> Option<PathBuf> {
    Some(crate::paths::legendary_data())
}

fn read_installed_file(path: &Path) -> Option<Vec<InstalledGame>> {
    let text = std::fs::read_to_string(path).ok()?;
    let games: BTreeMap<String, InstalledGame> = serde_json::from_str(&text).ok()?;
    Some(games.into_values().collect())
}

fn report_line(line: &str, events: &Events) {
    if let Some(msg) = translate_line(line) {
        if let LogMsg::LegendaryProblem { text, .. } = &msg {
            append_to_detail_log(text);
        }
        events.log(msg);
    }
}

fn consume_line(line: &str, progress: &mut Progress, events: &Events) {
    if progress.apply(line) {
        events.send(BackendEvent::Progress(progress.clone()));
    } else {
        report_line(line, events);
    }
}

fn forward_output(pipe: impl Read + Send + 'static, events: Events) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut progress = Progress::default();
        for_each_line(pipe, |line| consume_line(line, &mut progress, &events));
    })
}

/// Un proceso elevado no comparte tuberías con este: su salida se vuelca a un archivo
/// y aquí se va leyendo mientras crece, hasta que `done` indica que terminó.
fn tail_log(path: &Path, done: &AtomicBool, events: &Events) {
    use std::io::{Seek, SeekFrom};
    let mut progress = Progress::default();
    let mut position = 0u64;
    let mut pending: Vec<u8> = Vec::new();
    loop {
        let finished = done.load(Ordering::SeqCst);
        if let Ok(mut file) = std::fs::File::open(path) {
            if file.seek(SeekFrom::Start(position)).is_ok() {
                let mut buf = Vec::new();
                if let Ok(read) = file.read_to_end(&mut buf) {
                    position += read as u64;
                    for byte in buf {
                        if byte == b'\n' || byte == b'\r' {
                            consume_line(&String::from_utf8_lossy(&pending), &mut progress, events);
                            pending.clear();
                        } else {
                            pending.push(byte);
                        }
                    }
                }
            }
        }
        if finished {
            if !pending.is_empty() {
                consume_line(&String::from_utf8_lossy(&pending), &mut progress, events);
            }
            return;
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn quote_arg(arg: &str) -> String {
    format!("\"{}\"", arg.replace('"', "\"\""))
}

/// Parte por `\n` y también por `\r`, que es como `verify` reescribe su progreso.
fn for_each_line(pipe: impl Read, mut on_line: impl FnMut(&str)) {
    let mut line = Vec::new();
    for byte in BufReader::new(pipe).bytes().map_while(Result::ok) {
        if byte == b'\n' || byte == b'\r' {
            on_line(&String::from_utf8_lossy(&line));
            line.clear();
        } else {
            line.push(byte);
        }
    }
    if !line.is_empty() {
        on_line(&String::from_utf8_lossy(&line));
    }
}

/// Separa `[módulo] NIVEL: texto`. Lo demás (avisos sueltos, trazas) no es un mensaje.
fn log_level_and_text(line: &str) -> Option<(&str, &str)> {
    let (level, text) = line.strip_prefix('[')?.split_once("] ")?.1.split_once(": ")?;
    ["INFO", "WARNING", "ERROR", "CRITICAL"]
        .contains(&level)
        .then_some((level, text.trim()))
}

/// Legendary solo habla inglés: su texto original se guarda aquí para diagnosticar.
fn append_to_detail_log(text: &str) {
    let Some(dir) = crate::config::dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("legendary.log"))
    {
        let _ = writeln!(file, "{text}");
    }
}

fn translate_install_failure(reason: &str) -> LogMsg {
    let space = reason
        .strip_prefix("Not enough available disk space! ")
        .and_then(|sizes| sizes.split_once(" < "));
    match space {
        Some((free, required)) => LogMsg::NotEnoughDiskSpace {
            free: free.to_string(),
            required: required.to_string(),
        },
        None => LogMsg::LegendaryProblem { is_error: true, text: reason.to_string() },
    }
}

/// Convierte la salida de legendary, siempre en inglés, en mensajes traducibles.
/// El resto de la información interna no se muestra.
fn translate_line(line: &str) -> Option<LogMsg> {
    if let Some(reason) = line.trim_start().strip_prefix("! Failure:") {
        return Some(translate_install_failure(reason.trim()));
    }
    let (level, text) = log_level_and_text(line)?;
    // La carpeta sobrante la borra el launcher después de desinstalar.
    if text.starts_with("Removing game directory failed") {
        return None;
    }
    if text.starts_with("Some files are missing from the game installation") {
        return Some(LogMsg::FilesMissingWarning);
    }
    if text.starts_with("Installation cannot proceed") {
        return Some(LogMsg::InstallBlocked);
    }
    if text.contains("Failed to acquire installed data lock") {
        return Some(LogMsg::InstallLocked);
    }
    let per_file_error = ["File is missing", "File does not match hash", "Other failure"];
    if per_file_error.iter().any(|prefix| text.starts_with(prefix)) {
        return None;
    }
    if text.starts_with("Preparing download for") {
        return Some(LogMsg::PreparingDownload { title: between(text, "\"", "\"")?.to_string() });
    }
    if text.starts_with("Verifying \"") {
        return Some(LogMsg::VerifyingGame { title: between(text, "\"", "\"")?.to_string() });
    }
    if let Some(title) = text.strip_prefix("Launching ") {
        return Some(LogMsg::LaunchingGame { title: title.trim_end_matches('.').to_string() });
    }
    if text.starts_with("Download size: ") {
        return Some(LogMsg::DownloadSize { size: between(text, ": ", " (")?.to_string() });
    }
    if text.starts_with("Download size is 0") {
        return Some(LogMsg::AlreadyUpToDate);
    }
    if text.starts_with("Finished installation process") {
        return Some(LogMsg::InstallFinished);
    }
    if text.starts_with("Game has been uninstalled") {
        return Some(LogMsg::Uninstalled);
    }
    if text.starts_with("Verification finished successfully") {
        return Some(LogMsg::VerificationOk);
    }
    if text.starts_with("Verification failed") {
        return Some(LogMsg::VerificationFailed);
    }
    if text.starts_with("Login failed") || text.starts_with("Log in failed") {
        return Some(LogMsg::SessionRejected);
    }
    match level {
        "INFO" => None,
        _ => Some(LogMsg::LegendaryProblem {
            is_error: level != "WARNING",
            text: text.to_string(),
        }),
    }
}

fn between<'a>(line: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let rest = line.split_once(start)?.1;
    Some(rest.split_once(end).map_or(rest, |(value, _)| value).trim())
}

impl Progress {
    /// Actualiza el estado si `line` es de progreso. El formato no está documentado:
    /// sale de `downloader/mp/manager.py` y de `verify_game` en `cli.py`.
    fn apply(&mut self, line: &str) -> bool {
        if let Some(percent) = between(line, "= Progress: ", " ") {
            self.percent = percent.to_string();
            self.eta = between(line, "ETA: ", " ").unwrap_or_default().to_string();
            self.verifying = false;
        } else if let Some(speed) = between(line, "+ Download\t- ", " (raw)") {
            self.download = speed.to_string();
        } else if let Some(speed) = between(line, "+ Disk\t- ", " (write)") {
            self.disk = speed.to_string();
        } else if line.starts_with("Verification progress:") {
            self.percent = between(line, "(", ")").unwrap_or_default().to_string();
            self.disk = between(line, "[", "]").unwrap_or_default().to_string();
            self.download.clear();
            self.eta.clear();
            self.verifying = true;
        } else {
            return line.contains(" - Downloaded: ") || line.contains(" - Cache usage: ");
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_download_progress() {
        let mut p = Progress::default();
        assert!(p.apply("[DLManager] INFO: = Progress: 12.34% (5/40), Running for 00:00:10, ETA: 00:10:00"));
        assert!(p.apply("[DLManager] INFO:  - Downloaded: 1.00 MiB, Written: 1.00 MiB"));
        assert!(p.apply("[DLManager] INFO:  + Download\t- 7.50 MiB/s (raw) / 9.00 MiB/s (decompressed)"));
        assert!(p.apply("[DLManager] INFO:  + Disk\t- 3.25 MiB/s (write) / 1.00 MiB/s (read)"));
        assert_eq!(p.percent, "12.34%");
        assert_eq!(p.eta, "00:10:00");
        assert_eq!(p.download, "7.50 MiB/s");
        assert_eq!(p.disk, "3.25 MiB/s");
        assert!(!p.verifying);
    }

    #[test]
    fn parses_verification_progress() {
        let mut p = Progress::default();
        assert!(p.apply("Verification progress: 12/34 (35.3%) [8.1 MiB/s]\t"));
        assert_eq!(p.percent, "35.3%");
        assert_eq!(p.disk, "8.1 MiB/s");
        assert!(p.verifying);
    }

    #[test]
    fn leaves_other_lines_in_the_log() {
        assert!(!Progress::default().apply("[cli] INFO: Launching Fortnite..."));
    }

    #[test]
    fn translates_known_legendary_messages() {
        assert_eq!(
            translate_line("[cli] INFO: Launching Fortnite..."),
            Some(LogMsg::LaunchingGame { title: "Fortnite".to_string() })
        );
        assert_eq!(
            translate_line("[cli] CRITICAL: Failed to acquire installed data lock, only one instance"),
            Some(LogMsg::InstallLocked)
        );
    }

    #[test]
    fn explains_why_an_install_cannot_continue() {
        assert_eq!(
            translate_line(" ! Failure: Not enough available disk space! 77.00 GiB < 113.00 GiB"),
            Some(LogMsg::NotEnoughDiskSpace {
                free: "77.00 GiB".to_string(),
                required: "113.00 GiB".to_string(),
            })
        );
    }

    #[test]
    fn hides_internal_chatter_and_keeps_unknown_problems() {
        assert_eq!(translate_line("[Core] INFO: Trying to re-use existing login session..."), None);
        assert_eq!(translate_line("Traceback (most recent call last):"), None);
        assert_eq!(translate_line("[PYI-8336:ERROR] Failed to execute script"), None);
        assert_eq!(translate_line("[cli] ERROR: File is missing: \"a.pak\""), None);
        assert_eq!(
            translate_line("[cli] WARNING: Something odd"),
            Some(LogMsg::LegendaryProblem { is_error: false, text: "Something odd".to_string() })
        );
    }

    #[test]
    fn reads_the_installed_games_registry() {
        let path = std::env::temp_dir().join(format!("rootbound_installed_{}.json", std::process::id()));
        let json = r#"{"Fortnite":{"app_name":"Fortnite","title":"Fortnite","install_path":"G:\\Fortnite","install_tags":[],"is_dlc":false,"version":"1"}}"#;
        std::fs::write(&path, json).unwrap();
        let games = read_installed_file(&path).unwrap();
        std::fs::write(&path, "{ a medias").unwrap();
        let corrupted = read_installed_file(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(games[0].app_name, "Fortnite");
        assert_eq!(games[0].install_path, PathBuf::from(r"G:\Fortnite"));
        assert!(corrupted.is_none());
    }

    #[test]
    fn splits_on_carriage_returns() {
        let mut lines = Vec::new();
        for_each_line(&b"a\rb\nc"[..], |l| lines.push(l.to_string()));
        assert_eq!(lines, ["a", "b", "c"]);
    }
}
