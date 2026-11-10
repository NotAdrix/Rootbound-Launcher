use super::{busy_indicator, card, title, filled_button, terminal_button, ConfirmAction, LauncherApp, DARK_CYAN, RED};
use crate::i18n;
use crate::types::{Language, UiCommand, UpdateStatus};
use eframe::egui::{self, Color32, RichText};

const ACCENT: Color32 = Color32::from_rgb(80, 160, 255);

fn section(ui: &mut egui::Ui, heading_text: &str, color: Option<Color32>, content: impl FnOnce(&mut egui::Ui)) {
    card(6.0, 15.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let mut heading = title(heading_text, 16.0);
        if let Some(color) = color {
            heading = heading.color(color);
        }
        ui.label(heading);
        ui.add_space(10.0);
        content(ui);
    });
    ui.add_space(20.0);
}

pub(super) fn render(app: &mut LauncherApp, ui: &mut egui::Ui) {
    ui.add_space(5.0);
    ui.horizontal(|ui| {
        ui.label(title(app.t("settings"), 20.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            terminal_button(app, ui);
            busy_indicator(app, ui);
        });
    });
    ui.add_space(15.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        card(6.0, 10.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(format!("{}: {}", app.t("current_version"), crate::VERSION_LABEL));
        });
        ui.add_space(20.0);

        section(ui, app.t("appearance"), None, |ui| appearance(app, ui));
        section(ui, app.t("connection"), None, |ui| {
            let state = app.t(if app.config.offline_mode { "enabled" } else { "disabled" });
            let label = format!("{}: {state}", app.t("offline_mode"));
            if filled_button(ui, &label, DARK_CYAN) {
                app.config.offline_mode = !app.config.offline_mode;
                app.send(UiCommand::SetOfflineMode(app.config.offline_mode));
                app.save_config();
            }
        });
        section(ui, app.t("updates"), None, |ui| updates(app, ui));
        section(ui, app.t("factory_reset"), Some(Color32::from_rgb(220, 80, 80)), |ui| {
            ui.label(RichText::new(app.t("factory_reset_desc")).color(Color32::GRAY));
            ui.add_space(10.0);
            if filled_button(ui, app.t("reset_all"), RED) {
                app.confirm = Some(ConfirmAction::FactoryReset);
            }
            ui.add_space(8.0);
            if filled_button(ui, app.t("remove_app"), RED) {
                app.confirm = Some(ConfirmAction::RemoveApp);
            }
        });
    });
}

fn appearance(app: &mut LauncherApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for (language, name) in [(Language::Es, "Español"), (Language::En, "English")] {
            if ui.radio(app.config.language == language, name).clicked() {
                app.config.language = language;
                app.save_config();
            }
        }
    });
}

fn updates(app: &mut LauncherApp, ui: &mut egui::Ui) {
    let language = app.config.language;
    let message = app.update_status.as_ref().map(|s| i18n::render_update(language, s));
    let in_progress = matches!(
        app.update_status,
        Some(UpdateStatus::Checking | UpdateStatus::Downloading | UpdateStatus::Applying)
    );

    if in_progress {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new(message.unwrap_or_default()).color(ACCENT));
        });
        return;
    }

    if filled_button(ui, app.t("check_updates"), DARK_CYAN) {
        app.send(UiCommand::CheckForUpdates);
    }
    if let Some(message) = message {
        ui.label(RichText::new(message).color(Color32::GRAY));
    }
}
