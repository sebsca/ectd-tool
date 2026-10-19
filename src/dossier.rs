// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use percent_encoding::percent_decode_str;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub type LeafId = usize;

#[derive(Debug, Clone, Serialize)]
pub struct Backbone {
    pub sequence: String,
    pub path: PathBuf,
    pub kind: String,
    pub region: Option<String>,
    pub root_tag: String,
    pub namespaces: BTreeMap<String, String>,
    pub leaf_count: usize,
    pub parse_error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Leaf {
    pub sequence: String,
    pub backbone: usize,
    pub id: String,
    pub operation: String,
    pub title: String,
    pub href: String,
    pub file: Option<PathBuf>,
    pub modified_file: String,
    pub predecessor: Option<LeafId>,
    pub toc: Vec<String>,
    pub xml: String,
    pub attributes: BTreeMap<String, String>,
    pub metadata: BTreeMap<String, String>,
    pub missing: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Sequence {
    pub name: String,
    pub backbones: Vec<usize>,
    pub delta: Vec<LeafId>,
    pub consolidated: Vec<LeafId>,
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct Dossier {
    pub root: PathBuf,
    pub sequences: Vec<Sequence>,
    pub backbones: Vec<Backbone>,
    pub leaves: Vec<Leaf>,
    pub warnings: Vec<String>,
}

pub fn is_sequence(name: &str) -> bool {
    name.len() == 4 && name.bytes().all(|b| b.is_ascii_digit())
}

/// Canonicalize existing prefixes too, so references to missing files still compare consistently.
pub fn normalize(path: &Path) -> PathBuf {
    if let Ok(path) = path.canonicalize() {
        return path;
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name())
        && parent != path
        && let Ok(parent) = parent.canonicalize()
    {
        return parent.join(name);
    }
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            _ => out.push(part.as_os_str()),
        }
    }
    out
}

pub fn split_fragment(value: &str) -> (&str, Option<&str>) {
    value
        .split_once('#')
        .map_or((value, None), |(p, f)| (p, Some(f)))
}

pub fn resolve_path(xml: &Path, reference: &str) -> Result<PathBuf, String> {
    let (path, _) = split_fragment(reference.trim());
    let path = percent_decode_str(path)
        .decode_utf8()
        .map_err(|e| e.to_string())?;
    let path = path.replace('\\', "/");
    if path.contains('\0') || path.contains("://") || path.starts_with("mailto:") {
        return Err(format!("Not a local file reference: {reference}"));
    }
    if path.is_empty() {
        return Ok(normalize(xml));
    }
    Ok(normalize(
        &xml.parent().unwrap_or(Path::new(".")).join(path),
    ))
}

pub fn relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Do not follow directory symlinks: a dossier can otherwise contain directory cycles.
fn walk_files(root: &Path, out: &mut Vec<PathBuf>, warnings: &mut Vec<String>) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) => {
            warnings.push(format!("{}: {e}", root.display()));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                warnings.push(e.to_string());
                continue;
            }
        };
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => walk_files(&path, out, warnings),
            Ok(t) if t.is_file() || (t.is_symlink() && path.is_file()) => out.push(path),
            Ok(_) => {}
            Err(e) => warnings.push(format!("{}: {e}", path.display())),
        }
    }
}

impl Dossier {
    pub fn load(root: &Path) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|e| format!("{}: {e}", root.display()))?;
        let entries = fs::read_dir(&root).map_err(|e| e.to_string())?;
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_sequence(&name) && entry.path().is_dir() {
                names.push(name);
            }
        }
        names.sort();
        if names.is_empty() {
            return Err("No sequence directories (0000, 0001, …) found. Select their parent dossier directory.".into());
        }
        let mut dossier = Self {
            root,
            sequences: vec![],
            backbones: vec![],
            leaves: vec![],
            warnings: vec![],
        };
        for name in names {
            let dir = dossier.root.join(&name);
            let mut files = vec![];
            walk_files(&dir, &mut files, &mut dossier.warnings);
            files.sort();
            let mut paths = vec![];
            if dir.join("index.xml").is_file() {
                paths.push((dir.join("index.xml"), "index", None));
            }
            for path in &files {
                let is_xml = path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("xml"));
                let regional = path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().to_lowercase().contains("regional"));
                if is_xml
                    && regional
                    && let Ok(rel) = path.strip_prefix(dir.join("m1"))
                {
                    let region = rel
                        .parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .and_then(|_| rel.components().next())
                        .map(|p| p.as_os_str().to_string_lossy().into_owned());
                    paths.push((path.clone(), "regional", region));
                }
            }
            let mut seq = Sequence {
                name: name.clone(),
                backbones: vec![],
                delta: vec![],
                consolidated: vec![],
                files,
            };
            let mut seen = HashSet::new();
            for (path, kind, region) in paths {
                let path = normalize(&path);
                if !seen.insert(path.clone()) {
                    continue;
                }
                let bb_id = dossier.backbones.len();
                let mut bb = Backbone {
                    sequence: name.clone(),
                    path,
                    kind: kind.into(),
                    region,
                    root_tag: String::new(),
                    namespaces: BTreeMap::new(),
                    leaf_count: 0,
                    parse_error: None,
                };
                let leaves = crate::parser::parse_backbone(&mut bb, bb_id);
                seq.backbones.push(bb_id);
                for leaf in leaves {
                    seq.delta.push(dossier.leaves.len());
                    dossier.leaves.push(leaf);
                }
                dossier.backbones.push(bb);
            }
            if seq.backbones.is_empty() {
                dossier
                    .warnings
                    .push(format!("{name}: no backbone XMLs found."));
            }
            dossier.sequences.push(seq);
        }
        dossier.resolve_lifecycle();
        Ok(dossier)
    }

    fn resolve_lifecycle(&mut self) {
        let mut index = HashMap::new();
        for (i, leaf) in self.leaves.iter_mut().enumerate() {
            if !leaf.id.is_empty() {
                let key = (self.backbones[leaf.backbone].path.clone(), leaf.id.clone());
                if index.insert(key, i).is_some() {
                    leaf.warnings.push(
                        "Duplicate leaf ID in backbone; predecessor references are ambiguous."
                            .into(),
                    );
                }
            }
        }
        for leaf in &mut self.leaves {
            if !matches!(
                leaf.operation.as_str(),
                "new" | "replace" | "append" | "delete"
            ) {
                leaf.warnings
                    .push(format!("Unknown lifecycle operation: {}", leaf.operation));
            }
            if leaf.modified_file.is_empty() {
                if matches!(leaf.operation.as_str(), "replace" | "append" | "delete") {
                    leaf.warnings
                        .push("Lifecycle operation has no modified-file reference.".into());
                }
                continue;
            }
            let (_, fragment) = split_fragment(&leaf.modified_file);
            let Some(fragment) = fragment.filter(|s| !s.is_empty()) else {
                leaf.warnings.push(
                    "modified-file without fragment (#leafId); predecessor cannot be resolved."
                        .into(),
                );
                continue;
            };
            let xml = &self.backbones[leaf.backbone].path;
            match resolve_path(xml, &leaf.modified_file) {
                Ok(path) => {
                    let fragment = percent_decode_str(fragment)
                        .decode_utf8_lossy()
                        .into_owned();
                    leaf.predecessor = index.get(&(path, fragment)).copied();
                    if leaf.predecessor.is_none() {
                        leaf.warnings
                            .push("modified-file target leaf not found.".into());
                    }
                }
                Err(e) => leaf.warnings.push(e),
            }
        }
        let mut problems = vec![];
        for (id, leaf) in self.leaves.iter().enumerate() {
            let (_, cycle) = self.versions(id);
            let path = self.open_path(id);
            let missing = path.is_none_or(|p| !p.is_file());
            let future = leaf
                .predecessor
                .is_some_and(|p| self.leaves[p].sequence > leaf.sequence);
            problems.push((missing, cycle, future));
        }
        for (leaf, (missing, cycle, future)) in self.leaves.iter_mut().zip(problems) {
            leaf.missing = missing;
            if missing {
                leaf.warnings
                    .push("Document file is missing or no local file can be resolved.".into());
            }
            if cycle {
                leaf.warnings
                    .push("Cycle in modified-file predecessor chain.".into());
            }
            if future {
                leaf.warnings
                    .push("modified-file references a future sequence.".into());
            }
        }
        let mut active = BTreeSet::new();
        for seq in &mut self.sequences {
            for &id in &seq.delta {
                let leaf = &self.leaves[id];
                if matches!(leaf.operation.as_str(), "replace" | "delete")
                    && let Some(prev) = leaf.predecessor
                {
                    active.remove(&prev);
                }
                if leaf.operation != "delete" {
                    active.insert(id);
                }
            }
            seq.consolidated = active.iter().copied().collect();
        }
    }

    pub fn sequence(&self, name: &str) -> Option<&Sequence> {
        self.sequences.iter().find(|seq| seq.name == name)
    }

    pub fn open_path(&self, mut id: LeafId) -> Option<&Path> {
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(id) {
                return None;
            }
            let leaf = self.leaves.get(id)?;
            if leaf.operation == "delete"
                && let Some(prev) = leaf.predecessor
            {
                id = prev;
                continue;
            }
            return leaf.file.as_deref();
        }
    }

    pub fn versions(&self, id: LeafId) -> (Vec<LeafId>, bool) {
        let mut seen = HashSet::new();
        let mut versions = vec![];
        let mut current = Some(id);
        while let Some(id) = current {
            if !seen.insert(id) {
                return (versions, true);
            }
            versions.push(id);
            current = self.leaves[id].predecessor;
        }
        (versions, false)
    }

    pub fn status(&self, id: LeafId, consolidated: bool) -> String {
        let leaf = &self.leaves[id];
        let mut status = vec![];
        if !consolidated && leaf.operation == "delete" {
            status.push("DELETED");
        }
        if leaf.missing {
            status.push("MISSING");
        }
        if matches!(leaf.operation.as_str(), "replace" | "append" | "delete")
            && leaf.predecessor.is_none()
        {
            status.push("BROKEN-LINK");
        }
        if status.is_empty() && !leaf.warnings.is_empty() {
            status.push("WARNING");
        }
        if status.is_empty() {
            "OK".into()
        } else {
            status.join(" · ")
        }
    }

    pub fn filename(&self, id: LeafId) -> String {
        self.open_path(id)
            .and_then(Path::file_name)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                if self.leaves[id].title.is_empty() {
                    "<no file>".into()
                } else {
                    self.leaves[id].title.clone()
                }
            })
    }

    pub fn title(&self, id: LeafId) -> String {
        let leaf = &self.leaves[id];
        if leaf.title.is_empty() {
            self.filename(id)
        } else {
            leaf.title.clone()
        }
    }

    pub fn metadata(&self, id: LeafId) -> BTreeMap<String, String> {
        let leaf = &self.leaves[id];
        let bb = &self.backbones[leaf.backbone];
        let mut meta = BTreeMap::from([
            ("sequence".into(), leaf.sequence.clone()),
            ("operation".into(), leaf.operation.clone()),
            ("leaf_id".into(), leaf.id.clone()),
            ("title".into(), leaf.title.clone()),
            ("href".into(), leaf.href.clone()),
            ("modified_file".into(), leaf.modified_file.clone()),
            (
                "resolved_open_path".into(),
                self.open_path(id)
                    .map_or(String::new(), |p| p.display().to_string()),
            ),
            ("backbone_xml".into(), bb.path.display().to_string()),
            ("backbone_kind".into(), bb.kind.clone()),
            (
                "backbone_region".into(),
                bb.region.clone().unwrap_or_default(),
            ),
            ("missing_file".into(), leaf.missing.to_string()),
            ("ctd_toc".into(), leaf.toc.join(" / ")),
        ]);
        for (k, v) in &leaf.attributes {
            meta.insert(format!("leaf_attr:{k}"), v.clone());
        }
        for (k, v) in &leaf.metadata {
            meta.insert(format!("leaf_xml:{k}"), v.clone());
        }
        meta
    }
}
