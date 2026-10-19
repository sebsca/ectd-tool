// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{Result, dtd::normalized};
use regex::Regex;
use std::{fs, path::Path};

struct Entry {
    pattern: Regex,
    tag: String,
    specificity: usize,
    length: usize,
    depth: usize,
    file: bool,
}
pub struct Mapping {
    entries: Vec<Entry>,
}
impl Mapping {
    pub fn load(path: Option<&Path>, eu: bool) -> Result<Self> {
        let text = match path {
            Some(path) => {
                fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?
            }
            None => if eu {
                include_str!("../../resources/ectd_eu_m1_path_to_xml_element_mapping.csv")
            } else {
                include_str!("../../resources/ectd_3_2_2_path_to_xml_element_mapping.csv")
            }
            .into(),
        };
        Self::parse(&text)
    }
    pub fn parse(text: &str) -> Result<Self> {
        let mut csv = csv::Reader::from_reader(text.as_bytes());
        let headers = csv.headers().map_err(|e| e.to_string())?.clone();
        let column = |name| {
            headers
                .iter()
                .position(|h| h.trim() == name)
                .ok_or_else(|| format!("Mapping CSV is missing column '{name}'"))
        };
        let (kind, path, tag) = (
            column("item_type")?,
            column("relative_path")?,
            column("xml_element")?,
        );
        let dynamic = Regex::new(r"[A-Za-z].*-\d+$").unwrap();
        let mut entries = vec![];
        for row in csv.records() {
            let row = row.map_err(|e| e.to_string())?;
            let (kind, rel, tag) = (
                row.get(kind).unwrap_or("").trim(),
                row.get(path).unwrap_or("").trim(),
                row.get(tag).unwrap_or("").trim(),
            );
            if !matches!(kind, "directory" | "file") || rel.is_empty() || tag.is_empty() {
                continue;
            }
            let parts: Vec<_> = rel.split('/').collect();
            let specificity = parts.iter().filter(|s| !dynamic.is_match(s)).count();
            let mut pattern: Vec<_> = parts
                .iter()
                .map(|s| {
                    if dynamic.is_match(s) {
                        "[^/]+".into()
                    } else {
                        regex::escape(s)
                    }
                })
                .collect();
            let file = kind == "file";
            let length = if file {
                let last = parts.last().unwrap();
                let prefix = last.rsplit_once('.').map_or(*last, |(base, _)| base);
                *pattern.last_mut().unwrap() = format!("{}[^/]*", regex::escape(prefix));
                rel.len() - last.len() + prefix.len()
            } else {
                rel.len()
            };
            entries.push(Entry {
                pattern: Regex::new(&format!("^{}$", pattern.join("/")))
                    .map_err(|e| e.to_string())?,
                tag: tag.into(),
                specificity,
                length,
                depth: parts.len(),
                file,
            });
        }
        Ok(Self { entries })
    }
    pub fn matches(&self, path: &str, file: bool) -> Option<&str> {
        let depth = path.split('/').count();
        let mut best: Option<&Entry> = None;
        for e in &self.entries {
            if e.file == file
                && e.depth == depth
                && e.pattern.is_match(path)
                && best.is_none_or(|b| (e.specificity, e.length) > (b.specificity, b.length))
            {
                best = Some(e);
            }
        }
        best.map(|e| e.tag.as_str())
    }
    pub fn tags(&self, path: &str, is_dir: bool, current: &str) -> Vec<String> {
        let parts: Vec<_> = path
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        let dir_count = if is_dir {
            parts.len()
        } else {
            parts.len().saturating_sub(1)
        };
        let mut tags = vec![];
        let mut prev = current.to_owned();
        for size in 2..=dir_count {
            if let Some(tag) = self.matches(&parts[..size].join("/"), false)
                && tag != prev
            {
                tags.push(tag.into());
                prev = tag.into();
            }
        }
        if !is_dir
            && let Some(tag) = self.matches(path, true)
            && tag != prev
        {
            tags.push(tag.into());
        }
        tags
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn child_code(child: &str) -> String {
    let tokens: Vec<_> = child
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let mut code = String::new();
    let code_pattern = Regex::new(r"^(?:\d+[a-z]|[a-z]\d+)$").unwrap();
    for token in tokens.iter().skip(usize::from(
        tokens
            .first()
            .is_some_and(|s| s.len() == 2 && s.starts_with('m')),
    )) {
        if token.bytes().all(|b| b.is_ascii_digit())
            || token.len() == 1
            || code_pattern.is_match(token)
        {
            code.push_str(token);
        } else {
            break;
        }
    }
    code
}
pub fn match_child(children: &[String], segment: &str, module: &str) -> Option<String> {
    let seg = normalized(segment);
    let code = Regex::new(r"^(\d+)([a-z])?(\d+)?")
        .unwrap()
        .captures(&seg)
        .map(|c| {
            let mut first = c[1].to_owned();
            if module.len() == 2 && first.len() > 1 && first.starts_with(&module[1..]) {
                first.remove(0);
            }
            format!(
                "{first}{}{}",
                c.get(2).map_or("", |m| m.as_str()),
                c.get(3).map_or("", |m| m.as_str())
            )
        })
        .unwrap_or_default();
    let mut best = None;
    let mut rank = (0, usize::MAX);
    for child in children {
        let n = normalized(child);
        let c = child_code(child);
        let score = if !code.is_empty()
            && !c.is_empty()
            && (c == code || c.starts_with(&code) || c.contains(&code))
        {
            4
        } else if n == seg {
            3
        } else if n.ends_with(&seg) || seg.ends_with(&n) {
            2
        } else if n.contains(&seg) {
            1
        } else {
            0
        };
        if score > rank.0 || (score == rank.0 && n.len() < rank.1) {
            best = Some(child.clone());
            rank = (score, n.len());
        }
    }
    if rank.0 > 0 { best } else { None }
}
