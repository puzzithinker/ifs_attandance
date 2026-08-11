//! IFS Event Attendance — eframe GUI + CLI smoke mode.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod feedback;
mod fonts;
mod paths;
mod sound;
mod theme;
mod ui;
mod ui_format;

use clap::Parser;
use ifs_storage::{get_event_name, open_database_with_station, DbRole};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "ifs_attendance",
    about = "IFS Event Attendance — 活動出席 kiosk (single-exe)"
)]
struct Args {
    /// Path to SQLite database (default: ./agent.db or next to exe).
    #[arg(long)]
    db: Option<PathBuf>,

    /// Open as master merge database (no desk open-visit uniqueness).
    #[arg(long)]
    master: bool,

    /// Headless smoke: open DB, print version/counts, exit (no GUI).
    #[arg(long)]
    smoke: bool,

    /// Soft check-out when not locally checked in (default: on).
    #[arg(long, default_value_t = true)]
    soft_checkout: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let db_path = paths::resolve_db_path(args.db.as_deref());
    let role = if args.master {
        DbRole::Master
    } else {
        DbRole::Desk
    };

    if args.smoke {
        return run_smoke(&db_path, role);
    }

    match app::run_gui(db_path, role, args.soft_checkout) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ifs-app error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_smoke(db_path: &std::path::Path, role: DbRole) -> ExitCode {
    println!("IFS Event Attendance {} smoke", env!("CARGO_PKG_VERSION"));
    println!("db={}", db_path.display());
    match open_database_with_station(db_path, role) {
        Ok((conn, report, station)) => {
            let repo = ifs_storage::SqliteVisitRepository::new(&conn, station.clone(), role);
            let counts = repo.counts().unwrap_or_default();
            let event = get_event_name(&conn).unwrap_or_default();
            println!(
                "station={} ({}) migrate={}->{} inside={} visits={} event={}",
                station.station_name,
                station.station_id,
                report.from_version,
                report.to_version,
                counts.currently_inside,
                counts.total_visits,
                if event.is_empty() { "(none)" } else { &event }
            );
            println!("modes: 入場 Check-In | 離場 Check-Out");
            println!("gui: event/station editors, clock, copy, master dashboard, F11, sound");
            println!(
                "status sample: {}",
                ifs_core::message_zh(&ifs_core::ScanOutcome::EmptyInput)
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("smoke open failed: {e}");
            ExitCode::FAILURE
        }
    }
}
