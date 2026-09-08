//! Three metric cards — role-dependent (Desk vs Master).

use crate::app::AttendanceApp;
use crate::theme;
use eframe::egui::{self, Margin, RichText};
use ifs_storage::DbRole;

pub fn show(app: &AttendanceApp, ui: &mut egui::Ui) {
    let card_w = ((ui.available_width() - 20.0) / 3.0).max(100.0);
    ui.horizontal(|ui| {
        if app.store.role() == DbRole::Master {
            draw_metric_card(ui, "總出席（去重）", app.master_unique, theme::INFO, card_w);
            ui.add_space(8.0);
            draw_metric_card(ui, "需覆核", app.master_needs_review, theme::WARN, card_w);
            ui.add_space(8.0);
            draw_metric_card(
                ui,
                "仍在場",
                app.master_still_inside,
                theme::CHECK_OUT,
                card_w,
            );
        } else {
            draw_metric_card(ui, "目前在場", app.counts_inside, theme::CHECK_IN, card_w);
            ui.add_space(8.0);
            draw_metric_card(ui, "累計人次", app.counts_unique, theme::TEXT, card_w);
            ui.add_space(8.0);
            draw_metric_card(ui, "本機成功", app.session_ok, theme::OK, card_w);
        }
    });
}

fn draw_metric_card(ui: &mut egui::Ui, label: &str, value: u64, accent: egui::Color32, width: f32) {
    theme::card_frame()
        .inner_margin(Margin::symmetric(10, 10))
        .show(ui, |ui| {
            ui.set_min_width(width - 4.0);
            ui.set_max_width(width);
            ui.label(
                RichText::new(label)
                    .size(12.0)
                    .color(theme::TEXT_MUTED)
                    .strong(),
            );
            ui.label(
                RichText::new(value.to_string())
                    .size(30.0)
                    .color(accent)
                    .strong(),
            );
        });
}
