//! Master dashboard — needs-review / still-inside counts and previews.

use crate::app::AttendanceApp;
use crate::theme;
use eframe::egui;
use eframe::egui::RichText;

pub fn show(app: &AttendanceApp, ui: &mut egui::Ui) {
    theme::card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            RichText::new("主控儀表板（來自 rollup_master）")
                .size(13.0)
                .color(theme::TEXT)
                .strong(),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("需覆核 needs_review：{}", app.master_needs_review))
                    .size(14.0)
                    .color(if app.master_needs_review > 0 {
                        theme::WARN
                    } else {
                        theme::OK
                    })
                    .strong(),
            );
            ui.add_space(16.0);
            ui.label(
                RichText::new(format!("仍在場 / 僅入場：{}", app.master_still_inside))
                    .size(14.0)
                    .color(theme::CHECK_OUT)
                    .strong(),
            );
        });
        ui.add_space(6.0);
        ui.columns(2, |cols| {
            cols[0].label(
                RichText::new("需覆核預覽")
                    .size(12.0)
                    .color(theme::TEXT_MUTED)
                    .strong(),
            );
            if app.master_preview_review.is_empty() {
                cols[0].label(RichText::new("（無）").size(12.0).color(theme::TEXT_MUTED));
            } else {
                for line in &app.master_preview_review {
                    cols[0].label(RichText::new(line).size(12.0).color(theme::TEXT));
                }
            }
            cols[1].label(
                RichText::new("仍在場預覽")
                    .size(12.0)
                    .color(theme::TEXT_MUTED)
                    .strong(),
            );
            if app.master_preview_inside.is_empty() {
                cols[1].label(RichText::new("（無）").size(12.0).color(theme::TEXT_MUTED));
            } else {
                for line in &app.master_preview_inside {
                    cols[1].label(RichText::new(line).size(12.0).color(theme::TEXT));
                }
            }
        });
    });
}
