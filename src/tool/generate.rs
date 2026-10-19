// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{
    MODULES, Result, Row, XLINK, checksum, directory,
    dtd::{Schema, attributes, normalized},
    mapping::{Mapping, match_child},
    norm, operation,
    previous::{History, Previous},
    toc, value, warn,
    xml::{Xml, local},
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use uuid::Uuid;

type Outputs = Vec<(PathBuf, Vec<u8>)>;
#[derive(Clone)]
struct Context {
    parts: Vec<String>,
    tags: Vec<String>,
    node: usize,
    attrs: Row,
}

fn parts(path: &str) -> Vec<String> {
    path.trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}
fn previous_path(prev: &Previous) -> String {
    let parent = prev.xml_path.rsplit_once('/').map_or("", |(p, _)| p);
    if parent.is_empty() {
        prev.href.clone()
    } else {
        format!("{parent}/{}", prev.href)
    }
}
fn prepare_rows(rows: &[Row], history: &History) -> Result<Vec<Row>> {
    rows.iter().enumerate().map(|(index,row)| {
        let mut row = row.clone();
        let op = operation(&row);
        if !matches!(op.as_str(), "new"|"replace"|"append"|"delete") { return Err(format!("Metadata row {}: unknown operation '{op}'",index+2)); }
        let mut path = norm(value(&row,"file_path"));
        if path.is_empty() && op == "delete" {
            path = norm(value(&row,"modified-href"));
            if path.is_empty() && let Some(prev) = history.by_id(value(&row,"modified-leaf")) { path = previous_path(&prev); }
        }
        if path.is_empty() && toc(&row).is_empty() { return Err(format!("Metadata row {} requires file_path, ctd_toc, or a resolvable delete reference.",index+2)); }
        if !path.is_empty() && (Path::new(&path).is_absolute() || path.split('/').any(|p| p == "..") || path.contains(':')) { return Err(format!("file_path must be sequence-relative: {path}")); }
        row.insert("file_path".into(),path);
        Ok(row)
    }).collect()
}

struct Builder<'a> {
    seq: &'a Path,
    schema: &'a Schema,
    mapping: &'a Mapping,
    history: &'a History,
    regional: bool,
    pending_regional: bool,
    xml: Xml,
}
impl Builder<'_> {
    fn child(&mut self, parent: usize, tag: &str, attrs: &Row) -> usize {
        let order = self.schema.children(&self.xml.nodes[parent].name);
        self.xml.ensure(parent, tag, attrs, !self.regional, &order)
    }
    fn ensure_path(&mut self, mut parent: usize, tags: &[String], attrs: &Row) -> usize {
        let empty = Row::new();
        for (i, tag) in tags.iter().enumerate() {
            parent = self.child(
                parent,
                tag,
                if i + 1 == tags.len() { attrs } else { &empty },
            );
        }
        parent
    }
    fn tags(&self, module: &str, path: &str, toc: &str, is_dir: bool, key: &str) -> Vec<String> {
        if !toc.is_empty() {
            return self
                .schema
                .path(module, toc)
                .map(|p| p.into_iter().skip(1).collect())
                .unwrap_or_else(|| {
                    if toc == module {
                        vec![]
                    } else {
                        vec![toc.into()]
                    }
                });
        }
        if !self.mapping.is_empty() {
            return self.mapping.tags(path, is_dir, module);
        }
        let path = parts(path);
        let mut current = module.to_owned();
        let mut tags = vec![];
        for part in path
            .iter()
            .skip(1)
            .take(path.len().saturating_sub(if is_dir { 1 } else { 2 }))
        {
            if let Some(tag) = match_child(&self.schema.children(&current), part, key) {
                current = tag.clone();
                tags.push(tag);
            }
        }
        tags
    }
    fn filtered(&self, attrs: &Row, node: usize, path: &str) -> Row {
        self.schema.filter(&self.xml.nodes[node].name, attrs, path)
    }
    fn with_attrs(&mut self, node: usize, attrs: &Row, path: &str) -> usize {
        let filtered = self.filtered(attrs, node, path);
        if filtered.is_empty() {
            return node;
        }
        let mut merged = self.xml.nodes[node].attrs.clone();
        merged.extend(filtered);
        if let Some(parent) = self.xml.nodes[node].parent {
            let tag = self.xml.nodes[node].name.clone();
            self.child(parent, &tag, &merged)
        } else {
            self.xml.nodes[node].attrs = merged;
            node
        }
    }
    fn leaf(&mut self, parent: usize, row: &Row, contexts: &[Context]) -> Result<()> {
        let path = value(row, "file_path");
        let op = operation(row);
        let href = if self.regional {
            path.strip_prefix("m1/eu/").unwrap_or(path)
        } else {
            path
        };
        let checksum = if op == "delete" {
            String::new()
        } else if self.pending_regional && path == "m1/eu/eu-regional.xml" {
            // Filled from the in-memory EU XML before the index is validated.
            String::new()
        } else if self.seq.join(path).is_file() {
            checksum(&self.seq.join(path))?
        } else {
            warn(format!("Missing file for leaf: {path}"));
            String::new()
        };
        let mut attrs = Row::from([
            ("checksum".into(), checksum),
            ("checksum-type".into(), "MD5".into()),
            ("operation".into(), op.clone()),
        ]);
        if op != "delete" {
            if href.is_empty() {
                return Err("Non-delete operation requires a file path.".into());
            }
            attrs.insert("xlink:href".into(), href.into());
        }
        let mut previous = if matches!(op.as_str(), "replace" | "append" | "delete") {
            if !value(row, "modified-leaf").is_empty() {
                self.history
                    .by_id(value(row, "modified-leaf"))
                    .into_iter()
                    .collect::<Vec<_>>()
            } else {
                self.history
                    .by_paths(&[value(row, "modified-href"), path, href])
            }
        } else {
            vec![]
        };
        if op == "delete" && !contexts.is_empty() {
            previous.retain(|p| {
                contexts.iter().all(|ctx| {
                    p.tags.starts_with(&ctx.tags)
                        && (ctx.attrs.is_empty()
                            || ctx
                                .tags
                                .len()
                                .checked_sub(1)
                                .and_then(|i| p.attrs.get(i))
                                .is_some_and(|attrs| {
                                    ctx.attrs.iter().all(|(k, v)| attrs.get(k) == Some(v))
                                }))
                })
            });
        }
        if previous.is_empty() && matches!(op.as_str(), "replace" | "append" | "delete") {
            warn(format!(
                "No previous leaf found for {op} on {} in its TOC context",
                value(row, "modified-leaf").to_owned() + path
            ));
        }
        let previous: Vec<Option<Previous>> = if previous.is_empty() {
            vec![None]
        } else {
            previous.into_iter().map(Some).collect()
        };
        for prev in previous {
            let mut attrs = attrs.clone();
            attrs.insert("ID".into(), format!("N{}", Uuid::new_v4().simple()));
            if let Some(prev) = prev {
                let prefix = if self.regional { "../../../" } else { "../" };
                attrs.insert(
                    "modified-file".into(),
                    format!("{prefix}{:04}/{}#{}", prev.sequence, prev.xml_path, prev.id),
                );
            }
            let id = self.xml.add(parent, "leaf", attrs, "");
            let title = if value(row, "title").is_empty() {
                path.rsplit('/').next().unwrap_or("")
            } else {
                value(row, "title")
            };
            self.xml.add(id, "title", Row::new(), title);
            let order = self.schema.children(&self.xml.nodes[parent].name);
            if !self.regional && order.first().is_some_and(|s| s == "leaf") {
                self.xml.nodes[parent].children.pop();
                let pos = self.xml.nodes[parent]
                    .children
                    .iter()
                    .take_while(|&&id| self.xml.nodes[id].name == "leaf")
                    .count();
                self.xml.nodes[parent].children.insert(pos, id);
            }
        }
        Ok(())
    }

    fn ich_module(&mut self, key: &str, module: &str, rows: &[Row]) -> Result<()> {
        let module_node = self.child(0, module, &Row::new());
        let mut dirs: Vec<_> = rows
            .iter()
            .filter(|r| {
                directory(self.seq, value(r, "file_path"))
                    && toc(r).is_empty()
                    && !value(r, "attributes").is_empty()
            })
            .collect();
        dirs.sort_by_key(|r| parts(value(r, "file_path")).len());
        let mut dir_contexts: Vec<Context> = vec![];
        for row in dirs {
            let path = value(row, "file_path");
            let path_parts = parts(path);
            let tags = self.tags(module, path, "", true, key);
            let target = tags.last().map_or(module, String::as_str);
            let attrs = self
                .schema
                .filter(target, &attributes(value(row, "attributes")), path);
            let context = dir_contexts
                .iter()
                .filter(|c| path_parts.starts_with(&c.parts) && tags.starts_with(&c.tags))
                .max_by_key(|c| c.parts.len());
            let (base, skip) = context.map_or((module_node, 0), |c| (c.node, c.tags.len()));
            let node = if skip == tags.len() {
                self.xml.nodes[base].attrs.extend(attrs.clone());
                base
            } else {
                self.ensure_path(base, &tags[skip..], &attrs)
            };
            dir_contexts.push(Context {
                parts: path_parts,
                tags,
                node,
                attrs,
            });
        }
        dir_contexts.sort_by_key(|c| std::cmp::Reverse(c.parts.len()));
        let mut toc_contexts: Vec<Context> = vec![];
        for row in rows {
            let path = value(row, "file_path");
            let ctd = toc(row);
            let is_dir = directory(self.seq, path);
            let tags = self.tags(module, path, ctd, is_dir, key);
            let is_toc = !ctd.is_empty() && (path.is_empty() || is_dir);
            while toc_contexts.last().is_some_and(|c| {
                !tags.starts_with(&c.tags) || (is_toc && tags.len() == c.tags.len())
            }) {
                toc_contexts.pop();
            }
            let (mut base, mut skip) = toc_contexts
                .last()
                .map_or((module_node, 0), |c| (c.node, c.tags.len()));
            let custom = attributes(value(row, "attributes"));
            if is_toc {
                let target = tags.last().map_or(module, String::as_str);
                let attrs = self.schema.filter(target, &custom, path);
                let node = if tags.is_empty() {
                    self.xml.nodes[module_node].attrs.extend(attrs.clone());
                    module_node
                } else {
                    self.ensure_path(base, &tags[skip..], &attrs)
                };
                toc_contexts.push(Context {
                    parts: parts(path),
                    tags,
                    node,
                    attrs,
                });
                continue;
            }
            for context in &dir_contexts {
                if parts(path).starts_with(&context.parts)
                    && tags.starts_with(&context.tags)
                    && (skip == 0 || context.tags.starts_with(&tags[..skip]))
                {
                    base = context.node;
                    skip = context.tags.len();
                    break;
                }
            }
            let node = self.ensure_path(base, &tags[skip..], &Row::new());
            let node = self.with_attrs(node, &custom, path);
            if !is_dir {
                self.leaf(node, row, &toc_contexts)?;
            }
        }
        Ok(())
    }
}

fn root(schema: &Schema, eu: bool) -> Xml {
    let default = if eu {
        "http://europa.eu.int"
    } else {
        "http://www.ich.org/ectd"
    };
    let prefix = schema.root.split_once(':').map(|(prefix, _)| prefix);
    let namespace = prefix.map_or("xmlns".to_owned(), |p| format!("xmlns:{p}"));
    Xml::new(
        &schema.root,
        Row::from([
            (
                namespace.clone(),
                schema.fixed(&schema.root, &namespace, default),
            ),
            (
                "xmlns:xlink".into(),
                schema.fixed(&schema.root, "xmlns:xlink", XLINK),
            ),
            (
                "dtd-version".into(),
                schema.fixed(&schema.root, "dtd-version", if eu { "3.1" } else { "3.2" }),
            ),
        ]),
    )
}

fn wrap(xml: &Xml, schema: &Schema, system: &str, xsl: &str, public: bool) -> Result<String> {
    let public_id = if public {
        regex::Regex::new(r#"PUBLIC\s+"([^"]+)""#)
            .unwrap()
            .captures(&schema.text)
            .map(|c| c[1].to_owned())
    } else {
        None
    };
    let doctype = public_id.map_or_else(
        || format!("<!DOCTYPE {} SYSTEM \"{system}\">", schema.root),
        |p| format!("<!DOCTYPE {} PUBLIC \"{p}\" \"{system}\">", schema.root),
    );
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n{doctype}\n<?xml-stylesheet href=\"{xsl}\" type=\"text/xsl\"?>\n{}",
        xml.serialize()?
    ))
}

pub fn generate(
    base: &Path,
    seq: &Path,
    number: u32,
    sequence: &str,
    rows: &[Row],
    mapping: Option<&Path>,
    eu_mapping: Option<&Path>,
) -> Result<Outputs> {
    let history = History::load(base, number)?;
    let rows = prepare_rows(rows, &history)?;
    let ich = Schema::load(&seq.join("util/dtd/ich-ectd-3-2.dtd"), "ectd:ectd")?;
    let mapping = Mapping::load(mapping, false)?;
    let eu_mapping = Mapping::load(eu_mapping, true)?;
    let mut output = vec![];
    let eu_rows: Vec<_> = rows
        .iter()
        .filter(|r| {
            value(r, "file_path").starts_with("m1/eu/")
                && value(r, "file_path") != "m1/eu/eu-regional.xml"
        })
        .cloned()
        .collect();
    let eu_xml = if eu_rows.is_empty() {
        None
    } else {
        let eu_path = seq.join("util/dtd/eu-regional.dtd");
        let eu = if eu_path.is_file() {
            Schema::load(&eu_path, "eu:eu-backbone")?
        } else {
            Schema {
                root: "eu:eu-backbone".into(),
                ..Default::default()
            }
        };
        let mut builder = Builder {
            seq,
            schema: &eu,
            mapping: &eu_mapping,
            history: &history,
            regional: true,
            pending_regional: false,
            xml: root(&eu, true),
        };
        builder.envelope(&rows[0], sequence)?;
        builder.eu_documents(&eu_rows)?;
        let text = wrap(
            &builder.xml,
            &eu,
            "../../util/dtd/eu-regional.dtd",
            "../../util/style/eu-regional.xsl",
            false,
        )?;
        if eu_path.is_file() {
            eu.validate(&text, "eu-regional.xml")?;
        } else {
            warn("EU regional DTD not found; skipped validation for eu-regional.xml.");
        }
        Some(text)
    };
    if let Some(text) = &eu_xml {
        output.push((seq.join("m1/eu/eu-regional.xml"), text.as_bytes().to_vec()));
    }
    let mut builder = Builder {
        seq,
        schema: &ich,
        mapping: &mapping,
        history: &history,
        regional: false,
        pending_regional: eu_xml.is_some(),
        xml: root(&ich, false),
    };
    let mut module_tags: Vec<String> = MODULES.iter().map(|s| (*s).into()).collect();
    let mut module_order = vec![];
    for tag in ich.children(&ich.root) {
        for (i, module) in module_tags.iter_mut().enumerate() {
            let key = format!("m{}", i + 1);
            if normalized(&tag).starts_with(&key) {
                *module = tag.clone();
                if !module_order.contains(&i) {
                    module_order.push(i);
                }
                break;
            }
        }
    }
    for i in 0..5 {
        if !module_order.contains(&i) {
            module_order.push(i);
        }
    }
    for i in module_order {
        let key = format!("m{}", i + 1);
        let mut module_rows: Vec<_> = rows
            .iter()
            .filter(|r| {
                let path = value(r, "file_path");
                let module = if path.is_empty() {
                    toc(r).get(..2).unwrap_or("")
                } else {
                    path.split('/').next().unwrap_or("")
                };
                module == key && (!path.starts_with("m1/eu/") || path == "m1/eu/eu-regional.xml")
            })
            .cloned()
            .collect();
        if i == 0
            && eu_xml.is_some()
            && !module_rows
                .iter()
                .any(|r| value(r, "file_path") == "m1/eu/eu-regional.xml")
        {
            module_rows.push(Row::from([
                ("file_path".into(), "m1/eu/eu-regional.xml".into()),
                ("title".into(), "European Union".into()),
                ("operation".into(), "new".into()),
            ]));
        }
        if !module_rows.is_empty() {
            builder.ich_module(&key, &module_tags[i], &module_rows)?;
        }
    }
    // The EU backbone has not been published yet; checksum it from the exact bytes being committed.
    if let Some(eu) = &eu_xml {
        for node in &mut builder.xml.nodes {
            if node
                .attrs
                .get("xlink:href")
                .is_some_and(|v| v == "m1/eu/eu-regional.xml")
            {
                node.attrs.insert(
                    "checksum".into(),
                    format!("{:x}", md5::compute(eu.as_bytes())),
                );
            }
        }
    }
    let index = wrap(
        &builder.xml,
        &ich,
        "util/dtd/ich-ectd-3-2.dtd",
        "util/style/ectd-2-0.xsl",
        true,
    )?;
    ich.validate(&index, "index.xml")?;
    let md5 = format!("{:x}  index.xml\n", md5::compute(index.as_bytes()));
    output.push((seq.join("index.xml"), index.into_bytes()));
    output.push((seq.join("index-md5.txt"), md5.into_bytes()));
    Ok(output)
}

const COUNTRIES: &str = "at be bg common cy cz de dk edqm ee el es ema fi fr hr hu ie is it li lt lu lv mt nl no pl pt ro se si sk uk xi";
const LANGUAGES: &str =
    "bg cs da de el en es et fi fr ga hr hu is it lt lv mt nl no pl pt ro sk sl sv";
const PI_TYPES: &str = "spc annex2 outer interpack impack other pl combined";
const M1_ORDER: [&str; 12] = [
    "m1-0-cover",
    "m1-2-form",
    "m1-3-pi",
    "m1-4-expert",
    "m1-5-specific",
    "m1-6-environrisk",
    "m1-7-orphan",
    "m1-8-pharmacovigilance",
    "m1-9-clinical-trials",
    "m1-10-paediatrics",
    "m1-responses",
    "m1-additional-data",
];

fn validated(row: &Row, key: &str, allowed: &[String], default: &str) -> String {
    let v = value(row, key);
    if v.is_empty() || (!allowed.is_empty() && !allowed.iter().any(|s| s == v)) {
        default.into()
    } else {
        v.into()
    }
}
impl Builder<'_> {
    fn envelope(&mut self, row: &Row, sequence: &str) -> Result<()> {
        let mut countries = self.schema.enumeration("envelope", "country");
        if countries.is_empty() {
            countries = COUNTRIES.split_whitespace().map(String::from).collect();
        }
        let country = validated(row, "eu_country", &countries, "at");
        let eu_env = self.xml.add(0, "eu-envelope", Row::new(), "");
        let env = self.xml.add(
            eu_env,
            "envelope",
            Row::from([("country".into(), country)]),
            "",
        );
        let id = if value(row, "eu_identifier").is_empty() {
            Uuid::new_v4().to_string()
        } else {
            value(row, "eu_identifier").into()
        };
        self.xml.add(env, "identifier", Row::new(), &id);
        let sub = self.xml.add(
            env,
            "submission",
            Row::from([
                (
                    "type".into(),
                    validated(
                        row,
                        "eu_submission_type",
                        &self.schema.enumeration("submission", "type"),
                        "none",
                    ),
                ),
                (
                    "mode".into(),
                    validated(
                        row,
                        "eu_submission_mode",
                        &self.schema.enumeration("submission", "mode"),
                        "single",
                    ),
                ),
            ]),
            "",
        );
        if !value(row, "eu_submission_number").is_empty() {
            self.xml.add(
                sub,
                "number",
                Row::new(),
                value(row, "eu_submission_number"),
            );
        }
        let proc = self.xml.add(sub, "procedure-tracking", Row::new(), "");
        self.xml.add(
            proc,
            "number",
            Row::new(),
            &validated(row, "eu_procedure_number", &[], "1"),
        );
        self.xml.add(
            env,
            "submission-unit",
            Row::from([(
                "type".into(),
                validated(
                    row,
                    "eu_submission_unit_type",
                    &self.schema.enumeration("submission-unit", "type"),
                    "initial",
                ),
            )]),
            "",
        );
        self.xml.add(
            env,
            "applicant",
            Row::new(),
            &validated(row, "applicant_name", &[], "Unknown applicant"),
        );
        self.xml.add(
            env,
            "agency",
            Row::from([(
                "code".into(),
                validated(
                    row,
                    "eu_agency_code",
                    &self.schema.enumeration("agency", "code"),
                    "EU-EMA",
                ),
            )]),
            "",
        );
        self.xml.add(
            env,
            "procedure",
            Row::from([(
                "type".into(),
                validated(
                    row,
                    "eu_procedure_type",
                    &self.schema.enumeration("procedure", "type"),
                    "national",
                ),
            )]),
            "",
        );
        self.xml.add(
            env,
            "invented-name",
            Row::new(),
            &validated(row, "eu_invented_name", &[], "Unknown product"),
        );
        if !value(row, "eu_inn").is_empty() {
            self.xml.add(env, "inn", Row::new(), value(row, "eu_inn"));
        }
        self.xml.add(env, "sequence", Row::new(), sequence);
        self.xml.add(
            env,
            "related-sequence",
            Row::new(),
            &validated(row, "eu_related_sequence", &[], sequence),
        );
        self.xml.add(
            env,
            "submission-description",
            Row::new(),
            &validated(row, "sequence_description", &[], "Not provided"),
        );
        Ok(())
    }
    fn eu_container(&mut self, m1: usize, path: &str, is_dir: bool, ctd: &str) -> usize {
        let path_parts = parts(path);
        let eu_parts = &path_parts[2..];
        let mut tags = self.mapping.tags(path, is_dir, "m1-eu");
        tags.retain(|t| t != "m1-eu");
        if !ctd.is_empty() && !matches!(ctd, "specific" | "pi-doc" | "m1-eu") {
            tags = self
                .schema
                .path("m1-eu", ctd)
                .map(|p| p.into_iter().skip(1).collect())
                .unwrap_or_else(|| vec![ctd.into()]);
        }
        if tags.is_empty() {
            tags = fallback_eu(eu_parts);
        }
        let top = tags
            .first()
            .cloned()
            .unwrap_or_else(|| "m1-additional-data".into());
        let top_order = self.schema.children("m1-eu");
        let order = if top_order.is_empty() {
            M1_ORDER.iter().map(|s| (*s).into()).collect()
        } else {
            top_order
        };
        let mut node = self.xml.ensure(m1, &top, &Row::new(), false, &order);
        for child in tags.iter().skip(1) {
            node = self.child(node, child, &Row::new());
        }
        let children = self.schema.children(&self.xml.nodes[node].name);
        let kind = if !children.is_empty() {
            if children.iter().any(|c| c == "specific") {
                "specific"
            } else if children.iter().any(|c| c == "pi-doc") {
                "pi-doc"
            } else {
                "leaf"
            }
        } else if [
            "m1-0-cover",
            "m1-2-form",
            "m1-5-specific",
            "m1-responses",
            "m1-additional-data",
        ]
        .contains(&top.as_str())
        {
            "specific"
        } else if top == "m1-3-pi" {
            "pi-doc"
        } else {
            "leaf"
        };
        let find = |allowed: Vec<String>, fallback: &str| -> String {
            let set: HashSet<_> = allowed.iter().map(String::as_str).collect();
            eu_parts
                .iter()
                .find(|p| set.contains(p.as_str()))
                .map_or_else(|| fallback.into(), |s| s.clone())
        };
        let countries = self.schema.enumeration("specific", "country");
        let countries = if countries.is_empty() {
            COUNTRIES.split_whitespace().map(String::from).collect()
        } else {
            countries
        };
        if kind == "specific" {
            let country = eu_parts
                .get(1)
                .filter(|s| countries.contains(s))
                .cloned()
                .unwrap_or_else(|| "common".into());
            node = self.child(node, "specific", &Row::from([("country".into(), country)]));
        } else if kind == "pi-doc" {
            let mut languages = self.schema.enumeration("pi-doc", "xml:lang");
            if languages.is_empty() {
                languages = LANGUAGES.split_whitespace().map(String::from).collect();
            }
            let mut types = self.schema.enumeration("pi-doc", "type");
            if types.is_empty() {
                types = PI_TYPES.split_whitespace().map(String::from).collect();
            }
            let attrs = Row::from([
                ("country".into(), find(countries, "common")),
                ("xml:lang".into(), find(languages, "en")),
                ("type".into(), find(types, "other")),
            ]);
            node = self.child(node, "pi-doc", &attrs);
        }
        node
    }
    fn eu_documents(&mut self, rows: &[Row]) -> Result<()> {
        let m1 = self.xml.add(0, "m1-eu", Row::new(), "");
        let mut dirs: Vec<Context> = vec![];
        for row in rows.iter().filter(|r| {
            directory(self.seq, value(r, "file_path")) && !value(r, "attributes").is_empty()
        }) {
            let path = value(row, "file_path");
            let node = self.eu_container(m1, path, true, toc(row));
            let node = self.with_attrs(node, &attributes(value(row, "attributes")), path);
            dirs.push(Context {
                parts: parts(path),
                tags: vec![],
                node,
                attrs: Row::new(),
            });
        }
        dirs.sort_by_key(|c| std::cmp::Reverse(c.parts.len()));
        for row in rows {
            let path = value(row, "file_path");
            let is_dir = directory(self.seq, path);
            let node = if let Some(ctx) = dirs.iter().find(|c| parts(path).starts_with(&c.parts)) {
                ctx.node
            } else {
                self.eu_container(m1, path, is_dir, toc(row))
            };
            let node = self.with_attrs(node, &attributes(value(row, "attributes")), path);
            if !is_dir {
                let ctx = self
                    .xml
                    .ancestors(node)
                    .into_iter()
                    .chain(std::iter::once(node))
                    .skip(2)
                    .collect::<Vec<_>>();
                let contexts: Vec<_> = ctx
                    .iter()
                    .enumerate()
                    .map(|(i, &id)| Context {
                        parts: vec![],
                        tags: ctx[..=i]
                            .iter()
                            .map(|&n| local(&self.xml.nodes[n].name).into())
                            .collect(),
                        node: id,
                        attrs: self.xml.nodes[id].attrs.clone(),
                    })
                    .collect();
                self.leaf(node, row, &contexts)?;
            }
        }
        Ok(())
    }
}

fn fallback_eu(parts: &[String]) -> Vec<String> {
    let section = parts.first().map_or("", String::as_str);
    let groups: &[(&str, &str, &[&str])] = &[
        (
            "13",
            "m1-3-pi",
            &[
                "m1-3-1-spc-label-pl",
                "m1-3-2-mockup",
                "m1-3-3-specimen",
                "m1-3-4-consultation",
                "m1-3-5-approved",
                "m1-3-6-braille",
            ],
        ),
        (
            "14",
            "m1-4-expert",
            &["m1-4-1-quality", "m1-4-2-non-clinical", "m1-4-3-clinical"],
        ),
        (
            "15",
            "m1-5-specific",
            &[
                "m1-5-1-bibliographic",
                "m1-5-2-generic-hybrid-bio-similar",
                "m1-5-3-data-market-exclusivity",
                "m1-5-4-exceptional-circumstances",
                "m1-5-5-conditional-ma",
            ],
        ),
        ("16", "m1-6-environrisk", &["m1-6-1-non-gmo", "m1-6-2-gmo"]),
        (
            "17",
            "m1-7-orphan",
            &["m1-7-1-similarity", "m1-7-2-market-exclusivity"],
        ),
        (
            "18",
            "m1-8-pharmacovigilance",
            &[
                "m1-8-1-pharmacovigilance-system",
                "m1-8-2-risk-management-system",
            ],
        ),
    ];
    for (prefix, tag, children) in groups {
        if section.starts_with(prefix) {
            let index = parts
                .get(1)
                .and_then(|s| s.as_bytes().get(1))
                .filter(|b| b.is_ascii_digit())
                .map_or(0, |b| usize::from(b.saturating_sub(b'1')));
            return vec![
                (*tag).into(),
                children.get(index).unwrap_or(&children[0]).to_string(),
            ];
        }
    }
    vec![
        if section.starts_with("110") {
            "m1-10-paediatrics"
        } else if section.starts_with("10") {
            "m1-0-cover"
        } else if section.starts_with("12") {
            "m1-2-form"
        } else if section.starts_with("19") {
            "m1-9-clinical-trials"
        } else if section == "responses" {
            "m1-responses"
        } else {
            "m1-additional-data"
        }
        .into(),
    ]
}
