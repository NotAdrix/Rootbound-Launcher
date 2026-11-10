use crate::types::{DriversFailure, Language, LogMsg, LoginFailure, UpdateStatus};

/// Texto de la interfaz. Cada clave lleva sus dos idiomas juntos para que no se desincronicen.
pub fn t(lang: Language, key: &str) -> &'static str {
    let (es, en) = match key {
        "installed_games" => ("Juegos Instalados", "Installed Games"),
        "library" => ("Librería", "Library"),
        "settings" => ("Ajustes", "Settings"),
        "reload" => ("Recargar", "Reload"),
        "stats" => ("Estadísticas", "Stats"),
        "profile" => ("Perfil", "Profile"),
        "time_played" => ("Tiempo jugado", "Time played"),
        "no_playtime" => ("Todavía no hay tiempo registrado.", "No time recorded yet."),
        "name" => ("Nombre", "Name"),
        "email" => ("Correo electrónico", "Email"),
        "not_available" => ("No disponible", "Not available"),
        "offline_mode" => ("Modo sin conexión", "Offline Mode"),
        "connection" => ("Conexión", "Connection"),
        "auto_update" => ("Actualizar Automáticamente", "Auto-Update"),
        "please_wait" => ("Espere un momento...", "Please wait..."),
        "installing" => ("Instalando...", "Installing..."),
        "resume_install" => ("Resumir", "Resume"),
        "ask_remove_browser" => ("¿Desea desinstalar el navegador?", "Do you want to uninstall the browser?"),
        "browser_installed" => ("Navegador instalado", "Browser installed"),
        "yes" => ("Sí", "Yes"),
        "no" => ("No", "No"),
        "ghost_title" => ("Juego no encontrado", "Game not found"),
        "ask_ghost" => ("¿Desea reparar {n} o borrarlo definitivamente?", "Do you want to repair {n} or delete it permanently?"),
        "repair" => ("Reparar", "Repair"),
        "delete_permanently" => ("Borrar Definitivamente", "Delete Permanently"),
        "update_available" => ("Actualización disponible", "Update available"),
        "ask_update" => ("El launcher tiene una actualización, ¿desea actualizarlo?", "The launcher has an update. Do you want to install it?"),
        "enabled" => ("Activado", "On"),
        "disabled" => ("Desactivado", "Off"),
        "drivers" => ("Drivers", "Drivers"),
        "drivers_desc" => (
            "Si experimenta problemas con los juegos, reinstalar drivers podría solucionarlo (solo usuarios experimentados).",
            "If you have problems with games, reinstalling drivers could fix them (experienced users only).",
        ),
        "drivers_clean" => ("Eliminar Drivers de forma limpia con modo seguro", "Cleanly remove drivers using safe mode"),
        "confirm_drivers" => ("Eliminar drivers", "Remove drivers"),
        "confirm_drivers_desc" => (
            "El PC se reiniciará en modo seguro, se borrarán los drivers de vídeo y volverá a reiniciarse. Guarda tu trabajo y cierra lo que tengas abierto. Si tu cuenta pide contraseña, tendrás que escribirla al entrar en modo seguro.",
            "The PC will restart into safe mode, video drivers will be removed, and it will restart again. Save your work and close everything. If your account has a password, you will have to type it in safe mode.",
        ),
        "yes_clean_drivers" => ("Sí, eliminar", "Yes, remove"),
        "drivers_section" => ("Limpieza de drivers", "Driver cleanup"),
        "open_nvcleanstall_after" => ("Abrir NVCleanstall al finalizar", "Open NVCleanstall when finished"),
        "nvcleanstall_desc" => (
            "Herramienta de TechPowerUp para instalar los drivers de NVIDIA eligiendo qué componentes incluir.",
            "A TechPowerUp tool for installing NVIDIA drivers while choosing which components to include.",
        ),
        "nvcleanstall_warning" => (
            "Instalar drivers por este método puede romper GeForce Experience si se hace mal.",
            "Installing drivers this way can break GeForce Experience if done incorrectly.",
        ),
        "nvcleanstall_note" => (
            "No se recomienda instalar GeForce Experience por la telemetría, el uso de CPU/GPU y las interrupciones que empeoran el rendimiento del sistema operativo.",
            "Installing GeForce Experience is not recommended because of telemetry, CPU/GPU usage and interrupts that worsen system performance.",
        ),
        "use_with_care" => ("Utilice la herramienta con precaución.", "Use this tool with caution."),
        "open_nvcleanstall" => ("Abrir NVCleanstall", "Open NVCleanstall"),
        "all_games" => ("Todos", "All"),
        "automatic" => ("Automático", "Automatic"),
        "no_language_packs" => (
            "Este juego no tiene idiomas que se puedan descargar por separado.",
            "This game has no languages that can be downloaded separately.",
        ),
        "uninstall_dlcs" => ("Desinstalar DLCs", "Uninstall DLCs"),
        "no_dlcs_installed" => ("No hay DLCs instalados.", "There are no DLCs installed."),
        "no_dlcs" => ("No hay DLCs pendientes de instalar.", "There are no DLCs left to install."),
        "remove_app" => ("Eliminar App y todas sus dependencias", "Remove App and all its dependencies"),
        "confirm_remove_app" => ("Eliminar App", "Remove App"),
        "confirm_remove_app_desc" => (
            "Se borrarán el launcher, su copia de Legendary, tu sesión y toda su configuración. Los juegos instalados no se borran.",
            "The launcher, its copy of Legendary, your session and all its settings will be deleted. Installed games are kept.",
        ),
        "yes_remove_app" => ("Sí, eliminar", "Yes, remove"),
        "not_installed_games" => ("Juegos No Instalados", "Not Installed Games"),
        "external_games" => ("Juegos Externos", "External Games"),
        "search" => ("Buscar...", "Search..."),
        "no_results" => ("No hay juegos aquí.", "No games here."),
        "add_game" => ("Agregar juego", "Add game"),
        "executable" => ("Ejecutable", "Executable"),
        "choose_file" => ("Elegir...", "Choose..."),
        "no_file_selected" => ("Ningún archivo elegido", "No file selected"),
        "choose_game" => ("Elige un juego", "Choose a game"),
        "no_steam_games" => ("No se encontraron juegos de Steam para agregar.", "No Steam games found to add."),
        "game_name" => ("Nombre:", "Name:"),
        "image_optional" => ("Imagen (opcional)", "Image (optional)"),
        "remove_from_list" => ("Quitar de la lista", "Remove from list"),
        "terminal" => ("TERMINAL", "TERMINAL"),
        "play" => ("Iniciar", "Play"),
        "close" => ("Cerrar", "Close"),
        "auto_launch" => ("Ejecutar Automáticamente", "Run Automatically"),
        "install_dlcs" => ("Instalar DLCs", "Install DLCs"),
        "verify_files" => ("Verificar y reparar", "Verify and repair"),
        "verify_updates" => ("Buscar actualización", "Check for update"),
        "game_language" => ("Descargar Idioma", "Download Language"),
        "change_language" => ("Cambiar Idioma", "Change Language"),
        "apply" => ("Aplicar", "Apply"),
        "uninstall" => ("Desinstalar", "Uninstall"),
        "install" => ("Instalar", "Install"),
        "current_version" => ("Versión actual", "Current version"),
        "appearance" => ("Apariencia", "Appearance"),
        "updates" => ("Actualizaciones", "Updates"),
        "check_updates" => ("Buscar Actualizaciones", "Check for Updates"),
        "factory_reset" => ("Reinicio de Fábrica", "Factory Reset"),
        "factory_reset_desc" => (
            "Restablece la aplicación a su estado original. Se borrarán las sesiones y configuraciones.",
            "Resets the application to its original state. Sessions and settings will be deleted.",
        ),
        "reset_all" => ("Restablecer todo", "Reset all"),
        "install_game" => ("Instalar Juego", "Install Game"),
        "install_folder" => ("Carpeta de instalación:", "Install folder:"),
        "include_dlcs" => ("Incluir DLCs", "Include DLCs"),
        "language" => ("Idioma:", "Language:"),
        "game" => ("Juego:", "Game:"),
        "start_download" => ("Iniciar Descarga", "Start Download"),
        "cancel" => ("Cancelar", "Cancel"),
        "confirm_reset" => ("Confirmar Restablecimiento", "Confirm Reset"),
        "confirm_reset_desc" => (
            "¿Estás seguro? Se borrará tu sesión y configuración.",
            "Are you sure? Your session and settings will be deleted.",
        ),
        "resetting" => ("Restableciendo...", "Resetting..."),
        "yes_delete_all" => ("Sí, borrar todo", "Yes, delete everything"),
        "verifying" => ("Verificando...", "Verifying..."),
        "downloading" => ("Descargando...", "Downloading..."),
        "pause" => ("Pausar", "Pause"),
        "resume" => ("Reanudar", "Resume"),
        "close_app" => ("Cerrando aplicación...", "Closing app..."),
        "all_data_deleted" => ("Todos los datos han sido eliminados.", "All data has been deleted."),
        "reset_complete" => ("Reinicio Completado", "Reset Complete"),
        _ => ("???", "???"),
    };
    match lang {
        Language::Es => es,
        Language::En => en,
    }
}

fn pick(lang: Language, es: String, en: String) -> String {
    match lang {
        Language::Es => es,
        Language::En => en,
    }
}

pub fn render_log(lang: Language, msg: &LogMsg) -> String {
    let (es, en) = match msg {
        LogMsg::Welcome => ("Bienvenido a Rootbound Launcher.", "Welcome to Rootbound Launcher."),
        LogMsg::CheckingSession => ("Verificando sesión de Epic...", "Checking Epic session..."),
        LogMsg::NoSessionOpeningBrowser => ("No hay sesión. Abriendo navegador...", "No session. Opening browser..."),
        LogMsg::WaitingLogin => ("Esperando login...", "Waiting for login..."),
        LogMsg::CodeReceived => ("Código obtenido. Autenticando...", "Code received. Authenticating..."),
        LogMsg::AuthFinished => ("Autenticación finalizada.", "Authentication finished."),
        LogMsg::LoadingLocalLibrary => ("Cargando biblioteca...", "Loading library..."),
        LogMsg::SearchingNewGames => ("Buscando juegos de Epic en disco...", "Searching disks for Epic games..."),
        LogMsg::LibraryUpToDate => ("Biblioteca al día.", "Library up to date."),
        LogMsg::LibraryUpdated => ("Biblioteca actualizada.", "Library updated."),
        LogMsg::PausingDownload => ("Descarga en pausa.", "Download paused."),
        LogMsg::ResumingDownload => ("Reanudando descarga...", "Resuming download..."),
        LogMsg::DownloadCancelled => ("Descarga cancelada.", "Download cancelled."),
        LogMsg::DownloadingLegendary => ("Primer inicio: descargando Legendary...", "First run: downloading Legendary..."),
        LogMsg::FactoryResetStart => ("Iniciando restablecimiento de fábrica...", "Starting factory reset..."),
        LogMsg::LoginFailed(LoginFailure::NoBrowser) => ("[ERROR] No se encontró un navegador compatible.", "[ERROR] No compatible browser was found."),
        LogMsg::LoginFailed(LoginFailure::CouldNotOpenBrowser) => ("[ERROR] No se pudo abrir el navegador.", "[ERROR] The browser could not be opened."),
        LogMsg::LoginFailed(LoginFailure::Cancelled) => ("Login cancelado.", "Login cancelled."),
        LogMsg::LoginFailed(LoginFailure::TimedOut) => ("[ERROR] Tiempo agotado esperando el login.", "[ERROR] Timed out waiting for login."),
        LogMsg::LoginFailed(LoginFailure::Rejected) => ("[ERROR] Epic rechazó el inicio de sesión.", "[ERROR] Epic rejected the login."),
        LogMsg::LegendaryUnavailable => ("[ERROR] No se pudo descargar Legendary. Comprueba tu conexión y reinicia el launcher.", "[ERROR] Legendary could not be downloaded. Check your connection and restart the launcher."),
        LogMsg::CommandCouldNotStart => ("[ERROR] No se pudo ejecutar Legendary.", "[ERROR] Legendary could not be run."),
        LogMsg::LegendaryUpdated => ("Legendary actualizado.", "Legendary updated."),
        LogMsg::LegendaryUpdateFailed => ("[ERROR] No se pudo actualizar Legendary.", "[ERROR] Legendary could not be updated."),
        LogMsg::AutoUpdating { title } => {
            return pick(lang, format!("Actualizando {title} automáticamente..."), format!("Auto-updating {title}..."));
        }
        LogMsg::InstallBlocked => ("[ERROR] Legendary no puede continuar con la instalación.", "[ERROR] Legendary cannot continue with the installation."),
        LogMsg::NotEnoughDiskSpace { free, required } => {
            return pick(
                lang,
                format!("[ERROR] No hay espacio suficiente: libres {free}, necesarios {required}."),
                format!("[ERROR] Not enough disk space: {free} free, {required} required."),
            );
        }
        LogMsg::Separator => ("", ""),
        LogMsg::InstallingBrowser => ("No se encontró un navegador compatible. Instalando Chrome...", "No compatible browser found. Installing Chrome..."),
        LogMsg::FilesMissingWarning => ("[AVISO] Faltan archivos en la instalación del juego; puede estar dañada o no coincidir con la última versión.", "[WARNING] Some game files are missing; the install may be corrupted or not match the latest version."),
        LogMsg::FolderNotRemoved { path } => {
            return pick(lang, format!("[AVISO] No se pudo borrar la carpeta {path}."), format!("[WARNING] The folder {path} could not be deleted."));
        }
        LogMsg::FolderDeleted { path } => {
            return pick(lang, format!("Carpeta borrada: {path}"), format!("Folder deleted: {path}"));
        }
        LogMsg::DriversDownloading => ("Descargando DDU y preparando el proceso...", "Downloading DDU and preparing..."),
        LogMsg::NvcleanstallDownloading => ("Descargando NVCleanstall...", "Downloading NVCleanstall..."),
        LogMsg::DriversRestarting => ("Todo listo. El PC se reiniciará en 10 segundos.", "All set. The PC will restart in 10 seconds."),
        LogMsg::DriversFailed(DriversFailure::BitLocker) => ("[ERROR] No se puede continuar con BitLocker activo. No se cambió nada.", "[ERROR] Cannot continue with BitLocker enabled. Nothing was changed."),
        LogMsg::DriversFailed(DriversFailure::Prepare) => ("[ERROR] No se pudo preparar el proceso. El arranque no se modificó.", "[ERROR] The process could not be prepared. Boot settings were not changed."),
        LogMsg::DriversFailed(DriversFailure::Download) => ("[ERROR] No se pudo descargar. No se cambió nada.", "[ERROR] The download failed. Nothing was changed."),
        LogMsg::DriversFailed(DriversFailure::InvalidHash) => ("[ERROR] El archivo descargado no coincide con el publicado y se descartó. No se cambió nada.", "[ERROR] The downloaded file does not match the published one and was discarded. Nothing was changed."),
        LogMsg::DriversFailed(DriversFailure::Install) => ("[ERROR] No se pudo instalar DDU. No se cambió nada.", "[ERROR] DDU could not be installed. Nothing was changed."),
        LogMsg::DriversFailed(DriversFailure::Boot) => ("[ERROR] No se pudo preparar el modo seguro. No se reiniciará.", "[ERROR] Safe mode could not be prepared. The PC will not restart."),
        LogMsg::CommandFailed => ("[ERROR] La operación de Legendary no terminó correctamente.", "[ERROR] The Legendary operation did not finish correctly."),
        LogMsg::AlreadyUpToDate => ("Juego listo para ejecutar.", "Game ready to launch."),
        LogMsg::InstallFinished => ("Instalación finalizada.", "Installation finished."),
        LogMsg::Uninstalled => ("Desinstalación finalizada.", "Uninstall finished."),
        LogMsg::VerificationOk => ("Verificación correcta.", "Verification succeeded."),
        LogMsg::VerificationFailed => ("[ERROR] La verificación encontró archivos dañados o faltantes.", "[ERROR] Verification found corrupted or missing files."),
        LogMsg::SessionRejected => ("[ERROR] No se pudo validar tu sesión de Epic.", "[ERROR] Your Epic session could not be validated."),
        LogMsg::InstallLocked => ("[ERROR] Hay otro proceso de Legendary instalando o modificando juegos. Espera a que termine.", "[ERROR] Another Legendary process is installing or changing games. Wait for it to finish."),
        LogMsg::PreparingDownload { title } => {
            return pick(lang, format!("Preparando {title}..."), format!("Preparing {title}..."));
        }
        LogMsg::DownloadSize { size } => {
            return pick(lang, format!("Tamaño de la descarga: {size}"), format!("Download size: {size}"));
        }
        LogMsg::LaunchingGame { title } => {
            return pick(lang, format!("Iniciando {title}..."), format!("Launching {title}..."));
        }
        LogMsg::VerifyingGame { title } => {
            return pick(lang, format!("Verificando {title}..."), format!("Verifying {title}..."));
        }
        LogMsg::LegendaryProblem { is_error, text } => {
            return match (is_error, lang) {
                (true, Language::En) => format!("[ERROR] {text}"),
                (false, Language::En) => format!("[WARNING] {text}"),
                (true, Language::Es) => "[ERROR] Legendary informó de un error. El detalle está en legendary.log.".to_string(),
                (false, Language::Es) => "[AVISO] Legendary informó de un problema. El detalle está en legendary.log.".to_string(),
            };
        }
        LogMsg::Importing { app, path } => {
            return pick(lang, format!("[ACCION] Importando {app} desde {path}"), format!("[ACTION] Importing {app} from {path}"));
        }
        LogMsg::GameClosed { app } => {
            return pick(lang, format!("Juego cerrado: {app}"), format!("Game closed: {app}"));
        }
        LogMsg::ExternalGameFailed { title } => {
            return pick(lang, format!("[AVISO] {title} no llegó a iniciarse."), format!("[WARNING] {title} did not start."));
        }
        LogMsg::GameFailedToStart { app } => {
            return pick(
                lang,
                format!("[AVISO] {app} no llegó a iniciarse. Si se repite, usa \"Verificar y reparar\"."),
                format!("[WARNING] {app} did not start. If this keeps happening, use \"Verify and repair\"."),
            );
        }
    };
    pick(lang, es.to_string(), en.to_string())
}

pub fn render_update(lang: Language, status: &UpdateStatus) -> String {
    let (es, en) = match status {
        UpdateStatus::Checking => ("Buscando actualizaciones...", "Checking for updates..."),
        UpdateStatus::UpToDate => ("Estás en la última versión.", "You are on the latest version."),
        UpdateStatus::NoReleases => ("No se encontraron releases.", "No releases found."),
        UpdateStatus::NoExeAttached => ("Actualización encontrada, pero no tiene un .exe adjunto.", "Update found, but it has no .exe attached."),
        UpdateStatus::NetworkError => ("Error de red.", "Network error."),
        UpdateStatus::Downloading => ("Descargando actualización...", "Downloading update..."),
        UpdateStatus::Applying => ("Aplicando actualización, reiniciando...", "Applying update, restarting..."),
        UpdateStatus::Available { version, .. } => {
            return pick(lang, format!("¡Nueva versión disponible: v{version}!"), format!("New version available: v{version}!"));
        }
        UpdateStatus::Failed => ("La actualización falló.", "The update failed."),
    };
    pick(lang, es.to_string(), en.to_string())
}
