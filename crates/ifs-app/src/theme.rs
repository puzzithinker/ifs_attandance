//! Kiosk visual theme — soft light surface, strong mode colors, clear status.

use eframe::egui::{self, Color32, CornerRadius, Margin, Shadow, Stroke, Visuals};

/// Soft light palette (readable under seminar hall lighting).
pub const BG: Color32 = Color32::from_rgb(244, 246, 250);
pub const SURFACE: Color32 = Color32::from_rgb(255, 255, 255);
pub const SURFACE_MUTED: Color32 = Color32::from_rgb(236, 239, 245);
pub const BORDER: Color32 = Color32::from_rgb(210, 216, 228);
pub const TEXT: Color32 = Color32::from_rgb(28, 35, 51);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(100, 112, 130);

pub const CHECK_IN: Color32 = Color32::from_rgb(22, 163, 74);
pub const CHECK_IN_SOFT: Color32 = Color32::from_rgb(220, 252, 231);
pub const CHECK_OUT: Color32 = Color32::from_rgb(234, 88, 12);
pub const CHECK_OUT_SOFT: Color32 = Color32::from_rgb(255, 237, 213);

pub const OK: Color32 = Color32::from_rgb(22, 163, 74);
pub const OK_BG: Color32 = Color32::from_rgb(220, 252, 231);
pub const WARN: Color32 = Color32::from_rgb(202, 138, 4);
pub const WARN_BG: Color32 = Color32::from_rgb(254, 249, 195);
pub const ERR: Color32 = Color32::from_rgb(220, 38, 38);
pub const ERR_BG: Color32 = Color32::from_rgb(254, 226, 226);
pub const INFO: Color32 = Color32::from_rgb(37, 99, 235);
pub const INFO_BG: Color32 = Color32::from_rgb(219, 234, 254);
pub const NEUTRAL_BG: Color32 = Color32::from_rgb(241, 245, 249);

pub const ROUND: u8 = 12;
pub const ROUND_SM: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusTone {
    Neutral,
    Success,
    Warning,
    Error,
    Info,
}

impl StatusTone {
    pub fn colors(self) -> (Color32, Color32) {
        match self {
            Self::Neutral => (TEXT_MUTED, NEUTRAL_BG),
            Self::Success => (OK, OK_BG),
            Self::Warning => (WARN, WARN_BG),
            Self::Error => (ERR, ERR_BG),
            Self::Info => (INFO, INFO_BG),
        }
    }
}

/// Apply light kiosk visuals once at startup.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::light();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = SURFACE_MUTED;
    visuals.widgets.noninteractive.bg_fill = SURFACE;
    visuals.widgets.inactive.bg_fill = SURFACE_MUTED;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(226, 232, 240);
    visuals.widgets.active.bg_fill = Color32::from_rgb(203, 213, 225);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.selection.bg_fill = Color32::from_rgb(191, 219, 254);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(ROUND_SM);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(ROUND_SM);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(ROUND_SM);
    visuals.widgets.active.corner_radius = CornerRadius::same(ROUND_SM);
    visuals.window_corner_radius = CornerRadius::same(ROUND);
    visuals.menu_corner_radius = CornerRadius::same(ROUND_SM);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(26.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(16.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(14.0));
    ctx.set_style(style);
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE)
        .corner_radius(CornerRadius::same(ROUND))
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(Margin::symmetric(18, 14))
        .shadow(Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: Color32::from_black_alpha(12),
        })
}

pub fn banner_frame(bg: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(ROUND_SM))
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(Margin::symmetric(16, 14))
}
