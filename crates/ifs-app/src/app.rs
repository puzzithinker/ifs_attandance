//! eframe kiosk shell — event/station editors, clock, copy, master dashboard, F11, sound.

use crate::fonts;
use crate::sound;
use crate::theme::{self, StatusTone};
use crate::ui_format::{
    elapsed_secs, format_clock_now, format_copy_identity, format_seconds_ago, parse_subject_identity,
};
use eframe::egui::{
    self, Align, Color32, CornerRadius, Layout, Margin, RichText, Sense, StrokeKind,
    ViewportCommand,
};
use ifs_core::{
    outcome_from_parse_error, parse_qr_url, rollup_master, AttendanceMode, MasterAgentRow,
    MasterStatus, ScanOutcome,
};
use ifs_storage::{
    default_export_filename, export_master_csv, export_station_package, get_event_name,
    get_sound_enabled, import_station_package, master_export_filename, now_iso_local,
    open_database_with_station, set_event_name, set_sound_enabled, station_path_for_db,
    write_station_file, DbRole, SqliteVisitRepository, StationInfo,
};
use rusqlite::Connection;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::SystemTime;

const APP_TITLE: &str = "IFS 活動出席";
const APP_TITLE_EN: &str = "IFS Event Attendance";
const RECENT_MAX: usize = 12;
const MASTER_PREVIEW_MAX: usize = 8;

pub fn run_gui(db_path: PathBuf, role: DbRole, soft_checkout: bool) -> Result<(), String> {
    let (conn, _report, station) =
        open_database_with_station(&db_path, role).map_err(|e| e.to_string())?;

    let event_name = get_event_name(&conn).unwrap_or_default();
    let sound_enabled = get_sound_enabled(&conn).unwrap_or(true);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 780.0])
            .with_min_inner_size([600.0, 640.0])
            .with_title(format!("{APP_TITLE} · {APP_TITLE_EN}")),
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
        focus_scan: true,
        counts_ready: false,
        counts_inside: 0,
        counts_visits: 0,
        master_unique: 0,
        last: None,
        last_scan_at: None,
        recent: VecDeque::new(),
        session_ok: 0,
        session_warn: 0,
        session_err: 0,
        event_name,
        event_name_draft: String::new(),
        station_name_draft: String::new(),
        settings_open: false,
        fullscreen: false,
        sound_enabled,
        master_needs_review: 0,
        master_still_inside: 0,
        master_preview_review: Vec::new(),
        master_preview_inside: Vec::new(),
        copy_flash: None,
    };
    app.event_name_draft = app.event_name.clone();
    app.station_name_draft = app.station.station_name.clone();
    app.refresh_counts();
    app.counts_ready = true;

    eframe::run_native(
        APP_TITLE_EN,
        options,
        Box::new(move |cc| {
            fonts::install_cjk_fonts(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    )
    .map_err(|e| e.to_string())
}

#[derive(Clone)]
struct ScanFeedback {
    headline: String,
    subject: String,
    detail: String,
    tone: StatusTone,
    at: String,
}

struct AttendanceApp {
    conn: Connection,
    db_path: PathBuf,
    station: StationInfo,
    role: DbRole,
    soft_checkout: bool,
    mode: AttendanceMode,
    scan_input: String,
    focus_scan: bool,
    counts_ready: bool,
    counts_inside: u64,
    counts_visits: u64,
    master_unique: u64,
    last: Option<ScanFeedback>,
    last_scan_at: Option<SystemTime>,
    recent: VecDeque<ScanFeedback>,
    session_ok: u64,
    session_warn: u64,
    session_err: u64,
    event_name: String,
    event_name_draft: String,
    station_name_draft: String,
    settings_open: bool,
    fullscreen: bool,
    sound_enabled: bool,
    master_needs_review: u64,
    master_still_inside: u64,
    master_preview_review: Vec<String>,
    master_preview_inside: Vec<String>,
    copy_flash: Option<(String, SystemTime)>,
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
                self.apply_master_rollup(&r.rows);
            }
        }
    }

    /// Drive dashboard from real rollup rows (same path as master CSV).
    fn apply_master_rollup(&mut self, rows: &[MasterAgentRow]) {
        self.master_unique = rows.len() as u64;
        self.master_needs_review = rows.iter().filter(|r| r.needs_review).count() as u64;
        self.master_still_inside = rows
            .iter()
            .filter(|r| {
                matches!(
                    r.status,
                    MasterStatus::StillInside | MasterStatus::CheckInOnly
                ) || !r.open_stations.is_empty()
            })
            .count() as u64;

        self.master_preview_review = rows
            .iter()
            .filter(|r| r.needs_review)
            .take(MASTER_PREVIEW_MAX)
            .map(|r| {
                format!(
                    "{} · {}{}",
                    r.identity.category,
                    r.identity.license_no,
                    if r.open_stations.is_empty() {
                        String::new()
                    } else {
                        format!(" [open:{}]", r.open_stations.join(","))
                    }
                )
            })
            .collect();

        self.master_preview_inside = rows
            .iter()
            .filter(|r| {
                matches!(
                    r.status,
                    MasterStatus::StillInside | MasterStatus::CheckInOnly
                ) || !r.open_stations.is_empty()
            })
            .take(MASTER_PREVIEW_MAX)
            .map(|r| format!("{} · {}", r.identity.category, r.identity.license_no))
            .collect();
    }

    fn push_feedback(&mut self, fb: ScanFeedback) {
        match fb.tone {
            StatusTone::Success | StatusTone::Info => {
                self.session_ok += 1;
                sound::play_success(self.sound_enabled);
            }
            StatusTone::Warning => {
                self.session_warn += 1;
                sound::play_failure(self.sound_enabled);
            }
            StatusTone::Error => {
                self.session_err += 1;
                sound::play_failure(self.sound_enabled);
            }
            StatusTone::Neutral => {}
        }
        self.last_scan_at = Some(SystemTime::now());
        self.recent.push_front(fb.clone());
        while self.recent.len() > RECENT_MAX {
            self.recent.pop_back();
        }
        self.last = Some(fb);
    }

    fn feedback_system(
        &mut self,
        headline: impl Into<String>,
        detail: impl Into<String>,
        tone: StatusTone,
    ) {
        let at = now_iso_local();
        // system messages: no scan age reset unless we want — still set last_scan for ops feedback
        self.push_feedback(ScanFeedback {
            headline: headline.into(),
            subject: String::new(),
            detail: detail.into(),
            tone,
            at,
        });
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

    fn headline_for_outcome(outcome: &ScanOutcome) -> String {
        match outcome {
            ScanOutcome::CheckedIn { .. } => "入場成功".into(),
            ScanOutcome::AlreadyCheckedIn { .. } => "重複入場".into(),
            ScanOutcome::CheckedOut { .. } => "離場成功".into(),
            ScanOutcome::OrphanCheckOut { .. } => "跨站點離場".into(),
            ScanOutcome::NotCheckedIn { .. } => "無法離場".into(),
            ScanOutcome::InvalidQr { .. } => "QR 無效".into(),
            ScanOutcome::EmptyInput => "未輸入".into(),
            ScanOutcome::Failed { .. } => "操作失敗".into(),
        }
    }

    fn detail_for_outcome(outcome: &ScanOutcome, mode: AttendanceMode) -> String {
        let mode_s = match mode {
            AttendanceMode::CheckIn => "模式：入場",
            AttendanceMode::CheckOut => "模式：離場",
        };
        match outcome {
            ScanOutcome::CheckedIn { at, .. } => format!("{mode_s} · 登記時間 {at}"),
            ScanOutcome::AlreadyCheckedIn { check_in_at, .. } => {
                format!("{mode_s} · 本機已於 {check_in_at} 入場（未重複寫入）")
            }
            ScanOutcome::CheckedOut { check_out_at, .. } => {
                format!("{mode_s} · 離場時間 {check_out_at}")
            }
            ScanOutcome::OrphanCheckOut { at, .. } => {
                format!("{mode_s} · 已記錄離場意圖 {at}（入場可能在其他站點；主控合併時配對）")
            }
            ScanOutcome::NotCheckedIn { .. } => {
                format!("{mode_s} · 本機無未離場記錄")
            }
            ScanOutcome::InvalidQr { reason } => format!("原因：{reason}"),
            ScanOutcome::EmptyInput => "請掃描或貼上 QR URL 後按 Enter".into(),
            ScanOutcome::Failed { message } => format!("錯誤：{message}"),
        }
    }

    fn save_event_name(&mut self) {
        let name = self.event_name_draft.trim().to_string();
        if let Err(e) = set_event_name(&self.conn, &name) {
            self.feedback_system("儲存活動名稱失敗", e.to_string(), StatusTone::Error);
            return;
        }
        self.event_name = name;
        self.event_name_draft = self.event_name.clone();
        self.feedback_system(
            "活動名稱已儲存",
            if self.event_name.is_empty() {
                "（空白 = 匯出檔名不含活動）".into()
            } else {
                self.event_name.clone()
            },
            StatusTone::Success,
        );
    }

    fn save_station_name(&mut self) {
        let name = self.station_name_draft.trim().to_string();
        if name.is_empty() {
            self.feedback_system("站點名稱不可空白", "", StatusTone::Warning);
            return;
        }
        self.station.station_name = name;
        let path = station_path_for_db(&self.db_path);
        if let Err(e) = write_station_file(&path, &self.station) {
            self.feedback_system("儲存站點名稱失敗", e.to_string(), StatusTone::Error);
            return;
        }
        self.station_name_draft = self.station.station_name.clone();
        self.feedback_system(
            "站點名稱已儲存",
            self.station.station_name.clone(),
            StatusTone::Success,
        );
    }

    fn toggle_sound(&mut self) {
        self.sound_enabled = !self.sound_enabled;
        let _ = set_sound_enabled(&self.conn, self.sound_enabled);
    }

    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
    }

    fn copy_identity_text(&mut self, ctx: &egui::Context, category: &str, license: &str) {
        let text = format_copy_identity(category, license);
        if text.is_empty() {
            return;
        }
        ctx.copy_text(text.clone());
        self.copy_flash = Some((text, SystemTime::now()));
    }

    fn submit_scan(&mut self) {
        let raw = self.scan_input.clone();
        self.scan_input.clear();
        self.focus_scan = true;

        if self.role == DbRole::Master {
            self.feedback_system(
                "主控模式",
                "此庫不接受掃描 — 請匯入站點包後匯出主控 CSV",
                StatusTone::Info,
            );
            return;
        }

        let identity = match parse_qr_url(&raw) {
            Ok(id) => id,
            Err(e) => {
                let outcome = outcome_from_parse_error(&e);
                let at = now_iso_local();
                self.push_feedback(ScanFeedback {
                    headline: Self::headline_for_outcome(&outcome),
                    subject: String::new(),
                    detail: Self::detail_for_outcome(&outcome, self.mode),
                    tone: Self::tone_for_outcome(&outcome),
                    at,
                });
                return;
            }
        };

        let now = now_iso_local();
        let mode = self.mode;
        match self.repo().apply_mode(mode, &identity, &now) {
            Ok(outcome) => {
                self.push_feedback(ScanFeedback {
                    headline: Self::headline_for_outcome(&outcome),
                    subject: format!("{}  ·  {}", identity.category, identity.license_no),
                    detail: Self::detail_for_outcome(&outcome, mode),
                    tone: Self::tone_for_outcome(&outcome),
                    at: now.clone(),
                });
            }
            Err(e) => {
                let outcome = ScanOutcome::Failed {
                    message: e.to_string(),
                };
                self.push_feedback(ScanFeedback {
                    headline: Self::headline_for_outcome(&outcome),
                    subject: format!("{}  ·  {}", identity.category, identity.license_no),
                    detail: Self::detail_for_outcome(&outcome, mode),
                    tone: StatusTone::Error,
                    at: now,
                });
            }
        }
        self.refresh_counts();
    }

    fn export_csv_dialog(&mut self) {
        let name = default_export_filename(chrono::Local::now(), &self.event_name);
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&name)
            .add_filter("CSV", &["csv"])
            .save_file()
        {
            match self.repo().export_csv(&path, &self.event_name) {
                Ok(n) => self.feedback_system(
                    "已匯出 CSV",
                    format!("{n} 筆 → {}", path.display()),
                    StatusTone::Success,
                ),
                Err(e) => self.feedback_system("匯出失敗", e.to_string(), StatusTone::Error),
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
                Ok(m) => self.feedback_system(
                    "站點包已匯出",
                    format!(
                        "{} · visits={} → {}",
                        m.station_name,
                        m.visit_count,
                        path.display()
                    ),
                    StatusTone::Success,
                ),
                Err(e) => self.feedback_system("站點包失敗", e.to_string(), StatusTone::Error),
            }
        }
    }

    fn import_package_dialog(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter("SQLite package", &["db"])
            .pick_files()
        {
            let mut total_ins = 0u64;
            let mut total_skip = 0u64;
            let n_files = paths.len();
            for p in paths {
                match import_station_package(&self.conn, &p) {
                    Ok(r) => {
                        total_ins += r.rows_inserted;
                        total_skip += r.rows_skipped;
                    }
                    Err(e) => {
                        self.feedback_system(
                            "匯入失敗",
                            format!("{}: {e}", p.display()),
                            StatusTone::Error,
                        );
                        return;
                    }
                }
            }
            self.refresh_counts();
            self.feedback_system(
                "匯入完成",
                format!(
                    "{n_files} 包 · 新增 {total_ins} · 略過 {total_skip} · 總出席 {} · 需覆核 {}",
                    self.master_unique, self.master_needs_review
                ),
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
            self.apply_master_rollup(&rollup.rows);
            let name = master_export_filename(chrono::Local::now(), &self.event_name);
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name(&name)
                .add_filter("CSV", &["csv"])
                .save_file()
            {
                match export_master_csv(&path, &rollup.rows, &self.event_name) {
                    Ok(n) => self.feedback_system(
                        "主控 CSV 已匯出",
                        format!("{n} 位 → {}", path.display()),
                        StatusTone::Success,
                    ),
                    Err(e) => {
                        self.feedback_system("主控匯出失敗", e.to_string(), StatusTone::Error)
                    }
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

        let desired = egui::vec2(ui.available_width().max(120.0), 56.0);
        let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());
        ui.painter().rect(
            rect,
            CornerRadius::same(theme::ROUND_SM),
            fill,
            egui::Stroke::new(if selected { 0.0 } else { 1.5 }, stroke_c),
            StrokeKind::Inside,
        );
        let center = rect.center();
        ui.painter().text(
            egui::pos2(center.x, center.y - 8.0),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(18.0),
            text_c,
        );
        ui.painter().text(
            egui::pos2(center.x, center.y + 12.0),
            egui::Align2::CENTER_CENTER,
            sub,
            egui::FontId::proportional(12.0),
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

    fn draw_metric_card(ui: &mut egui::Ui, label: &str, value: u64, accent: Color32, width: f32) {
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
                ui.label(RichText::new(value.to_string()).size(30.0).color(accent).strong());
            });
    }

    fn draw_last_result(&self, ui: &mut egui::Ui) {
        let (headline, subject, detail, tone, at) = match &self.last {
            Some(f) => (
                f.headline.as_str(),
                f.subject.as_str(),
                f.detail.as_str(),
                f.tone,
                f.at.as_str(),
            ),
            None => (
                "等待掃描",
                "",
                "掃描後結果顯示於此 · 含身分與說明",
                StatusTone::Neutral,
                "",
            ),
        };
        let (fg, bg) = tone.colors();
        let age = self
            .last_scan_at
            .map(|t| format_seconds_ago(elapsed_secs(t, SystemTime::now())))
            .unwrap_or_else(|| "—".into());

        theme::banner_frame(bg)
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let icon = match tone {
                        StatusTone::Success => "✓",
                        StatusTone::Warning => "!",
                        StatusTone::Error => "✕",
                        StatusTone::Info => "i",
                        StatusTone::Neutral => "◎",
                    };
                    ui.label(RichText::new(icon).size(32.0).color(fg).strong());
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("最近一次掃描")
                                    .size(12.0)
                                    .color(fg.gamma_multiply(0.85)),
                            );
                            ui.label(
                                RichText::new(format!("· {age}"))
                                    .size(12.0)
                                    .color(fg.gamma_multiply(0.85)),
                            );
                        });
                        ui.label(RichText::new(headline).size(26.0).color(fg).strong());
                        if !subject.is_empty() {
                            ui.label(RichText::new(subject).size(18.0).color(theme::TEXT).strong());
                        }
                        if !detail.is_empty() {
                            ui.label(RichText::new(detail).size(13.0).color(theme::TEXT_MUTED));
                        }
                        if !at.is_empty() {
                            ui.label(
                                RichText::new(format!("回報 {at}"))
                                    .size(11.0)
                                    .color(theme::TEXT_MUTED),
                            );
                        }
                    });
                });
            });
    }

    fn draw_recent_list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
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
                if let Some((ref t, when)) = self.copy_flash {
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
            if self.recent.is_empty() {
                ui.label(
                    RichText::new("尚無掃描記錄")
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                );
                return;
            }
            // Collect copy actions outside borrow
            let mut copy_req: Option<(String, String)> = None;
            egui::ScrollArea::vertical()
                .max_height(140.0)
                .show(ui, |ui| {
                    for (i, row) in self.recent.iter().enumerate() {
                        let (fg, _) = row.tone.colors();
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}.", i + 1))
                                    .size(11.0)
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.label(
                                RichText::new(&row.headline)
                                    .size(13.0)
                                    .color(fg)
                                    .strong(),
                            );
                            if !row.subject.is_empty() {
                                ui.label(RichText::new(&row.subject).size(12.0).color(theme::TEXT));
                                if ui.small_button("複製").clicked() {
                                    let (c, l) = parse_subject_identity(&row.subject);
                                    copy_req = Some((c, l));
                                }
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(&row.at).size(11.0).color(theme::TEXT_MUTED),
                                );
                            });
                        });
                    }
                });
            if let Some((c, l)) = copy_req {
                self.copy_identity_text(ctx, &c, &l);
            }
        });
    }

    fn draw_master_dashboard(&self, ui: &mut egui::Ui) {
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
                    RichText::new(format!("需覆核 needs_review：{}", self.master_needs_review))
                        .size(14.0)
                        .color(if self.master_needs_review > 0 {
                            theme::WARN
                        } else {
                            theme::OK
                        })
                        .strong(),
                );
                ui.add_space(16.0);
                ui.label(
                    RichText::new(format!("仍在場 / 僅入場：{}", self.master_still_inside))
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
                if self.master_preview_review.is_empty() {
                    cols[0].label(RichText::new("（無）").size(12.0).color(theme::TEXT_MUTED));
                } else {
                    for line in &self.master_preview_review {
                        cols[0].label(RichText::new(line).size(12.0).color(theme::TEXT));
                    }
                }
                cols[1].label(
                    RichText::new("仍在場預覽")
                        .size(12.0)
                        .color(theme::TEXT_MUTED)
                        .strong(),
                );
                if self.master_preview_inside.is_empty() {
                    cols[1].label(RichText::new("（無）").size(12.0).color(theme::TEXT_MUTED));
                } else {
                    for line in &self.master_preview_inside {
                        cols[1].label(RichText::new(line).size(12.0).color(theme::TEXT));
                    }
                }
            });
        });
    }

    fn draw_settings_panel(&mut self, ui: &mut egui::Ui) {
        theme::card_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                RichText::new("活動 / 站點設定")
                    .size(13.0)
                    .color(theme::TEXT)
                    .strong(),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("活動名稱：");
                ui.add(
                    egui::TextEdit::singleline(&mut self.event_name_draft)
                        .desired_width(220.0)
                        .hint_text("例如：CPD 下午場"),
                );
                if ui.button("儲存活動").clicked() {
                    self.save_event_name();
                }
            });
            ui.horizontal(|ui| {
                ui.label("站點顯示名：");
                ui.add(
                    egui::TextEdit::singleline(&mut self.station_name_draft)
                        .desired_width(220.0)
                        .hint_text("入口-1 / 出口-2"),
                );
                if ui.button("儲存站點").clicked() {
                    self.save_station_name();
                }
            });
            ui.label(
                RichText::new("活動名稱會寫入 app_meta，並出現在 CSV 的 event 欄與檔名。站點名寫入 station.toml。")
                    .size(11.0)
                    .color(theme::TEXT_MUTED),
            );
        });
    }
}

impl eframe::App for AttendanceApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.counts_ready {
            self.refresh_counts();
            self.counts_ready = true;
        }

        // Keep clock / age labels live
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            self.toggle_fullscreen(ctx);
        }

        // Top bar
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
                        let ev = if self.event_name.is_empty() {
                            "（未設定活動名稱）".to_string()
                        } else {
                            self.event_name.clone()
                        };
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                if self.role == DbRole::Master {
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

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Live clock
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
                                    self.feedback_system(
                                        "關於",
                                        format!(
                                            "{APP_TITLE_EN} {} · {} · 站點 {} ({}) · 音效 {} · F11 全螢幕",
                                            env!("CARGO_PKG_VERSION"),
                                            self.db_path.display(),
                                            self.station.station_name,
                                            short_id(&self.station.station_id),
                                            if self.sound_enabled { "開" } else { "關" }
                                        ),
                                        StatusTone::Info,
                                    );
                                    ui.close_menu();
                                }
                            });
                            ui.menu_button("檢視", |ui| {
                                let fs = if self.fullscreen {
                                    "結束全螢幕"
                                } else {
                                    "全螢幕 (F11)"
                                };
                                if ui.button(fs).clicked() {
                                    self.toggle_fullscreen(ctx);
                                    ui.close_menu();
                                }
                                let sound_label = if self.sound_enabled {
                                    "音效：開（點擊關閉）"
                                } else {
                                    "音效：關（點擊開啟）"
                                };
                                if ui.button(sound_label).clicked() {
                                    self.toggle_sound();
                                    ui.close_menu();
                                }
                                if ui.button("活動 / 站點設定…").clicked() {
                                    self.settings_open = !self.settings_open;
                                    ui.close_menu();
                                }
                            });
                            ui.menu_button("檔案", |ui| {
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

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(16, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    theme::banner_frame(theme::SURFACE_MUTED)
                        .inner_margin(Margin::symmetric(10, 4))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "站點 {} · {}",
                                    self.station.station_name,
                                    short_id(&self.station.station_id)
                                ))
                                .size(12.0)
                                .color(theme::TEXT_MUTED),
                            );
                        });
                    ui.label(
                        RichText::new(format!(
                            "時段 成功{} 提醒{} 錯誤{} · 音效{}",
                            self.session_ok,
                            self.session_warn,
                            self.session_err,
                            if self.sound_enabled { "開" } else { "關" }
                        ))
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                    );
                });

                if self.settings_open {
                    ui.add_space(8.0);
                    self.draw_settings_panel(ui);
                }

                ui.add_space(8.0);

                if self.role == DbRole::Desk {
                    ui.horizontal(|ui| {
                        let half = (ui.available_width() - 10.0) / 2.0;
                        ui.allocate_ui_with_layout(
                            egui::vec2(half, 60.0),
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
                        ui.add_space(10.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(half, 60.0),
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
                    ui.add_space(8.0);
                } else {
                    theme::banner_frame(theme::INFO_BG).show(ui, |ui| {
                        ui.label(
                            RichText::new("主控：掃描停用 · 匯入站點包後看下方儀表板")
                                .size(13.0)
                                .color(theme::INFO),
                        );
                    });
                    ui.add_space(8.0);
                }

                // Scan
                theme::card_frame().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(
                        RichText::new("中介人一戶通 QR")
                            .size(13.0)
                            .color(theme::TEXT_MUTED)
                            .strong(),
                    );
                    let hint = match self.mode {
                        AttendanceMode::CheckIn => "入場 — 掃描後 Enter",
                        AttendanceMode::CheckOut => "離場 — 可跨站",
                    };
                    let edit = egui::TextEdit::singleline(&mut self.scan_input)
                        .desired_width(f32::INFINITY)
                        .hint_text(if self.role == DbRole::Master {
                            "主控無需掃描"
                        } else {
                            hint
                        })
                        .margin(egui::vec2(10.0, 10.0));
                    let resp = ui.add_sized([ui.available_width(), 40.0], edit);
                    if self.focus_scan && self.role == DbRole::Desk {
                        resp.request_focus();
                        self.focus_scan = false;
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && (resp.has_focus() || resp.lost_focus())
                    {
                        self.submit_scan();
                    }
                });

                ui.add_space(8.0);

                // Metrics
                let card_w = ((ui.available_width() - 20.0) / 3.0).max(100.0);
                ui.horizontal(|ui| {
                    if self.role == DbRole::Master {
                        Self::draw_metric_card(
                            ui,
                            "總出席（去重）",
                            self.master_unique,
                            theme::INFO,
                            card_w,
                        );
                        ui.add_space(8.0);
                        Self::draw_metric_card(
                            ui,
                            "需覆核",
                            self.master_needs_review,
                            theme::WARN,
                            card_w,
                        );
                        ui.add_space(8.0);
                        Self::draw_metric_card(
                            ui,
                            "仍在場",
                            self.master_still_inside,
                            theme::CHECK_OUT,
                            card_w,
                        );
                    } else {
                        Self::draw_metric_card(
                            ui,
                            "目前在場",
                            self.counts_inside,
                            theme::CHECK_IN,
                            card_w,
                        );
                        ui.add_space(8.0);
                        Self::draw_metric_card(
                            ui,
                            "累計人次",
                            self.counts_visits,
                            theme::TEXT,
                            card_w,
                        );
                        ui.add_space(8.0);
                        Self::draw_metric_card(
                            ui,
                            "本機成功",
                            self.session_ok,
                            theme::OK,
                            card_w,
                        );
                    }
                });

                ui.add_space(8.0);
                self.draw_last_result(ui);
                ui.add_space(8.0);
                if self.role == DbRole::Master {
                    self.draw_master_dashboard(ui);
                    ui.add_space(8.0);
                }
                self.draw_recent_list(ui, ctx);
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
