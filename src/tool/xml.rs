// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{Result, Row};
use quick_xml::{
    Reader, Writer,
    events::{BytesEnd, BytesStart, BytesText, Event},
};
use std::{fs, path::Path};

pub fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}
#[derive(Clone, Debug, Default)]
pub struct Node {
    pub name: String,
    pub attrs: Row,
    pub text: String,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
}
#[derive(Clone, Debug, Default)]
pub struct Xml {
    pub nodes: Vec<Node>,
}

impl Xml {
    pub fn new(name: &str, attrs: Row) -> Self {
        Self {
            nodes: vec![Node {
                name: name.into(),
                attrs,
                ..Default::default()
            }],
        }
    }
    pub fn add(&mut self, parent: usize, name: &str, attrs: Row, text: &str) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node {
            name: name.into(),
            attrs,
            text: text.into(),
            children: vec![],
            parent: Some(parent),
        });
        self.nodes[parent].children.push(id);
        id
    }
    pub fn ensure(
        &mut self,
        parent: usize,
        name: &str,
        attrs: &Row,
        exact_empty: bool,
        order: &[String],
    ) -> usize {
        for &id in &self.nodes[parent].children {
            let node = &self.nodes[id];
            if node.name == name
                && ((!attrs.is_empty() && attrs.iter().all(|(k, v)| node.attrs.get(k) == Some(v)))
                    || (attrs.is_empty() && (!exact_empty || node.attrs.is_empty())))
            {
                return id;
            }
        }
        let id = self.add(parent, name, attrs.clone(), "");
        if let Some(rank) = order.iter().position(|t| t == name) {
            self.nodes[parent].children.pop();
            let pos = self.nodes[parent]
                .children
                .iter()
                .position(|&c| {
                    order
                        .iter()
                        .position(|t| t == &self.nodes[c].name)
                        .is_some_and(|r| r > rank)
                })
                .unwrap_or(self.nodes[parent].children.len());
            self.nodes[parent].children.insert(pos, id);
        }
        id
    }
    pub fn descendants(&self, id: usize) -> Vec<usize> {
        let mut stack = vec![id];
        let mut out = vec![];
        while let Some(id) = stack.pop() {
            out.push(id);
            stack.extend(self.nodes[id].children.iter().rev().copied());
        }
        out
    }
    pub fn ancestors(&self, id: usize) -> Vec<usize> {
        let mut out = vec![];
        let mut current = self.nodes[id].parent;
        while let Some(id) = current {
            out.push(id);
            current = self.nodes[id].parent;
        }
        out.reverse();
        out
    }
    pub fn child(&self, id: usize, name: &str) -> Option<usize> {
        self.nodes[id]
            .children
            .iter()
            .copied()
            .find(|&c| local(&self.nodes[c].name) == name)
    }
    pub fn descendant(&self, id: usize, name: &str) -> Option<usize> {
        self.descendants(id)
            .into_iter()
            .find(|&c| local(&self.nodes[c].name) == name)
    }
    pub fn href(&self, id: usize) -> &str {
        let node = &self.nodes[id];
        node.attrs
            .get("xlink:href")
            .or_else(|| node.attrs.get("href"))
            .or_else(|| {
                node.attrs
                    .iter()
                    .find(|(k, _)| local(k) == "href")
                    .map(|(_, v)| v)
            })
            .map_or("", String::as_str)
    }
    pub fn read(path: &Path) -> Result<Self> {
        Self::parse(&fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
            .map_err(|e| format!("{}: {e}", path.display()))
    }
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut reader = Reader::from_reader(data);
        let mut xml = Self::default();
        let mut stack: Vec<usize> = vec![];
        loop {
            let event = reader.read_event().map_err(|e| e.to_string())?;
            match event {
                Event::Start(ref start) | Event::Empty(ref start) => {
                    let name = reader
                        .decoder()
                        .decode(start.name().as_ref())
                        .map_err(|e| e.to_string())?
                        .into_owned();
                    let mut attrs = Row::new();
                    for a in start.attributes() {
                        let a = a.map_err(|e| e.to_string())?;
                        attrs.insert(
                            reader
                                .decoder()
                                .decode(a.key.as_ref())
                                .map_err(|e| e.to_string())?
                                .into_owned(),
                            a.decode_and_unescape_value(reader.decoder())
                                .map_err(|e| e.to_string())?
                                .into_owned(),
                        );
                    }
                    let id = if let Some(&parent) = stack.last() {
                        xml.add(parent, &name, attrs, "")
                    } else {
                        if !xml.nodes.is_empty() {
                            return Err("Multiple XML roots.".into());
                        }
                        xml = Self::new(&name, attrs);
                        0
                    };
                    if matches!(event, Event::Start(_)) {
                        stack.push(id);
                    }
                }
                Event::End(_) => {
                    if stack.pop().is_none() {
                        return Err("Unexpected closing XML tag.".into());
                    }
                }
                Event::Text(t) => {
                    if let Some(&id) = stack.last() {
                        xml.nodes[id]
                            .text
                            .push_str(&t.xml_content().map_err(|e| e.to_string())?);
                    }
                }
                Event::CData(t) => {
                    if let Some(&id) = stack.last() {
                        xml.nodes[id]
                            .text
                            .push_str(&t.xml_content().map_err(|e| e.to_string())?);
                    }
                }
                Event::GeneralRef(r) => {
                    let name = r.decode().map_err(|e| e.to_string())?;
                    let text = if let Some(c) = r.resolve_char_ref().map_err(|e| e.to_string())? {
                        c.to_string()
                    } else {
                        quick_xml::escape::resolve_predefined_entity(&name)
                            .ok_or_else(|| format!("Unsupported entity: &{name};"))?
                            .into()
                    };
                    if let Some(&id) = stack.last() {
                        xml.nodes[id].text.push_str(&text);
                    }
                }
                Event::Eof => {
                    if xml.nodes.is_empty() || !stack.is_empty() {
                        return Err("Empty or incomplete XML.".into());
                    }
                    return Ok(xml);
                }
                _ => {}
            }
        }
    }
    pub fn serialize(&self) -> Result<String> {
        fn write(xml: &Xml, id: usize, writer: &mut Writer<Vec<u8>>) -> Result<()> {
            let node = &xml.nodes[id];
            if !regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_.:-]*$")
                .unwrap()
                .is_match(&node.name)
            {
                return Err(format!("Invalid XML element name: {}", node.name));
            }
            let mut start = BytesStart::new(&node.name);
            for (key, value) in &node.attrs {
                start.push_attribute((key.as_str(), value.as_str()));
            }
            if node.text.is_empty() && node.children.is_empty() {
                return writer
                    .write_event(Event::Empty(start))
                    .map_err(|e| e.to_string());
            }
            writer
                .write_event(Event::Start(start))
                .map_err(|e| e.to_string())?;
            if !node.text.is_empty() {
                writer
                    .write_event(Event::Text(BytesText::new(&node.text)))
                    .map_err(|e| e.to_string())?;
            }
            for &child in &node.children {
                write(xml, child, writer)?;
            }
            writer
                .write_event(Event::End(BytesEnd::new(&node.name)))
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        let mut writer = Writer::new_with_indent(vec![], b' ', 2);
        write(self, 0, &mut writer)?;
        String::from_utf8(writer.into_inner()).map_err(|e| e.to_string())
    }
}
