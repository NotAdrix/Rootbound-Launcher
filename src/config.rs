use crate::juegos::logica::{ExternalGame, Section, SortOrder};
use crate::types::Language;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub auto_launch_game: Option<String>,
    pub language: Language,
    /// Idioma por juego (`app_name` -> código). Legendary lo aplica al lanzar.
    pub game_languages: HashMap<String, String>,
    pub external_games: Vec<ExternalGame>,
    pub auto_update: HashSet<String>,
    pub offline_mode: bool,
    pub use_nvcleanstall: bool,
    pub terminal_hidden: bool,
    pub sort_order: SortOrder,
    pub last_section: Section,
}

pub fn dir() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("rootbound_launcher"))
}

pub fn load() -> Config {
    dir()
        .and_then(|d| std::fs::read_to_string(d.join("config.json")).ok())
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn save(config: &Config) {
    let Some(dir) = dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(data) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(dir.join("config.json"), data);
    }
}
