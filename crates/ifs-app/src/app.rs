//! eframe kiosk shell — app state, business logic, and update orchestration.
//! All per-widget layout code lives in `ui/` modules; this file orchestrates.

use crate::feedback::{detail_for_outcome, headline_for_outcome, tone_for_outcome, ScanFeedback};
use crate::fonts;
use crate::sound;
use crate::theme::{self, StatusTone};
use crate::ui;
use crate::ui_format::format_copy_identity;
use eframe::egui::{self, Margin, RichText, ViewportCommand};
use ifs_core::{AttendanceMode, MasterAgentRow, MasterStatus};
use ifs_storage::{
    default_export_filename, master_export_filename, now_iso_local, AttendanceStore, DbRole,
};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::SystemTime;

const APP_TITLE_EN: &str = "IFS Event Attendance";
const RECENT_MAX: usize = 12;
const MASTER_PREVIEW_MAX: usize = 8;

pub fn run_gui(db_path: PathBuf, role: DbRole, soft_checkout: bool) -> Result<(), String> {
    let store = AttendanceStore::open(&db_path, role, soft_checkout).map_err(|e| e.to_string())?;
    let event_name = store.event_name();
    let sound_enabled = store.sound_enabled();
    let station_name = store.station().station_name.clone();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 780.0])
            .with_min_inner_size([600.0, 640.0])
            .with_title("IFS 活動出席 · IFS Event Attendance"),
        ..Default::default()
    };

    let mut app = AttendanceApp {
        store,
        db_path,
        mode: AttendanceMode::CheckIn,
        scan_input: String::new(),
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
        db_dialog_open: false,
        db_dialog_path: String::new(),
        db_dialog_role: role,
    };
    app.event_name_draft = app.event_name.clone();
    app.station_name_draft = station_name;
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

pub(crate) struct AttendanceApp {
    pub(crate) store: AttendanceStore,
    pub(crate) db_path: PathBuf,
    pub(crate) mode: AttendanceMode,
    pub(crate) scan_input: String,
    pub(crate) counts_ready: bool,
    pub(crate) counts_inside: u64,
    pub(crate) counts_visits: u64,
    pub(crate) master_unique: u64,
    pub(crate) last: Option<ScanFeedback>,
    pub(crate) last_scan_at: Option<SystemTime>,
    pub(crate) recent: VecDeque<ScanFeedback>,
    pub(crate) session_ok: u64,
    pub(crate) session_warn: u64,
    pub(crate) session_err: u64,
    pub(crate) event_name: String,
    pub(crate) event_name_draft: String,
    pub(crate) station_name_draft: String,
    pub(crate) settings_open: bool,
    pub(crate) fullscreen: bool,
    pub(crate) sound_enabled: bool,
    pub(crate) master_needs_review: u64,
    pub(crate) master_still_inside: u64,
    pub(crate) master_preview_review: Vec<String>,
    pub(crate) master_preview_inside: Vec<String>,
    pub(crate) copy_flash: Option<(String, SystemTime)>,
    pub(crate) db_dialog_open: bool,
    pub(crate) db_dialog_path: String,
    pub(crate) db_dialog_role: DbRole,
}

impl AttendanceApp {
    fn refresh_counts(&mut self) {
        if let Ok(c) = self.store.counts() {
            self.counts_inside = c.currently_inside;
            self.counts_visits = c.total_visits;
        }
        if self.store.role() == DbRole::Master {
            if let Ok(rows) = self.store.master_rollup_rows() {
                self.apply_master_rollup(&rows);
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

    fn push_scan_feedback(&mut self, fb: ScanFeedback) {
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

    /// System messages (export/import/settings/about): banner only, never the
    /// scan list/counters/sound — they are not scans.
    pub(crate) fn feedback_system(
        &mut self,
        headline: impl Into<String>,
        detail: impl Into<String>,
        tone: StatusTone,
    ) {
        let at = now_iso_local();
        self.last = Some(ScanFeedback {
            headline: headline.into(),
            subject: String::new(),
            detail: detail.into(),
            tone,
            at,
        });
    }

    pub(crate) fn save_event_name(&mut self) {
        let name = self.event_name_draft.trim().to_string();
        if let Err(e) = self.store.set_event_name(&name) {
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

    pub(crate) fn save_station_name(&mut self) {
        let name = self.station_name_draft.trim().to_string();
        if name.is_empty() {
            self.feedback_system("站點名稱不可空白", "", StatusTone::Warning);
            return;
        }
        if let Err(e) = self.store.rename_station(&name) {
            self.feedback_system("儲存站點名稱失敗", e.to_string(), StatusTone::Error);
            return;
        }
        self.station_name_draft = self.store.station().station_name.clone();
        self.feedback_system(
            "站點名稱已儲存",
            self.store.station().station_name.clone(),
            StatusTone::Success,
        );
    }

    pub(crate) fn toggle_sound(&mut self) {
        self.sound_enabled = !self.sound_enabled;
        let _ = self.store.set_sound_enabled(self.sound_enabled);
    }

    pub(crate) fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
    }

    pub(crate) fn copy_identity_text(
        &mut self,
        ctx: &egui::Context,
        category: &str,
        license: &str,
    ) {
        let text = format_copy_identity(category, license);
        if text.is_empty() {
            return;
        }
        ctx.copy_text(text.clone());
        self.copy_flash = Some((text, SystemTime::now()));
    }

    pub(crate) fn submit_scan(&mut self) {
        let raw = self.scan_input.clone();
        self.scan_input.clear();

        if self.store.role() == DbRole::Master {
            self.feedback_system(
                "主控模式",
                "此庫不接受掃描 — 請匯入站點包後匯出主控 CSV",
                StatusTone::Info,
            );
            return;
        }

        let now = now_iso_local();
        let mode = self.mode;
        let result = self.store.handle_scan(&raw, mode, &now);
        let subject = result
            .identity
            .map(|id| format!("{}  ·  {}", id.category, id.license_no))
            .unwrap_or_default();
        self.push_scan_feedback(ScanFeedback {
            headline: headline_for_outcome(&result.outcome),
            subject,
            detail: detail_for_outcome(&result.outcome, mode),
            tone: tone_for_outcome(&result.outcome),
            at: result.at,
        });
        self.refresh_counts();
    }

    pub(crate) fn export_csv_dialog(&mut self) {
        let name = default_export_filename(chrono::Local::now(), &self.event_name);
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&name)
            .add_filter("CSV", &["csv"])
            .save_file()
        {
            match self.store.export_csv(&path, &self.event_name) {
                Ok(n) => self.feedback_system(
                    "已匯出 CSV",
                    format!("{n} 筆 → {}", path.display()),
                    StatusTone::Success,
                ),
                Err(e) => self.feedback_system("匯出失敗", e.to_string(), StatusTone::Error),
            }
        }
    }

    pub(crate) fn export_package_dialog(&mut self) {
        let name = format!(
            "IFS_station_{}_{}.db",
            self.store.station().station_name,
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        );
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&name)
            .add_filter("SQLite", &["db"])
            .save_file()
        {
            match self.store.export_package(&path) {
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

    pub(crate) fn import_package_dialog(&mut self) {
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter("SQLite package", &["db"])
            .pick_files()
        {
            let mut total_ins = 0u64;
            let mut total_skip = 0u64;
            let n_files = paths.len();
            for p in paths {
                match self.store.import_package(&p) {
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

    pub(crate) fn open_database(&mut self, path: PathBuf, role: DbRole) {
        match AttendanceStore::open(&path, role, self.store.soft_checkout()) {
            Ok(store) => self.adopt_store(store, path),
            Err(e) => self.feedback_system("開啟資料庫失敗", e.to_string(), StatusTone::Error),
        }
    }

    fn adopt_store(&mut self, store: AttendanceStore, db_path: PathBuf) {
        self.store = store;
        self.db_path = db_path;
        self.mode = AttendanceMode::CheckIn;
        self.scan_input.clear();
        self.last = None;
        self.last_scan_at = None;
        self.recent.clear();
        self.session_ok = 0;
        self.session_warn = 0;
        self.session_err = 0;
        self.event_name = self.store.event_name();
        self.event_name_draft = self.event_name.clone();
        self.station_name_draft = self.store.station().station_name.clone();
        self.sound_enabled = self.store.sound_enabled();
        self.master_needs_review = 0;
        self.master_still_inside = 0;
        self.master_preview_review.clear();
        self.master_preview_inside.clear();
        self.copy_flash = None;
        self.refresh_counts();
        let role_label = if self.store.role() == DbRole::Master {
            "主控"
        } else {
            "簽到站"
        };
        self.feedback_system(
            "資料庫已開啟",
            format!("{role_label} · {}", self.db_path.display()),
            StatusTone::Success,
        );
    }

    pub(crate) fn export_master_csv_dialog(&mut self) {
        let rows = match self.store.master_rollup_rows() {
            Ok(r) => r,
            Err(e) => {
                self.feedback_system("主控匯出失敗", e.to_string(), StatusTone::Error);
                return;
            }
        };
        self.apply_master_rollup(&rows);
        let name = master_export_filename(chrono::Local::now(), &self.event_name);
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&name)
            .add_filter("CSV", &["csv"])
            .save_file()
        {
            match self.store.export_master_csv(&path, &self.event_name) {
                Ok(n) => self.feedback_system(
                    "主控 CSV 已匯出",
                    format!("{n} 位 → {}", path.display()),
                    StatusTone::Success,
                ),
                Err(e) => self.feedback_system("主控匯出失敗", e.to_string(), StatusTone::Error),
            }
        }
    }
}

impl eframe::App for AttendanceApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.counts_ready {
            self.refresh_counts();
            self.counts_ready = true;
        }

        // Keep clock / age labels live.
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        // F11 fullscreen.
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            self.toggle_fullscreen(ctx);
        }

        // F1/F2 mode shortcuts (Desk only) — UX improvement #2.
        if self.store.role() == DbRole::Desk {
            if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
                self.mode = AttendanceMode::CheckIn;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
                self.mode = AttendanceMode::CheckOut;
            }
        }

        // Top bar.
        ui::top_bar::show(self, ctx);

        // Settings floating window (UX #3) — does not shift central panel.
        ui::settings::show(self, ctx);

        ui::open_db::show(self, ctx);

        // Central panel.
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(16, 10)),
            )
            .show(ctx, |ui| {
                // Session chips + station badge — UX #4.
                ui::status_banner::show_session_chips(self, ui);
                ui.add_space(8.0);

                if self.store.role() == DbRole::Desk {
                    ui::mode_selector::show(self, ui);
                } else {
                    theme::banner_frame(theme::INFO_BG).show(ui, |ui| {
                        ui.label(
                            RichText::new("主控：掃描停用 · 匯入站點包後看下方儀表板")
                                .size(13.0)
                                .color(theme::INFO),
                        );
                    });
                }
                ui.add_space(8.0);

                // Scan card (with big mode banner for Desk — UX #1).
                ui::mode_selector::show_scan_card(self, ui, ctx);
                ui.add_space(8.0);

                // Metrics.
                ui::metrics::show(self, ui);
                ui.add_space(8.0);

                // Last result banner.
                ui::status_banner::show_last_result(self, ui);
                ui.add_space(8.0);

                if self.store.role() == DbRole::Master {
                    ui::master_dashboard::show(self, ui);
                    ui.add_space(8.0);
                }

                // Recent list (max_height 220 — UX #5).
                ui::recent_list::show(self, ui, ctx);
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> AttendanceApp {
        AttendanceApp {
            store: AttendanceStore::open_in_memory(DbRole::Desk).unwrap(),
            db_path: PathBuf::new(),
            mode: AttendanceMode::CheckIn,
            scan_input: String::new(),
            counts_ready: true,
            counts_inside: 0,
            counts_visits: 0,
            master_unique: 0,
            last: None,
            last_scan_at: None,
            recent: VecDeque::new(),
            session_ok: 0,
            session_warn: 0,
            session_err: 0,
            event_name: String::new(),
            event_name_draft: String::new(),
            station_name_draft: String::new(),
            settings_open: false,
            fullscreen: false,
            sound_enabled: false,
            master_needs_review: 0,
            master_still_inside: 0,
            master_preview_review: Vec::new(),
            master_preview_inside: Vec::new(),
            copy_flash: None,
            db_dialog_open: false,
            db_dialog_path: String::new(),
            db_dialog_role: DbRole::Desk,
        }
    }

    #[test]
    fn system_messages_stay_out_of_scan_list_and_counters() {
        let mut app = test_app();
        app.feedback_system("關於", "版本資訊", StatusTone::Info);
        assert!(
            app.recent.is_empty(),
            "system message must not enter recent scans"
        );
        assert_eq!(app.session_ok, 0);
        assert_eq!(app.session_warn, 0);
        assert_eq!(app.session_err, 0);
        assert!(app.last.is_some(), "banner should still show the message");
        assert!(app.last_scan_at.is_none(), "no scan happened");
    }

    #[test]
    fn real_scans_enter_recent_list_and_counters() {
        let mut app = test_app();
        app.scan_input = "https://example.hk/?categoryCode=IA&licenseNo=T-1".into();
        app.submit_scan();
        assert_eq!(app.recent.len(), 1);
        assert_eq!(app.session_ok, 1);
        assert!(app.last_scan_at.is_some());
        assert_eq!(app.counts_visits, 1);
    }

    #[test]
    fn master_mode_scan_rejection_also_stays_out_of_scan_list() {
        let mut app = AttendanceApp {
            store: AttendanceStore::open_in_memory(DbRole::Master).unwrap(),
            ..test_app()
        };
        app.scan_input = "https://example.hk/?categoryCode=IA&licenseNo=T-1".into();
        app.submit_scan();
        assert!(app.recent.is_empty());
        assert_eq!(app.session_ok, 0);
    }

    #[test]
    fn adopt_store_resets_session_state_and_switches_role() {
        let mut app = test_app();
        app.scan_input = "https://example.hk/?categoryCode=IA&licenseNo=T-1".into();
        app.submit_scan();
        assert_eq!(app.recent.len(), 1);
        assert_eq!(app.session_ok, 1);

        let master = AttendanceStore::open_in_memory(DbRole::Master).unwrap();
        app.adopt_store(master, PathBuf::from("master.db"));

        assert_eq!(app.store.role(), DbRole::Master);
        assert_eq!(app.db_path, PathBuf::from("master.db"));
        assert!(app.recent.is_empty(), "old DB's scans must not carry over");
        assert_eq!(app.session_ok, 0);
        assert_eq!(app.session_warn, 0);
        assert_eq!(app.session_err, 0);
        assert!(app.last.is_some(), "banner confirms the switch");
        assert_eq!(app.master_unique, 0);
    }
}
