//! Session stat chips (UX #4) + last-result banner.

use crate::app::AttendanceApp;
use crate::theme::{self, StatusTone};
use crate::ui::short_id;
use crate::ui_format::{elapsed_secs, format_seconds_ago};
use eframe::egui::{self, Color32, Margin, RichText};
use std::time::SystemTime;

/// Station badge + three tone-colored session chips + muted sound label.
pub fn show_session_chips(app: &AttendanceApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        // Station badge (kept from original).
        theme::banner_frame(theme::SURFACE_MUTED)
            .inner_margin(Margin::symmetric(10, 4))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!(
                        "站點 {} · {}",
                        app.store.station().station_name,
                        short_id(&app.store.station().station_id)
                    ))
                    .size(12.0)
                    .color(theme::TEXT_MUTED),
                );
            });

        // Session chips — UX improvement #4.
        draw_chip(ui, "✓ 成功", app.session_ok, theme::OK, theme::OK_BG);
        draw_chip(ui, "! 提醒", app.session_warn, theme::WARN, theme::WARN_BG);
        draw_chip(ui, "✕ 錯誤", app.session_err, theme::ERR, theme::ERR_BG);

        // Muted sound label.
        ui.label(
            RichText::new(if app.sound_enabled {
                "音效開"
            } else {
                "音效關"
            })
            .size(12.0)
            .color(theme::TEXT_MUTED),
        );
    });
}

fn draw_chip(ui: &mut egui::Ui, label: &str, count: u64, fg: Color32, bg: Color32) {
    theme::banner_frame(bg)
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!("{label} {count}"))
                    .size(12.0)
                    .color(fg)
                    .strong(),
            );
        });
}

/// Last scan result banner — icon + headline + subject + detail + age + timestamp.
pub fn show_last_result(app: &AttendanceApp, ui: &mut egui::Ui) {
    let (headline, subject, detail, tone, at) = match &app.last {
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
    let age = app
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
                            RichText::new("最近結果")
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
                        ui.label(
                            RichText::new(subject)
                                .size(18.0)
                                .color(theme::TEXT)
                                .strong(),
                        );
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
