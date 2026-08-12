//! UI widget modules — each renders one panel of the kiosk shell.

pub mod master_dashboard;
pub mod metrics;
pub mod mode_selector;
pub mod open_db;
pub mod recent_list;
pub mod settings;
pub mod status_banner;
pub mod top_bar;

/// Truncate a station id to a short display form for badges/menus.
pub(crate) fn short_id(id: &str) -> String {
    if id.len() <= 8 {
        id.to_string()
    } else {
        format!("{}…", &id[..8])
    }
}
