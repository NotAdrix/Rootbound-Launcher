//! Toda la presentación de la biblioteca: navegación, barra de herramientas, tarjetas
//! y diálogos. Las decisiones viven en `logica.rs`.

use super::logica::{self, DlcAction, ExternalDraft, GameInfo, GameSource, Section, SortOrder, AUTO_LANGUAGE, LANGUAGES};
use crate::frontend::modals::centered;
use crate::frontend::{busy_indicator, card, title, filled_button, icon_button, terminal_button, ConfirmAction, InstallDialog, LauncherApp, View, BLUE, DARK_CYAN, RED};
use crate::types::UiCommand;
use eframe::egui::{self, Color32, RichText};
use std::f32::consts::TAU;

const PLAY_BLUE: Color32 = Color32::from_rgb(50, 130, 200);
const ART_SIZE: egui::Vec2 = egui::vec2(128.0, 72.0);
const IMAGE_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];
pub const SIDE_MARGIN: f32 = 16.0;
const ACTION_BUTTON_SIZE: [f32; 2] = [120.0, 40.0];
const GEAR_SIZE: f32 = 36.0;
const GEAR_MARGIN: f32 = 18.0;

fn title_key(section: Section) -> &'static str {
    match section {
        Section::All => "all_games",
        Section::Installed => "installed_games",
        Section::NotInstalled => "not_installed_games",
        Section::External => "external_games",
    }
}

fn nav_button(ui: &mut egui::Ui, text: &str, selected: bool, width: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 40.0), egui::Sense::click());
    let fill = if selected {
        Color32::from_rgb(8, 8, 10)
    } else if response.hovered() {
        Color32::from_rgb(62, 62, 72)
    } else {
        Color32::from_rgb(45, 45, 52)
    };
    ui.painter().rect_filled(rect, 6.0, fill);
    if selected {
        let border = egui::Stroke::new(1.0_f32, Color32::from_rgb(90, 90, 100));
        ui.painter().rect_stroke(rect, 6.0, border);
    }
    ui.painter().text(
        rect.left_center() + egui::vec2(14.0, 0.0),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(16.0),
        Color32::WHITE,
    );
    response
}

pub fn sidebar_navigation(app: &mut LauncherApp, ui: &mut egui::Ui) {
    let width = ui.available_width() - 2.0 * SIDE_MARGIN;
    let items = [
        ("library", View::Library(Section::All), matches!(app.view, View::Library(_))),
        ("stats", View::Stats, app.view == View::Stats),
        ("profile", View::Profile, app.view == View::Profile),
        ("drivers", View::Drivers, app.view == View::Drivers),
    ];
    ui.spacing_mut().item_spacing.y = 8.0;
    for (key, view, selected) in items {
        ui.horizontal(|ui| {
            ui.add_space(SIDE_MARGIN);
            if nav_button(ui, app.t(key), selected, width).clicked() {
                open_view(app, view);
            }
        });
    }
}

fn open_view(app: &mut LauncherApp, view: View) {
    match view {
        View::Stats => app.playtime = logica::load_playtime(),
        View::Profile if app.profile.is_none() => app.send(UiCommand::LoadProfile),
        _ => {}
    }
    app.view = view;
}

pub fn sidebar_footer(app: &mut LauncherApp, ui: &mut egui::Ui) {
    let panel = ui.max_rect();
    let offset = GEAR_MARGIN + GEAR_SIZE / 2.0;
    let center = egui::pos2(panel.left() + offset, panel.bottom() - offset);
    let rect = egui::Rect::from_center_size(center, egui::Vec2::splat(GEAR_SIZE));
    let response = ui.interact(rect, ui.id().with("settings_gear"), egui::Sense::click());

    let selected = app.view == View::Settings;
    let active = selected || response.hovered();
    // Al abrir o cerrar Ajustes la tuerca gira un cuarto de vuelta.
    let turn = ui.ctx().animate_bool_with_time(response.id, selected, 0.4) * TAU / 4.0;
    let color = if active { Color32::WHITE } else { Color32::LIGHT_GRAY };
    for tooth in 0..8 {
        let angle = tooth as f32 * TAU / 8.0 + turn;
        let direction = egui::vec2(angle.cos(), angle.sin());
        ui.painter().line_segment(
            [center + direction * 8.0, center + direction * 12.5],
            egui::Stroke::new(4.0_f32, color),
        );
    }
    ui.painter().circle_stroke(center, 7.0, egui::Stroke::new(3.0_f32, color));

    if response.on_hover_text(app.t("settings")).clicked() {
        app.view = View::Settings;
    }
}

fn section_header(app: &mut LauncherApp, ui: &mut egui::Ui, section: Section) {
    ui.horizontal(|ui| {
        ui.label(title(app.t("library"), 20.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            terminal_button(app, ui);
            if !app.is_loading && icon_button(ui, app.t("reload"), draw_reload_icon) {
                app.send(UiCommand::ReloadLibrary);
                app.is_loading = true;
            }
            busy_indicator(app, ui);
            if section == Section::External && filled_button(ui, app.t("add_game"), BLUE) {
                app.add_external_dialog = Some(ExternalDraft::new(&app.config.external_games));
            }
        });
    });
}

fn draw_reload_icon(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "↻",
        egui::FontId::proportional(20.0),
        color,
    );
}

/// Botón principal de cada tarjeta de juego.
fn action_button(ui: &mut egui::Ui, text: &str, fill: Color32) -> bool {
    let label = RichText::new(text).size(16.0).color(Color32::WHITE);
    let button = egui::Button::new(label).fill(fill).rounding(6.0);
    ui.add_sized(ACTION_BUTTON_SIZE, button).clicked()
}

fn section_tabs(app: &mut LauncherApp, ui: &mut egui::Ui, current: Section) {
    ui.horizontal(|ui| {
        for section in Section::ALL {
            let label = RichText::new(app.t(title_key(section))).size(15.0);
            if ui.selectable_label(section == current, label).clicked() {
                app.view = View::Library(section);
                app.config.last_section = section;
                app.save_config();
            }
        }
    });
}

fn toolbar(app: &mut LauncherApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let hint = app.t("search");
        ui.add(
            egui::TextEdit::singleline(&mut app.search)
                .hint_text(hint)
                .margin(egui::vec2(8.0, 6.0))
                .desired_width(340.0),
        );
        let sort_label = match app.config.sort_order {
            SortOrder::Ascending => "A-Z",
            SortOrder::Descending => "Z-A",
        };
        if ui.button(sort_label).clicked() {
            app.config.sort_order = app.config.sort_order.toggled();
            app.save_config();
        }
    });
}

pub fn render_section(app: &mut LauncherApp, ui: &mut egui::Ui, section: Section) {
    ui.add_space(5.0);
    section_header(app, ui, section);
    ui.add_space(8.0);
    section_tabs(app, ui, section);
    ui.add_space(4.0);
    toolbar(app, ui);
    ui.add_space(8.0);

    if app.is_loading && app.games.is_empty() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(app.t("please_wait"));
        });
        return;
    }

    // La lista sale de `app` mientras se recorre para no clonarla en cada frame.
    let games = std::mem::take(&mut app.games);
    let mut to_remove = None;
    {
        let visible = logica::visible_games(&games, section, &app.search, app.config.sort_order);
        if visible.is_empty() {
            ui.label(RichText::new(app.t("no_results")).color(Color32::GRAY));
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for game in visible {
                card(8.0, 10.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    game_card(app, ui, game, &mut to_remove);
                });
                ui.add_space(6.0);
            }
        });
    }
    app.games = games;

    if let Some(id) = to_remove {
        app.remove_external(&id);
    }
}

fn page_header(app: &mut LauncherApp, ui: &mut egui::Ui, key: &str) {
    ui.add_space(5.0);
    ui.horizontal(|ui| {
        ui.label(title(app.t(key), 20.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            terminal_button(app, ui);
            busy_indicator(app, ui);
        });
    });
    ui.add_space(15.0);
}

pub fn render_stats(app: &mut LauncherApp, ui: &mut egui::Ui) {
    page_header(app, ui, "stats");
    let mut entries: Vec<(u64, &GameInfo)> = app
        .games
        .iter()
        .filter_map(|game| Some((*app.playtime.get(&game.app_name)?, game)))
        .collect();
    entries.sort_by_key(|(seconds, _)| std::cmp::Reverse(*seconds));
    if entries.is_empty() {
        ui.label(RichText::new(app.t("no_playtime")).color(Color32::GRAY));
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (seconds, game) in entries {
            card(8.0, 10.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    game_art(app, ui, game);
                    ui.vertical(|ui| {
                        ui.add_space(14.0);
                        ui.label(title(&game.title, 16.0));
                        ui.label(
                            RichText::new(format!(
                                "{}: {}",
                                app.t("time_played"),
                                logica::format_duration(seconds)
                            ))
                            .color(Color32::GRAY),
                        );
                    });
                });
            });
            ui.add_space(6.0);
        }
    });
}

fn wide_button(ui: &mut egui::Ui, text: &str, fill: Color32) -> bool {
    let label = RichText::new(text).size(15.0).color(Color32::WHITE);
    let button = egui::Button::new(label)
        .fill(fill)
        .rounding(6.0)
        .min_size(egui::vec2(0.0, 40.0));
    ui.add(button).clicked()
}

fn card_title(ui: &mut egui::Ui, text: &str) {
    ui.label(title(text, 17.0));
    ui.add_space(6.0);
}

fn warning_box(ui: &mut egui::Ui, text: &str) {
    let amber = Color32::from_rgb(232, 170, 60);
    egui::Frame::none()
        .fill(Color32::from_rgba_premultiplied(40, 28, 8, 90))
        .stroke(egui::Stroke::new(1.0_f32, amber))
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).color(amber));
        });
}

fn drivers_cleanup_card(app: &mut LauncherApp, ui: &mut egui::Ui) {
    card(8.0, 18.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        card_title(ui, app.t("drivers_section"));
        ui.label(RichText::new(app.t("drivers_desc")).color(Color32::LIGHT_GRAY));
        ui.add_space(14.0);
        if wide_button(ui, app.t("drivers_clean"), RED) {
            app.confirm = Some(ConfirmAction::CleanDrivers);
        }
        if app.has_nvidia {
            ui.add_space(10.0);
            let label = app.t("open_nvcleanstall_after");
            if ui.checkbox(&mut app.config.use_nvcleanstall, label).changed() {
                app.save_config();
            }
        }
    });
}

fn nvcleanstall_card(app: &mut LauncherApp, ui: &mut egui::Ui) {
    card(8.0, 18.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        card_title(ui, "NVCleanstall");
        ui.label(RichText::new(app.t("nvcleanstall_desc")).color(Color32::LIGHT_GRAY));
        ui.add_space(12.0);
        warning_box(ui, app.t("nvcleanstall_warning"));
        ui.add_space(10.0);
        ui.label(RichText::new(app.t("nvcleanstall_note")).size(14.0).color(Color32::from_rgb(178, 178, 188)));
        ui.add_space(10.0);
        ui.label(title(app.t("use_with_care"), 15.0));
        ui.add_space(14.0);
        if wide_button(ui, app.t("open_nvcleanstall"), DARK_CYAN) {
            app.send(UiCommand::OpenNvcleanstall);
        }
    });
}

pub fn render_drivers(app: &mut LauncherApp, ui: &mut egui::Ui) {
    page_header(app, ui, "drivers");
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        drivers_cleanup_card(app, ui);
        if app.has_nvidia {
            ui.add_space(14.0);
            nvcleanstall_card(app, ui);
        }
    });
}

pub fn render_profile(app: &mut LauncherApp, ui: &mut egui::Ui) {
    page_header(app, ui, "profile");
    let Some(profile) = app.profile.clone() else {
        ui.spinner();
        return;
    };
    card(8.0, 15.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let fallback = app.t("not_available").to_string();
        for (key, value) in [("name", profile.name), ("email", profile.email)] {
            ui.label(RichText::new(app.t(key)).color(Color32::GRAY));
            ui.label(title(value.unwrap_or_else(|| fallback.clone()), 16.0));
            ui.add_space(8.0);
        }
    });
}

fn game_art(app: &LauncherApp, ui: &mut egui::Ui, game: &GameInfo) {
    if let Some(texture) = app.art.get(&game.app_name) {
        ui.add(egui::Image::new((texture.id(), ART_SIZE)).rounding(6.0));
        return;
    }
    let (rect, _) = ui.allocate_exact_size(ART_SIZE, egui::Sense::hover());
    ui.painter().rect_filled(rect, 6.0, Color32::from_rgb(45, 45, 55));
    let initial = game.title.chars().next().unwrap_or('?').to_uppercase().to_string();
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        egui::FontId::proportional(30.0),
        Color32::LIGHT_GRAY,
    );
}

fn game_card(app: &mut LauncherApp, ui: &mut egui::Ui, game: &GameInfo, to_remove: &mut Option<String>) {
    ui.horizontal(|ui| {
        game_art(app, ui, game);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.label(title(&game.title, 16.0));
            if game.is_installed {
                let mut is_auto = app.config.auto_launch_game.as_ref() == Some(&game.app_name);
                if ui.checkbox(&mut is_auto, app.t("auto_launch")).changed() {
                    app.config.auto_launch_game = is_auto.then(|| game.app_name.clone());
                    app.save_config();
                }
                if game.source == GameSource::Epic {
                    let mut is_auto_update = app.config.auto_update.contains(&game.app_name);
                    if ui.checkbox(&mut is_auto_update, app.t("auto_update")).changed() {
                        if is_auto_update {
                            app.config.auto_update.insert(game.app_name.clone());
                        } else {
                            app.config.auto_update.remove(&game.app_name);
                        }
                        app.save_config();
                    }
                }
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(12.0);
            if game.is_installed {
                options_menu(app, ui, game, to_remove);
                run_controls(app, ui, game);
            } else if app.installing.as_ref() == Some(&game.app_name) {
                install_progress_button(app, ui);
            } else if action_button(ui, app.t("install"), Color32::from_rgb(40, 40, 45)) {
                app.install_dialog = Some(InstallDialog {
                    game: game.clone(),
                    path: default_install_path(),
                    dlcs: true,
                    language: AUTO_LANGUAGE.to_string(),
                });
            }
        });
    });
}

fn install_progress_button(app: &mut LauncherApp, ui: &mut egui::Ui) {
    if !app.is_paused {
        let label = app.t("installing");
        ui.add_enabled_ui(false, |ui| action_button(ui, label, Color32::from_rgb(40, 40, 45)));
    } else if action_button(ui, app.t("resume_install"), BLUE) {
        app.is_paused = false;
        app.send(UiCommand::ResumeDownload);
    }
}

fn run_controls(app: &mut LauncherApp, ui: &mut egui::Ui, game: &GameInfo) {
    if app.running_game.as_ref() == Some(&game.app_name) {
        if action_button(ui, app.t("close"), RED) {
            app.stop_game(&game.app_name);
        }
        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, Color32::GREEN);
    } else if action_button(ui, app.t("play"), PLAY_BLUE) {
        app.play(game);
    }
}

fn options_menu(app: &mut LauncherApp, ui: &mut egui::Ui, game: &GameInfo, to_remove: &mut Option<String>) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    let dots = ui.style().visuals.text_color();
    if response.hovered() {
        ui.painter().rect_filled(rect, 4.0, ui.style().visuals.widgets.hovered.bg_fill);
    }
    for dy in [-4.5, 0.0, 4.5] {
        ui.painter().circle_filled(rect.center() + egui::vec2(0.0, dy), 1.5, dots);
    }

    let popup_id = ui.make_persistent_id(("menu", &game.app_name));
    if response.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    egui::popup::popup_below_widget(ui, popup_id, &response, |ui| {
        ui.set_min_width(180.0);
        match game.source {
            GameSource::Epic => epic_options(app, ui, game),
            GameSource::External(_) => {
                if ui.button(RichText::new(app.t("remove_from_list")).color(Color32::RED)).clicked() {
                    *to_remove = Some(game.app_name.clone());
                }
            }
        }
    });
}

fn epic_options(app: &mut LauncherApp, ui: &mut egui::Ui, game: &GameInfo) {
    let app_name = || game.app_name.clone();
    if ui.button(app.t("install_dlcs")).clicked() {
        app.open_dlc_dialog(game, DlcAction::Install);
    }
    if ui.button(app.t("uninstall_dlcs")).clicked() {
        app.open_dlc_dialog(game, DlcAction::Uninstall);
    }
    if ui.button(app.t("verify_files")).clicked() {
        app.send(UiCommand::RepairGame(app_name()));
    }
    if ui.button(app.t("verify_updates")).clicked() {
        app.send(UiCommand::UpdateGame(app_name()));
    }
    if ui.button(app.t("change_language")).clicked() {
        app.open_launch_language_dialog(game);
    }
    if ui.button(app.t("game_language")).clicked() {
        app.open_language_dialog(game);
    }
    ui.separator();
    if ui.button(RichText::new(app.t("uninstall")).color(Color32::RED)).clicked() {
        app.send(UiCommand::UninstallGame(app_name()));
    }
}

fn default_install_path() -> String {
    dirs::data_dir()
        .map(|dir| dir.join("Legendary").to_string_lossy().into_owned())
        .unwrap_or_else(|| r"C:\Legendary".to_string())
}

pub fn render_dialogs(app: &mut LauncherApp, ctx: &egui::Context) {
    render_install(app, ctx);
    render_language(app, ctx);
    render_launch_language(app, ctx);
    render_dlcs(app, ctx);
    render_add_external(app, ctx);
}

fn render_install(app: &mut LauncherApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.install_dialog.take() else {
        return;
    };
    let mut start = false;
    let mut cancel = false;

    centered(app.t("install_game")).show(ctx, |ui| {
        ui.label(&dialog.game.title);
        ui.label(app.t("install_folder"));
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut dialog.path);
            if ui.button("...").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    dialog.path = path.to_string_lossy().into_owned();
                }
            }
        });
        ui.checkbox(&mut dialog.dlcs, app.t("include_dlcs"));
        ui.horizontal(|ui| {
            ui.label(app.t("language"));
            egui::ComboBox::from_id_source("lang_combo")
                .selected_text(language_label(app, &dialog.language))
                .show_ui(ui, |ui| {
                    for lang in LANGUAGES {
                        ui.selectable_value(&mut dialog.language, lang.to_string(), language_label(app, lang));
                    }
                });
        });
        ui.add_space(15.0);
        ui.horizontal(|ui| {
            start = ui.add(egui::Button::new(app.t("start_download")).fill(BLUE)).clicked();
            cancel = ui.button(app.t("cancel")).clicked();
        });
    });

    if start {
        app.set_game_language(&dialog.game.app_name, &dialog.language);
        app.installing = Some(dialog.game.app_name.clone());
        app.send(UiCommand::InstallGame {
            app_name: dialog.game.app_name,
            path: dialog.path,
            dlcs: dialog.dlcs,
        });
    } else if !cancel {
        app.install_dialog = Some(dialog);
    }
}

fn language_label(app: &LauncherApp, code: &str) -> &'static str {
    if code == AUTO_LANGUAGE {
        app.t("automatic")
    } else {
        logica::language_name(code)
    }
}

fn show_waiting(app: &LauncherApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(app.t("please_wait"));
    });
}

fn render_language(app: &mut LauncherApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.language_dialog.take() else {
        return;
    };
    let mut install = false;
    let mut cancel = false;

    centered(app.t("game_language")).show(ctx, |ui| {
        ui.label(format!("{} {}", app.t("game"), dialog.title));
        ui.add_space(10.0);
        match &dialog.options {
            None => show_waiting(app, ui),
            Some(options) if options.is_empty() => {
                ui.label(RichText::new(app.t("no_language_packs")).color(Color32::GRAY));
            }
            Some(options) => {
                for option in options {
                    let is_selected = dialog.selected.as_ref() == Some(&option.tag);
                    let radio = egui::RadioButton::new(is_selected, &option.name);
                    if ui.add_enabled(!option.installed, radio).clicked() {
                        dialog.selected = Some(option.tag.clone());
                    }
                }
            }
        }
        ui.add_space(15.0);
        ui.horizontal(|ui| {
            let button = egui::Button::new(app.t("install")).fill(BLUE);
            install = ui.add_enabled(dialog.selected.is_some(), button).clicked();
            cancel = ui.button(app.t("cancel")).clicked();
        });
    });

    if install {
        if let Some(tag) = dialog.selected {
            app.send(UiCommand::InstallLanguage { app_name: dialog.app_name, tag });
        }
    } else if !cancel {
        app.language_dialog = Some(dialog);
    }
}

fn render_launch_language(app: &mut LauncherApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.launch_language_dialog.take() else {
        return;
    };
    let mut apply = false;
    let mut cancel = false;

    centered(app.t("change_language")).show(ctx, |ui| {
        ui.label(format!("{} {}", app.t("game"), dialog.title));
        ui.add_space(10.0);
        match &dialog.languages {
            None => show_waiting(app, ui),
            Some(languages) => {
                ui.radio_value(&mut dialog.selected, AUTO_LANGUAGE.to_string(), app.t("automatic"));
                for language in languages {
                    ui.radio_value(&mut dialog.selected, language.locale.clone(), &language.name);
                }
            }
        }
        ui.add_space(15.0);
        ui.horizontal(|ui| {
            let button = egui::Button::new(app.t("apply")).fill(BLUE);
            apply = ui.add_enabled(dialog.languages.is_some(), button).clicked();
            cancel = ui.button(app.t("cancel")).clicked();
        });
    });

    if apply {
        app.set_game_language(&dialog.app_name, &dialog.selected);
    } else if !cancel {
        app.launch_language_dialog = Some(dialog);
    }
}

fn render_dlcs(app: &mut LauncherApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.dlc_dialog.take() else {
        return;
    };
    let mut install = false;
    let mut cancel = false;
    let (title_key, empty_key, button_key) = match dialog.action {
        DlcAction::Install => ("install_dlcs", "no_dlcs", "install"),
        DlcAction::Uninstall => ("uninstall_dlcs", "no_dlcs_installed", "uninstall"),
    };

    centered(app.t(title_key)).show(ctx, |ui| {
        ui.label(format!("{} {}", app.t("game"), dialog.title));
        ui.add_space(10.0);
        match &mut dialog.dlcs {
            None => show_waiting(app, ui),
            Some(dlcs) if dlcs.is_empty() => {
                ui.label(RichText::new(app.t(empty_key)).color(Color32::GRAY));
            }
            Some(dlcs) => {
                for (dlc, checked) in dlcs.iter_mut() {
                    ui.checkbox(checked, &dlc.title);
                }
            }
        }
        ui.add_space(15.0);
        let any_checked = dialog.dlcs.iter().flatten().any(|(_, checked)| *checked);
        ui.horizontal(|ui| {
            let button = egui::Button::new(app.t(button_key)).fill(BLUE);
            install = ui.add_enabled(any_checked, button).clicked();
            cancel = ui.button(app.t("cancel")).clicked();
        });
    });

    if install {
        let selected = dialog
            .dlcs
            .into_iter()
            .flatten()
            .filter(|(_, checked)| *checked)
            .map(|(dlc, _)| dlc.app_name)
            .collect();
        app.send(match dialog.action {
            DlcAction::Install => UiCommand::InstallDlcs(selected),
            DlcAction::Uninstall => UiCommand::UninstallDlcs(selected),
        });
    } else if !cancel {
        app.dlc_dialog = Some(dialog);
    }
}

fn executable_fields(app: &LauncherApp, ui: &mut egui::Ui, draft: &mut ExternalDraft) {
    ui.horizontal(|ui| {
        let shown = draft.path.as_ref().map_or_else(
            || app.t("no_file_selected").to_string(),
            |path| path.display().to_string(),
        );
        ui.label(shown);
        if ui.button(app.t("choose_file")).clicked() {
            let picked = rfd::FileDialog::new().add_filter("exe", &["exe"]).pick_file();
            if let Some(path) = picked {
                draft.select_executable(path);
            }
        }
    });
}

fn steam_fields(app: &LauncherApp, ui: &mut egui::Ui, draft: &mut ExternalDraft) {
    if draft.steam_apps.is_empty() {
        ui.label(RichText::new(app.t("no_steam_games")).color(Color32::GRAY));
        return;
    }
    let selected_title = draft
        .selected_steam
        .and_then(|index| draft.steam_apps.get(index))
        .map_or(app.t("choose_game"), |steam_app| steam_app.title.as_str())
        .to_string();
    let mut clicked = None;
    egui::ComboBox::from_id_source("steam_combo")
        .selected_text(selected_title)
        .width(280.0)
        .show_ui(ui, |ui| {
            for (index, steam_app) in draft.steam_apps.iter().enumerate() {
                let is_selected = draft.selected_steam == Some(index);
                if ui.selectable_label(is_selected, &steam_app.title).clicked() {
                    clicked = Some(index);
                }
            }
        });
    if let Some(index) = clicked {
        draft.select_steam_app(index);
    }
}

fn render_add_external(app: &mut LauncherApp, ctx: &egui::Context) {
    let Some(mut draft) = app.add_external_dialog.take() else {
        return;
    };
    let mut add = false;
    let mut cancel = false;

    centered(app.t("add_game")).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.radio_value(&mut draft.is_steam, false, app.t("executable"));
            ui.radio_value(&mut draft.is_steam, true, "Steam");
        });
        ui.add_space(8.0);
        if draft.is_steam {
            steam_fields(app, ui, &mut draft);
        } else {
            executable_fields(app, ui, &mut draft);
        }
        ui.add_space(8.0);
        ui.label(app.t("game_name"));
        ui.text_edit_singleline(&mut draft.title);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let shown = draft.image.as_ref().map_or_else(
                || app.t("image_optional").to_string(),
                |path| path.display().to_string(),
            );
            ui.label(shown);
            if ui.button(app.t("choose_file")).clicked() {
                let picked = rfd::FileDialog::new()
                    .add_filter("image", &IMAGE_EXTENSIONS)
                    .pick_file();
                if picked.is_some() {
                    draft.image = picked;
                }
            }
        });
        ui.add_space(15.0);
        ui.horizontal(|ui| {
            let button = egui::Button::new(app.t("add_game")).fill(BLUE);
            add = ui.add_enabled(draft.is_complete(), button).clicked();
            cancel = ui.button(app.t("cancel")).clicked();
        });
    });

    if add {
        app.add_external(draft);
    } else if !cancel {
        app.add_external_dialog = Some(draft);
    }
}
