// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod dtd;
pub mod extract;
pub mod generate;
pub mod mapping;
pub mod previous;
pub mod workbook;
pub mod xml;

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
};

pub type Row = BTreeMap<String, String>;
pub type Result<T> = std::result::Result<T, String>;
pub const FIELDS: [&str; 22] = [
    "file_path",
    "title",
    "attributes",
    "operation",
    "modified-leaf",
    "modified-href",
    "ctd_toc",
    "applicant_name",
    "submission_type",
    "sequence_description",
    "eu_country",
    "eu_identifier",
    "eu_submission_type",
    "eu_submission_mode",
    "eu_submission_number",
    "eu_procedure_number",
    "eu_submission_unit_type",
    "eu_agency_code",
    "eu_procedure_type",
    "eu_invented_name",
    "eu_inn",
    "eu_related_sequence",
];
pub const XLINK: &str = "http://www.w3.org/1999/xlink";
pub const MODULES: [&str; 5] = [
    "m1-administrative-information-and-prescribing-information",
    "m2-common-technical-document-summaries",
    "m3-quality",
    "m4-nonclinical-study-reports",
    "m5-clinical-study-reports",
];

pub fn value<'a>(row: &'a Row, key: &str) -> &'a str {
    row.get(key).map_or("", |s| s.trim())
}
pub fn toc(row: &Row) -> &str {
    ["CTD-TOC", "ctd_toc", "ctd-toc"]
        .iter()
        .map(|k| value(row, k))
        .find(|s| !s.is_empty())
        .unwrap_or("")
}
pub fn operation(row: &Row) -> String {
    let op = value(row, "operation").to_lowercase();
    if op.is_empty() { "new".into() } else { op }
}
pub fn path_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
pub fn norm(value: &str) -> String {
    value.trim().replace('\\', "/")
}
pub fn directory(seq: &Path, path: &str) -> bool {
    !path.is_empty() && (path.ends_with('/') || seq.join(path).is_dir())
}
pub fn warn(message: impl AsRef<str>) {
    eprintln!("WARNING: {}", message.as_ref());
}

pub fn checksum(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut digest = md5::Context::new();
    let mut buffer = [0; 65536];
    loop {
        let size = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if size == 0 {
            break;
        }
        digest.consume(&buffer[..size]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// Explicit --yes is available for scripts; otherwise keep the original interactive overwrite policy.
#[derive(Default)]
pub struct OutputPolicy {
    pub yes: bool,
    confirmed: HashSet<PathBuf>,
}
impl OutputPolicy {
    pub fn new(yes: bool) -> Self {
        Self {
            yes,
            ..Default::default()
        }
    }
    pub fn approve(&mut self, path: &Path, creating_metadata: bool) -> Result<()> {
        let exists = path.exists();
        if self.yes
            || self.confirmed.contains(path)
            || (!exists && (!creating_metadata || !io::stdin().is_terminal()))
        {
            return Ok(());
        }
        if !io::stdin().is_terminal() {
            return Err(format!(
                "Refusing to overwrite {} without confirmation. Use --yes to allow it.",
                path.display()
            ));
        }
        print!(
            "{} {}? [Y/n]: ",
            path.display(),
            if exists {
                "exists. Overwrite"
            } else {
                "does not exist. Create it"
            }
        );
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .map_err(|e| e.to_string())?;
        if !matches!(answer.trim().to_lowercase().as_str(), "" | "y" | "yes") {
            return Err(format!("Cancelled writing {}", path.display()));
        }
        self.confirmed.insert(path.into());
        Ok(())
    }
}

pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(data).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

pub fn files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut result = vec![];
    let mut dirs = vec![root.to_owned()];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let ty = entry.file_type().map_err(|e| e.to_string())?;
            if ty.is_dir() {
                dirs.push(entry.path());
            } else if ty.is_file() || (ty.is_symlink() && entry.path().is_file()) {
                result.push(entry.path());
            }
        }
    }
    result.sort();
    Ok(result)
}

#[derive(Debug)]
pub struct Options {
    pub base: PathBuf,
    pub sequence: String,
    pub scan: bool,
    pub extract: bool,
    pub metadata: Option<PathBuf>,
    pub mapping: Option<PathBuf>,
    pub eu_mapping: Option<PathBuf>,
    pub yes: bool,
}

pub fn run(options: Options) -> Result<()> {
    let base = options
        .base
        .canonicalize()
        .map_err(|e| format!("{}: {e}", options.base.display()))?;
    let number: u32 = options
        .sequence
        .parse()
        .map_err(|_| "Sequence number must be numeric (for example: 0001).")?;
    let mut sequence = options.sequence;
    let padded = format!("{number:04}");
    if !base.join(&sequence).is_dir() && base.join(&padded).is_dir() {
        sequence = padded;
    }
    let seq = base.join(&sequence);
    if !seq.is_dir() {
        return Err(format!(
            "Sequence directory does not exist: {}",
            seq.display()
        ));
    }
    let metadata = options.metadata.map_or_else(
        || base.join(format!("metadata-{sequence}.xlsx")),
        |p| if p.is_absolute() { p } else { base.join(p) },
    );
    let mut policy = OutputPolicy::new(options.yes);
    if options.scan {
        let count = workbook::scan(&seq, &metadata, &mut policy)?;
        println!(
            "Wrote {count} new rows to {}. Backbone generation skipped due to -scan.",
            metadata.display()
        );
        return Ok(());
    }
    if options.extract {
        let rows = extract::extract(&seq)?;
        workbook::replace(&metadata, &rows, &mut policy)?;
        println!("Extracted {} rows to {}", rows.len(), metadata.display());
        return Ok(());
    }
    let mut book = workbook::MetadataBook::read(&metadata)?;
    let rows = book.rows()?;
    if rows.is_empty() {
        return Err("Metadata contains no document or TOC rows.".into());
    }
    // Build and validate in memory before replacing any backbone on disk.
    let output = generate::generate(
        &base,
        &seq,
        number,
        &sequence,
        &rows,
        options.mapping.as_deref(),
        options.eu_mapping.as_deref(),
    )?;
    for (path, _) in &output {
        policy.approve(path, false)?;
    }
    if book.ensure_columns()? {
        book.save(&metadata, &mut policy)?;
    }
    for (path, data) in output {
        atomic_write(&path, &data)?;
        println!("Created {}", path.display());
    }
    println!("Backbones created. Available DTD checks passed.");
    Ok(())
}
