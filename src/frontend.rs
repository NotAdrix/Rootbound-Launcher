use crate::types::{BackendEvent, GameInfo, Language, UiCommand};
use eframe::egui;
use std::sync::mpsc::{Receiver, Sender};

#[derive(PartialEq)]
enum Tab {
    Installed,
    Library,
    Settings,
}

#[derive(Default)]
pub struct DownloadProgress {
    pub pct: String,
    pub dl_speed: String,
    pub read_speed: String,
    pub eta: String,
}

pub struct LauncherApp {
    tab: Tab,
    games: Vec<GameInfo>,
    running_game: Option<String>,
    tx_cmd: Sender<UiCommand>,
    rx_evt: Receiver<BackendEvent>,
    logs: String,
    show_terminal: bool,
    progress: Option<DownloadProgress>,
    is_paused: bool,

    show_reset_popup: bool,
    reset_delete_games: bool,
    is_resetting: bool,
    reset_complete: bool,
    reset_timer: Option<std::time::Instant>,

    auto_launch_cfg: Option<String>,
    language: Language,
    dark_mode: bool,

    install_app_name: String,
    install_path: String,
    install_dlcs: bool,
    install_lang: String,
    show_install_popup: bool,

    is_loading: bool,
    has_auto_launched: bool,
    update_status: String,
}

impl LauncherApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        tx_cmd: Sender<UiCommand>,
        rx_evt: Receiver<BackendEvent>,
        config: crate::backend::Config,
    ) -> Self {
        let default_path = dirs::data_dir()
            .map(|p| p.join("Legendary").to_string_lossy().to_string())
            .unwrap_or_else(|| "C:\\Legendary".to_string());

        if config.dark_mode {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
        } else {
            cc.egui_ctx.set_visuals(egui::Visuals::light());
        }

        Self {
            tab: Tab::Installed,
            games: Vec::new(),
            running_game: None,
            tx_cmd,
            rx_evt,
            logs: "Bienvenido a Rootbound Launcher.\n".to_string(),
            show_terminal: true,
            progress: None,
            is_paused: false,
            show_reset_popup: false,
            reset_delete_games: false,
            is_resetting: false,
            reset_complete: false,
            reset_timer: None,
            auto_launch_cfg: config.auto_launch_game,
            language: config.language,
            dark_mode: config.dark_mode,
            install_app_name: String::new(),
            install_path: default_path,
            install_dlcs: true,
            install_lang: "en".to_string(),
            show_install_popup: false,
            is_loading: true,
            has_auto_launched: false,
            update_status: "".to_string(),
        }
    }

    fn t(&self, es: &str, en: &str) -> String {
        match self.language {
            Language::Es => es.to_string(),
            Language::En => en.to_string(),
        }
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.dark_mode && !ctx.style().visuals.dark_mode {
            ctx.set_visuals(egui::Visuals::dark());
        } else if !self.dark_mode && ctx.style().visuals.dark_mode {
            ctx.set_visuals(egui::Visuals::light());
        }

        while let Ok(event) = self.rx_evt.try_recv() {
            match event {
                BackendEvent::Log(line) => {
                    let mut is_progress_info = false;

                    if line.contains("Progress:") {
                        if self.progress.is_none() {
                            self.progress = Some(DownloadProgress::default());
                        }
                        if let Some(p) = &mut self.progress {
                            if let Some(start) = line.find("Progress: ") {
                                if let Some(end) = line[start + 10..].find('%') {
                                    p.pct = line[start + 10..start + 10 + end + 1].to_string();
                                }
                            }
                            if let Some(start) = line.find("ETA: ") {
                                p.eta = line[start + 5..].trim().to_string();
                            }
                        }
                        is_progress_info = true;
                    } else if line.contains("+ Download") {
                        if let Some(p) = &mut self.progress {
                            if let Some(start) = line.find("- ") {
                                p.dl_speed = line[start + 2..].trim().to_string();
                            }
                        }
                        is_progress_info = true;
                    } else if line.contains("+ Disk") {
                        if let Some(p) = &mut self.progress {
                            if let Some(start) = line.find("- ") {
                                p.read_speed = line[start + 2..].trim().to_string();
                            }
                        }
                        is_progress_info = true;
                    } else if line.contains("[DLManager]") {
                        is_progress_info = true;
                    }

                    if !is_progress_info {
                        self.logs.push_str(&line);
                        self.logs.push('\n');
                    }
                }
                BackendEvent::LibraryLoaded(games) => {
                    self.games = games;
                    self.progress = None;
                    self.is_loading = false;
                    self.is_paused = false;

                    if !self.has_auto_launched {
                        if let Some(ref auto_game) = self.auto_launch_cfg {
                            if self
                                .games
                                .iter()
                                .any(|g| g.app_name == *auto_game && g.is_installed)
                            {
                                self.launch_game(auto_game.clone());
                            }
                        }
                        self.has_auto_launched = true;
                    }
                }
                BackendEvent::DownloadPaused => {
                    self.is_paused = true;
                }
                BackendEvent::DownloadFinished => {
                    self.progress = None;
                    self.is_loading = false;
                    self.is_paused = false;
                }
                BackendEvent::UpdateStatus(status) => self.update_status = status,
                BackendEvent::GameStarted(g) => self.running_game = Some(g),
                BackendEvent::GameExited(g) => {
                    self.running_game = None;
                    self.logs.push_str(&format!("Juego cerrado: {}\n", g));
                }
                BackendEvent::Error(e) => self.logs.push_str(&format!("ERROR: {}\n", e)),
                BackendEvent::Status(s) => self.logs.push_str(&format!("INFO: {}\n", s)),
                BackendEvent::ResetComplete => {
                    self.is_resetting = false;
                    self.show_reset_popup = false;
                    self.reset_complete = true;
                    self.reset_timer = Some(std::time::Instant::now());
                }
            }
        }

        if self.reset_complete {
            let win_title = self.t("Reinicio Completado", "Reset Complete");
            egui::Window::new(win_title)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new(self.t(
                                "Todos los datos han sido eliminados.",
                                "All data has been deleted.",
                            ))
                            .size(16.0)
                            .strong(),
                        );
                        ui.add_space(20.0);
                        ui.spinner();
                        ui.add_space(10.0);
                        ui.label(self.t(
                            "Cerrando aplicación para aplicar cambios...",
                            "Closing app to apply changes...",
                        ));
                        ui.add_space(10.0);
                    });
                });

            if let Some(timer) = self.reset_timer {
                if timer.elapsed() > std::time::Duration::from_millis(2500) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }

        // ── PANEL IZQUIERDO ──
        egui::SidePanel::left("sidebar")
            .resizable(false)
            .exact_width(220.0)
            .show(ctx, |ui| {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("ROOTBOUND")
                            .size(32.0)
                            .strong()
                            .color(egui::Color32::from_rgb(100, 150, 255)),
                    );
                    ui.label(
                        egui::RichText::new("LAUNCHER")
                            .size(20.0)
                            .strong()
                            .color(egui::Color32::GRAY),
                    );
                });

                ui.add_space(50.0);

                let text_installed = self.t("🎮 Juegos Instalados", "🎮 Installed Games");
                let text_library = self.t("📚 Librería", "📚 Library");
                let text_settings = self.t("⚙ Ajustes", "⚙ Settings");

                if ui
                    .add_sized(
                        [200.0, 45.0],
                        egui::SelectableLabel::new(self.tab == Tab::Installed, text_installed),
                    )
                    .clicked()
                {
                    self.tab = Tab::Installed;
                }
                ui.add_space(5.0);
                if ui
                    .add_sized(
                        [200.0, 45.0],
                        egui::SelectableLabel::new(self.tab == Tab::Library, text_library),
                    )
                    .clicked()
                {
                    self.tab = Tab::Library;
                }
                ui.add_space(5.0);
                if ui
                    .add_sized(
                        [200.0, 45.0],
                        egui::SelectableLabel::new(self.tab == Tab::Settings, text_settings),
                    )
                    .clicked()
                {
                    self.tab = Tab::Settings;
                }
            });

        // ── PANEL DERECHO (Terminal) ──
        if self.show_terminal {
            egui::SidePanel::right("terminal")
                .resizable(true)
                .default_width(350.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Terminal").strong());
                        if ui.button("❌").clicked() {
                            self.show_terminal = false;
                        }
                    });
                    ui.separator();

                    if let Some(p) = &self.progress {
                        ui.group(|ui| {
                            ui.vertical(|ui| {
                                ui.add_space(5.0);
                                ui.label(egui::RichText::new("Progreso:").strong());
                                ui.label(
                                    egui::RichText::new(&p.pct)
                                        .size(24.0)
                                        .color(egui::Color32::LIGHT_GREEN),
                                );
                                ui.add_space(10.0);
                                ui.label(egui::RichText::new("Velocidad de descarga:").strong());
                                ui.label(&p.dl_speed);
                                ui.add_space(10.0);
                                ui.label(egui::RichText::new("Velocidad de lectura:").strong());
                                ui.label(&p.read_speed);
                                ui.add_space(10.0);
                                ui.label(egui::RichText::new("Tiempo Estimado:").strong());
                                ui.label(&p.eta);
                                ui.add_space(10.0);

                                ui.horizontal(|ui| {
                                    if self.is_paused {
                                        if ui.button("▶ Reanudar").clicked() {
                                            self.tx_cmd.send(UiCommand::ResumeDownload).unwrap();
                                            self.is_paused = false;
                                        }
                                    } else {
                                        if ui.button("⏸ Pausar").clicked() {
                                            self.tx_cmd.send(UiCommand::PauseDownload).unwrap();
                                        }
                                    }
                                    if ui.button("❌ Cancelar").clicked() {
                                        self.tx_cmd.send(UiCommand::CancelDownload).unwrap();
                                    }
                                });
                                ui.add_space(5.0);
                            });
                        });
                        ui.add_space(10.0);
                        ui.separator();
                    }

                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.logs.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .interactive(false),
                            );
                        });
                });
        }

        // ── BARRA SUPERIOR ──
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(" ");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.show_terminal {
                        let btn_text = self.t("Abrir Terminal", "Open Terminal");
                        if ui.button(btn_text).clicked() {
                            self.show_terminal = true;
                        }
                    }
                });
            });
        });

        // ── PANEL CENTRAL ──
        egui::CentralPanel::default().show(ctx, |ui| {
            // Botón de recarga en la parte superior
            ui.horizontal(|ui| {
                ui.label(" ");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let reload_btn = self.t("🔄 Actualizar", "🔄 Refresh");
                    if ui.button(reload_btn).clicked() {
                        self.tx_cmd.send(UiCommand::ReloadLibrary).unwrap();
                        self.is_loading = true;
                    }
                });
            });
            ui.separator();

            // Aquí empieza el match de las pestañas
            match self.tab {
                Tab::Installed => {
                    ui.heading(self.t("Juegos Instalados", "Installed Games"));
                    ui.add_space(10.0);

                    if self.is_loading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(self.t("Buscando Juegos...", "Searching Games..."));
                        });
                    } else {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            let installed: Vec<_> = self.games.iter().filter(|g| g.is_installed).cloned().collect();
                            for game in installed {
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new(&game.title).size(16.0).strong());

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.menu_button("...", |ui| {
                                                if ui.button(self.t("Instalar DLCs", "Install DLCs")).clicked() {
                                                    self.tx_cmd.send(UiCommand::InstallDlc(game.app_name.clone())).unwrap();
                                                    ui.close_menu();
                                                }
                                                if ui.button(self.t("Verificar Archivos", "Verify Files")).clicked() {
                                                    self.tx_cmd.send(UiCommand::VerifyGame(game.app_name.clone())).unwrap();
                                                    ui.close_menu();
                                                }
                                                if ui.button(self.t("Verificar Updates", "Verify Updates")).clicked() {
                                                    self.tx_cmd.send(UiCommand::VerifyUpdate(game.app_name.clone())).unwrap();
                                                    ui.close_menu();
                                                }
                                                ui.menu_button(self.t("Instalar Idioma", "Install Language"), |ui| {
                                                    for lang in ["en", "es", "fr", "de", "it"] {
                                                        if ui.button(lang).clicked() {
                                                            self.tx_cmd.send(UiCommand::InstallLang(game.app_name.clone(), lang.to_string())).unwrap();
                                                            ui.close_menu();
                                                        }
                                                    }
                                                });
                                                ui.separator();
                                                let del_text = self.t("❌ Desinstalar Juego", "❌ Uninstall Game");
                                                if ui.button(egui::RichText::new(del_text).color(egui::Color32::LIGHT_RED)).clicked() {
                                                    self.tx_cmd.send(UiCommand::UninstallGame(game.app_name.clone())).unwrap();
                                                    ui.close_menu();
                                                }
                                            });

                                            if self.running_game.as_ref() == Some(&game.app_name) {
                                                if ui.button(self.t("Cerrar Juego", "Close Game")).clicked() {
                                                    self.tx_cmd.send(UiCommand::KillGame(game.app_name.clone())).unwrap();
                                                }
                                                ui.label(egui::RichText::new("🟢 Ejecutando").color(egui::Color32::GREEN));
                                            } else {
                                                if ui.button(self.t("▶ Jugar", "▶ Play")).clicked() {
                                                    if self.running_game.is_some() {
                                                        self.tx_cmd.send(UiCommand::KillGame(self.running_game.clone().unwrap())).unwrap();
                                                    }
                                                    self.tx_cmd.send(UiCommand::LaunchGame(game.app_name.clone())).unwrap();
                                                }
                                            }
                                        });
                                    });

                                    let mut is_auto = self.auto_launch_cfg.as_ref() == Some(&game.app_name);
                                    let cb_text = self.t("Ejecutar automáticamente siempre", "Auto-launch always");
                                    if ui.checkbox(&mut is_auto, cb_text).changed() {
                                        if is_auto {
                                            self.auto_launch_cfg = Some(game.app_name.clone());
                                            self.tx_cmd.send(UiCommand::SetAutoLaunch(Some(game.app_name.clone()))).unwrap();
                                        } else {
                                            self.auto_launch_cfg = None;
                                            self.tx_cmd.send(UiCommand::SetAutoLaunch(None)).unwrap();
                                        }
                                    }
                                });
                                ui.add_space(5.0);
                            }
                        });
                    }
                }

                Tab::Library => {
                    ui.heading(self.t("Librería (No Instalados)", "Library (Not Installed)"));
                    ui.add_space(10.0);

                    if self.is_loading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(self.t("Buscando Juegos...", "Searching Games..."));
                        });
                    } else {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            let not_installed: Vec<_> = self.games.iter().filter(|g| !g.is_installed).cloned().collect();
                            for game in not_installed {
                                ui.horizontal(|ui| {
                                    ui.label(&game.title);
                                    if ui.button(self.t("⬇ Instalar", "⬇ Install")).clicked() {
                                        self.install_app_name = game.app_name.clone();
                                        self.show_install_popup = true;
                                    }
                                });
                                ui.add_space(12.0);
                            }
                        });
                    }
                }

                Tab::Settings => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.heading(self.t("Ajustes", "Settings"));
                        ui.add_space(20.0);

                        ui.label(self.t("Idioma de la interfaz:", "Interface language:"));
                        ui.horizontal(|ui| {
                            if ui.radio(self.language == Language::Es, "Español").clicked() {
                                self.language = Language::Es;
                                let mut cfg = crate::backend::load_config();
                                cfg.language = Language::Es;
                                crate::backend::save_config(&cfg);
                            }
                            if ui.radio(self.language == Language::En, "English").clicked() {
                                self.language = Language::En;
                                let mut cfg = crate::backend::load_config();
                                cfg.language = Language::En;
                                crate::backend::save_config(&cfg);
                            }
                        });

                        ui.add_space(20.0);
                        let cb_dark = self.t("Modo Oscuro", "Dark Mode");
                        if ui.checkbox(&mut self.dark_mode, cb_dark).changed() {
                            let mut cfg = crate::backend::load_config();
                            cfg.dark_mode = self.dark_mode;
                            crate::backend::save_config(&cfg);
                        }

                        ui.add_space(30.0);
                        ui.separator();
                        ui.add_space(15.0);

                        let upd_title = self.t("Actualizaciones del Launcher", "Launcher Updates");
                        ui.label(egui::RichText::new(upd_title).strong());
                        ui.add_space(10.0);
                        let btn_update = self.t("Buscar Actualizaciones", "Check for Updates");
                        if ui.button(btn_update).clicked() {
                            self.tx_cmd.send(UiCommand::CheckForUpdates).unwrap();
                        }
                        if !self.update_status.is_empty() {
                            ui.add_space(5.0);
                            ui.label(egui::RichText::new(&self.update_status).color(egui::Color32::LIGHT_BLUE));
                        }

                        ui.add_space(30.0);
                        ui.separator();
                        ui.add_space(15.0);

                        let danger_title = self.t("Zona de Peligro", "Danger Zone");
                        ui.label(egui::RichText::new(danger_title).strong().color(egui::Color32::RED));
                        ui.add_space(5.0);
                        let danger_text = self.t(
                            "Restablece el launcher a su estado original. Borrará sesiones, configuración y dependencias.",
                            "Resets the launcher to its original state. Will delete sessions, config, and dependencies."
                        );
                        ui.label(egui::RichText::new(danger_text).small());
                        ui.add_space(10.0);

                        let reset_btn_text = self.t("⚠ Restablecer todo", "⚠ Factory Reset");
                        let reset_btn = egui::Button::new(egui::RichText::new(reset_btn_text).color(egui::Color32::WHITE)).fill(egui::Color32::DARK_RED);

                        if ui.add(reset_btn).clicked() {
                            self.show_reset_popup = true;
                        }
                    });
                }
            }
        });

        // ── VENTANA MODAL DE INSTALACIÓN ──
        if self.show_install_popup {
            let win_title = self.t("Opciones de Instalación", "Install Options");
            egui::Window::new(win_title)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    let label_path = self.t("Carpeta de instalación:", "Install folder:");
                    ui.label(label_path);

                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.install_path);
                        if ui.button("📁").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.install_path = path.to_string_lossy().to_string();
                            }
                        }
                    });

                    let label_dlcs = self.t("Incluir DLCs", "Include DLCs");
                    ui.checkbox(&mut self.install_dlcs, label_dlcs);

                    let label_lang = self.t("Idioma del juego:", "Game language:");
                    ui.label(label_lang);

                    egui::ComboBox::from_id_source("lang_combo")
                        .selected_text(&self.install_lang)
                        .show_ui(ui, |ui| {
                            for lang in ["en", "es", "fr", "de", "it", "ru", "pt", "ja", "zh"] {
                                ui.selectable_value(&mut self.install_lang, lang.to_string(), lang);
                            }
                        });

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let btn_start = self.t("Iniciar Descarga", "Start Download");
                        if ui.button(btn_start).clicked() {
                            self.tx_cmd
                                .send(UiCommand::InstallGame {
                                    app_name: self.install_app_name.clone(),
                                    path: self.install_path.clone(),
                                    dlcs: self.install_dlcs,
                                    lang: self.install_lang.clone(),
                                })
                                .unwrap();
                            self.show_install_popup = false;
                            self.tab = Tab::Installed;
                        }
                        let btn_cancel = self.t("Cancelar", "Cancel");
                        if ui.button(btn_cancel).clicked() {
                            self.show_install_popup = false;
                        }
                    });
                });
        }

        // ── VENTANA MODAL DE CONFIRMACIÓN DE RESET ──
        if self.show_reset_popup {
            let win_title = self.t("Confirmar Restablecimiento", "Confirm Reset");
            egui::Window::new(win_title)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    let warn_text = self.t(
                        "¿Estás seguro? Esto borrará tu sesión de Epic, la configuración y desinstalará Legendary de Python.",
                        "Are you sure? This will delete your Epic session, configuration, and uninstall Legendary from Python."
                    );
                    ui.label(warn_text);
                    ui.add_space(10.0);

                    let cb_text = self.t(
                        "También borrar juegos de la lista (NO borra los archivos del disco).",
                        "Also remove games from list (DOES NOT delete files from disk)."
                    );
                    ui.checkbox(&mut self.reset_delete_games, cb_text);

                    ui.add_space(15.0);
                    ui.horizontal(|ui| {
                        if self.is_resetting {
                            ui.spinner();
                            ui.label(self.t("Restableciendo...", "Resetting..."));
                        } else {
                            let btn_yes = self.t("Sí, borrar todo", "Yes, delete everything");
                            if ui.button(egui::RichText::new(btn_yes).color(egui::Color32::RED)).clicked() {
                                self.is_resetting = true;
                                self.tx_cmd.send(UiCommand::FactoryReset(self.reset_delete_games)).unwrap();
                            }
                            let btn_cancel = self.t("Cancelar", "Cancel");
                            if ui.button(btn_cancel).clicked() {
                                self.show_reset_popup = false;
                            }
                        }
                    });
                });
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

impl LauncherApp {
    fn launch_game(&mut self, game: String) {
        self.tx_cmd
            .send(UiCommand::LaunchGame(game.clone()))
            .unwrap();
        self.tab = Tab::Installed;
        self.running_game = Some(game);
    }
}
