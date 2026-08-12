//! Open/switch database dialog — pick or create a .db file and choose the
//! role (簽到站 Desk / 主控 Master) without touching the command line.

use crate::app::AttendanceApp;
use crate::theme;
use eframe::egui::{self, RichText};
use ifs_storage::DbRole;
use std::path::PathBuf;

pub fn show(app: &mut AttendanceApp, ctx: &egui::Context) {
    if !app.db_dialog_open {
        return;
    }
    let mut open = true;
    egui::Window::new("開啟 / 切換資料庫")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            let current_role = if app.store.role() == DbRole::Master {
                "主控"
            } else {
                "簽到站"
            };
            ui.label(
                RichText::new(format!("目前：{current_role} · {}", app.db_path.display()))
                    .size(12.0)
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                if ui.button("選擇現有資料庫…").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("SQLite", &["db"])
                        .pick_file()
                    {
                        app.db_dialog_path = p.display().to_string();
                    }
                }
                if ui.button("建立新資料庫…").clicked() {
                    let default_name = if app.db_dialog_role == DbRole::Master {
                        "master.db"
                    } else {
                        "agent.db"
                    };
                    if let Some(p) = rfd::FileDialog::new()
                        .set_file_name(default_name)
                        .add_filter("SQLite", &["db"])
                        .save_file()
                    {
                        app.db_dialog_path = p.display().to_string();
                    }
                }
            });
            ui.add(
                egui::TextEdit::singleline(&mut app.db_dialog_path)
                    .desired_width(360.0)
                    .hint_text("資料庫路徑 (.db)"),
            );
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("角色：");
                ui.radio_value(&mut app.db_dialog_role, DbRole::Desk, "簽到站 Desk");
                ui.radio_value(&mut app.db_dialog_role, DbRole::Master, "主控 Master");
            });
            ui.label(
                RichText::new(
                    "簽到站：現場掃描入場/離場。主控：匯入各站點包、檢視儀表板、匯出主控 CSV。",
                )
                .size(11.0)
                .color(theme::TEXT_MUTED),
            );
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                let can_open = !app.db_dialog_path.trim().is_empty();
                if ui
                    .add_enabled(
                        can_open,
                        egui::Button::new("開啟").min_size(egui::vec2(80.0, 0.0)),
                    )
                    .clicked()
                {
                    let path = PathBuf::from(app.db_dialog_path.trim());
                    let role = app.db_dialog_role;
                    app.open_database(path, role);
                    app.db_dialog_open = false;
                }
                if ui.button("取消").clicked() {
                    app.db_dialog_open = false;
                }
            });
        });
    if !open {
        app.db_dialog_open = false;
    }
}
