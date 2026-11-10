use super::{ConfirmAction, LauncherApp, PendingUpdate, BLUE, RED};
use crate::types::UiCommand;
use eframe::egui::{self, Color32, RichText};

pub(crate) fn centered(title: &str) -> egui::Window<'static> {
    egui::Window::new(title.to_owned())
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
}

pub(crate) fn render_confirm(app: &mut LauncherApp, ctx: &egui::Context) {
    let Some(action) = app.confirm else {
        return;
    };
    let (title, description, accept, command) = match action {
        ConfirmAction::FactoryReset => (
            "confirm_reset",
            "confirm_reset_desc",
            "yes_delete_all",
            UiCommand::FactoryReset,
        ),
        ConfirmAction::RemoveApp => (
            "confirm_remove_app",
            "confirm_remove_app_desc",
            "yes_remove_app",
            UiCommand::RemoveApp,
        ),
        ConfirmAction::CleanDrivers => (
            "confirm_drivers",
            "confirm_drivers_desc",
            "yes_clean_drivers",
            UiCommand::CleanDrivers,
        ),
    };
    let mut confirm = false;
    let mut cancel = false;

    centered(app.t(title)).show(ctx, |ui| {
        ui.label(app.t(description));
        ui.add_space(15.0);
        ui.horizontal(|ui| {
            if app.is_resetting {
                ui.spinner();
                ui.label(app.t("resetting"));
            } else {
                let text = RichText::new(app.t(accept)).color(Color32::WHITE);
                confirm = ui.add(egui::Button::new(text).fill(RED)).clicked();
                cancel = ui.button(app.t("cancel")).clicked();
            }
        });
    });

    if confirm {
        app.send(command);
        if action == ConfirmAction::CleanDrivers {
            app.confirm = None;
        } else {
            app.is_resetting = true;
        }
    }
    if cancel {
        app.confirm = None;
    }
}

pub(crate) fn render_remove_browser(app: &mut LauncherApp, ctx: &egui::Context) {
    if !app.ask_remove_browser {
        return;
    }
    let mut remove = false;
    let mut keep = false;
    centered(app.t("browser_installed")).show(ctx, |ui| {
        ui.label(app.t("ask_remove_browser"));
        ui.add_space(15.0);
        ui.horizontal(|ui| {
            let text = RichText::new(app.t("yes")).color(Color32::WHITE);
            remove = ui.add(egui::Button::new(text).fill(RED)).clicked();
            keep = ui.button(app.t("no")).clicked();
        });
    });
    if remove {
        app.send(UiCommand::RemoveBrowser);
    }
    if remove || keep {
        app.ask_remove_browser = false;
    }
}

pub(crate) fn render_ghost_game(app: &mut LauncherApp, ctx: &egui::Context) {
    let Some((app_name, path)) = app.ghost_games.front().cloned() else {
        return;
    };
    let title = app.game_title(&app_name);
    let mut repair = false;
    let mut delete = false;
    centered(app.t("ghost_title")).show(ctx, |ui| {
        ui.label(app.t("ask_ghost").replace("{n}", &title));
        ui.label(RichText::new(&path).small().color(Color32::GRAY));
        ui.add_space(15.0);
        ui.horizontal(|ui| {
            repair = ui.button(app.t("repair")).clicked();
            let text = RichText::new(app.t("delete_permanently")).color(Color32::WHITE);
            delete = ui.add(egui::Button::new(text).fill(RED)).clicked();
        });
    });
    if repair {
        app.installing = Some(app_name.clone());
        app.send(UiCommand::RepairGhost { app_name, path });
        app.ghost_games.pop_front();
    } else if delete {
        app.send(UiCommand::DeleteGhost { path });
        app.ghost_games.pop_front();
    }
}

pub(crate) fn render_update_offer(app: &mut LauncherApp, ctx: &egui::Context) {
    let Some(update) = app.pending_updates.front().cloned() else {
        return;
    };
    let mut accept = false;
    let mut decline = false;
    centered(app.t("update_available")).show(ctx, |ui| {
        ui.label(app.t("ask_update"));
        ui.add_space(15.0);
        ui.horizontal(|ui| {
            accept = ui.add(egui::Button::new(app.t("yes")).fill(BLUE)).clicked();
            decline = ui.button(app.t("no")).clicked();
        });
    });
    if accept {
        app.send(match update {
            PendingUpdate::Launcher(url) => UiCommand::PerformUpdate(url),
            PendingUpdate::Legendary => UiCommand::UpdateLegendary,
        });
    }
    if accept || decline {
        app.pending_updates.pop_front();
    }
}
