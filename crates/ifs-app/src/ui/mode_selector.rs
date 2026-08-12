//! Mode selection pills (Desk only) + scan input card with big mode banner.

use crate::app::AttendanceApp;
use crate::theme;
use eframe::egui::{self, Align, Color32, CornerRadius, Layout, RichText, Sense, StrokeKind};
use ifs_core::AttendanceMode;
use ifs_storage::DbRole;

/// Draw the two mode-selection pills (Desk role only).
pub fn show(app: &mut AttendanceApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let half = (ui.available_width() - 10.0) / 2.0;
        ui.allocate_ui_with_layout(
            egui::vec2(half, 60.0),
            Layout::top_down(Align::Center),
            |ui| {
                draw_mode_pill(
                    app,
                    ui,
                    AttendanceMode::CheckIn,
                    "入場",
                    "Check-In · F1",
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
                draw_mode_pill(
                    app,
                    ui,
                    AttendanceMode::CheckOut,
                    "離場",
                    "Check-Out · F2",
                    theme::CHECK_OUT,
                    theme::CHECK_OUT_SOFT,
                );
            },
        );
    });
}

fn draw_mode_pill(
    app: &mut AttendanceApp,
    ui: &mut egui::Ui,
    mode: AttendanceMode,
    label: &str,
    sub: &str,
    active_fill: Color32,
    soft_fill: Color32,
) {
    let selected = app.mode == mode;
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
        app.mode = mode;
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

/// Draw the scan input card. For Desk role, includes a full-width mode banner
/// at the top (~48px) showing the current mode in large bold text.
pub fn show_scan_card(app: &mut AttendanceApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    theme::card_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(
            RichText::new("中介人一戶通 QR")
                .size(13.0)
                .color(theme::TEXT_MUTED)
                .strong(),
        );

        // Big mode banner (Desk only) — UX improvement #1.
        if app.store.role() == DbRole::Desk {
            draw_mode_banner(ui, app.mode);
            ui.add_space(4.0);
        }

        let hint = match app.mode {
            AttendanceMode::CheckIn => "入場 — 掃描後 Enter",
            AttendanceMode::CheckOut => "離場 — 可跨站",
        };
        let edit = egui::TextEdit::singleline(&mut app.scan_input)
            .desired_width(f32::INFINITY)
            .hint_text(if app.store.role() == DbRole::Master {
                "主控無需掃描"
            } else {
                hint
            })
            .margin(egui::vec2(10.0, 10.0));
        let resp = ui.add_sized([ui.available_width(), 40.0], edit);

        // Enter handling — egui idiom only (bug fix).
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            app.submit_scan();
        }

        // Auto-refocus (kiosk-critical) — UX bug fix #2.
        // If Desk role, settings window closed, and nothing else has focus
        // (or the scan field itself has focus), reclaim focus so keystrokes
        // from the USB scanner always reach the scan field.
        if app.store.role() == DbRole::Desk && !app.settings_open && !app.db_dialog_open {
            let focused = ctx.memory(|m| m.focused());
            if focused.is_none() || focused == Some(resp.id) {
                resp.request_focus();
            }
        }
    });
}

/// Full-width strip at the top of the scan card showing the current mode.
fn draw_mode_banner(ui: &mut egui::Ui, mode: AttendanceMode) {
    let (soft, strong, label) = match mode {
        AttendanceMode::CheckIn => (theme::CHECK_IN_SOFT, theme::CHECK_IN, "入場模式 Check-In"),
        AttendanceMode::CheckOut => (
            theme::CHECK_OUT_SOFT,
            theme::CHECK_OUT,
            "離場模式 Check-Out",
        ),
    };
    let desired = egui::vec2(ui.available_width(), 48.0);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
    ui.painter().rect(
        rect,
        CornerRadius::same(theme::ROUND_SM),
        soft,
        egui::Stroke::new(1.0, theme::BORDER),
        StrokeKind::Inside,
    );
    let center_y = rect.center().y;
    ui.painter().text(
        egui::pos2(rect.left() + 16.0, center_y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(20.0),
        strong,
    );
    ui.painter().text(
        egui::pos2(rect.right() - 16.0, center_y),
        egui::Align2::RIGHT_CENTER,
        "F1 入場 · F2 離場",
        egui::FontId::proportional(12.0),
        theme::TEXT_MUTED,
    );
}
