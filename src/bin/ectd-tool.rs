// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use ectd_rust::tool::{Options, run};
use std::{path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    match cli() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ERROR: {e}");
            ExitCode::FAILURE
        }
    }
}
fn cli() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let mut positional = vec![];
    let mut options = Options {
        base: PathBuf::new(),
        sequence: String::new(),
        scan: false,
        extract: false,
        metadata: None,
        mapping: None,
        eu_mapping: None,
        yes: false,
    };
    while let Some(arg) = args.next() {
        match arg.to_str().unwrap_or("") {
            "--help" | "-h" => {
                println!(
                    "eCTD-Tool {} (Rust)\n\nUsage: ectd-tool BASE_DIRECTORY SEQUENCE [-scan] [-extractXML] [-metadata PATH] [-mapfile PATH] [-eu_mapfile PATH] [--yes]\n\n-scan         Append unlisted document files to the metadata workbook; do not generate XML.\n-extractXML   Extract ordered document/branch metadata from index.xml and regional backbones.\n-metadata     Metadata workbook; relative paths are resolved against BASE_DIRECTORY.\n-mapfile      Override the embedded ICH path-to-element CSV mapping.\n-eu_mapfile   Override the embedded EU M1 path-to-element CSV mapping.\n--yes, -y     Allow overwriting existing output files without an interactive prompt.\n\nGeneration requires SEQUENCE/util/dtd/ich-ectd-3-2.dtd. EU DTDs are used if provided.\nSequence directories like 0001 are found when SEQUENCE is given as 1.",
                    env!("CARGO_PKG_VERSION")
                );
                return Ok(());
            }
            "--version" | "-version" | "-V" => {
                println!("eCTD-Tool {} (Rust)", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-scan" | "--scan" => options.scan = true,
            "-extractXML" | "--extractXML" | "--extract-xml" => options.extract = true,
            "--yes" | "-y" => options.yes = true,
            "-metadata" | "--metadata" => {
                options.metadata = Some(args.next().ok_or("-metadata requires a path")?.into())
            }
            "-mapfile" | "--mapfile" => {
                options.mapping = Some(args.next().ok_or("-mapfile requires a path")?.into())
            }
            "-eu_mapfile" | "--eu-mapfile" => {
                options.eu_mapping = Some(args.next().ok_or("-eu_mapfile requires a path")?.into())
            }
            flag if flag.starts_with('-') => {
                return Err(format!("Unknown option: {flag}. See --help."));
            }
            _ => positional.push(arg),
        }
    }
    if positional.len() != 2 {
        return Err("Usage: ectd-tool BASE_DIRECTORY SEQUENCE [OPTIONS]. See --help.".into());
    }
    options.base = positional.remove(0).into();
    options.sequence = positional
        .remove(0)
        .into_string()
        .map_err(|_| "Invalid sequence number")?;
    println!("eCTD-Tool {} (Rust)", env!("CARGO_PKG_VERSION"));
    run(options)
}
