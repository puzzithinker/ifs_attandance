//! eframe kiosk shell — mode pills, metric cards, colored status.

use crate::fonts;
use crate::theme::{self, StatusTone};
use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, RichText, Sense, StrokeKind};
use ifs_core::{
    message_zh, outcome_from_parse_error, parse_qr_url, rollup_master, AttendanceMode, ScanOutcome,
};
use ifs_storage::{
    default_export_filename, export_master_csv, export_station_package, import_station_package,
    now_iso_local, open_database_with_station, DbRole, SqliteVisitRepository, StationInfo,
};
use rusqlite::Connection;
use std::path::PathBuf;

pub fn run_gui(db_path: PathBuf, role: DbRole, soft_checkout: bool) -> Result<(), String> {
    let (conn, _report, station) =
        open_database_with_station(&db_path, role).map_err(|e| e.to_string())?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 560.0])
            .with_min_inner_size([520.0, 480.0])
            .with_title("IFS AML Seminar Attendance"),
        ..Default::default()
    };

    let mut app = AttendanceApp {
        conn,
        db_path,
        station,
        role,
        soft_checkout,
        mode: AttendanceMode::CheckIn,
        scan_input: String::new(),
        status: "準備就緒 — 請掃描 QR Code".into(),
        status_tone: StatusTone::Neutral,
        focus_scan: true,
        counts_ready: false,
        counts_inside: 0,
        counts_visits: 0,
        master_unique: 0,
    };
    app.refresh_counts();
    app.counts_ready = true;

    eframe::run_native(
        "IFS AML Seminar Attendance",
        options,
        Box::new(move |cc| {
            fonts::install_cjk_fonts(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    )
    .map_err(|e| e.to_string())
}

struct AttendanceApp {
    conn: Connection,
    db_path: PathBuf,
    station: StationInfo,
    role: DbRole,
    soft_checkout: bool,
    mode: AttendanceMode,
    scan_input: String,
    status: String,
    status_tone: StatusTone,
    focus_scan: bool,
    counts_ready: bool,
    counts_inside: u64,
    counts_visits: u64,
    master_unique: u64,
}

impl AttendanceApp {
    fn repo(&self) -> SqliteVisitRepository<'_> {
        SqliteVisitRepository::new(&self.conn, self.station.clone(), self.role)
            .with_soft_checkout(self.soft_checkout)
    }

    fn refresh_counts(&mut self) {
        if let Ok(c) = self.repo().counts() {
            self.counts_inside = c.currently_inside;
            self.counts_visits = c.total_visits;
        }
        if self.role == DbRole::Master {
            if let (Ok(v), Ok(o)) = (
                self.repo().list_visit_snapshots(),
                self.repo().list_orphan_checkouts(),
            ) {
                let r = rollup_master(&v, &o);
                self.master_unique = r.rows.len() as u64;
            }
        }
    }

    fn set_status(&mut self, text: impl Into<String>, tone: StatusTone) {
        self.status = text.into();
        self.status_tone = tone;
    }

    fn tone_for_outcome(outcome: &ScanOutcome) -> StatusTone {
        match outcome {
            ScanOutcome::CheckedIn { .. } | ScanOutcome::CheckedOut { .. } => StatusTone::Success,
            ScanOutcome::OrphanCheckOut { .. } => StatusTone::Info,
            ScanOutcome::AlreadyCheckedIn { .. } | ScanOutcome::NotCheckedIn { .. } => {
                StatusTone::Warning
            }
            ScanOutcome::InvalidQr { .. } | ScanOutcome::Failed { .. } => StatusTone::Error,
            ScanOutcome::EmptyInput => StatusTone::Neutral,
        }
    }

    fn submit_scan(&mut self) {
        let raw = self.scan_input.clone();
        self.scan_input.clear();
        self.focus_scan = true;

        if self.role == DbRole::Master {
            self.set_status("主控庫不接受掃描 — 請用選單匯入站點包", StatusTone::Info);
            return;
        }

        let identity = match parse_qr_url(&raw) {
            Ok(id) => id,
            Err(e) => {
                let outcome = outcome_from_parse_error(&e);
                self.set_status(message_zh(&outcome), Self::tone_for_outcome(&outcome));
                return;
            }
        };

        let now = now_iso_local();
        match self.repo().apply_mode(self.mode, &identity, &now) {
            Ok(outcome) => {
                let msg = format!(
                    "{}  ·  {} / {}",
                    message_zh(&outcome),
                    identity.category,
                    identity.license_no
                );
                self.set_status(msg, Self::tone_for_outcome(&outcome));
            }
            Err(e) => {
                let outcome = ScanOutcome::Failed {
                    message: e.to_string(),
                };
                self.set_status(message_zh(&outcome), StatusTone::Error);
            }
        }
        self.refresh_counts();
    }

    fn export_csv_dialog(&mut self) {
        let name = default_export_filename(chrono::Local::now());
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&name)
            .add_filter("CSV", &["csv"])
            .save_file()
        {
            match self.repo().export_csv(&path) {
                Ok(n) => self.set_status(
                    format!("已匯出 {n} 筆 → {}", path.display()),
                    StatusTone::Success,
                ),
                Err(e) => self.set_status(format!("匯出失敗: {e}"), StatusTone::Error),
            }
        }
    }

    fn export_package_dialog(&mut self) {
        let name = format!(
            "IFS_station_{}_{}.db",
            self.station.station_name,
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        );
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&name)
            .add_filter("SQLite", &["db"])
            .save_file()
        {
            match export_station_package(&self.db_path, &path, &self.station) {
                Ok(m) => self.set_status(
                    format!(
                        "站點包已匯出 · visits={} → {}",
                        m.visit_count,
                        path.display()
                    ),
                    StatusTone::Success,
                ),
                Err(e) => self.set_status(format!("站點包失敗: {e}"), StatusTone::Error),
            }
        }
    }

    fn import_package_dialog(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter("SQLite package", &["db"])
            .pick_files()
        {
            let mut total_ins = 0u64;
            for p in paths {
                match import_station_package(&self.conn, &p) {
                    Ok(r) => total_ins += r.rows_inserted,
                    Err(e) => {
                        self.set_status(
                            format!("匯入失敗 {}: {e}", p.display()),
                            StatusTone::Error,
                        );
                        return;
                    }
                }
            }
            self.refresh_counts();
            self.set_status(
                format!("匯入完成 · 新增 visits={total_ins}"),
                StatusTone::Success,
            );
        }
    }

    fn export_master_csv_dialog(&mut self) {
        if let (Ok(v), Ok(o)) = (
            self.repo().list_visit_snapshots(),
            self.repo().list_orphan_checkouts(),
        ) {
            let rollup = rollup_master(&v, &o);
            let name = format!(
                "IFS_master_attendance_{}.csv",
                chrono::Local::now().format("%Y-%m-%d")
            );
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name(&name)
                .add_filter("CSV", &["csv"])
                .save_file()
            {
                match export_master_csv(&path, &rollup.rows) {
                    Ok(n) => self.set_status(
                        format!("主控 CSV {n} 人 → {}", path.display()),
                        StatusTone::Success,
                    ),
                    Err(e) => self.set_status(format!("主控匯出失敗: {e}"), StatusTone::Error),
                }
            }
        }
    }

    fn draw_mode_pill(
        &mut self,
        ui: &mut egui::Ui,
        mode: AttendanceMode,
        label: &str,
        sub: &str,
        active_fill: Color32,
        soft_fill: Color32,
    ) {
        let selected = self.mode == mode;
        let (fill, stroke_c, text_c) = if selected {
            (active_fill, active_fill, Color32::WHITE)
        } else {
            (soft_fill, theme::BORDER, theme::TEXT)
        };

        let desired = egui::vec2(ui.available_width().max(120.0), 72.0);
        let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());
        let rounding = CornerRadius::same(theme::ROUND_SM);

        ui.painter().rect(
            rect,
            rounding,
            fill,
            egui::Stroke::new(if selected { 0.0 } else { 1.5 }, stroke_c),
            StrokeKind::Inside,
        );

        let center = rect.center();
        ui.painter().text(
            egui::pos2(center.x, center.y - 10.0),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(20.0),
            text_c,
        );
        ui.painter().text(
            egui::pos2(center.x, center.y + 14.0),
            egui::Align2::CENTER_CENTER,
            sub,
            egui::FontId::proportional(13.0),
            if selected {
                Color32::from_white_alpha(220)
            } else {
                theme::TEXT_MUTED
            },
        );

        if resp.clicked() {
            self.mode = mode;
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }

    fn draw_metric_card(ui: &mut egui::Ui, label: &str, value: u64, accent: Color32) {
        let width = ((ui.available_width() - 12.0) / 2.0).max(140.0);
        theme::card_frame()
            .inner_margin(Margin::symmetric(16, 16))
            .show(ui, |ui| {
                ui.set_min_width(width - 8.0);
                ui.set_max_width(width);
                ui.label(
                    RichText::new(label)
                        .size(14.0)
                        .color(theme::TEXT_MUTED)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.label(RichText::new(value.to_string()).size(42.0).color(accent).strong());
            });
    }

    fn draw_status_banner(&self, ui: &mut egui::Ui) {
        let (fg, bg) = self.status_tone.colors();
        theme::banner_frame(bg).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let badge = match self.status_tone {
                    StatusTone::Success => "✓",
                    StatusTone::Warning => "!",
                    StatusTone::Error => "✕",
                    StatusTone::Info => "i",
                    StatusTone::Neutral => "·",
                };
                ui.label(RichText::new(badge).size(22.0).color(fg).strong());
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("狀態")
                            .size(12.0)
                            .color(fg.gamma_multiply(0.85)),
                    );
                    ui.label(RichText::new(&self.status).size(20.0).color(fg).strong());
                });
            });
        });
    }
}

impl eframe::App for AttendanceApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.counts_ready {
            self.refresh_counts();
            self.counts_ready = true;
        }

        // Top bar
        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                    .inner_margin(Margin::symmetric(16, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("IFS AML Seminar")
                                .size(18.0)
                                .color(theme::TEXT)
                                .strong(),
                        );
                        ui.label(
                            RichText::new(if self.role == DbRole::Master {
                                "主控合併 · Master"
                            } else {
                                "簽到台 · Desk"
                            })
                            .size(13.0)
                            .color(theme::TEXT_MUTED),
                        );
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        egui::menu::bar(ui, |ui| {
                            ui.menu_button(RichText::new("說明").strong(), |ui| {
                                if ui.button("關於 About").clicked() {
                                    self.set_status(
                                        format!(
                                            "ifs_attendance {} · {} · {} ({})",
                                            env!("CARGO_PKG_VERSION"),
                                            self.db_path.display(),
                                            self.station.station_name,
                                            self.station.station_id
                                        ),
                                        StatusTone::Info,
                                    );
                                    ui.close_menu();
                                }
                            });
                            ui.menu_button(RichText::new("檔案 File").strong(), |ui| {
                                if ui.button("匯出出席 CSV…").clicked() {
                                    self.export_csv_dialog();
                                    ui.close_menu();
                                }
                                if ui.button("匯出站點包…").clicked() {
                                    self.export_package_dialog();
                                    ui.close_menu();
                                }
                                if self.role == DbRole::Master {
                                    ui.separator();
                                    if ui.button("匯入站點包…").clicked() {
                                        self.import_package_dialog();
                                        ui.close_menu();
                                    }
                                    if ui.button("匯出主控 CSV…").clicked() {
                                        self.export_master_csv_dialog();
                                        ui.close_menu();
                                    }
                                }
                            });
                        });
                    });
                });
            });

        // Bottom status
        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(20, 14)),
            )
            .show(ctx, |ui| {
                self.draw_status_banner(ui);
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .show(ctx, |ui| {
                // Station chip
                ui.horizontal(|ui| {
                    theme::banner_frame(theme::SURFACE_MUTED)
                        .inner_margin(Margin::symmetric(12, 6))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "站點  {}  ·  {}",
                                    self.station.station_name,
                                    short_id(&self.station.station_id)
                                ))
                                .size(13.0)
                                .color(theme::TEXT_MUTED),
                            );
                        });
                });

                ui.add_space(14.0);

                // Mode pills
                if self.role == DbRole::Desk {
                    ui.label(
                        RichText::new("作業模式")
                            .size(13.0)
                            .color(theme::TEXT_MUTED)
                            .strong(),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let half = (ui.available_width() - 12.0) / 2.0;
                        ui.allocate_ui_with_layout(
                            egui::vec2(half, 80.0),
                            Layout::top_down(Align::Center),
                            |ui| {
                                self.draw_mode_pill(
                                    ui,
                                    AttendanceMode::CheckIn,
                                    "入場",
                                    "Check-In",
                                    theme::CHECK_IN,
                                    theme::CHECK_IN_SOFT,
                                );
                            },
                        );
                        ui.add_space(12.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(half, 80.0),
                            Layout::top_down(Align::Center),
                            |ui| {
                                self.draw_mode_pill(
                                    ui,
                                    AttendanceMode::CheckOut,
                                    "離場",
                                    "Check-Out",
                                    theme::CHECK_OUT,
                                    theme::CHECK_OUT_SOFT,
                                );
                            },
                        );
                    });
                    ui.add_space(16.0);
                } else {
                    theme::banner_frame(theme::INFO_BG).show(ui, |ui| {
                        ui.label(
                            RichText::new("主控模式：掃描已停用 · 請匯入各站點包後匯出主控 CSV")
                                .size(14.0)
                                .color(theme::INFO),
                        );
                    });
                    ui.add_space(16.0);
                }

                // Scan field card
                theme::card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(
                        RichText::new("輸入中介人一戶通 QR Code")
                            .size(14.0)
                            .color(theme::TEXT_MUTED)
                            .strong(),
                    );
                    ui.add_space(8.0);

                    let mode_hint = match self.mode {
                        AttendanceMode::CheckIn => "入場模式 · 掃描後按 Enter",
                        AttendanceMode::CheckOut => "離場模式 · 掃描後按 Enter",
                    };

                    let edit = egui::TextEdit::singleline(&mut self.scan_input)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Body)
                        .hint_text(if self.role == DbRole::Master {
                            "主控庫無需掃描"
                        } else {
                            mode_hint
                        })
                        .margin(egui::vec2(12.0, 12.0));

                    let resp = ui.add_sized([ui.available_width(), 44.0], edit);

                    if self.focus_scan && self.role == DbRole::Desk {
                        resp.request_focus();
                        self.focus_scan = false;
                    }

                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if enter && (resp.has_focus() || resp.lost_focus()) {
                        self.submit_scan();
                    }

                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("掃描器鍵盤楔入 URL 後自動送出 Enter")
                            .size(12.0)
                            .color(theme::TEXT_MUTED),
                    );
                });

                ui.add_space(16.0);

                // Metrics
                ui.horizontal(|ui| {
                    if self.role == DbRole::Master {
                        Self::draw_metric_card(
                            ui,
                            "總出席人數",
                            self.master_unique,
                            theme::INFO,
                        );
                        ui.add_space(12.0);
                        Self::draw_metric_card(
                            ui,
                            "累計人次 (列)",
                            self.counts_visits,
                            theme::TEXT,
                        );
                    } else {
                        Self::draw_metric_card(
                            ui,
                            "目前在場",
                            self.counts_inside,
                            match self.mode {
                                AttendanceMode::CheckIn => theme::CHECK_IN,
                                AttendanceMode::CheckOut => theme::CHECK_OUT,
                            },
                        );
                        ui.add_space(12.0);
                        Self::draw_metric_card(
                            ui,
                            "累計人次",
                            self.counts_visits,
                            theme::TEXT,
                        );
                    }
                });

                if self.role == DbRole::Master {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!("仍在場 (未配對離場列): {}", self.counts_inside))
                            .size(13.0)
                            .color(theme::TEXT_MUTED),
                    );
                }
            });
    }
}

fn short_id(id: &str) -> String {
    if id.len() <= 8 {
        id.to_string()
    } else {
        format!("{}…", &id[..8])
    }
}
