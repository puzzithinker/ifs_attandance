//! Chinese font load: system-first, then embedded fallback strategy.
//!
//! On Linux CI hosts, system Windows fonts are absent; we try common CJK paths
//! and fall back to egui default (may tofu Chinese). Production Windows loads YaHei.

use eframe::egui;

/// Install CJK-capable fonts into egui context when available on disk.
pub fn install_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let candidates = system_font_candidates();
    let mut loaded = false;
    for path in candidates {
        if let Ok(bytes) = std::fs::read(&path) {
            fonts.font_data.insert(
                "ifs_cjk".to_owned(),
                egui::FontData::from_owned(bytes).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "ifs_cjk".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "ifs_cjk".to_owned());
            loaded = true;
            break;
        }
    }
    if loaded {
        ctx.set_fonts(fonts);
    }
    // Embedded Noto subset would go here via include_bytes! when asset is shipped.
}

fn system_font_candidates() -> Vec<std::path::PathBuf> {
    let mut v = Vec::new();
    // Windows
    for name in [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msjh.ttc",
        r"C:\Windows\Fonts\msjh.ttf",
        r"C:\Windows\Fonts\msyhbd.ttc",
    ] {
        v.push(std::path::PathBuf::from(name));
    }
    // Linux common CJK
    for name in [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/truetype/arphic/uming.ttc",
    ] {
        v.push(std::path::PathBuf::from(name));
    }
    v
}
