// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{
    Result, Row,
    dtd::format_attributes,
    norm, path_text, warn,
    xml::{Xml, local},
};
use crate::dossier::resolve_path;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

fn relative_document(path: &Path, seq: &Path) -> String {
    let relative = path
        .strip_prefix(seq)
        .or_else(|_| path.strip_prefix(seq.parent().unwrap_or(seq)));
    let text =
        path_text(relative.unwrap_or_else(|_| Path::new(path.file_name().unwrap_or_default())));
    let mut parts: Vec<_> = text
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    while parts.first() == Some(&"..") {
        parts.remove(0);
    }
    if parts
        .first()
        .is_some_and(|s| s.len() == 4 && s.bytes().all(|b| b.is_ascii_digit()))
    {
        parts.remove(0);
    }
    parts.join("/")
}

struct Extractor<'a> {
    seq: &'a Path,
    cache: HashMap<PathBuf, Xml>,
    stack: HashSet<PathBuf>,
    fields: Row,
}
impl Extractor<'_> {
    fn predecessor(&mut self, current: &Path, modified: &str) -> Option<(String, String, String)> {
        let (_, id) = modified.split_once('#')?;
        let target = resolve_path(current, modified).ok()?;
        if !self.cache.contains_key(&target) {
            self.cache.insert(target.clone(), Xml::read(&target).ok()?);
        }
        let xml = &self.cache[&target];
        let leaf = xml
            .nodes
            .iter()
            .position(|n| local(&n.name) == "leaf" && n.attrs.get("ID").is_some_and(|v| v == id))?;
        let href = norm(xml.href(leaf));
        let file = resolve_path(&target, &href).ok()?;
        Some((id.into(), href, relative_document(&file, self.seq)))
    }
    fn leaf(&mut self, xml: &Xml, id: usize, current: &Path) -> Option<Row> {
        let node = &xml.nodes[id];
        let href = norm(xml.href(id));
        if href
            .split('#')
            .next()
            .unwrap_or("")
            .to_lowercase()
            .ends_with(".xml")
        {
            return None;
        }
        let mut file = if href.is_empty() {
            String::new()
        } else {
            resolve_path(current, &href)
                .ok()
                .map_or(String::new(), |p| relative_document(&p, self.seq))
        };
        let title = xml
            .child(id, "title")
            .map(|i| xml.nodes[i].text.trim().to_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                Path::new(&file)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let modified = node.attrs.get("modified-file").map_or("", String::as_str);
        let mut prev_id = modified.split_once('#').map_or("", |(_, id)| id).to_owned();
        let mut prev_href = String::new();
        if !modified.is_empty()
            && let Some((id, href, prevfile)) = self.predecessor(current, modified)
        {
            prev_id = id;
            prev_href = href;
            if file.is_empty() {
                file = prevfile;
            }
        }
        let skip = [
            "leaf",
            "title",
            "specific",
            "pi-doc",
            "m1-eu",
            "eu-backbone",
            "ectd",
        ];
        let toc = xml
            .ancestors(id)
            .iter()
            .rev()
            .filter(|&&a| a != 0)
            .map(|&a| local(&xml.nodes[a].name))
            .find(|name| !skip.contains(name))
            .unwrap_or("");
        Some(Row::from([
            ("file_path".into(), file),
            ("title".into(), title),
            (
                "operation".into(),
                node.attrs
                    .get("operation")
                    .cloned()
                    .unwrap_or_else(|| "new".into()),
            ),
            ("attributes".into(), String::new()),
            ("modified-leaf".into(), prev_id),
            ("modified-href".into(), prev_href),
            ("ctd_toc".into(), toc.into()),
        ]))
    }
    fn first_directory(&mut self, xml: &Xml, id: usize, current: &Path) -> Option<String> {
        for leaf in xml
            .descendants(id)
            .into_iter()
            .filter(|&id| local(&xml.nodes[id].name) == "leaf")
        {
            if let Some(row) = self.leaf(xml, leaf, current)
                && let Some(parent) = Path::new(&row["file_path"])
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
            {
                return Some(format!("{}/", path_text(parent)));
            }
        }
        None
    }
    fn envelope(&mut self, xml: &Xml) {
        let Some(env) = xml.descendant(0, "envelope") else {
            return;
        };
        let get_attr = |id: usize, name| xml.nodes[id].attrs.get(name).cloned().unwrap_or_default();
        self.fields
            .insert("eu_country".into(), get_attr(env, "country"));
        for (tag, key, attr) in [
            ("identifier", "eu_identifier", ""),
            ("submission", "eu_submission_type", "type"),
            ("submission", "eu_submission_mode", "mode"),
            ("submission-unit", "eu_submission_unit_type", "type"),
            ("applicant", "applicant_name", ""),
            ("agency", "eu_agency_code", "code"),
            ("procedure", "eu_procedure_type", "type"),
            ("invented-name", "eu_invented_name", ""),
            ("inn", "eu_inn", ""),
            ("related-sequence", "eu_related_sequence", ""),
            ("submission-description", "sequence_description", ""),
        ] {
            if let Some(id) = xml.child(env, tag) {
                self.fields.insert(
                    key.into(),
                    if attr.is_empty() {
                        xml.nodes[id].text.trim().into()
                    } else {
                        get_attr(id, attr)
                    },
                );
            }
        }
        if let Some(submission) = xml.child(env, "submission") {
            if let Some(n) = xml.child(submission, "number") {
                self.fields.insert(
                    "eu_submission_number".into(),
                    xml.nodes[n].text.trim().into(),
                );
            }
            if let Some(n) = xml
                .descendant(submission, "procedure-tracking")
                .and_then(|p| xml.child(p, "number"))
            {
                self.fields.insert(
                    "eu_procedure_number".into(),
                    xml.nodes[n].text.trim().into(),
                );
            }
        }
    }
    fn document(&mut self, current: &Path, follow: bool, rows: &mut Vec<Row>) -> Result<()> {
        let path = current
            .canonicalize()
            .map_err(|e| format!("{}: {e}", current.display()))?;
        if !self.stack.insert(path.clone()) {
            warn(format!("Skipping XML reference cycle: {}", path.display()));
            return Ok(());
        }
        let xml = Xml::read(&path)?;
        if local(&xml.nodes[0].name) == "eu-backbone" && self.fields.is_empty() {
            self.envelope(&xml);
        }
        for id in xml.descendants(0).into_iter().skip(1) {
            let node = &xml.nodes[id];
            let name = local(&node.name);
            if name == "title" {
                continue;
            }
            if name == "leaf" {
                if let Some(row) = self.leaf(&xml, id, &path) {
                    rows.push(row);
                }
                let href = xml.href(id);
                let filename = href.split('#').next().unwrap_or("");
                if follow
                    && filename.to_lowercase().ends_with(".xml")
                    && let Ok(target) = resolve_path(&path, href)
                    && target.starts_with(self.seq)
                    && target.is_file()
                    && let Err(e) = self.document(&target, true, rows)
                {
                    warn(format!("Skipping unreadable referenced XML: {e}"));
                }
                continue;
            }
            let attributes = format_attributes(&node.attrs);
            if attributes.is_empty() {
                continue;
            }
            let dir = self.first_directory(&xml, id, &path);
            if dir.is_some() || path == self.seq.join("index.xml") {
                rows.push(Row::from([
                    ("file_path".into(), dir.unwrap_or_default()),
                    ("attributes".into(), attributes),
                    ("ctd_toc".into(), name.into()),
                ]));
            }
        }
        self.stack.remove(&path);
        Ok(())
    }
}

pub fn extract(seq: &Path) -> Result<Vec<Row>> {
    let seq = seq.canonicalize().map_err(|e| e.to_string())?;
    let mut extractor = Extractor {
        seq: &seq,
        cache: HashMap::new(),
        stack: HashSet::new(),
        fields: Row::new(),
    };
    let mut rows = vec![];
    extractor.document(&seq.join("index.xml"), true, &mut rows)?;
    if let Some(first) = rows.first_mut() {
        first.extend(extractor.fields.into_iter().filter(|(_, v)| !v.is_empty()));
    }
    Ok(rows)
}
