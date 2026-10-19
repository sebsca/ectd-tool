// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{Result, Row, warn};
use regex::Regex;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
};
use xmloxide::validation::dtd::{
    self, AttributeDefault, AttributeType, ContentModel, ContentSpec, ContentSpecKind, Dtd,
};

#[derive(Clone, Default)]
pub struct Schema {
    pub root: String,
    pub order: BTreeMap<String, Vec<String>>,
    pub parsed: Dtd,
    pub text: String,
}

fn expand_file(
    path: &Path,
    entities: &mut HashMap<String, String>,
    stack: &mut HashSet<std::path::PathBuf>,
    depth: usize,
) -> Result<String> {
    if depth > 32 {
        return Err("DTD parameter entity nesting exceeds 32 levels.".into());
    }
    let path = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !stack.insert(path.clone()) {
        return Err(format!("DTD module cycle: {}", path.display()));
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let raw = Regex::new(r"(?s)<!--.*?-->")
        .unwrap()
        .replace_all(&raw, "")
        .into_owned();
    let pattern = Regex::new(r#"(?s)<!ENTITY\s+%\s*([^\s]+)\s+(?:(SYSTEM)\s+(?:"([^"]*)"|'([^']*)')|(PUBLIC)\s+(?:"[^"]*"|'[^']*')\s+(?:"([^"]*)"|'([^']*)')|"([^"]*)"|'([^']*)')\s*>"#).unwrap();
    // External modules may reference entities declared earlier in their parent DTD.
    for capture in pattern.captures_iter(&raw) {
        let name = capture[1].to_owned();
        let external = capture.get(2).is_some() || capture.get(5).is_some();
        let value = if external {
            let reference = [3, 4, 6, 7]
                .iter()
                .find_map(|&i| capture.get(i))
                .map_or("", |m| m.as_str());
            if reference.contains("://") || Path::new(reference).is_absolute() {
                return Err(format!(
                    "Only local relative DTD module references are supported: {reference}"
                ));
            }
            expand_file(
                &path.parent().unwrap().join(reference),
                entities,
                stack,
                depth + 1,
            )?
        } else {
            capture
                .get(8)
                .or_else(|| capture.get(9))
                .map_or("", |m| m.as_str())
                .to_owned()
        };
        entities.entry(name).or_insert(value);
    }
    let mut text = pattern.replace_all(&raw, "").into_owned();
    let refs = Regex::new(r"%([A-Za-z0-9_.:-]+);").unwrap();
    for _ in 0..32 {
        let next = refs
            .replace_all(&text, |c: &regex::Captures<'_>| {
                entities.get(&c[1]).cloned().unwrap_or_else(|| c[0].into())
            })
            .into_owned();
        if next == text {
            break;
        }
        if next.len() > 32 * 1024 * 1024 {
            return Err("Expanded DTD exceeds 32 MiB.".into());
        }
        text = next;
    }
    stack.remove(&path);
    Ok(text)
}

pub fn normalized(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
fn names(spec: &ContentSpec, result: &mut Vec<String>) {
    match &spec.kind {
        ContentSpecKind::Name(name) => {
            if !result.contains(name) {
                result.push(name.clone());
            }
        }
        ContentSpecKind::Seq(parts) | ContentSpecKind::Choice(parts) => {
            for part in parts {
                names(part, result);
            }
        }
    }
}
impl Schema {
    pub fn load(path: &Path, preferred: &str) -> Result<Self> {
        let text = expand_file(path, &mut HashMap::new(), &mut HashSet::new(), 0)?;
        let parsed =
            dtd::parse_dtd(&text).map_err(|e| format!("Invalid DTD {}: {e}", path.display()))?;
        let mut order = BTreeMap::new();
        for (name, decl) in &parsed.elements {
            let mut children = vec![];
            if let ContentModel::Children(spec) = &decl.content_model {
                names(spec, &mut children);
            }
            if !children.is_empty() {
                order.insert(name.clone(), children);
            }
        }
        let root = if parsed.elements.contains_key(preferred) {
            preferred.into()
        } else {
            Regex::new(r"<!ELEMENT\s+([^\s>]+)")
                .unwrap()
                .captures(&text)
                .map(|c| c[1].to_owned())
                .ok_or("DTD has no element declarations")?
        };
        Ok(Self {
            root,
            order,
            parsed,
            text,
        })
    }
    pub fn fixed(&self, element: &str, name: &str, default: &str) -> String {
        self.parsed
            .attributes
            .get(element)
            .and_then(|attrs| attrs.iter().find(|a| a.attribute_name == name))
            .and_then(|a| match &a.default {
                AttributeDefault::Fixed(v) => Some(v.clone()),
                _ => None,
            })
            .unwrap_or_else(|| default.into())
    }
    pub fn enumeration(&self, element: &str, name: &str) -> Vec<String> {
        self.parsed
            .attributes
            .get(element)
            .and_then(|attrs| attrs.iter().find(|a| a.attribute_name == name))
            .and_then(|a| match &a.attribute_type {
                AttributeType::Enumeration(v) => Some(v.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }
    pub fn children(&self, name: &str) -> Vec<String> {
        self.order
            .get(name)
            .or_else(|| {
                self.order
                    .iter()
                    .find(|(k, _)| normalized(k) == normalized(name))
                    .map(|(_, v)| v)
            })
            .cloned()
            .unwrap_or_default()
    }
    pub fn path(&self, start: &str, target: &str) -> Option<Vec<String>> {
        fn find(
            s: &Schema,
            current: &str,
            target: &str,
            seen: &mut HashSet<String>,
        ) -> Option<Vec<String>> {
            if current == target {
                return Some(vec![current.into()]);
            }
            if !seen.insert(current.into()) {
                return None;
            }
            for child in s.children(current) {
                if let Some(mut path) = find(s, &child, target, seen) {
                    path.insert(0, current.into());
                    return Some(path);
                }
            }
            None
        }
        find(self, start, target, &mut HashSet::new())
    }
    pub fn filter(&self, element: &str, attrs: &Row, context: &str) -> Row {
        let allowed: HashSet<_> = self
            .parsed
            .attributes
            .get(element)
            .into_iter()
            .flatten()
            .map(|a| a.attribute_name.as_str())
            .collect();
        attrs
            .iter()
            .filter_map(|(k, v)| {
                if allowed.contains(k.as_str()) {
                    Some((k.clone(), v.clone()))
                } else {
                    warn(format!(
                        "Attribute '{k}' not allowed for {element} ({context})"
                    ));
                    None
                }
            })
            .collect()
    }
    pub fn validate(&self, xml: &str, label: &str) -> Result<()> {
        // DTD names are lexical QNames, whereas xmloxide's namespace-aware parser stores
        // local names. Check well-formedness first, then validate a QName-preserving tree.
        xmloxide::Document::parse_str(xml).map_err(|e| format!("{label}: {e}"))?;
        let source = super::xml::Xml::parse(xml.as_bytes())?;
        let mut document = xmloxide::Document::new();
        let mut ids = Vec::with_capacity(source.nodes.len());
        for node in &source.nodes {
            let id = document.create_element(&node.name);
            for (key, value) in &node.attrs {
                if !self
                    .parsed
                    .attributes
                    .get(&node.name)
                    .is_some_and(|attrs| attrs.iter().any(|a| a.attribute_name == *key))
                {
                    return Err(format!(
                        "DTD validation failed for {label}: attribute '{key}' is not declared for '{}'",
                        node.name
                    ));
                }
                document.set_attribute(id, key, value);
            }
            let parent = node.parent.map_or(document.root(), |p| ids[p]);
            document.append_child(parent, id);
            if !node.text.is_empty() {
                let text = document.create_text(&node.text);
                document.append_child(id, text);
            }
            ids.push(id);
        }
        let result = dtd::validate(&mut document, &self.parsed);
        if !result.is_valid {
            return Err(format!(
                "DTD validation failed for {label}:\n{}",
                result
                    .errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        Ok(())
    }
}

pub fn attributes(raw: &str) -> Row {
    let text = raw.replace(['“', '”'], "\"").replace(['‘', '’'], "'");
    let pattern = Regex::new(r#"([^\s=]+)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap();
    pattern
        .captures_iter(&text)
        .map(|c| {
            (
                c[1].into(),
                quick_xml::escape::unescape(c.get(2).or_else(|| c.get(3)).unwrap().as_str())
                    .map_or_else(
                        |_| c.get(2).or_else(|| c.get(3)).unwrap().as_str().into(),
                        |s| s.into_owned(),
                    ),
            )
        })
        .collect()
}

pub fn format_attributes(attrs: &Row) -> String {
    attrs
        .iter()
        .filter(|(k, v)| {
            !v.trim().is_empty() && !k.starts_with("xmlns") && super::xml::local(k) != "href"
        })
        .map(|(k, v)| {
            if v.contains('"') && !v.contains('\'') {
                format!("{k}='{v}'")
            } else {
                format!("{k}=\"{}\"", v.replace('"', "&quot;"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
