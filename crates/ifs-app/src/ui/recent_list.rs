//! Recent scan list with copy buttons and copy-flash message.

use crate::app::AttendanceApp;
use crate::theme;
use crate::ui_format::{elapsed_secs, parse_subject_identity};
use eframe::egui::{self, Align, Layout, RichText};
use std::time::SystemTime;

const RECENT_SCROLL_HEIGHT: f32 = 220.0;

pub fn show(app: &mut AttendanceApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    theme::card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("本機近期掃描")
                    .size(13.0)
                    .color(theme::TEXT)
                    .strong(),
            );
            ui.label(
                RichText::new("（點「複製」可複製 類別·編號）")
                    .size(11.0)
                    .color(theme::TEXT_MUTED),
            );
            if let Some((ref t, when)) = app.copy_flash {
                if elapsed_secs(when, SystemTime::now()) < 3 {
                    ui.label(
                        RichText::new(format!("已複製：{t}"))
                            .size(11.0)
                            .color(theme::OK),
                    );
                }
            }
        });
        ui.add_space(4.0);
        if app.recent.is_empty() {
            ui.label(
                RichText::new("尚無掃描記錄")
                    .size(12.0)
                    .color(theme::TEXT_MUTED),
            );
            return;
        }
        // Collect copy actions outside borrow.
        let mut copy_req: Option<(String, String)> = None;
        egui::ScrollArea::vertical()
            .max_height(RECENT_SCROLL_HEIGHT)
            .show(ui, |ui| {
                for (i, row) in app.recent.iter().enumerate() {
                    let (fg, _) = row.tone.colors();
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{}.", i + 1))
                                .size(11.0)
                                .color(theme::TEXT_MUTED),
                        );
                        ui.label(RichText::new(&row.headline).size(13.0).color(fg).strong());
                        if !row.subject.is_empty() {
                            ui.label(RichText::new(&row.subject).size(12.0).color(theme::TEXT));
                            if ui.small_button("複製").clicked() {
                                let (c, l) = parse_subject_identity(&row.subject);
                                copy_req = Some((c, l));
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(&row.at).size(11.0).color(theme::TEXT_MUTED));
                        });
                    });
                }
            });
        if let Some((c, l)) = copy_req {
            app.copy_identity_text(ctx, &c, &l);
        }
    });
}
