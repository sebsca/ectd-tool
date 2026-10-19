// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

mod app;

use ectd_rust::{dossier::Dossier, export::export_consolidated};
use std::{path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("eCTD Viewer: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    if first.as_deref().is_some_and(|a| a == "--help" || a == "-h") {
        println!(
            "eCTD Viewer (Rust)\n\nUsage:\n  ectd-viewer [DOSSIER]\n  ectd-viewer --inspect DOSSIER\n  ectd-viewer --export DOSSIER SEQUENCE DESTINATION\n\nWithout flags, opens the desktop viewer. --inspect prints JSON without opening a window.\n--export copies active documents into a new or empty destination; no new XML is generated."
        );
        return Ok(());
    }
    if first.as_deref().is_some_and(|a| a == "--inspect") {
        let root = args.next().ok_or("Usage: ectd-viewer --inspect DOSSIER")?;
        if args.next().is_some() {
            return Err("Too many arguments for --inspect".into());
        }
        let dossier = Dossier::load(&PathBuf::from(root))?;
        println!(
            "{}",
            serde_json::to_string_pretty(&dossier).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if first.as_deref().is_some_and(|a| a == "--export") {
        let root = args
            .next()
            .ok_or("Usage: ectd-viewer --export DOSSIER SEQUENCE DESTINATION")?;
        let seq = args
            .next()
            .ok_or("Missing sequence")?
            .into_string()
            .map_err(|_| "Invalid sequence")?;
        let destination = args.next().ok_or("Missing destination")?;
        if args.next().is_some() {
            return Err("Too many arguments for --export".into());
        }
        let dossier = Dossier::load(&PathBuf::from(root))?;
        let report = export_consolidated(&dossier, &seq, &PathBuf::from(destination))?;
        println!("{}", report.summary());
        if !report.errors.is_empty() || report.missing > 0 {
            return Err("Export incomplete; see report above.".into());
        }
        return Ok(());
    }
    if first
        .as_deref()
        .is_some_and(|a| a.to_string_lossy().starts_with('-'))
        || args.next().is_some()
    {
        return Err("Invalid arguments. Run with --help for usage.".into());
    }
    let root = first.map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(app::APP_TITLE)
            .with_decorations(true)
            .with_inner_size([1500.0, 850.0])
            .with_min_inner_size([900.0, 550.0]),
        ..Default::default()
    };
    eframe::run_native(
        "eCTD Viewer (Rust)",
        options,
        Box::new(move |cc| Ok(Box::new(app::ViewerApp::new(cc, root)))),
    )
    .map_err(|e| e.to_string())
}
