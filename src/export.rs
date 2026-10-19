// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::dossier::{Dossier, normalize};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Default, Serialize)]
pub struct ExportReport {
    pub sequence: String,
    pub destination: PathBuf,
    pub copied: usize,
    pub missing: usize,
    pub skipped_backbones: usize,
    pub renamed: usize,
    pub errors: Vec<String>,
}

impl ExportReport {
    pub fn summary(&self) -> String {
        let mut text = format!(
            "Sequence: {}\nDestination: {}\nCopied files: {}\nMissing files: {}\nSkipped backbone XMLs: {}\nRenamed due to collisions: {}",
            self.sequence,
            self.destination.display(),
            self.copied,
            self.missing,
            self.skipped_backbones,
            self.renamed
        );
        for e in &self.errors {
            text.push_str(&format!("\n{e}"));
        }
        text
    }
}

/// Export the active documents, preserving module paths. This does not generate a new backbone.
pub fn export_consolidated(
    dossier: &Dossier,
    sequence: &str,
    destination: &Path,
) -> Result<ExportReport, String> {
    let seq = dossier
        .sequence(sequence)
        .ok_or_else(|| format!("Unknown sequence: {sequence}"))?;
    let destination = if destination.is_absolute() {
        destination.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(destination)
    };
    let destination = normalize(&destination);
    if dossier.root.starts_with(&destination)
        || dossier
            .sequences
            .iter()
            .any(|s| destination.starts_with(dossier.root.join(&s.name)))
    {
        return Err("Choose an export folder outside the source sequence directories.".into());
    }
    if destination.exists()
        && (!destination.is_dir()
            || fs::read_dir(&destination)
                .map_err(|e| e.to_string())?
                .next()
                .is_some())
    {
        return Err("Choose a new or empty export folder.".into());
    }
    fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    let destination = destination.canonicalize().map_err(|e| e.to_string())?;
    // Recheck after mkdir to catch parent symlinks that did not exist in the lexical path.
    if dossier.root.starts_with(&destination)
        || dossier
            .sequences
            .iter()
            .any(|s| destination.starts_with(dossier.root.join(&s.name)))
    {
        return Err("Export destination resolves inside a source sequence.".into());
    }
    let backbone_paths: HashSet<_> = dossier
        .backbones
        .iter()
        .map(|bb| bb.path.as_path())
        .collect();
    let mut report = ExportReport {
        sequence: sequence.into(),
        destination: destination.clone(),
        ..Default::default()
    };
    let mut ids = seq.consolidated.clone();
    ids.sort_by_key(|id| {
        let leaf = &dossier.leaves[*id];
        (&leaf.sequence, &leaf.href, &leaf.id)
    });
    for id in ids {
        let leaf = &dossier.leaves[id];
        let Some(source) = dossier.open_path(id).filter(|p| p.is_file()) else {
            report.missing += 1;
            continue;
        };
        if backbone_paths.contains(source) {
            report.skipped_backbones += 1;
            continue;
        }
        let rel = source
            .strip_prefix(dossier.root.join(&leaf.sequence))
            .ok()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(source.file_name().unwrap_or_default()));
        let dest = destination.join(rel);
        let result = (|| -> Result<bool, String> {
            let parent = dest.parent().ok_or("Invalid destination path")?;
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            if !parent
                .canonicalize()
                .map_err(|e| e.to_string())?
                .starts_with(&destination)
            {
                return Err("Export path leaves the destination directory.".into());
            }
            let mut count = 0;
            loop {
                let candidate = if count == 0 {
                    dest.clone()
                } else {
                    let stem = dest.file_stem().unwrap_or_default().to_string_lossy();
                    let ext = dest
                        .extension()
                        .map_or(String::new(), |s| format!(".{}", s.to_string_lossy()));
                    let suffix = if count == 1 {
                        format!("-{}", leaf.sequence)
                    } else {
                        format!("-{}-{count}", leaf.sequence)
                    };
                    dest.with_file_name(format!("{stem}{suffix}{ext}"))
                };
                match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&candidate)
                {
                    Ok(mut output) => {
                        let copied = copy_document(source, &mut output);
                        if let Err(e) = copied {
                            drop(output);
                            let _ = fs::remove_file(&candidate);
                            return Err(e.to_string());
                        }
                        return Ok(count > 0);
                    }
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => count += 1,
                    Err(e) => return Err(e.to_string()),
                }
            }
        })();
        match result {
            Ok(renamed) => {
                report.copied += 1;
                report.renamed += usize::from(renamed);
            }
            Err(e) => report.errors.push(format!("{}: {e}", source.display())),
        }
    }
    Ok(report)
}

fn copy_document(source: &Path, output: &mut fs::File) -> io::Result<()> {
    let mut input = fs::File::open(source)?;
    io::copy(&mut input, output)?;
    output.sync_all()?;
    if let Ok(meta) = input.metadata() {
        let mut times = fs::FileTimes::new();
        if let Ok(t) = meta.modified() {
            times = times.set_modified(t);
        }
        if let Ok(t) = meta.accessed() {
            times = times.set_accessed(t);
        }
        let _ = output.set_times(times);
    }
    Ok(())
}
