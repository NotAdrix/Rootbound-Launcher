//! Toda la lógica de la biblioteca: modelo de juegos, filtrado, juegos externos,
//! detección de Steam e imágenes. La presentación está en `interfaz.rs`.

use crate::frontend::LauncherApp;
use crate::config;
use crate::legendary::{self, hidden_command, Legendary};
use crate::process::{self, ProcessSnapshot};
use crate::types::{BackendEvent, Events, LogMsg, UiCommand};
use anyhow::{Context, Result};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

pub const AUTO_LANGUAGE: &str = "auto";
pub const LANGUAGES: [&str; 6] = [AUTO_LANGUAGE, "en", "es", "fr", "de", "it"];

/// Nombre de cada idioma escrito en ese mismo idioma.
pub fn language_name(code: &str) -> &'static str {
    match code {
        "en" => "English",
        "es" => "Español",
        "fr" => "Français",
        "de" => "Deutsch",
        "it" => "Italiano",
        _ => "?",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DlcEntry {
    pub app_name: String,
    pub title: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DlcAction {
    Install,
    Uninstall,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LanguageOption {
    pub name: String,
    pub locale: String,
    pub tag: String,
    pub installed: bool,
}

/// Idioma que se le puede pasar al juego con `legendary launch --language`.
#[derive(Debug, Clone, PartialEq)]
pub struct LaunchLanguage {
    pub locale: String,
    pub name: String,
}

pub struct LaunchLanguageDialog {
    pub app_name: String,
    pub title: String,
    pub languages: Option<Vec<LaunchLanguage>>,
    pub selected: String,
}

pub struct DlcDialog {
    pub app_name: String,
    pub title: String,
    pub action: DlcAction,
    /// `None` mientras legendary responde; el booleano es la casilla marcada.
    pub dlcs: Option<Vec<(DlcEntry, bool)>>,
}

pub struct LanguageDialog {
    pub app_name: String,
    pub title: String,
    pub options: Option<Vec<LanguageOption>>,
    pub selected: Option<String>,
}

/// Nombre y código `-epiclocale` del idioma de una etiqueta del manifiesto.
/// Las fuentes de la interfaz no incluyen caracteres CJK, por eso van en inglés.
fn language_of_tag(tag: &str) -> Option<(String, String)> {
    let code = if let Some(rest) = tag.strip_prefix("Lang") {
        if rest.is_empty() || rest.ends_with("Optional") {
            return None;
        }
        rest.to_lowercase()
    } else {
        tag.strip_prefix("voice_")?.split('_').next()?.to_lowercase()
    };
    let (name, locale) = match code.as_str() {
        "de" => ("Deutsch", "de"),
        "en" => ("English", "en"),
        "es" => ("Español", "es"),
        "eses" => ("Español (España)", "es-ES"),
        "es419" => ("Español (Latinoamérica)", "es-419"),
        "fr" => ("Français", "fr"),
        "it" => ("Italiano", "it"),
        "pl" => ("Polski", "pl"),
        "pt" => ("Português", "pt"),
        "ru" => ("Русский", "ru"),
        "ja" => ("Japanese", "ja"),
        "ko" => ("Korean", "ko"),
        "zh" | "zhcn" | "cn" => ("Chinese", "zh-CN"),
        _ => return Some((code.clone(), code)),
    };
    Some((name.to_string(), locale.to_string()))
}

/// Sin etiquetas instaladas, legendary descargó todos los archivos del juego.
pub fn language_options(manifest_tags: &[String], installed_tags: &[String]) -> Vec<LanguageOption> {
    let mut options: Vec<LanguageOption> = manifest_tags
        .iter()
        .filter_map(|tag| {
            let (name, locale) = language_of_tag(tag)?;
            Some(LanguageOption {
                name,
                locale,
                tag: tag.clone(),
                installed: installed_tags.is_empty() || installed_tags.contains(tag),
            })
        })
        .collect();
    options.sort_by_cached_key(|option| option.name.to_lowercase());
    options
}

/// Idiomas de lanzamiento: los que el juego declara y el inglés. Epic no publica
/// esa lista, así que sin etiquetas de idioma se ofrecen los habituales.
pub fn launch_languages(options: &[LanguageOption]) -> Vec<LaunchLanguage> {
    let mut languages: Vec<LaunchLanguage> = if options.is_empty() {
        LANGUAGES
            .iter()
            .filter(|code| **code != AUTO_LANGUAGE)
            .map(|code| LaunchLanguage {
                locale: code.to_string(),
                name: language_name(code).to_string(),
            })
            .collect()
    } else {
        std::iter::once(LaunchLanguage {
            locale: "en".to_string(),
            name: language_name("en").to_string(),
        })
        .chain(options.iter().map(|option| LaunchLanguage {
            locale: option.locale.clone(),
            name: option.name.clone(),
        }))
        .collect()
    };
    let mut seen = HashSet::new();
    languages.retain(|language| seen.insert(language.locale.clone()));
    languages.sort_by_cached_key(|language| language.name.to_lowercase());
    languages
}

/// Debe incluir todo lo ya instalado: legendary borra lo que quede sin etiquetar.
pub fn tags_after_adding(installed_tags: &[String], new_tag: &str) -> Vec<String> {
    let mut tags = installed_tags.to_vec();
    for required in ["", new_tag] {
        if !tags.iter().any(|tag| tag == required) {
            tags.push(required.to_string());
        }
    }
    tags
}

/// Argumentos para actualizar o reparar sin cambiar lo instalado. Sin etiquetas
/// guardadas solo se pide lo obligatorio: `--disable-sdl` descargaría todo el
/// contenido opcional, que en Fortnite son más de 100 GB.
pub fn selection_args(installed_tags: &[String]) -> Vec<String> {
    if installed_tags.is_empty() {
        return vec!["--skip-sdl".to_string()];
    }
    tags_after_adding(installed_tags, "")
        .into_iter()
        .flat_map(|tag| ["--install-tag".to_string(), tag])
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub name: Option<String>,
    pub email: Option<String>,
}

/// Extrae de `list-installed --check-updates --tsv` los juegos con actualización.
pub fn parse_pending_updates(tsv: &str) -> Vec<String> {
    tsv.lines()
        .skip(1)
        .filter_map(|line| {
            let mut columns = line.split('\t');
            let app_name = columns.next()?;
            (columns.nth(3)? == "True").then(|| app_name.to_string())
        })
        .collect()
}

/// Actualiza los juegos marcados, salvo los que están abiertos en ese momento.
pub fn auto_update(cli: &Legendary, events: &Events) {
    let config = config::load();
    if config.offline_mode || config.auto_update.is_empty() {
        return;
    }
    let pending = cli.games_with_updates();
    let mut snapshot = ProcessSnapshot::new();
    snapshot.refresh();
    for game in cli.installed() {
        let wanted = config.auto_update.contains(&game.app_name) && pending.contains(&game.app_name);
        if wanted && !snapshot.folder_is_running(&game.install_path) {
            events.log(LogMsg::AutoUpdating { title: game.title.clone() });
            let mut update: Vec<String> =
                ["update", &game.app_name, "--skip-dlcs", "-y"].iter().map(|arg| arg.to_string()).collect();
            update.extend(selection_args(&game.install_tags));
            cli.run(&update, events);
        }
    }
}

fn library_cache_path() -> Option<PathBuf> {
    Some(config::dir()?.join("library.json"))
}

/// Última biblioteca conocida, para mostrarla sin esperar a Epic.
pub fn load_cached_library() -> Vec<GameInfo> {
    library_cache_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn save_cached_library(games: &[GameInfo]) {
    let (Some(path), Ok(data)) = (library_cache_path(), serde_json::to_string(games)) else {
        return;
    };
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let _ = std::fs::write(path, data);
}

fn playtime_path() -> Option<PathBuf> {
    Some(config::dir()?.join("playtime.json"))
}

pub fn load_playtime() -> HashMap<String, u64> {
    playtime_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

fn save_playtime(playtime: &HashMap<String, u64>) {
    let (Some(path), Ok(data)) = (playtime_path(), serde_json::to_string(playtime)) else {
        return;
    };
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let _ = std::fs::write(path, data);
}

pub fn format_duration(seconds: u64) -> String {
    let minutes = seconds / 60;
    match (minutes / 60, minutes % 60) {
        (0, 0) => "< 1 min".to_string(),
        (0, rest) => format!("{rest} min"),
        (hours, rest) => format!("{hours} h {rest} min"),
    }
}

enum PlayTarget {
    Folder(PathBuf),
    Exe(PathBuf),
}

/// Suma tiempo a los juegos que tengan algún proceso abierto. No depende de cómo
/// se lanzaron ni de que la interfaz siga viva.
pub struct PlaytimeTracker {
    snapshot: ProcessSnapshot,
    targets: Vec<(String, PlayTarget)>,
    targets_loaded_at: Option<Instant>,
    last_tick: Instant,
    tick: Duration,
}

impl PlaytimeTracker {
    const TARGETS_LIFETIME: Duration = Duration::from_secs(5 * 60);

    pub fn new(tick: Duration) -> Self {
        Self {
            snapshot: ProcessSnapshot::new(),
            targets: Vec::new(),
            targets_loaded_at: None,
            last_tick: Instant::now(),
            tick,
        }
    }

    fn reload_targets(&mut self, cli: &Legendary) {
        let is_fresh = self.targets_loaded_at.is_some_and(|at| at.elapsed() < Self::TARGETS_LIFETIME);
        if is_fresh {
            return;
        }
        self.targets = cli
            .installed()
            .into_iter()
            .map(|game| (game.app_name, PlayTarget::Folder(game.install_path)))
            .chain(config::load().external_games.into_iter().map(|game| {
                let target = match game.kind {
                    ExternalKind::Executable { path } => PlayTarget::Exe(path),
                    ExternalKind::Steam { install_dir, .. } => PlayTarget::Folder(install_dir),
                };
                (game.id, target)
            }))
            .collect();
        self.targets_loaded_at = Some(Instant::now());
    }

    pub fn tick(&mut self, cli: &Legendary) {
        self.reload_targets(cli);
        self.snapshot.refresh();
        // Tras una suspensión del PC, el tiempo transcurrido no es tiempo de juego.
        let elapsed = self.last_tick.elapsed().min(self.tick * 2).as_secs();
        self.last_tick = Instant::now();

        let running: Vec<&String> = self
            .targets
            .iter()
            .filter(|(_, target)| match target {
                PlayTarget::Folder(root) => self.snapshot.folder_is_running(root),
                PlayTarget::Exe(exe) => self.snapshot.exe_is_running(exe),
            })
            .map(|(id, _)| id)
            .collect();
        if running.is_empty() {
            return;
        }
        let mut playtime = load_playtime();
        for id in running {
            *playtime.entry(id.clone()).or_default() += elapsed;
        }
        save_playtime(&playtime);
    }
}

/// Tamaño en pantalla de las portadas; la textura se genera a esa resolución exacta.
const ART_WIDTH: f32 = 128.0;
const ART_HEIGHT: f32 = 72.0;
/// Componente de Steam que aparece como juego instalado sin serlo.
const STEAM_REDISTRIBUTABLES_ID: u32 = 228980;
const STEAM_FULLY_INSTALLED: u32 = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExternalKind {
    Executable { path: PathBuf },
    Steam { app_id: u32, install_dir: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalGame {
    pub id: String,
    pub title: String,
    pub kind: ExternalKind,
    pub image: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GameSource {
    Epic,
    External(ExternalGame),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ArtSource {
    Url(String),
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameInfo {
    pub title: String,
    pub app_name: String,
    pub is_installed: bool,
    pub source: GameSource,
    pub art: Option<ArtSource>,
}

#[derive(Debug, Clone)]
pub struct ArtRequest {
    pub id: String,
    pub source: ArtSource,
}

#[derive(PartialEq, Clone, Copy, Default, Serialize, Deserialize)]
pub enum Section {
    #[default]
    All,
    Installed,
    NotInstalled,
    External,
}

impl Section {
    pub const ALL: [Section; 4] = [Section::All, Section::Installed, Section::NotInstalled, Section::External];

    fn contains(self, game: &GameInfo) -> bool {
        match self {
            Section::All => true,
            Section::Installed => game.is_installed,
            Section::NotInstalled => !game.is_installed,
            Section::External => matches!(game.source, GameSource::External(_)),
        }
    }
}

#[derive(PartialEq, Clone, Copy, Default, Serialize, Deserialize)]
pub enum SortOrder {
    #[default]
    Ascending,
    Descending,
}

impl SortOrder {
    pub fn toggled(self) -> Self {
        match self {
            SortOrder::Ascending => SortOrder::Descending,
            SortOrder::Descending => SortOrder::Ascending,
        }
    }
}

pub fn visible_games<'a>(
    games: &'a [GameInfo],
    section: Section,
    query: &str,
    order: SortOrder,
) -> Vec<&'a GameInfo> {
    let query = query.trim().to_lowercase();
    let mut visible: Vec<_> = games
        .iter()
        .filter(|game| section.contains(game) && game.title.to_lowercase().contains(&query))
        .collect();
    visible.sort_by_cached_key(|game| game.title.to_lowercase());
    if order == SortOrder::Descending {
        visible.reverse();
    }
    visible
}

impl ExternalGame {
    fn art(&self) -> Option<ArtSource> {
        if let Some(image) = &self.image {
            return Some(ArtSource::File(image.clone()));
        }
        match self.kind {
            ExternalKind::Steam { app_id, .. } => Some(ArtSource::Url(format!(
                "https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/header.jpg"
            ))),
            ExternalKind::Executable { .. } => None,
        }
    }

    fn to_game_info(&self) -> GameInfo {
        GameInfo {
            title: self.title.clone(),
            app_name: self.id.clone(),
            is_installed: true,
            source: GameSource::External(self.clone()),
            art: self.art(),
        }
    }
}

fn merge_library(epic: &[GameInfo], external: &[ExternalGame]) -> Vec<GameInfo> {
    epic.iter()
        .cloned()
        .chain(external.iter().map(ExternalGame::to_game_info))
        .collect()
}

/// Datos del diálogo de "agregar juego", todavía sin validar.
#[derive(Default)]
pub struct ExternalDraft {
    pub is_steam: bool,
    pub title: String,
    pub path: Option<PathBuf>,
    pub steam_apps: Vec<SteamApp>,
    pub selected_steam: Option<usize>,
    pub image: Option<PathBuf>,
}

impl ExternalDraft {
    pub fn new(already_added: &[ExternalGame]) -> Self {
        Self {
            steam_apps: steam_games_not_added(already_added),
            ..Default::default()
        }
    }

    pub fn select_executable(&mut self, path: PathBuf) {
        if self.title.trim().is_empty() {
            if let Some(stem) = path.file_stem() {
                self.title = stem.to_string_lossy().into_owned();
            }
        }
        self.path = Some(path);
    }

    pub fn select_steam_app(&mut self, index: usize) {
        self.title = self.steam_apps[index].title.clone();
        self.selected_steam = Some(index);
    }

    fn kind(&self) -> Option<ExternalKind> {
        if self.is_steam {
            let app = self.steam_apps.get(self.selected_steam?)?;
            return Some(ExternalKind::Steam {
                app_id: app.app_id,
                install_dir: app.install_dir.clone(),
            });
        }
        Some(ExternalKind::Executable {
            path: self.path.clone()?,
        })
    }

    pub fn is_complete(&self) -> bool {
        !self.title.trim().is_empty() && self.kind().is_some()
    }

    fn into_game(self) -> Option<ExternalGame> {
        let kind = self.kind().filter(|_| !self.title.trim().is_empty())?;
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_nanos();
        Some(ExternalGame {
            id: format!("external-{nanos}"),
            title: self.title.trim().to_string(),
            kind,
            image: self.image,
        })
    }
}

/// Acciones de la biblioteca que la interfaz dispara.
impl LauncherApp {
    pub(crate) fn rebuild_library(&mut self) {
        self.games = merge_library(&self.epic_games, &self.config.external_games);
        let requests: Vec<ArtRequest> = self
            .games
            .iter()
            .filter(|game| !self.art_requested.contains(&game.app_name))
            .filter_map(|game| {
                Some(ArtRequest {
                    id: game.app_name.clone(),
                    source: game.art.clone()?,
                })
            })
            .collect();
        if !requests.is_empty() {
            self.art_requested
                .extend(requests.iter().map(|request| request.id.clone()));
            self.send(UiCommand::LoadArt(requests));
        }
    }

    pub(crate) fn play(&mut self, game: &GameInfo) {
        if let Some(previous) = self.running_game.take() {
            self.stop_game(&previous);
        }
        match &game.source {
            GameSource::Epic => self.send(UiCommand::LaunchGame {
                app_name: game.app_name.clone(),
                language: self.config.game_languages.get(&game.app_name).cloned(),
            }),
            GameSource::External(external) => {
                self.send(UiCommand::LaunchExternal(external.clone()))
            }
        }
        self.running_game = Some(game.app_name.clone());
    }

    pub(crate) fn stop_game(&self, id: &str) {
        // Se busca en la configuración: `games` está vacía mientras se dibuja la lista.
        match self.config.external_games.iter().find(|game| game.id == id) {
            Some(external) => self.send(UiCommand::KillExternal(external.clone())),
            None => self.send(UiCommand::KillGame(id.to_string())),
        }
    }

    pub(crate) fn try_auto_launch(&mut self) {
        if std::mem::replace(&mut self.has_auto_launched, true) {
            return;
        }
        let auto_game = self.config.auto_launch_game.as_ref().and_then(|id| {
            self.games
                .iter()
                .find(|game| game.app_name == *id && game.is_installed)
                .cloned()
        });
        if let Some(game) = auto_game {
            self.play(&game);
        }
    }

    pub(crate) fn set_game_language(&mut self, app_name: &str, language: &str) {
        if language == AUTO_LANGUAGE {
            self.config.game_languages.remove(app_name);
        } else {
            self.config
                .game_languages
                .insert(app_name.to_string(), language.to_string());
        }
        self.save_config();
    }

    pub(crate) fn open_dlc_dialog(&mut self, game: &GameInfo, action: DlcAction) {
        self.send(UiCommand::ListDlcs(game.app_name.clone()));
        self.dlc_dialog = Some(DlcDialog {
            app_name: game.app_name.clone(),
            title: game.title.clone(),
            action,
            dlcs: None,
        });
    }

    pub(crate) fn open_language_dialog(&mut self, game: &GameInfo) {
        self.send(UiCommand::ListLanguages(game.app_name.clone()));
        self.language_dialog = Some(LanguageDialog {
            app_name: game.app_name.clone(),
            title: game.title.clone(),
            options: None,
            selected: None,
        });
    }

    pub(crate) fn open_launch_language_dialog(&mut self, game: &GameInfo) {
        self.send(UiCommand::ListLanguages(game.app_name.clone()));
        let current = self.config.game_languages.get(&game.app_name);
        self.launch_language_dialog = Some(LaunchLanguageDialog {
            app_name: game.app_name.clone(),
            title: game.title.clone(),
            languages: None,
            selected: current.map_or(AUTO_LANGUAGE.to_string(), Clone::clone),
        });
    }

    pub(crate) fn receive_dlcs(&mut self, app_name: &str, dlcs: Vec<DlcEntry>) {
        if let Some(dialog) = self.dlc_dialog.as_mut().filter(|d| d.app_name == app_name) {
            let wanted = dialog.action == DlcAction::Uninstall;
            dialog.dlcs = Some(
                dlcs.into_iter()
                    .filter(|dlc| dlc.installed == wanted)
                    .map(|dlc| (dlc, false))
                    .collect(),
            );
        }
    }

    pub(crate) fn receive_languages(&mut self, app_name: &str, options: Vec<LanguageOption>) {
        if let Some(dialog) = self.launch_language_dialog.as_mut().filter(|d| d.app_name == app_name) {
            dialog.languages = Some(launch_languages(&options));
        }
        if let Some(dialog) = self.language_dialog.as_mut().filter(|d| d.app_name == app_name) {
            dialog.options = Some(options);
        }
    }

    pub(crate) fn add_external(&mut self, draft: ExternalDraft) {
        if let Some(game) = draft.into_game() {
            self.config.external_games.push(game);
            self.save_config();
            self.rebuild_library();
        }
    }

    pub(crate) fn remove_external(&mut self, id: &str) {
        self.config.external_games.retain(|game| game.id != id);
        if self.config.auto_launch_game.as_deref() == Some(id) {
            self.config.auto_launch_game = None;
        }
        self.art.remove(id);
        self.art_requested.remove(id);
        self.save_config();
        self.rebuild_library();
    }
}

/// Lanza y detiene juegos que no pasan por legendary.
#[derive(Clone, Default)]
pub struct ExternalRunner {
    pids: Arc<Mutex<HashMap<String, u32>>>,
}

impl ExternalRunner {
    pub fn launch(&self, game: ExternalGame, events: Events) {
        let runner = self.clone();
        thread::spawn(move || {
            let started = match &game.kind {
                ExternalKind::Executable { path } => runner.run_executable(&game.id, path, &events),
                ExternalKind::Steam { app_id, install_dir } => {
                    run_steam_game(&game.id, *app_id, install_dir, &events)
                }
            };
            if !started {
                events.log(LogMsg::ExternalGameFailed { title: game.title });
            }
            events.send(BackendEvent::GameExited(game.id));
        });
    }

    /// Solo se sigue al proceso lanzado, no a su carpeta: un ejecutable puede estar
    /// en el Escritorio o en Descargas, junto a programas que no son el juego.
    fn run_executable(&self, id: &str, path: &Path, events: &Events) -> bool {
        let mut command = Command::new(path);
        if let Some(folder) = path.parent() {
            command.current_dir(folder);
        }
        let Ok(mut child) = command.spawn() else {
            return false;
        };
        self.pids.lock().unwrap().insert(id.to_string(), child.id());
        events.send(BackendEvent::GameStarted(id.to_string()));
        let _ = child.wait();
        self.pids.lock().unwrap().remove(id);
        true
    }

    pub fn kill(&self, game: &ExternalGame) {
        match &game.kind {
            ExternalKind::Executable { .. } => {
                if let Some(pid) = self.pids.lock().unwrap().remove(&game.id) {
                    process::kill_tree(pid);
                }
            }
            ExternalKind::Steam { install_dir, .. } => process::kill_game(install_dir),
        }
    }
}

fn run_steam_game(id: &str, app_id: u32, install_dir: &Path, events: &Events) -> bool {
    let url = format!("steam://rungameid/{app_id}");
    if hidden_command("cmd").args(["/C", "start", "", &url]).output().is_err() {
        return false;
    }
    events.send(BackendEvent::GameStarted(id.to_string()));
    process::wait_for_exit(install_dir)
}

#[derive(Debug, Clone, PartialEq)]
pub struct SteamApp {
    pub app_id: u32,
    pub title: String,
    pub install_dir: PathBuf,
}

/// Separa una línea `"clave"   "valor"` de los ficheros VDF/ACF de Steam.
fn vdf_pair(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.split('"').filter(|part| !part.trim().is_empty());
    Some((parts.next()?, parts.next()?))
}

fn parse_library_paths(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(vdf_pair)
        .filter(|(key, _)| *key == "path")
        .map(|(_, value)| PathBuf::from(value.replace(r"\\", r"\")))
        .collect()
}

fn parse_manifest(acf: &str, library: &Path) -> Option<SteamApp> {
    let mut fields: HashMap<&str, &str> = HashMap::new();
    for (key, value) in acf.lines().filter_map(vdf_pair) {
        fields.entry(key).or_insert(value);
    }
    let app_id: u32 = fields.get("appid")?.parse().ok()?;
    let flags: u32 = fields.get("StateFlags")?.parse().ok()?;
    if app_id == STEAM_REDISTRIBUTABLES_ID || flags & STEAM_FULLY_INSTALLED == 0 {
        return None;
    }
    Some(SteamApp {
        app_id,
        title: fields.get("name")?.to_string(),
        install_dir: library.join("steamapps").join("common").join(fields.get("installdir")?),
    })
}

fn steam_root() -> Option<PathBuf> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Valve\Steam")
        .and_then(|key| key.get_value::<String, _>("SteamPath"))
        .ok()
        .map(PathBuf::from)
}

fn installed_steam_games() -> Vec<SteamApp> {
    let Some(root) = steam_root() else {
        return Vec::new();
    };
    let vdf_path = root.join("steamapps").join("libraryfolders.vdf");
    let mut libraries = parse_library_paths(&std::fs::read_to_string(vdf_path).unwrap_or_default());
    libraries.push(root);

    let mut seen_ids = HashSet::new();
    let mut apps = Vec::new();
    for library in libraries {
        let Ok(entries) = std::fs::read_dir(library.join("steamapps")) else {
            continue;
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            let is_manifest = path.extension().is_some_and(|ext| ext == "acf");
            let app = is_manifest
                .then(|| std::fs::read_to_string(&path).ok())
                .flatten()
                .and_then(|text| parse_manifest(&text, &library));
            if let Some(app) = app.filter(|app| seen_ids.insert(app.app_id)) {
                apps.push(app);
            }
        }
    }
    apps.sort_by_cached_key(|app| app.title.to_lowercase());
    apps
}

fn steam_games_not_added(already_added: &[ExternalGame]) -> Vec<SteamApp> {
    let is_added = |id: u32| {
        already_added
            .iter()
            .any(|game| matches!(game.kind, ExternalKind::Steam { app_id, .. } if app_id == id))
    };
    installed_steam_games()
        .into_iter()
        .filter(|app| !is_added(app.app_id))
        .collect()
}

/// Mejor imagen de Epic para una tarjeta, pedida ya reducida a su CDN.
pub fn epic_art(images: &[(String, String)]) -> Option<ArtSource> {
    ["DieselGameBox", "OfferImageWide", "Featured", "Thumbnail", "DieselGameBoxTall"]
        .iter()
        .find_map(|wanted| images.iter().find(|(kind, _)| kind == wanted))
        .map(|(_, url)| ArtSource::Url(format!("{url}?h=288&resize=1&w=512")))
}

/// Imagen ya descargada, para cuando no se puede consultar a Epic.
pub fn cached_art(id: &str) -> Option<ArtSource> {
    cached_art_path(id).filter(|path| path.exists()).map(ArtSource::File)
}

fn cached_art_path(id: &str) -> Option<PathBuf> {
    let file_name: String = id.chars().filter(char::is_ascii_alphanumeric).collect();
    Some(legendary::install_dir()?.join("art").join(file_name))
}

fn art_bytes(request: &ArtRequest) -> Result<Vec<u8>> {
    let url = match &request.source {
        ArtSource::File(path) => return Ok(std::fs::read(path)?),
        ArtSource::Url(url) => url,
    };
    let cached = cached_art_path(&request.id).context("Sin carpeta de caché")?;
    if !cached.exists() {
        if let Some(folder) = cached.parent() {
            std::fs::create_dir_all(folder)?;
        }
        crate::updater::download(url, &cached)?;
    }
    Ok(std::fs::read(cached)?)
}

fn decode_art(bytes: &[u8], scale: f32) -> Result<egui::ColorImage> {
    let image = image::load_from_memory(bytes)?
        .resize_to_fill(
            (ART_WIDTH * scale).round() as u32,
            (ART_HEIGHT * scale).round() as u32,
            image::imageops::FilterType::Lanczos3,
        )
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()))
}

/// Si una imagen falla, la tarjeta se queda con su inicial.
pub fn load_art(requests: Vec<ArtRequest>, events: &Events) {
    let queue = Mutex::new(requests.into_iter());
    let scale = events.pixels_per_point();
    let workers = thread::available_parallelism().map_or(4, |count| count.get()).min(8);
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let Some(request) = queue.lock().unwrap().next() else {
                    return;
                };
                if let Ok(image) = art_bytes(&request).and_then(|bytes| decode_art(&bytes, scale)) {
                    events.send(BackendEvent::ArtLoaded { id: request.id, image });
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(title: &str, installed: bool, external: bool) -> GameInfo {
        let source = if external {
            GameSource::External(ExternalGame {
                id: title.to_string(),
                title: title.to_string(),
                kind: ExternalKind::Executable { path: PathBuf::from(r"C:\g\a.exe") },
                image: None,
            })
        } else {
            GameSource::Epic
        };
        GameInfo {
            title: title.to_string(),
            app_name: title.to_string(),
            is_installed: installed,
            source,
            art: None,
        }
    }

    fn titles(games: &[&GameInfo]) -> Vec<String> {
        games.iter().map(|game| game.title.clone()).collect()
    }

    #[test]
    fn filters_by_section_and_sorts_ignoring_case() {
        let games = [game("zeta", true, false), game("Alfa", false, false), game("osu", true, true)];
        let all = visible_games(&games, Section::All, "", SortOrder::Ascending);
        assert_eq!(titles(&all), ["Alfa", "osu", "zeta"]);
        let reversed = visible_games(&games, Section::All, "", SortOrder::Descending);
        assert_eq!(titles(&reversed), ["zeta", "osu", "Alfa"]);
        let installed = visible_games(&games, Section::Installed, "", SortOrder::Ascending);
        assert_eq!(titles(&installed), ["osu", "zeta"]);
        let missing = visible_games(&games, Section::NotInstalled, "", SortOrder::Ascending);
        assert_eq!(titles(&missing), ["Alfa"]);
        let external = visible_games(&games, Section::External, "", SortOrder::Ascending);
        assert_eq!(titles(&external), ["osu"]);
    }

    #[test]
    fn search_ignores_case_and_surrounding_spaces() {
        let games = [game("Geometry Dash", true, true), game("Fortnite", true, false)];
        let found = visible_games(&games, Section::All, "  geoM ", SortOrder::Ascending);
        assert_eq!(titles(&found), ["Geometry Dash"]);
    }

    #[test]
    fn reads_steam_library_paths() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t\t\"label\"\t\t\"\"\n\t}\n}";
        assert_eq!(parse_library_paths(vdf), [PathBuf::from(r"D:\SteamLibrary")]);
    }

    #[test]
    fn reads_only_fully_installed_steam_games() {
        let acf = |id: &str, flags: &str| {
            format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{id}\"\n\t\"name\"\t\t\"Geometry Dash\"\n\t\"StateFlags\"\t\t\"{flags}\"\n\t\"installdir\"\t\t\"Geometry Dash\"\n}}")
        };
        let library = Path::new(r"C:\Steam");
        let app = parse_manifest(&acf("322170", "4"), library).unwrap();
        assert_eq!(app.app_id, 322170);
        assert_eq!(app.install_dir, Path::new(r"C:\Steam\steamapps\common\Geometry Dash"));
        assert!(parse_manifest(&acf("322170", "2"), library).is_none());
        assert!(parse_manifest(&acf("228980", "4"), library).is_none());
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn a_full_install_has_every_language() {
        let manifest = strings(&["", "FortniteBR", "Langfr", "LangfrOptional", "LangesES", "voice_de_de"]);
        let options = language_options(&manifest, &[]);
        let names: Vec<_> = options.iter().map(|option| option.name.as_str()).collect();
        assert_eq!(names, ["Deutsch", "Español (España)", "Français"]);
        assert!(options.iter().all(|option| option.installed));
    }

    #[test]
    fn a_selective_install_only_has_its_tagged_languages() {
        let manifest = strings(&["Langfr", "Langde"]);
        let options = language_options(&manifest, &strings(&["", "Langfr"]));
        assert_eq!(options[0].tag, "Langde");
        assert!(!options[0].installed);
        assert!(options[1].installed);
    }

    #[test]
    fn launch_languages_come_from_the_game_plus_english() {
        let options = language_options(&strings(&["Langfr", "LangesES", "Langes419"]), &[]);
        let locales: Vec<_> = launch_languages(&options).into_iter().map(|l| l.locale).collect();
        assert_eq!(locales, ["en", "es-ES", "es-419", "fr"]);
    }

    #[test]
    fn launch_languages_fall_back_to_the_common_ones() {
        let locales: Vec<_> = launch_languages(&[]).into_iter().map(|l| l.locale).collect();
        assert_eq!(locales, ["de", "en", "es", "fr", "it"]);
    }

    #[test]
    fn games_without_language_tags_offer_nothing() {
        assert!(language_options(&strings(&["", "HighRes"]), &[]).is_empty());
    }

    #[test]
    fn adding_a_language_keeps_everything_already_installed() {
        let tags = tags_after_adding(&strings(&["chunk0", "Langfr"]), "Langde");
        assert_eq!(tags, strings(&["chunk0", "Langfr", "", "Langde"]));
    }

    #[test]
    fn without_saved_tags_only_required_data_is_requested() {
        assert_eq!(selection_args(&[]), ["--skip-sdl"]);
    }

    #[test]
    fn a_selective_install_keeps_its_tags() {
        assert_eq!(
            selection_args(&strings(&["", "chunk0"])),
            strings(&["--install-tag", "", "--install-tag", "chunk0"])
        );
    }

    #[test]
    fn finds_games_with_pending_updates() {
        let tsv = "App name\tApp title\tInstalled version\tAvailable version\tUpdate available\tInstall size\n\
                   Fortnite\tFortnite\t1\t2\tTrue\t10\n\
                   Other\tOther\t1\t1\tFalse\t10\n";
        assert_eq!(parse_pending_updates(tsv), ["Fortnite"]);
    }

    #[test]
    fn formats_played_time() {
        assert_eq!(format_duration(20), "< 1 min");
        assert_eq!(format_duration(45 * 60), "45 min");
        assert_eq!(format_duration(12 * 3600 + 30 * 60), "12 h 30 min");
    }

    #[test]
    fn prefers_wide_epic_images() {
        let images = [
            ("DieselGameBoxTall".to_string(), "https://x/tall".to_string()),
            ("DieselGameBox".to_string(), "https://x/wide".to_string()),
        ];
        assert_eq!(
            epic_art(&images),
            Some(ArtSource::Url("https://x/wide?h=288&resize=1&w=512".to_string()))
        );
    }

    #[test]
    fn draft_requires_a_title_and_a_target() {
        let mut draft = ExternalDraft::default();
        assert!(!draft.is_complete());
        draft.select_executable(PathBuf::from(r"C:\osu\osu!.exe"));
        assert_eq!(draft.title, "osu!");
        assert!(draft.is_complete());
        draft.title = "  ".to_string();
        assert!(!draft.is_complete());
    }
}
