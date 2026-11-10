mod ajustes;
pub(crate) mod modals;

use crate::config::{self, Config};
use crate::i18n;
use crate::juegos::interfaz;
use crate::juegos::logica::{
    self, DlcDialog, ExternalDraft, GameInfo, LanguageDialog, LaunchLanguageDialog, Profile, Section,
};
use crate::types::{BackendEvent, LogMsg, Progress, UiCommand, UpdateStatus};
use eframe::egui::{self, Color32, FontFamily, FontId, RichText, TextStyle};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_LOG_LINES: usize = 500;
const LOGO_SIZE: f32 = 64.0;
const SEMIBOLD_FAMILY: &str = "semibold";
/// (nombre interno, archivo en C:\\Windows\\Fonts)
const BODY_FONT: (&str, &str) = ("segoe-ui", "segoeui.ttf");
const SEMIBOLD_FONT: (&str, &str) = ("segoe-ui-semibold", "seguisb.ttf");
const MONO_FONT: (&str, &str) = ("consolas", "consola.ttf");
const PANEL_FILL: Color32 = Color32::from_rgba_premultiplied(17, 17, 19, 220);
pub(crate) const BLUE: Color32 = Color32::from_rgb(35, 85, 135);
pub(crate) const RED: Color32 = Color32::from_rgb(180, 50, 50);
pub(crate) const DARK_CYAN: Color32 = Color32::from_rgb(24, 104, 132);

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum View {
    Library(Section),
    Stats,
    Profile,
    Drivers,
    Settings,
}

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum ConfirmAction {
    FactoryReset,
    RemoveApp,
    CleanDrivers,
}

#[derive(PartialEq, Clone)]
pub(crate) enum PendingUpdate {
    Launcher(String),
    Legendary,
}

pub(crate) struct InstallDialog {
    pub game: GameInfo,
    pub path: String,
    pub dlcs: bool,
    pub language: String,
}

pub struct LauncherApp {
    pub(crate) view: View,
    pub(crate) epic_games: Vec<GameInfo>,
    pub(crate) games: Vec<GameInfo>,
    pub(crate) library_version: u64,
    pub(crate) visible: logica::VisibleCache,
    pub(crate) running_game: Option<String>,
    pub(crate) search: String,
    pub(crate) art: HashMap<String, egui::TextureHandle>,
    pub(crate) art_requested: HashSet<String>,
    tx_cmd: Sender<UiCommand>,
    rx_evt: Receiver<BackendEvent>,
    logs: VecDeque<LogMsg>,
    /// Texto ya traducido de cada línea: se genera al llegar, no en cada fotograma.
    log_text: VecDeque<String>,
    log_language: crate::types::Language,
    pub(crate) show_terminal: bool,
    progress: Option<Progress>,
    pub(crate) is_paused: bool,
    logo_texture: Option<egui::TextureHandle>,
    bg_texture: Option<egui::TextureHandle>,
    pub(crate) confirm: Option<ConfirmAction>,
    pub(crate) is_resetting: bool,
    reset_finished_at: Option<Instant>,
    pub(crate) language_dialog: Option<LanguageDialog>,
    pub(crate) launch_language_dialog: Option<LaunchLanguageDialog>,
    pub(crate) dlc_dialog: Option<DlcDialog>,
    pub(crate) install_dialog: Option<InstallDialog>,
    pub(crate) add_external_dialog: Option<ExternalDraft>,
    pub(crate) config: Config,
    pub(crate) is_loading: bool,
    busy_jobs: usize,
    pub(crate) installing: Option<String>,
    pub(crate) ask_remove_browser: bool,
    pub(crate) has_nvidia: bool,
    tray: Option<Arc<crate::tray::TrayState>>,
    quitting: bool,
    pub(crate) ghost_games: VecDeque<(String, String)>,
    pub(crate) playtime: HashMap<String, u64>,
    pub(crate) profile: Option<Profile>,
    pub(crate) pending_updates: VecDeque<PendingUpdate>,
    pub(crate) has_auto_launched: bool,
    pub(crate) update_status: Option<UpdateStatus>,
}

fn load_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<egui::TextureHandle> {
    let rgba = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    Some(ctx.load_texture(name, image, egui::TextureOptions::LINEAR))
}

fn read_system_font(name: &str, file: &str, fonts: &mut egui::FontDefinitions) -> bool {
    let folder = std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let Ok(bytes) = std::fs::read(folder.join("Fonts").join(file)) else {
        return false;
    };
    fonts.font_data.insert(name.to_owned(), egui::FontData::from_owned(bytes));
    true
}

/// Las fuentes de Windows están pensadas para esta pantalla; las de egui son finas y
/// se ven borrosas a tamaños pequeños. Si faltan, se siguen usando las de egui.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut proportional = fonts.families[&FontFamily::Proportional].clone();
    if read_system_font(BODY_FONT.0, BODY_FONT.1, &mut fonts) {
        proportional.insert(0, BODY_FONT.0.to_owned());
    }
    let mut semibold = proportional.clone();
    if read_system_font(SEMIBOLD_FONT.0, SEMIBOLD_FONT.1, &mut fonts) {
        semibold.insert(0, SEMIBOLD_FONT.0.to_owned());
    }
    if read_system_font(MONO_FONT.0, MONO_FONT.1, &mut fonts) {
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, MONO_FONT.0.to_owned());
    }
    fonts.families.insert(FontFamily::Proportional, proportional);
    fonts.families.insert(FontFamily::Name(SEMIBOLD_FAMILY.into()), semibold);
    ctx.set_fonts(fonts);
}

/// Texto de títulos y etiquetas importantes.
pub(crate) fn title(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text.into())
        .font(FontId::new(size, FontFamily::Name(SEMIBOLD_FAMILY.into())))
        .strong()
}

fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgba_premultiplied(20, 20, 22, 200);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(40, 40, 45);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(50, 50, 60);
    visuals.widgets.active.bg_fill = BLUE;
    visuals.selection.bg_fill = BLUE;
    ctx.set_visuals(visuals);
}

/// Se escala una vez al tamaño exacto en pantalla: egui no filtra bien al reducir
/// una textura grande, y por eso el logo se veía pixelado.
fn load_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let scale = ctx.native_pixels_per_point().unwrap_or(1.0);
    let side = (LOGO_SIZE * scale).round() as u32;
    let rgba = image::load_from_memory(crate::LOGO_PNG)
        .ok()?
        .resize_exact(side, side, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let image = egui::ColorImage::from_rgba_unmultiplied([side as usize; 2], rgba.as_raw());
    Some(ctx.load_texture("logo", image, egui::TextureOptions::LINEAR))
}

pub(crate) fn card(rounding: f32, margin: f32) -> egui::Frame {
    egui::Frame::none()
        .stroke(egui::Stroke::new(
            1.0_f32,
            Color32::from_rgba_premultiplied(20, 20, 20, 20),
        ))
        .rounding(egui::Rounding::same(rounding))
        .inner_margin(egui::Margin::same(margin))
}

pub(crate) fn filled_button(ui: &mut egui::Ui, text: &str, fill: Color32) -> bool {
    let label = RichText::new(text).color(Color32::WHITE);
    ui.add(egui::Button::new(label).fill(fill)).clicked()
}

pub(crate) const ICON_BUTTON_SIZE: f32 = 34.0;

pub(crate) fn icon_button(
    ui: &mut egui::Ui,
    tooltip: &str,
    draw: impl FnOnce(&egui::Painter, egui::Rect, Color32),
) -> bool {
    let size = egui::Vec2::splat(ICON_BUTTON_SIZE);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let fill = if response.hovered() {
        Color32::from_rgb(62, 62, 72)
    } else {
        Color32::from_rgb(45, 45, 52)
    };
    ui.painter().rect_filled(rect, 6.0, fill);
    draw(ui.painter(), rect, Color32::LIGHT_GRAY);
    response.on_hover_text(tooltip).clicked()
}

fn draw_terminal_icon(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    let stroke = egui::Stroke::new(1.5_f32, color);
    let frame = rect.shrink(8.0);
    painter.rect_stroke(frame, 2.0, stroke);
    let start = frame.left_center();
    let tip = start + egui::vec2(7.0, 0.0);
    painter.line_segment([start + egui::vec2(3.5, -3.5), tip], stroke);
    painter.line_segment([start + egui::vec2(3.5, 3.5), tip], stroke);
    let underscore_y = start.y + 4.5;
    painter.line_segment(
        [
            egui::pos2(frame.center().x + 1.0, underscore_y),
            egui::pos2(frame.right() - 3.0, underscore_y),
        ],
        stroke,
    );
}

/// Aviso para los momentos en los que se está trabajando sin progreso que mostrar.
pub(crate) fn busy_indicator(app: &LauncherApp, ui: &mut egui::Ui) {
    if app.is_loading || (app.busy_jobs > 0 && app.progress.is_none()) {
        ui.spinner();
        ui.label(RichText::new(app.t("please_wait")).color(Color32::GRAY));
    }
}

/// Abre y cierra la terminal desde la esquina superior derecha.
pub(crate) fn terminal_button(app: &mut LauncherApp, ui: &mut egui::Ui) {
    if icon_button(ui, app.t("terminal"), draw_terminal_icon) {
        app.set_terminal_visible(!app.show_terminal);
    }
}

impl LauncherApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        tx_cmd: Sender<UiCommand>,
        rx_evt: Receiver<BackendEvent>,
        config: Config,
    ) -> Self {
        let ctx = &cc.egui_ctx;
        ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(12.0, 12.0);
            style.spacing.button_padding = egui::vec2(12.0, 6.0);
            for (text_style, size, family) in [
                (TextStyle::Small, 12.5, FontFamily::Proportional),
                (TextStyle::Body, 15.0, FontFamily::Proportional),
                (TextStyle::Button, 15.0, FontFamily::Proportional),
                (TextStyle::Heading, 20.0, FontFamily::Proportional),
                (TextStyle::Monospace, 13.5, FontFamily::Monospace),
            ] {
                style.text_styles.insert(text_style, FontId::new(size, family));
            }
        });
        // Bordes más definidos: el suavizado por defecto ocupa un píxel entero.
        ctx.tessellation_options_mut(|options| options.feathering_size_in_pixels = 0.7);
        install_fonts(ctx);
        apply_theme(ctx);
        let language = config.language;

        let mut app = Self {
            view: View::Library(config.last_section),
            epic_games: logica::load_cached_library(),
            games: Vec::new(),
            library_version: 0,
            visible: logica::VisibleCache::default(),
            running_game: None,
            search: String::new(),
            art: HashMap::new(),
            art_requested: HashSet::new(),
            tx_cmd,
            rx_evt,
            logs: VecDeque::new(),
            log_text: VecDeque::new(),
            log_language: config.language,
            show_terminal: !config.terminal_hidden,
            progress: None,
            is_paused: false,
            logo_texture: load_logo(ctx),
            bg_texture: load_texture(ctx, "fondo", include_bytes!("../../monte.jpg")),
            confirm: None,
            is_resetting: false,
            reset_finished_at: None,
            language_dialog: None,
            launch_language_dialog: None,
            dlc_dialog: None,
            install_dialog: None,
            add_external_dialog: None,
            config,
            is_loading: true,
            busy_jobs: 0,
            installing: None,
            ask_remove_browser: false,
            has_nvidia: crate::drivers::has_nvidia_gpu(),
            tray: Some(crate::tray::spawn(ctx.clone(), language)),
            quitting: false,
            ghost_games: VecDeque::new(),
            playtime: logica::load_playtime(),
            profile: None,
            pending_updates: VecDeque::new(),
            has_auto_launched: false,
            update_status: None,
        };
        app.push_log(LogMsg::Welcome);
        // Los juegos externos no dependen del login de Epic: se ven desde el primer frame.
        app.rebuild_library();
        app
    }

    pub(crate) fn t(&self, key: &str) -> &'static str {
        i18n::t(self.config.language, key)
    }

    pub(crate) fn send(&self, command: UiCommand) {
        let _ = self.tx_cmd.send(command);
    }

    fn offer_update(&mut self, update: PendingUpdate) {
        if !self.pending_updates.contains(&update) {
            self.pending_updates.push_back(update);
        }
    }

    pub(crate) fn set_terminal_visible(&mut self, visible: bool) {
        self.show_terminal = visible;
        self.config.terminal_hidden = !visible;
        self.save_config();
    }

    fn push_log(&mut self, msg: LogMsg) {
        if msg == LogMsg::Separator && self.logs.back() == Some(&LogMsg::Separator) {
            return;
        }
        if self.logs.len() == MAX_LOG_LINES {
            self.logs.pop_front();
            self.log_text.pop_front();
        }
        self.log_text.push_back(i18n::render_log(self.log_language, &msg));
        self.logs.push_back(msg);
    }

    fn refresh_log_language(&mut self) {
        if self.log_language != self.config.language {
            self.log_language = self.config.language;
            self.log_text = self.logs.iter().map(|msg| i18n::render_log(self.log_language, msg)).collect();
        }
    }

    /// Activa o detiene RootboundBackground según los ajustes recién guardados.
    pub(crate) fn sync_background(&self) {
        std::thread::spawn(crate::service::sync);
    }

    pub(crate) fn save_config(&self) {
        config::save(&self.config);
    }

    pub(crate) fn game_title(&self, id: &str) -> String {
        self.games
            .iter()
            .find(|game| game.app_name == id)
            .map_or_else(|| id.to_string(), |game| game.title.clone())
    }

    fn handle_event(&mut self, ctx: &egui::Context, event: BackendEvent) {
        match event {
            BackendEvent::Log(msg) => self.push_log(msg),
            BackendEvent::LibraryLoaded(games) => {
                // Sin datos nuevos se conserva lo último conocido.
                if !games.is_empty() {
                    logica::save_cached_library(&games);
                    self.epic_games = games;
                }
                self.is_loading = false;
                self.rebuild_library();
                self.try_auto_launch();
            }
            BackendEvent::ArtLoaded { id, image } => {
                let texture = ctx.load_texture(&id, image, egui::TextureOptions::LINEAR);
                self.art.insert(id, texture);
            }
            BackendEvent::DlcsListed { app_name, dlcs } => self.receive_dlcs(&app_name, dlcs),
            BackendEvent::LanguagesListed { app_name, options } => {
                self.receive_languages(&app_name, options)
            }
            BackendEvent::Progress(progress) => self.progress = Some(progress),
            BackendEvent::DownloadPaused => self.is_paused = true,
            BackendEvent::DownloadCancelled => {
                self.progress = None;
                self.is_paused = false;
                self.installing = None;
            }
            BackendEvent::JobStarted => self.busy_jobs += 1,
            BackendEvent::InstallFinished(app_name) => {
                if self.installing.as_ref() == Some(&app_name) {
                    self.installing = None;
                }
            }
            BackendEvent::AskRemoveBrowser => self.ask_remove_browser = true,
            BackendEvent::GhostGameFound { app_name, path } => {
                self.ghost_games.push_back((app_name, path));
            }
            BackendEvent::JobFinished => {
                self.busy_jobs = self.busy_jobs.saturating_sub(1);
                if !self.is_paused {
                    self.progress = None;
                }
            }
            BackendEvent::Update(status) => {
                if let UpdateStatus::Available { url, .. } = &status {
                    self.offer_update(PendingUpdate::Launcher(url.clone()));
                }
                self.update_status = Some(status);
            }
            BackendEvent::LegendaryUpdateOffered => self.offer_update(PendingUpdate::Legendary),
            BackendEvent::ProfileLoaded(profile) => self.profile = Some(profile),
            BackendEvent::GameStarted(game) => self.running_game = Some(game),
            BackendEvent::GameExited(game) => {
                if self.running_game.as_ref() == Some(&game) {
                    self.running_game = None;
                }
                let app = self.game_title(&game);
                self.push_log(LogMsg::Separator);
                self.push_log(LogMsg::GameClosed { app });
            }
            BackendEvent::ResetComplete => {
                self.is_resetting = false;
                self.confirm = None;
                self.reset_finished_at = Some(Instant::now());
            }
        }
    }

    fn show_reset_finished(&mut self, ctx: &egui::Context, finished_at: Instant) {
        modals::centered(self.t("reset_complete")).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(15.0);
                ui.label(title(self.t("all_data_deleted"), 18.0));
                ui.add_space(20.0);
                ui.spinner();
                ui.add_space(15.0);
                ui.label(RichText::new(self.t("close_app")).color(Color32::GRAY));
                ui.add_space(15.0);
            });
        });
        if finished_at.elapsed() > Duration::from_millis(2500) {
            self.quitting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn show_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("sidebar")
            .resizable(false)
            .exact_width(220.0)
            .frame(egui::Frame::none().fill(PANEL_FILL))
            .show(ctx, |ui| {
                ui.add_space(22.0);
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    if let Some(tex) = &self.logo_texture {
                        ui.image((tex.id(), egui::Vec2::splat(LOGO_SIZE)));
                    }
                    ui.add_space(8.0);
                    ui.label(title("Rootbound Launcher", 19.0).color(Color32::WHITE));
                    ui.label(
                        RichText::new(crate::VERSION_LABEL)
                            .size(12.5)
                            .color(Color32::from_rgb(150, 150, 160)),
                    );
                });
                ui.add_space(18.0);
                let line_y = ui.cursor().top();
                let panel = ui.max_rect();
                ui.painter().hline(
                    (panel.left() + interfaz::SIDE_MARGIN)..=(panel.right() - interfaz::SIDE_MARGIN),
                    line_y,
                    egui::Stroke::new(1.0_f32, Color32::from_rgb(45, 45, 52)),
                );
                ui.add_space(18.0);
                interfaz::sidebar_navigation(self, ui);
                interfaz::sidebar_footer(self, ui);
            });
    }

    fn show_progress(&mut self, ui: &mut egui::Ui, progress: &Progress) {
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        let action = if progress.verifying { "verifying" } else { "downloading" };
                        ui.label(title(self.t(action), 15.0));
                        ui.label(
                            RichText::new(&progress.percent)
                                .size(24.0)
                                .color(Color32::from_rgb(80, 160, 255)),
                        );
                    });
                    ui.add_space(20.0);
                    ui.vertical(|ui| {
                        for (icon, value) in [
                            ("↓", &progress.download),
                            ("💿", &progress.disk),
                            ("⏱", &progress.eta),
                        ] {
                            if !value.is_empty() {
                                ui.label(RichText::new(format!("{icon} {value}")).small());
                            }
                        }
                    });
                });
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    let toggle = if self.is_paused { "resume" } else { "pause" };
                    if ui.button(self.t(toggle)).clicked() {
                        if self.is_paused {
                            self.is_paused = false;
                            self.send(UiCommand::ResumeDownload);
                        } else {
                            self.send(UiCommand::PauseDownload);
                        }
                    }
                    if ui.button(self.t("cancel")).clicked() {
                        self.send(UiCommand::CancelDownload);
                    }
                });
            });
        ui.add_space(10.0);
    }

    fn show_log(&self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 5.0;
        // Las líneas largas se parten al ancho de la terminal y sin barras visibles.
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                for (msg, text) in self.logs.iter().zip(&self.log_text) {
                    if *msg == LogMsg::Separator {
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(4.0);
                        continue;
                    }
                    ui.add(
                        egui::Label::new(RichText::new(text).monospace().color(Color32::LIGHT_GRAY))
                            .wrap(true),
                    );
                }
            });
    }

    fn show_terminal(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("terminal")
            .resizable(true)
            .default_width(380.0)
            .frame(
                egui::Frame::none()
                    .fill(PANEL_FILL)
                    .inner_margin(egui::Margin::same(15.0)),
            )
            .show(ctx, |ui| {
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.add_space(ui.available_width() / 2.0 - 40.0);
                    ui.label(title(self.t("terminal"), 15.0).color(Color32::GRAY));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if filled_button(ui, "X", RED) {
                            self.set_terminal_visible(false);
                        }
                    });
                });
                ui.separator();

                if let Some(progress) = self.progress.clone() {
                    self.show_progress(ui, &progress);
                }
                self.show_log(ui);
            });
    }

    fn show_central(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().inner_margin(egui::Margin::same(20.0)))
            .show(ctx, |ui| match self.view {
                View::Library(section) => interfaz::render_section(self, ui, section),
                View::Stats => interfaz::render_stats(self, ui),
                View::Profile => interfaz::render_profile(self, ui),
                View::Drivers => interfaz::render_drivers(self, ui),
                View::Settings => ajustes::render(self, ui),
            });
    }
}

impl LauncherApp {
    /// La X oculta la ventana y deja el launcher en la bandeja; solo se cierra de verdad
    /// desde "Cerrar" en la bandeja, el reinicio de fábrica o "Eliminar App".
    fn handle_tray(&mut self, ctx: &egui::Context) {
        if self.tray.as_ref().is_some_and(|tray| tray.should_quit()) {
            self.quitting = true;
        }
        if self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        if self.tray.as_ref().is_some_and(|tray| tray.take_show()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(tex) = &self.bg_texture {
            ctx.layer_painter(egui::LayerId::background()).image(
                tex.id(),
                ctx.screen_rect(),
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::from_white_alpha(40),
            );
        }

        self.handle_tray(ctx);
        self.refresh_log_language();
        while let Ok(event) = self.rx_evt.try_recv() {
            self.handle_event(ctx, event);
        }

        if let Some(finished_at) = self.reset_finished_at {
            self.show_reset_finished(ctx, finished_at);
            return;
        }

        self.show_sidebar(ctx);
        if self.show_terminal {
            self.show_terminal(ctx);
        }
        self.show_central(ctx);
        interfaz::render_dialogs(self, ctx);
        modals::render_confirm(self, ctx);
        modals::render_remove_browser(self, ctx);
        modals::render_ghost_game(self, ctx);
        modals::render_update_offer(self, ctx);
    }
}
