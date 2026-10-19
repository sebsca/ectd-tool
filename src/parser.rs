// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::dossier::{Backbone, Leaf, resolve_path};
use quick_xml::{
    Reader, Writer,
    events::{BytesStart, Event},
};
use std::{collections::BTreeMap, fs::File, io::BufReader};

#[derive(Default)]
struct Element {
    name: String,
    attributes: BTreeMap<String, String>,
    text: String,
    children: Vec<Element>,
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn element(start: &BytesStart<'_>, reader: &Reader<BufReader<File>>) -> Result<Element, String> {
    let name = reader
        .decoder()
        .decode(start.name().as_ref())
        .map_err(|e| e.to_string())?
        .into_owned();
    let mut attributes = BTreeMap::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|e| e.to_string())?;
        let key = reader
            .decoder()
            .decode(attr.key.as_ref())
            .map_err(|e| e.to_string())?
            .into_owned();
        let value = attr
            .decode_and_unescape_value(reader.decoder())
            .map_err(|e| e.to_string())?
            .into_owned();
        attributes.insert(key, value);
    }
    Ok(Element {
        name,
        attributes,
        ..Default::default()
    })
}

fn collect_namespaces(elem: &Element, bb: &mut Backbone) {
    for (key, value) in &elem.attributes {
        if key == "xmlns" {
            bb.namespaces.insert(String::new(), value.clone());
        }
        if let Some(prefix) = key.strip_prefix("xmlns:") {
            bb.namespaces.insert(prefix.into(), value.clone());
        }
    }
}

fn attr<'a>(elem: &'a Element, names: &[&str]) -> Option<&'a str> {
    names
        .iter()
        .find_map(|key| elem.attributes.get(*key).map(String::as_str))
}

fn label(elem: &Element, m1: bool) -> Vec<String> {
    let tag = local(&elem.name);
    if m1 {
        if tag == "specific" {
            if let Some(country) = attr(elem, &["country", "Country"]) {
                return vec![country.into()];
            }
            if let Some(value) = elem.attributes.values().next() {
                return vec![value.clone()];
            }
        }
        if tag == "pi-doc" {
            let parts: Vec<_> = [
                attr(elem, &["xml:lang", "lang"]),
                attr(elem, &["type"]),
                attr(elem, &["country"]),
            ]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
            if !parts.is_empty() {
                return parts;
            }
        }
    } else {
        let mut attrs: Vec<_> = elem
            .attributes
            .iter()
            .filter(|(k, _)| *k != "xmlns" && !k.starts_with("xmlns:"))
            .map(|(k, v)| (local(k), v))
            .collect();
        attrs.sort_by_key(|(k, _)| *k);
        if !attrs.is_empty() {
            let text = attrs
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(", ");
            return vec![format!("{tag} ({text})")];
        }
    }
    for (keys, separator) in [
        (&["title", "Title"][..], ": "),
        (&["name", "Name"][..], ": "),
        (&["number", "Number", "num"][..], " "),
    ] {
        if let Some(value) = attr(elem, keys) {
            return vec![format!("{tag}{separator}{value}")];
        }
    }
    vec![tag.into()]
}

fn flatten(elem: &Element, prefix: &str, out: &mut BTreeMap<String, String>) {
    let key = if prefix.is_empty() {
        elem.name.clone()
    } else {
        format!("{prefix}/{}", elem.name)
    };
    for (name, value) in &elem.attributes {
        out.insert(format!("{key}@{name}"), value.clone());
    }
    if !elem.text.trim().is_empty() {
        out.insert(format!("{key}#text"), elem.text.trim().into());
    }
    for (i, child) in elem.children.iter().enumerate() {
        flatten(child, &format!("{key}[{i}]"), out);
    }
}

fn make_leaf(
    elem: Element,
    ancestors: &[Element],
    bb: &Backbone,
    bb_id: usize,
    xml: String,
) -> Leaf {
    let mut leaf = Leaf {
        sequence: bb.sequence.clone(),
        backbone: bb_id,
        xml,
        ..Default::default()
    };
    leaf.id = attr(&elem, &["ID", "id", "Id"]).unwrap_or_default().into();
    leaf.operation = attr(&elem, &["operation"])
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("new")
        .trim()
        .to_lowercase();
    leaf.title = attr(&elem, &["title"]).unwrap_or_default().trim().into();
    if leaf.title.is_empty() {
        leaf.title = elem
            .children
            .iter()
            .find(|e| local(&e.name) == "title")
            .map(|e| e.text.trim().into())
            .unwrap_or_default();
    }
    leaf.href = elem
        .attributes
        .iter()
        .find(|(k, _)| local(k) == "href")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    leaf.modified_file = attr(&elem, &["modified-file", "modified_file"])
        .unwrap_or_default()
        .trim()
        .into();
    if !leaf.href.is_empty() {
        match resolve_path(&bb.path, &leaf.href) {
            Ok(path) => leaf.file = Some(path),
            Err(e) => leaf.warnings.push(e),
        }
    }
    let mut module = None;
    for anc in ancestors.iter().skip(1) {
        let name = local(&anc.name).to_lowercase();
        if module.is_none()
            && name.starts_with('m')
            && matches!(name.as_bytes().get(1), Some(b'1'..=b'5'))
        {
            module = name.get(..2).map(String::from);
        }
        leaf.toc.extend(label(anc, module.as_deref() == Some("m1")));
    }
    flatten(&elem, "", &mut leaf.metadata);
    leaf.attributes = elem.attributes;
    leaf
}

/// Streaming reader: retain only ancestor attributes and the current complete leaf subtree.
/// External DTDs and entities are never fetched; ordinary eCTD DOCTYPE declarations are accepted.
pub fn parse_backbone(bb: &mut Backbone, bb_id: usize) -> Vec<Leaf> {
    let mut leaves = vec![];
    let result = parse(bb, bb_id, &mut leaves);
    if let Err(error) = result {
        bb.parse_error = Some(error);
    }
    bb.leaf_count = leaves.len();
    leaves
}

fn parse(bb: &mut Backbone, bb_id: usize, leaves: &mut Vec<Leaf>) -> Result<(), String> {
    let file = File::open(&bb.path).map_err(|e| e.to_string())?;
    let mut reader = Reader::from_reader(BufReader::new(file));
    let mut buffer = vec![];
    let mut stack: Vec<Element> = vec![];
    let mut raw: Option<Writer<Vec<u8>>> = None;
    let mut leaf_depth = None;
    let mut root_seen = false;
    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|e| format!("XML error at byte {}: {e}", reader.error_position()))?;
        if let Some(writer) = raw.as_mut() {
            writer
                .write_event(event.borrow())
                .map_err(|e| e.to_string())?;
        }
        match event {
            Event::Start(ref start) | Event::Empty(ref start) => {
                let elem = element(start, &reader)?;
                collect_namespaces(&elem, bb);
                if stack.is_empty() {
                    if root_seen {
                        return Err("XML contains multiple root elements.".into());
                    }
                    bb.root_tag = local(&elem.name).into();
                    root_seen = true;
                }
                let is_leaf = local(&elem.name) == "leaf";
                if is_leaf && leaf_depth.is_some() {
                    return Err("Nested leaf elements are invalid.".into());
                }
                if is_leaf {
                    let mut writer = Writer::new(vec![]);
                    writer
                        .write_event(event.borrow())
                        .map_err(|e| e.to_string())?;
                    raw = Some(writer);
                    leaf_depth = Some(stack.len());
                }
                if matches!(event, Event::Empty(_)) {
                    if is_leaf {
                        let bytes = raw.take().unwrap().into_inner();
                        let xml = reader
                            .decoder()
                            .decode(&bytes)
                            .map_err(|e| e.to_string())?
                            .into_owned();
                        leaves.push(make_leaf(elem, &stack, bb, bb_id, xml));
                        leaf_depth = None;
                    } else if leaf_depth.is_some()
                        && let Some(parent) = stack.last_mut()
                    {
                        parent.children.push(elem);
                    }
                } else {
                    stack.push(elem);
                }
            }
            Event::End(_) => {
                let Some(elem) = stack.pop() else {
                    return Err("Unexpected closing XML tag.".into());
                };
                if leaf_depth == Some(stack.len()) {
                    let bytes = raw.take().unwrap().into_inner();
                    let xml = reader
                        .decoder()
                        .decode(&bytes)
                        .map_err(|e| e.to_string())?
                        .into_owned();
                    leaves.push(make_leaf(elem, &stack, bb, bb_id, xml));
                    leaf_depth = None;
                } else if leaf_depth.is_some()
                    && let Some(parent) = stack.last_mut()
                {
                    parent.children.push(elem);
                }
            }
            Event::Text(ref text) => {
                let text = text.xml_content().map_err(|e| e.to_string())?;
                if leaf_depth.is_some()
                    && let Some(parent) = stack.last_mut()
                {
                    parent.text.push_str(&text);
                }
                if stack.is_empty() && !text.trim().is_empty() {
                    return Err("Text outside XML root.".into());
                }
            }
            Event::CData(ref text) => {
                if leaf_depth.is_some()
                    && let Some(parent) = stack.last_mut()
                {
                    parent
                        .text
                        .push_str(&text.xml_content().map_err(|e| e.to_string())?);
                }
            }
            Event::GeneralRef(ref reference) => {
                let name = reference.decode().map_err(|e| e.to_string())?;
                let value =
                    if let Some(c) = reference.resolve_char_ref().map_err(|e| e.to_string())? {
                        c.to_string()
                    } else {
                        quick_xml::escape::resolve_predefined_entity(&name)
                            .ok_or_else(|| format!("Unresolved XML entity: &{name};"))?
                            .into()
                    };
                if leaf_depth.is_some()
                    && let Some(parent) = stack.last_mut()
                {
                    parent.text.push_str(&value);
                }
            }
            Event::Eof => {
                if !root_seen {
                    return Err("Empty XML document.".into());
                }
                if !stack.is_empty() {
                    return Err("Unexpected end of XML document.".into());
                }
                return Ok(());
            }
            _ => {}
        }
        buffer.clear();
    }
}
