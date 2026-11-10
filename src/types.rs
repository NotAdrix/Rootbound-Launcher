use crate::juegos::logica::{ArtRequest, DlcEntry, ExternalGame, GameInfo, LanguageOption, Profile};
use eframe::egui;
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum Language {
    #[default]
    Es,
    En,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DriversFailure {
    BitLocker,
    Prepare,
    Download,
    InvalidHash,
    Install,
    Boot,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoginFailure {
    NoBrowser,
    CouldNotOpenBrowser,
    Cancelled,
    TimedOut,
    Rejected,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogMsg {
    Welcome,
    CheckingSession,
    NoSessionOpeningBrowser,
    WaitingLogin,
    CodeReceived,
    AuthFinished,
    LoginFailed(LoginFailure),
    LoadingLocalLibrary,
    SearchingNewGames,
    Importing { app: String, path: String },
    LibraryUpToDate,
    LibraryUpdated,
    GameClosed { app: String },
    GameFailedToStart { app: String },
    ExternalGameFailed { title: String },
    PausingDownload,
    ResumingDownload,
    DownloadCancelled,
    DownloadingLegendary,
    LegendaryUnavailable,
    CommandCouldNotStart,
    CommandFailed,
    DriversDownloading,
    NvcleanstallDownloading,
    DriversRestarting,
    DriversFailed(DriversFailure),
    FolderDeleted { path: String },
    Separator,
    InstallingBrowser,
    FilesMissingWarning,
    FolderNotRemoved { path: String },
    InstallBlocked,
    NotEnoughDiskSpace { free: String, required: String },
    AutoUpdating { title: String },
    LegendaryUpdated,
    LegendaryUpdateFailed,
    PreparingDownload { title: String },
    DownloadSize { size: String },
    AlreadyUpToDate,
    InstallFinished,
    Uninstalled,
    LaunchingGame { title: String },
    VerifyingGame { title: String },
    VerificationOk,
    VerificationFailed,
    SessionRejected,
    InstallLocked,
    LegendaryProblem { is_error: bool, text: String },
    FactoryResetStart,
}

/// Estado de la autoactualización del propio launcher.
#[derive(Debug, Clone)]
pub enum UpdateStatus {
    Checking,
    UpToDate,
    NoReleases,
    NoExeAttached,
    NetworkError,
    Available { version: String, url: String },
    Downloading,
    Applying,
    Failed,
}

/// Última lectura del progreso que imprime legendary.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Progress {
    pub percent: String,
    pub download: String,
    pub disk: String,
    pub eta: String,
    pub verifying: bool,
}

#[derive(Debug, Clone)]
pub enum UiCommand {
    LaunchGame {
        app_name: String,
        language: Option<String>,
    },
    KillGame(String),
    LaunchExternal(ExternalGame),
    KillExternal(ExternalGame),
    LoadArt(Vec<ArtRequest>),
    ReloadLibrary,
    InstallGame {
        app_name: String,
        path: String,
        dlcs: bool,
    },
    UninstallGame(String),
    ListDlcs(String),
    UninstallDlcs(Vec<String>),
    InstallDlcs(Vec<String>),
    ListLanguages(String),
    InstallLanguage { app_name: String, tag: String },
    RepairGame(String),
    UpdateGame(String),
    CheckForUpdates,
    PerformUpdate(String),
    PauseDownload,
    ResumeDownload,
    CancelDownload,
    UpdateLegendary,
    LoadProfile,
    SetOfflineMode(bool),
    RemoveBrowser,
    CleanDrivers,
    OpenNvcleanstall,
    RepairGhost { app_name: String, path: String },
    DeleteGhost { path: String },
    FactoryReset,
    RemoveApp,
}

#[derive(Debug, Clone)]
pub enum BackendEvent {
    LibraryLoaded(Vec<GameInfo>),
    ArtLoaded { id: String, image: egui::ColorImage },
    DlcsListed { app_name: String, dlcs: Vec<DlcEntry> },
    LanguagesListed { app_name: String, options: Vec<LanguageOption> },
    GameStarted(String),
    GameExited(String),
    Log(LogMsg),
    Update(UpdateStatus),
    Progress(Progress),
    DownloadPaused,
    DownloadCancelled,
    JobStarted,
    JobFinished,
    InstallFinished(String),
    AskRemoveBrowser,
    GhostGameFound { app_name: String, path: String },
    LegendaryUpdateOffered,
    ProfileLoaded(Profile),
    ResetComplete,
}

/// Canal hacia la interfaz: cada evento despierta a egui, así la UI no hace polling.
#[derive(Clone)]
pub struct Events {
    tx: Sender<BackendEvent>,
    ctx: Option<egui::Context>,
}

impl Events {
    pub fn new(tx: Sender<BackendEvent>, ctx: egui::Context) -> Self {
        Self { tx, ctx: Some(ctx) }
    }

    /// Para el proceso en segundo plano, que no tiene interfaz a la que avisar.
    pub fn detached() -> Self {
        let (tx, _) = std::sync::mpsc::channel();
        Self { tx, ctx: None }
    }

    pub fn send(&self, event: BackendEvent) {
        let _ = self.tx.send(event);
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
    }

    /// Píxeles físicos por unidad de interfaz; 1.0 si no hay ventana.
    pub fn pixels_per_point(&self) -> f32 {
        self.ctx.as_ref().and_then(egui::Context::native_pixels_per_point).unwrap_or(1.0)
    }

    pub fn log(&self, msg: LogMsg) {
        self.send(BackendEvent::Log(msg));
    }
}
