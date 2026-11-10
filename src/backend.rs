use crate::juegos::logica::{self, ExternalRunner};
use crate::legendary::{self, ImportOutcome, Legendary};
use crate::types::{BackendEvent, Events, LogMsg, LoginFailure, UiCommand};
use crate::{config, drivers, import, login, process, service, updater};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;

const AUTO_UPDATE_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// Tareas que se ejecutan de una en una: legendary solo admite un proceso
/// instalando, importando o moviendo juegos a la vez.
enum Job {
    Startup,
    AutoUpdate,
    UpdateLegendary,
    Run { args: Vec<String>, reload: bool },
    Uninstall(String),
    DeleteGhost(String),
    InstallDlcs(Vec<String>),
    UninstallDlcs(Vec<String>),
    Update(String),
    Repair(String),
    InstallLanguage { app_name: String, tag: String },
    Import(Vec<(String, String)>),
    Launch { app_name: String, language: Option<String> },
}

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| part.to_string()).collect()
}

pub fn start(commands: Receiver<UiCommand>, events: Events) {
    thread::spawn(move || {
        let external = ExternalRunner::default();
        let cli = match Legendary::locate(&events) {
            Ok(cli) => cli,
            Err(_) => {
                events.log(LogMsg::LegendaryUnavailable);
                events.send(BackendEvent::LibraryLoaded(Vec::new()));
                // Sin legendary, los juegos externos deben seguir funcionando.
                for command in commands {
                    handle_independent(command, &external, &events);
                }
                return;
            }
        };

        let (jobs, job_queue) = channel();
        let _ = jobs.send(Job::Startup);
        let timer_jobs = jobs.clone();
        thread::spawn(move || loop {
            thread::sleep(AUTO_UPDATE_INTERVAL);
            let _ = timer_jobs.send(Job::AutoUpdate);
        });
        let worker = (cli.clone(), events.clone(), jobs.clone());
        thread::spawn(move || {
            let (cli, events, jobs) = worker;
            for job in job_queue {
                if !cli.is_shutting_down() {
                    events.log(LogMsg::Separator);
                    events.send(BackendEvent::JobStarted);
                    run_job(job, &cli, &events, &jobs);
                    events.send(BackendEvent::JobFinished);
                }
            }
        });

        let mut paused = None;
        for command in commands {
            if let Some(command) = handle_independent(command, &external, &events) {
                handle_command(command, &cli, &events, &jobs, &mut paused);
            }
        }
    });
}

/// Atiende lo que no necesita legendary y devuelve el resto de órdenes.
fn handle_independent(
    command: UiCommand,
    external: &ExternalRunner,
    events: &Events,
) -> Option<UiCommand> {
    match command {
        UiCommand::LaunchExternal(game) => external.launch(game, events.clone()),
        UiCommand::KillExternal(game) => {
            let external = external.clone();
            thread::spawn(move || external.kill(&game));
        }
        UiCommand::LoadArt(requests) => {
            let events = events.clone();
            thread::spawn(move || logica::load_art(requests, &events));
        }
        other => return Some(other),
    }
    None
}

fn in_background(
    cli: &Legendary,
    events: &Events,
    task: impl FnOnce(Legendary, Events) + Send + 'static,
) {
    let (cli, events) = (cli.clone(), events.clone());
    thread::spawn(move || task(cli, events));
}

fn handle_command(
    command: UiCommand,
    cli: &Legendary,
    events: &Events,
    jobs: &Sender<Job>,
    paused: &mut Option<Vec<String>>,
) {
    let queue = |job| {
        let _ = jobs.send(job);
    };

    match command {
        UiCommand::LaunchGame { app_name, language } => {
            if cli.is_busy() {
                // Sin actualizar: si no, esperaría a que acabe la descarga en curso.
                in_background(cli, events, move |cli, events| {
                    start_game(&cli, &app_name, language.as_deref(), &events)
                });
            } else {
                queue(Job::Launch { app_name, language });
            }
        }
        UiCommand::KillGame(app_name) => in_background(cli, events, move |cli, _| {
            if let Some(root) = cli.install_path(&app_name) {
                process::kill_game(&root);
            }
        }),
        UiCommand::ReloadLibrary => {
            in_background(cli, events, |cli, events| reload(&cli, &events))
        }
        UiCommand::InstallGame { app_name, path, dlcs } => {
            let mut install = args(&["install", &app_name, "--base-path", &path, "--skip-sdl", "-y"]);
            if !dlcs {
                install.push("--skip-dlcs".into());
            }
            queue(Job::Run { args: install, reload: true });
        }
        UiCommand::UninstallGame(app_name) => queue(Job::Uninstall(app_name)),
        UiCommand::ListDlcs(app_name) => in_background(cli, events, move |cli, events| {
            let dlcs = cli.dlcs(&app_name);
            events.send(BackendEvent::DlcsListed { app_name, dlcs });
        }),
        UiCommand::InstallDlcs(dlcs) => queue(Job::InstallDlcs(dlcs)),
        UiCommand::UninstallDlcs(dlcs) => queue(Job::UninstallDlcs(dlcs)),
        UiCommand::ListLanguages(app_name) => in_background(cli, events, move |cli, events| {
            let options = cli.language_options(&app_name);
            events.send(BackendEvent::LanguagesListed { app_name, options });
        }),
        UiCommand::InstallLanguage { app_name, tag } => queue(Job::InstallLanguage { app_name, tag }),
        UiCommand::RepairGame(app_name) => queue(Job::Repair(app_name)),
        UiCommand::UpdateGame(app_name) => queue(Job::Update(app_name)),
        UiCommand::CheckForUpdates => {
            in_background(cli, events, |cli, events| check_for_updates(&cli, &events))
        }
        UiCommand::PerformUpdate(url) => {
            in_background(cli, events, move |_, events| updater::perform(&url, &events))
        }
        UiCommand::PauseDownload => {
            if let Some(interrupted) = cli.stop() {
                *paused = Some(interrupted);
                events.log(LogMsg::PausingDownload);
                events.send(BackendEvent::DownloadPaused);
            }
        }
        UiCommand::ResumeDownload => {
            if let Some(interrupted) = paused.take() {
                events.log(LogMsg::ResumingDownload);
                queue(Job::Run { args: interrupted, reload: true });
            }
        }
        UiCommand::CancelDownload => {
            cli.stop();
            *paused = None;
            events.log(LogMsg::DownloadCancelled);
            events.send(BackendEvent::DownloadCancelled);
        }
        UiCommand::FactoryReset => {
            in_background(cli, events, |cli, events| factory_reset(&cli, &events))
        }
        UiCommand::UpdateLegendary => queue(Job::UpdateLegendary),
        UiCommand::RepairGhost { app_name, path } => {
            let folder = std::path::Path::new(&path);
            let parent = folder.parent().map(|p| p.display().to_string()).unwrap_or_default();
            let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let install = args(&[
                "install", &app_name, "--base-path", &parent, "--game-folder", &name,
                "--skip-sdl", "--skip-dlcs", "-y",
            ]);
            queue(Job::Run { args: install, reload: true });
        }
        UiCommand::DeleteGhost { path } => queue(Job::DeleteGhost(path)),
        UiCommand::CleanDrivers => in_background(cli, events, |_, events| {
            events.send(BackendEvent::JobStarted);
            drivers::clean_drivers(&events);
            events.send(BackendEvent::JobFinished);
        }),
        UiCommand::OpenNvcleanstall => in_background(cli, events, |_, events| {
            events.send(BackendEvent::JobStarted);
            drivers::open_nvcleanstall(&events);
            events.send(BackendEvent::JobFinished);
        }),
        UiCommand::RemoveBrowser => in_background(cli, events, |_, _| login::uninstall_chrome()),
        UiCommand::LoadProfile => in_background(cli, events, |cli, events| {
            events.send(BackendEvent::ProfileLoaded(cli.profile()));
        }),
        UiCommand::SetOfflineMode(offline) => cli.set_offline(offline),
        UiCommand::RemoveApp => in_background(cli, events, |cli, events| {
            service::unregister();
            updater::delete_own_exe_after_exit();
            factory_reset(&cli, &events);
        }),
        UiCommand::LaunchExternal(_) | UiCommand::KillExternal(_) | UiCommand::LoadArt(_) => {}
    }
}

fn reload(cli: &Legendary, events: &Events) {
    events.send(BackendEvent::LibraryLoaded(cli.library()));
}

fn run_job(job: Job, cli: &Legendary, events: &Events, jobs: &Sender<Job>) {
    match job {
        Job::Startup => startup(cli, events, jobs),
        Job::AutoUpdate => logica::auto_update(cli, events),
        Job::UpdateLegendary => match cli.replace_with_latest() {
            Ok(()) => events.log(LogMsg::LegendaryUpdated),
            Err(_) => events.log(LogMsg::LegendaryUpdateFailed),
        },
        Job::Run { args, reload: should_reload } => {
            let completed = cli.run(&args, events);
            if completed && args.first().is_some_and(|command| command == "install") {
                if let Some(app_name) = args.get(1) {
                    events.send(BackendEvent::InstallFinished(app_name.clone()));
                }
            }
            if should_reload {
                reload(cli, events);
            }
        }
        Job::Uninstall(app_name) => {
            let root = cli.install_path(&app_name);
            if let Some(root) = &root {
                process::kill_game(root);
            }
            cli.run(&args(&["uninstall", &app_name, "-y"]), events);
            let leftover = root.filter(|_| cli.install_path(&app_name).is_none());
            if let Some(root) = leftover.filter(|root| !process::remove_leftover_folder(root)) {
                events.log(LogMsg::FolderNotRemoved { path: root.display().to_string() });
            }
            reload(cli, events);
        }
        Job::DeleteGhost(path) => {
            let folder = std::path::Path::new(&path);
            let holds_game = cli.installed().iter().any(|game| process::is_inside(&game.install_path, folder));
            if !holds_game && process::remove_leftover_folder(folder) {
                events.log(LogMsg::FolderDeleted { path });
            } else {
                events.log(LogMsg::FolderNotRemoved { path });
            }
        }
        Job::InstallDlcs(dlcs) => {
            for dlc in dlcs {
                if !cli.run(&args(&["install", &dlc, "-y"]), events) {
                    break;
                }
            }
        }
        Job::UninstallDlcs(dlcs) => {
            for dlc in dlcs {
                if !cli.run(&args(&["uninstall", &dlc, "-y"]), events) {
                    break;
                }
            }
        }
        Job::Update(app_name) => {
            if !cli.has_update(&app_name) {
                events.log(LogMsg::AlreadyUpToDate);
                return;
            }
            let mut update = args(&["update", &app_name, "--skip-dlcs", "-y"]);
            update.extend(cli.selection_args(&app_name));
            cli.run(&update, events);
        }
        Job::Repair(app_name) => {
            let mut repair = args(&["repair", &app_name, "--skip-dlcs", "-y"]);
            repair.extend(cli.selection_args(&app_name));
            cli.run(&repair, events);
        }
        Job::InstallLanguage { app_name, tag } => {
            // `repair` porque con la misma versión del juego `install` no baja nada nuevo.
            let mut repair = args(&["repair", &app_name, "--skip-dlcs", "-y"]);
            for tag in logica::tags_after_adding(&cli.installed_tags(&app_name), &tag) {
                repair.push("--install-tag".to_string());
                repair.push(tag);
            }
            cli.run(&repair, events);
        }
        Job::Import(games) => {
            import_games(games, cli, events);
            check_for_updates(cli, events);
            logica::auto_update(cli, events);
        }
        Job::Launch { app_name, language } => {
            let mut update = args(&["update", &app_name, "--skip-dlcs", "-y"]);
            update.extend(cli.selection_args(&app_name));
            if !cli.has_update(&app_name) || cli.run(&update, events) {
                in_background(cli, events, move |cli, events| {
                    start_game(&cli, &app_name, language.as_deref(), &events)
                });
            } else {
                events.send(BackendEvent::GameExited(app_name));
            }
        }
    }
}

/// Revisa el launcher y legendary. Si hay novedades, la interfaz pregunta al usuario.
fn check_for_updates(cli: &Legendary, events: &Events) {
    updater::check(events);
    if cli.version().is_some_and(|version| updater::newer_legendary_release(&version)) {
        events.send(BackendEvent::LegendaryUpdateOffered);
    }
}

fn log_in(cli: &Legendary, events: &Events) {
    let browser = match login::find_or_install_browser(events) {
        Ok(browser) => browser,
        Err(failure) => {
            events.log(LogMsg::LoginFailed(failure));
            return;
        }
    };
    match login::wait_for_code(cli, events, &browser.path) {
        Ok(code) => {
            events.log(LogMsg::CodeReceived);
            if cli.authenticate(&code) {
                events.log(LogMsg::AuthFinished);
            } else {
                events.log(LogMsg::LoginFailed(LoginFailure::Rejected));
            }
        }
        Err(failure) => events.log(LogMsg::LoginFailed(failure)),
    }
    if browser.installed_by_launcher {
        events.send(BackendEvent::AskRemoveBrowser);
    }
}

fn startup(cli: &Legendary, events: &Events, jobs: &Sender<Job>) {
    if cli.is_offline() {
        events.log(LogMsg::LoadingLocalLibrary);
        reload(cli, events);
        return;
    }
    events.log(LogMsg::CheckingSession);
    let games = cli.library_checking_session().unwrap_or_else(|| {
        events.log(LogMsg::NoSessionOpeningBrowser);
        log_in(cli, events);
        cli.library()
    });
    events.log(LogMsg::LoadingLocalLibrary);
    events.send(BackendEvent::LibraryLoaded(games));

    // Recorrer los discos puede tardar: no debe retener la cola de tareas.
    events.log(LogMsg::SearchingNewGames);
    let (jobs, scan_events) = (jobs.clone(), events.clone());
    thread::spawn(move || {
        scan_events.send(BackendEvent::JobStarted);
        let _ = jobs.send(Job::Import(import::find_epic_games()));
        scan_events.send(BackendEvent::JobFinished);
    });
}

fn import_games(found: Vec<(String, String)>, cli: &Legendary, events: &Events) {
    let installed = cli.installed();
    let new_games: Vec<_> = found
        .into_iter()
        .filter(|(app, _)| !installed.iter().any(|game| game.app_name == *app))
        .collect();
    if new_games.is_empty() {
        events.log(LogMsg::LibraryUpToDate);
        return;
    }
    let mut imported_any = false;
    for (app, path) in new_games {
        events.log(LogMsg::Importing { app: app.clone(), path: path.clone() });
        match cli.import_game(&app, &path, events) {
            ImportOutcome::Finished => imported_any = true,
            ImportOutcome::NoGameFiles => {
                events.send(BackendEvent::GhostGameFound { app_name: app, path });
            }
        }
    }
    if imported_any {
        reload(cli, events);
        events.log(LogMsg::LibraryUpdated);
    } else {
        events.log(LogMsg::LibraryUpToDate);
    }
}

fn start_game(cli: &Legendary, app_name: &str, language: Option<&str>, events: &Events) {
    let Some(root) = cli.install_path(app_name) else {
        events.log(LogMsg::GameFailedToStart { app: app_name.to_string() });
        events.send(BackendEvent::GameExited(app_name.to_string()));
        return;
    };
    events.send(BackendEvent::GameStarted(app_name.to_string()));

    let mut launch = args(&["launch", app_name, "-y"]);
    if cli.is_offline() {
        launch.push("--offline".to_string());
    }
    if let Some(language) = language {
        // `-epiclocale` (lo añade legendary) no cambia el idioma en todos los juegos;
        // `-culture` es el argumento propio de Unreal Engine.
        launch.extend(args(&["--language", language, &format!("-culture={language}")]));
    }
    in_background(cli, events, move |cli, events| cli.run_untracked(&launch, &events));

    if !process::wait_for_exit(&root) {
        events.log(LogMsg::GameFailedToStart { app: app_name.to_string() });
    }
    events.send(BackendEvent::GameExited(app_name.to_string()));
}

fn factory_reset(cli: &Legendary, events: &Events) {
    events.log(LogMsg::FactoryResetStart);
    cli.begin_shutdown();
    service::stop();

    if let Some(home) = dirs::home_dir() {
        let _ = std::fs::remove_dir_all(home.join(".config").join("legendary"));
    }
    if let Some(dir) = config::dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join("rootbound_launcher_profile"));

    if let Some(dir) = legendary::install_dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
    thread::sleep(Duration::from_secs(2));
    events.send(BackendEvent::ResetComplete);
}
