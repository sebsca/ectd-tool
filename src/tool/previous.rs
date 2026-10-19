// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{
    Result, Row, norm, path_text,
    xml::{Xml, local},
};
use crate::dossier::resolve_path;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Previous {
    pub sequence: u32,
    pub xml_path: String,
    pub id: String,
    pub href: String,
    pub tags: Vec<String>,
    pub attrs: Vec<Row>,
}
pub struct History {
    sequences: Vec<(u32, Vec<Previous>)>,
}
impl History {
    pub fn load(base: &Path, current: u32) -> Result<Self> {
        let mut sequences = vec![];
        let mut dirs = vec![];
        for entry in fs::read_dir(base).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if let Ok(number) = entry.file_name().to_string_lossy().parse::<u32>()
                && number < current
                && entry.path().is_dir()
            {
                dirs.push((number, entry.path()));
            }
        }
        dirs.sort_by_key(|a| std::cmp::Reverse(a.0));
        for (sequence, dir) in dirs {
            let paths = referenced(&dir);
            let mut previous = vec![];
            for path in paths {
                let Ok(xml) = Xml::read(&path) else {
                    continue;
                };
                let xml_path = path_text(path.strip_prefix(&dir).unwrap_or(Path::new("index.xml")));
                for id in xml.descendants(0) {
                    let node = &xml.nodes[id];
                    if local(&node.name) != "leaf" {
                        continue;
                    }
                    let Some(leaf_id) = node.attrs.get("ID") else {
                        continue;
                    };
                    let ancestors: Vec<_> = xml.ancestors(id).into_iter().skip(2).collect();
                    previous.push(Previous {
                        sequence,
                        xml_path: xml_path.clone(),
                        id: leaf_id.clone(),
                        href: norm(xml.href(id)),
                        tags: ancestors
                            .iter()
                            .map(|&a| local(&xml.nodes[a].name).to_owned())
                            .collect(),
                        attrs: ancestors
                            .iter()
                            .map(|&a| xml.nodes[a].attrs.clone())
                            .collect(),
                    });
                }
            }
            sequences.push((sequence, previous));
        }
        Ok(Self { sequences })
    }
    pub fn by_id(&self, id: &str) -> Option<Previous> {
        self.sequences
            .iter()
            .flat_map(|(_, leaves)| leaves)
            .find(|l| l.id == id)
            .cloned()
    }
    pub fn by_paths(&self, paths: &[&str]) -> Vec<Previous> {
        let wanted: Vec<_> = paths
            .iter()
            .map(|s| norm(s))
            .filter(|s| !s.is_empty())
            .collect();
        let basename = |s: &str| s.rsplit('/').next().unwrap_or("").to_owned();
        let names: Vec<_> = wanted.iter().map(|s| basename(s)).collect();
        // Prefer all matches in the most recent matching index.
        for (_, leaves) in &self.sequences {
            let index: Vec<_> = leaves
                .iter()
                .filter(|l| l.xml_path == "index.xml" && !l.href.is_empty())
                .collect();
            let exact: Vec<_> = index
                .iter()
                .filter(|l| wanted.contains(&l.href))
                .map(|l| (*l).clone())
                .collect();
            if !exact.is_empty() {
                return exact;
            }
            let bases: Vec<_> = index
                .iter()
                .filter(|l| names.contains(&basename(&l.href)))
                .map(|l| (*l).clone())
                .collect();
            if !bases.is_empty() {
                return bases;
            }
        }
        // Regional XML fallback, in sequence and XML order, matching the reference tool.
        for (_, leaves) in &self.sequences {
            let mut xmls = vec![];
            for leaf in leaves {
                if !xmls.contains(&leaf.xml_path) {
                    xmls.push(leaf.xml_path.clone());
                }
            }
            for xml in xmls {
                let matching: Vec<_> = leaves
                    .iter()
                    .filter(|l| l.xml_path == xml && !l.href.is_empty())
                    .collect();
                if let Some(l) = matching
                    .iter()
                    .find(|l| wanted.contains(&l.href))
                    .or_else(|| matching.iter().find(|l| names.contains(&basename(&l.href))))
                {
                    return vec![(*l).clone()];
                }
            }
        }
        vec![]
    }
}

pub fn referenced(seq: &Path) -> Vec<PathBuf> {
    let index = seq.join("index.xml");
    let mut paths = vec![];
    if !index.is_file() {
        return paths;
    }
    paths.push(index.clone());
    if let Ok(xml) = Xml::read(&index) {
        for id in xml.descendants(0) {
            if local(&xml.nodes[id].name) != "leaf" {
                continue;
            }
            let href = xml.href(id);
            let path = href.split('#').next().unwrap_or("");
            if path.to_lowercase().ends_with(".xml")
                && let Ok(path) = resolve_path(&index, href)
                && path.starts_with(seq)
                && path.is_file()
                && !paths.contains(&path)
            {
                paths.push(path);
            }
        }
    }
    paths
}
