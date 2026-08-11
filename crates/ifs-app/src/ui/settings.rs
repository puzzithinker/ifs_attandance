//! Settings as a floating egui::Window (UX #3) — does not shift central panel.

use crate::app::AttendanceApp;
use crate::theme;
use eframe::egui::RichText;

pub fn show(app: &mut AttendanceApp, ctx: &eframe::egui::Context) {
    if !app.settings_open {
        return;
    }
    // Use a local `open` flag to avoid double-mut-borrow of `app`:
    // the window closure also captures `app` for the editors.
    let mut open = true;
    eframe::egui::Window::new("活動 / 站點設定")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("活動名稱：");
                ui.add(
                    eframe::egui::TextEdit::singleline(&mut app.event_name_draft)
                        .desired_width(220.0)
                        .hint_text("例如：CPD 下午場"),
                );
                if ui.button("儲存活動").clicked() {
                    app.save_event_name();
                }
            });
            ui.horizontal(|ui| {
                ui.label("站點顯示名：");
                ui.add(
                    eframe::egui::TextEdit::singleline(&mut app.station_name_draft)
                        .desired_width(220.0)
                        .hint_text("入口-1 / 出口-2"),
                );
                if ui.button("儲存站點").clicked() {
                    app.save_station_name();
                }
            });
            ui.label(
                RichText::new(
                    "活動名稱會寫入 app_meta，並出現在 CSV 的 event 欄與檔名。站點名寫入 station.toml。",
                )
                .size(11.0)
                .color(theme::TEXT_MUTED),
            );
        });
    if !open {
        app.settings_open = false;
    }
}
