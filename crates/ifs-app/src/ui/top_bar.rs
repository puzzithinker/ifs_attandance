//! Top bar: app title, event name, live clock, 檔案/檢視/說明 menus.

use crate::app::AttendanceApp;
use crate::theme;
use crate::ui::short_id;
use crate::ui_format::format_clock_now;
use eframe::egui::{self, Layout, Margin, RichText};
use ifs_storage::DbRole;

const APP_TITLE: &str = "IFS 活動出席";
const APP_TITLE_EN: &str = "IFS Event Attendance";

pub fn show(app: &mut AttendanceApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("top_bar")
        .frame(
            egui::Frame::new()
                .fill(theme::SURFACE)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(Margin::symmetric(14, 8)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(APP_TITLE)
                            .size(18.0)
                            .color(theme::TEXT)
                            .strong(),
                    );
                    let ev = if app.event_name.is_empty() {
                        "（未設定活動名稱）".to_string()
                    } else {
                        app.event_name.clone()
                    };
                    ui.label(
                        RichText::new(format!(
                            "{} · {}",
                            if app.store.role() == DbRole::Master {
                                "主控"
                            } else {
                                "簽到站"
                            },
                            ev
                        ))
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                    );
                });

                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format_clock_now(chrono::Local::now()))
                            .size(16.0)
                            .color(theme::TEXT)
                            .strong(),
                    );
                    ui.add_space(8.0);

                    egui::menu::bar(ui, |ui| {
                        ui.menu_button("說明", |ui| {
                            if ui.button("關於").clicked() {
                                app.feedback_system(
                                    "關於",
                                    format!(
                                        "{APP_TITLE_EN} {} · {} · 站點 {} ({}) · 音效 {} · F11 全螢幕",
                                        env!("CARGO_PKG_VERSION"),
                                        app.db_path.display(),
                                        app.store.station().station_name,
                                        short_id(&app.store.station().station_id),
                                        if app.sound_enabled { "開" } else { "關" }
                                    ),
                                    theme::StatusTone::Info,
                                );
                                ui.close_menu();
                            }
                        });
                        ui.menu_button("檢視", |ui| {
                            let fs = if app.fullscreen {
                                "結束全螢幕"
                            } else {
                                "全螢幕 (F11)"
                            };
                            if ui.button(fs).clicked() {
                                app.toggle_fullscreen(ctx);
                                ui.close_menu();
                            }
                            let sound_label = if app.sound_enabled {
                                "音效：開（點擊關閉）"
                            } else {
                                "音效：關（點擊開啟）"
                            };
                            if ui.button(sound_label).clicked() {
                                app.toggle_sound();
                                ui.close_menu();
                            }
                            if ui.button("活動 / 站點設定…").clicked() {
                                app.settings_open = !app.settings_open;
                                ui.close_menu();
                            }
                        });
                        ui.menu_button("檔案", |ui| {
                            if ui.button("匯出出席 CSV…").clicked() {
                                app.export_csv_dialog();
                                ui.close_menu();
                            }
                            if ui.button("匯出站點包…").clicked() {
                                app.export_package_dialog();
                                ui.close_menu();
                            }
                            if app.store.role() == DbRole::Master {
                                ui.separator();
                                if ui.button("匯入站點包…").clicked() {
                                    app.import_package_dialog();
                                    ui.close_menu();
                                }
                                if ui.button("匯出主控 CSV…").clicked() {
                                    app.export_master_csv_dialog();
                                    ui.close_menu();
                                }
                            }
                        });
                    });
                });
            });
        });
}
