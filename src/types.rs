#[derive(Debug, Clone, PartialEq)]
pub struct GameInfo {
    pub title: String,
    pub app_name: String,
    pub is_installed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Language {
    Es,
    En,
}

impl Default for Language {
    fn default() -> Self {
        Language::Es
    }
}

#[derive(Debug, Clone)]
pub enum UiCommand {
    LaunchGame(String),
    KillGame(String),
    SetAutoLaunch(Option<String>),
    ReloadLibrary,
    InstallGame {
        app_name: String,
        path: String,
        dlcs: bool,
        lang: String,
    },
    UninstallGame(String),
    InstallDlc(String),
    VerifyGame(String),
    VerifyUpdate(String),
    InstallLang(String, String),
    CheckForUpdates,
    PauseDownload,
    ResumeDownload,
    CancelDownload,
    FactoryReset(bool),
}

#[derive(Debug, Clone)]
pub enum BackendEvent {
    Status(String),
    Error(String),
    LibraryLoaded(Vec<GameInfo>),
    GameStarted(String),
    GameExited(String),
    Log(String),
    UpdateStatus(String),
    DownloadPaused,
    DownloadFinished,
    ResetComplete,
}
