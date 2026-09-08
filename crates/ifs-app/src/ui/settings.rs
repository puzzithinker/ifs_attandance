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
            ui.separator();
            ui.label(RichText::new("CPD 時間窗（匯出 CSV 的 CPD 欄）").strong().size(12.0));
            ui.horizontal(|ui| {
                ui.label("入場：");
                ui.add(
                    eframe::egui::TextEdit::singleline(&mut app.cpd_in_from_draft)
                        .desired_width(70.0)
                        .hint_text("14:30"),
                );
                ui.label("至");
                ui.add(
                    eframe::egui::TextEdit::singleline(&mut app.cpd_in_until_draft)
                        .desired_width(70.0)
                        .hint_text("15:00"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("離場：");
                ui.add(
                    eframe::egui::TextEdit::singleline(&mut app.cpd_out_from_draft)
                        .desired_width(70.0)
                        .hint_text("17:10"),
                );
                ui.label("至");
                ui.add(
                    eframe::egui::TextEdit::singleline(&mut app.cpd_out_until_draft)
                        .desired_width(70.0)
                        .hint_text("17:30"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("CPD 點數：");
                ui.add(
                    eframe::egui::TextEdit::singleline(&mut app.cpd_points_draft)
                        .desired_width(70.0)
                        .hint_text("2"),
                );
                if ui.button("儲存 CPD").clicked() {
                    app.save_cpd_config();
                }
            });
            ui.label(
                RichText::new(
                    "時間格式 HH:MM，含端點；留白＝該邊界不限。CSV 的 CPD 欄：入場且離場都在窗口內＝點數，否則 0；全部留白＝空欄。留白點數預設 2。",
                )
                .size(11.0)
                .color(theme::TEXT_MUTED),
            );
        });
    if !open {
        app.settings_open = false;
    }
}
