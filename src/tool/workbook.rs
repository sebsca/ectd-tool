// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{FIELDS, OutputPolicy, Result, Row, atomic_write, files, norm, path_text, value};
use std::{collections::HashSet, path::Path};
use umya_spreadsheet::{Workbook, Worksheet};

pub struct MetadataBook {
    pub book: Workbook,
    sheet: String,
}
impl MetadataBook {
    pub fn read(path: &Path) -> Result<Self> {
        let book = umya_spreadsheet::reader::xlsx::read(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let sheet = if book.sheet_by_name("metadata").is_ok() {
            "metadata".into()
        } else {
            book.active_sheet().name().into()
        };
        Ok(Self { book, sheet })
    }
    fn new() -> Self {
        let mut book = umya_spreadsheet::new_file();
        book.active_sheet_mut().set_name("metadata");
        Self {
            book,
            sheet: "metadata".into(),
        }
    }
    fn sheet(&self) -> Result<&Worksheet> {
        self.book
            .sheet_by_name(&self.sheet)
            .map_err(|e| e.to_string())
    }
    fn sheet_mut(&mut self) -> Result<&mut Worksheet> {
        self.book
            .sheet_by_name_mut(&self.sheet)
            .map_err(|e| e.to_string())
    }
    pub fn headers(&self) -> Result<Vec<String>> {
        let sheet = self.sheet()?;
        Ok((1..=sheet.highest_column())
            .map(|c| sheet.value((c, 1)).trim().to_owned())
            .collect())
    }
    pub fn rows(&self) -> Result<Vec<Row>> {
        let headers = self.headers()?;
        let sheet = self.sheet()?;
        if !headers.iter().any(|h| h == "file_path") {
            return Err("Metadata sheet is missing the file_path column.".into());
        }
        let mut rows = vec![];
        for index in 2..=sheet.highest_row() {
            let mut row = Row::new();
            for (col, header) in headers.iter().enumerate() {
                if !header.is_empty() {
                    row.insert(
                        header.clone(),
                        sheet.value((col as u32 + 1, index)).trim().into(),
                    );
                }
            }
            if row.values().all(|s| s.is_empty()) {
                continue;
            }
            let file = norm(value(&row, "file_path"));
            row.insert("file_path".into(), file);
            // Keep explicit TOC rows even when they have no file; the generator supports them.
            if value(&row, "file_path").is_empty()
                && value(&row, "operation") != "delete"
                && value(&row, "modified-href").is_empty()
                && value(&row, "modified-leaf").is_empty()
                && super::toc(&row).is_empty()
            {
                continue;
            }
            rows.push(row);
        }
        Ok(rows)
    }
    pub fn ensure_columns(&mut self) -> Result<bool> {
        let mut headers = self.headers()?;
        let mut changed = false;
        if headers.iter().all(|h| h.is_empty()) {
            headers.clear();
        }
        for field in FIELDS {
            if headers
                .iter()
                .any(|h| h == field || (field == "ctd_toc" && h == "ctd-toc"))
            {
                continue;
            }
            headers.push(field.into());
            self.sheet_mut()?
                .cell_mut((headers.len() as u32, 1))
                .set_value_string(field);
            changed = true;
        }
        Ok(changed)
    }
    pub fn save(&self, path: &Path, policy: &mut OutputPolicy) -> Result<()> {
        policy.approve(path, true)?;
        let mut data = vec![];
        umya_spreadsheet::writer::xlsx::write_writer(&self.book, &mut data)
            .map_err(|e| e.to_string())?;
        atomic_write(path, &data)
    }
    fn append(&mut self, row: &Row) -> Result<()> {
        let headers = self.headers()?;
        let index = self.sheet()?.highest_row() + 1;
        for (col, header) in headers.iter().enumerate() {
            if let Some(value) = row.get(header).filter(|s| !s.is_empty()) {
                self.sheet_mut()?
                    .cell_mut((col as u32 + 1, index))
                    .set_value_string(value);
            }
        }
        Ok(())
    }
}

pub fn scan(seq: &Path, path: &Path, policy: &mut OutputPolicy) -> Result<usize> {
    let mut book = if path.exists() {
        MetadataBook::read(path)?
    } else {
        MetadataBook::new()
    };
    let mut changed = book.ensure_columns()?;
    let existing: HashSet<_> = book
        .rows()?
        .iter()
        .map(|r| norm(value(r, "file_path")))
        .collect();
    let mut count = 0;
    for file in files(seq)? {
        let rel = path_text(file.strip_prefix(seq).map_err(|e| e.to_string())?);
        if !rel.contains('/')
            || rel == "m1/eu/eu-regional.xml"
            || rel.starts_with("util/")
            || existing.contains(&rel)
        {
            continue;
        }
        let title = file
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .replace(['-', '_'], " ")
            .trim()
            .to_owned();
        book.append(&Row::from([
            ("file_path".into(), rel),
            ("title".into(), title),
            ("operation".into(), "new".into()),
        ]))?;
        count += 1;
        changed = true;
    }
    if changed || !path.exists() {
        book.save(path, policy)?;
    }
    Ok(count)
}

pub fn replace(path: &Path, rows: &[Row], policy: &mut OutputPolicy) -> Result<()> {
    let mut book = if path.exists() {
        MetadataBook::read(path)?
    } else {
        MetadataBook::new()
    };
    if book.sheet != "metadata" {
        book.sheet_mut()?.set_name("metadata");
        book.sheet = "metadata".into();
    }
    let count = book.sheet()?.highest_row();
    if count > 0 {
        book.sheet_mut()?.remove_row(1, count);
    }
    for (col, header) in FIELDS.iter().enumerate() {
        book.sheet_mut()?
            .cell_mut((col as u32 + 1, 1))
            .set_value_string(*header);
    }
    for row in rows {
        book.append(row)?;
    }
    book.save(path, policy)
}
