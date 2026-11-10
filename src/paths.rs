//! Todo lo del launcher vive en una sola carpeta: `%SystemDrive%\Rootbound`.

use std::path::PathBuf;

pub const LAUNCHER_EXE: &str = "RootboundLauncher.exe";
pub const BACKGROUND_EXE: &str = "RootboundBackground.exe";

pub fn root() -> PathBuf {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    PathBuf::from(format!(r"{drive}\Rootbound"))
}

pub fn launcher_exe() -> PathBuf {
    root().join(LAUNCHER_EXE)
}

pub fn background_exe() -> PathBuf {
    root().join(BACKGROUND_EXE)
}

/// Ajustes, tiempo jugado y caché de la biblioteca.
pub fn config_dir() -> PathBuf {
    root().join("config")
}

/// Sesión de Epic y registro de juegos de legendary.
pub fn legendary_data() -> PathBuf {
    root().join("legendary")
}

pub fn games_dir() -> PathBuf {
    root().join("Games")
}

pub fn drivers_dir() -> PathBuf {
    root().join("drivers")
}
