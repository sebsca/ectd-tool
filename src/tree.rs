// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::dossier::{Dossier, LeafId, is_sequence, normalize, relative};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

pub const HEADERS: [&str; 12] = [
    "Name",
    "Type",
    "Op",
    "Seq",
    "Status",
    "Backbone",
    "Leaf ID",
    "Title",
    "Href",
    "modified-file",
    "File Path",
    "CTD-TOC",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewOptions {
    pub toc: bool,
    pub consolidated: bool,
    pub show_backbones: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Dossier,
    Sequence(String),
    Group,
    Leaf(LeafId),
    Backbone(usize),
    Orphan(PathBuf),
}

#[derive(Debug)]
pub struct Node {
    pub kind: NodeKind,
    /// The sequence containing the row; an inherited leaf may originate in an earlier sequence.
    pub sequence: Option<String>,
    pub columns: [String; 12],
    pub children: Vec<usize>,
    pub parent: Option<usize>,
    pub populated: bool,
}

impl Node {
    fn new(kind: NodeKind, sequence: Option<String>, name: String, node_type: &str) -> Self {
        let mut columns = std::array::from_fn(|_| String::new());
        columns[0] = name;
        columns[1] = node_type.into();
        columns[3] = sequence.clone().unwrap_or_default();
        Self {
            kind,
            sequence,
            columns,
            children: vec![],
            parent: None,
            populated: true,
        }
    }
}

type BranchPath = Vec<(NodeKind, String, String)>;

pub struct Tree {
    pub nodes: Vec<Node>,
    pub expanded: HashSet<usize>,
    pub options: ViewOptions,
    pub sequence_nodes: HashMap<String, usize>,
    query: String,
    matches: HashSet<usize>,
    expansion_memory: HashMap<BranchPath, bool>,
}

impl Tree {
    pub fn new(dossier: &Dossier, options: ViewOptions) -> Self {
        let name = dossier
            .root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut root = Node::new(NodeKind::Dossier, None, name, "Dossier");
        root.columns[10] = dossier.root.display().to_string();
        let mut tree = Self {
            nodes: vec![root],
            expanded: HashSet::from([0]),
            options,
            sequence_nodes: HashMap::new(),
            query: String::new(),
            matches: HashSet::new(),
            expansion_memory: HashMap::new(),
        };
        for seq in &dossier.sequences {
            let mut node = Node::new(
                NodeKind::Sequence(seq.name.clone()),
                Some(seq.name.clone()),
                seq.name.clone(),
                "Sequence",
            );
            node.populated = false;
            node.columns[10] = dossier.root.join(&seq.name).display().to_string();
            let id = tree.push(0, node);
            tree.sequence_nodes.insert(seq.name.clone(), id);
        }
        tree
    }

    fn ancestors(&self, id: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(Some(id), |&id| self.nodes[id].parent)
    }

    fn branch_path(&self, id: usize) -> BranchPath {
        self.ancestors(id)
            .map(|id| {
                let node = &self.nodes[id];
                (
                    node.kind.clone(),
                    node.columns[0].clone(),
                    node.columns[1].clone(),
                )
            })
            .collect()
    }

    /// Keep exact branch states for each hierarchy and project open document areas
    /// when a hierarchy is visited for the first time. Row indices are not stable.
    pub fn set_options(&mut self, dossier: &Dossier, options: ViewOptions) {
        if self.options == options {
            return;
        }
        let mut memory = std::mem::take(&mut self.expansion_memory);
        let mut visibility = HashMap::new();
        for (id, node) in self.nodes.iter().enumerate() {
            if !node.children.is_empty() || !node.populated {
                memory.insert(self.branch_path(id), self.expanded.contains(&id));
            }
            if matches!(
                node.kind,
                NodeKind::Leaf(_) | NodeKind::Backbone(_) | NodeKind::Orphan(_)
            ) {
                // Ignore the shared Documents/Backbones, sequence and root containers.
                let open = self
                    .ancestors(id)
                    .skip(1)
                    .take_while(|&id| {
                        self.nodes[id]
                            .parent
                            .is_some_and(|p| !matches!(self.nodes[p].kind, NodeKind::Sequence(_)))
                    })
                    .all(|id| self.expanded.contains(&id));
                visibility.insert((&node.sequence, &node.kind), open);
            }
        }
        let mut tree = Self::new(dossier, options);
        for seq in &dossier.sequences {
            if self.nodes[self.sequence_nodes[&seq.name]].populated {
                tree.populate(dossier, &seq.name);
            }
        }
        let mut projected = HashMap::new();
        for (id, node) in tree.nodes.iter().enumerate() {
            if let Some(&open) = visibility.get(&(&node.sequence, &node.kind)) {
                for ancestor in tree.ancestors(id) {
                    *projected.entry(ancestor).or_insert(false) |= open;
                }
            }
        }
        tree.expanded = (0..tree.nodes.len())
            .filter(|&id| {
                memory
                    .get(&tree.branch_path(id))
                    .or_else(|| projected.get(&id))
                    .copied()
                    .unwrap_or_else(|| tree.expanded.contains(&id))
            })
            .collect();
        tree.expansion_memory = memory;
        *self = tree;
    }

    fn push(&mut self, parent: usize, mut node: Node) -> usize {
        let id = self.nodes.len();
        node.parent = Some(parent);
        self.nodes.push(node);
        self.nodes[parent].children.push(id);
        id
    }

    fn group(&mut self, parent: usize, name: &str, node_type: &str) -> usize {
        let node = Node::new(
            NodeKind::Group,
            self.nodes[parent].sequence.clone(),
            name.into(),
            node_type,
        );
        self.push(parent, node)
    }

    fn path(
        &mut self,
        parent: usize,
        labels: &[String],
        groups: &mut HashMap<Vec<String>, usize>,
    ) -> usize {
        let mut cur = parent;
        for (i, label) in labels.iter().enumerate() {
            let key = labels[..=i].to_vec();
            cur = *groups.entry(key).or_insert_with(|| {
                self.group(
                    cur,
                    label,
                    if self.options.toc {
                        "Section"
                    } else {
                        "Folder"
                    },
                )
            });
        }
        cur
    }

    pub fn populate(&mut self, dossier: &Dossier, seq_name: &str) {
        let Some(&seq_node) = self.sequence_nodes.get(seq_name) else {
            return;
        };
        if self.nodes[seq_node].populated {
            return;
        }
        self.nodes[seq_node].populated = true;
        let seq = dossier.sequence(seq_name).unwrap();
        let bb_group = self.group(seq_node, "Backbones", "Group");
        let documents = self.group(seq_node, "Documents", "Group");
        self.expanded.extend([bb_group, documents]);
        for &id in &seq.backbones {
            let bb = &dossier.backbones[id];
            let mut node = Node::new(
                NodeKind::Backbone(id),
                Some(seq_name.into()),
                bb.path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                "Backbone",
            );
            node.columns[4] = if bb.parse_error.is_some() {
                "ERROR"
            } else {
                "OK"
            }
            .into();
            node.columns[5] = relative(&bb.path, &dossier.root.join(seq_name));
            node.columns[7] = format!(
                "{}{}, leaves={}",
                bb.kind,
                bb.region
                    .as_ref()
                    .map_or(String::new(), |r| format!(" ({r})")),
                bb.leaf_count
            );
            node.columns[9] = bb.parse_error.clone().unwrap_or_default();
            node.columns[10] = bb.path.display().to_string();
            self.push(bb_group, node);
        }
        let ids = if self.options.consolidated {
            &seq.consolidated
        } else {
            &seq.delta
        };
        let backbone_paths: HashSet<_> =
            dossier.backbones.iter().map(|b| b.path.as_path()).collect();
        let mut entries: Vec<(Vec<String>, Node)> = vec![];
        for &id in ids {
            let leaf = &dossier.leaves[id];
            let file = dossier.open_path(id);
            if !self.options.show_backbones
                && file.is_some_and(|p| backbone_paths.contains(p) || auxiliary(dossier, p))
            {
                continue;
            }
            let mut parts = if self.options.toc {
                leaf.toc.clone()
            } else {
                filesystem_parts(dossier, file, seq_name, self.options.consolidated)
            };
            let name = if self.options.toc {
                dossier.title(id)
            } else {
                parts.pop().unwrap_or_else(|| dossier.filename(id))
            };
            let mut node = Node::new(
                NodeKind::Leaf(id),
                Some(seq_name.into()),
                name,
                if self.options.toc { "Document" } else { "File" },
            );
            node.columns[2] = leaf.operation.clone();
            node.columns[3] = leaf.sequence.clone();
            node.columns[4] = dossier.status(id, self.options.consolidated);
            node.columns[5] = relative(
                &dossier.backbones[leaf.backbone].path,
                &dossier.root.join(&leaf.sequence),
            );
            node.columns[6] = leaf.id.clone();
            node.columns[7] = if self.options.toc {
                dossier.filename(id)
            } else {
                leaf.title.clone()
            };
            node.columns[8] = leaf.href.clone();
            node.columns[9] = leaf.modified_file.clone();
            node.columns[10] = file.map_or(String::new(), |p| relative(p, &dossier.root));
            node.columns[11] = leaf.toc.join(" / ");
            entries.push((parts, node));
        }
        if !self.options.toc {
            // A superseded document is still XML-referenced and must not reappear as an orphan.
            let referenced: HashSet<_> = dossier
                .leaves
                .iter()
                .filter_map(|leaf| leaf.file.as_deref())
                .collect();
            for file in &seq.files {
                let file = normalize(file);
                if referenced.contains(file.as_path()) {
                    continue;
                }
                if !self.options.show_backbones
                    && (backbone_paths.contains(file.as_path()) || auxiliary(dossier, &file))
                {
                    continue;
                }
                let mut parts =
                    filesystem_parts(dossier, Some(&file), seq_name, self.options.consolidated);
                let name = parts.pop().unwrap_or_default();
                let mut node = Node::new(
                    NodeKind::Orphan(file.clone()),
                    Some(seq_name.into()),
                    name,
                    "File",
                );
                node.columns[4] = "ORPHAN".into();
                node.columns[7] = "(not referenced in XML)".into();
                node.columns[10] = relative(&file, &dossier.root);
                entries.push((parts, node));
            }
        }
        entries.sort_by(|(a, an), (b, bn)| {
            a.cmp(b)
                .then(an.columns[0].cmp(&bn.columns[0]))
                .then(an.columns[10].cmp(&bn.columns[10]))
        });
        let mut groups = HashMap::new();
        let mut leaf_nodes = HashMap::new();
        for (parts, node) in entries {
            let parent = self.path(documents, &parts, &mut groups);
            let leaf = if let NodeKind::Leaf(id) = node.kind {
                Some(id)
            } else {
                None
            };
            let node_id = self.push(parent, node);
            if let Some(id) = leaf {
                leaf_nodes.insert(id, node_id);
            }
        }
        if self.options.toc && self.options.consolidated {
            let mut append_groups = HashMap::new();
            let mut ids: Vec<_> = leaf_nodes.keys().copied().collect();
            ids.sort_unstable();
            for id in ids {
                let leaf = &dossier.leaves[id];
                if leaf.operation != "append" {
                    continue;
                }
                let Some(base) = leaf.predecessor.and_then(|p| leaf_nodes.get(&p).copied()) else {
                    continue;
                };
                let node_id = leaf_nodes[&id];
                // Invalid lifecycle cycles must never create cycles in the visual tree.
                let mut current = Some(base);
                let mut cycle = false;
                while let Some(node) = current {
                    if node == node_id {
                        cycle = true;
                        break;
                    }
                    current = self.nodes[node].parent;
                }
                if cycle {
                    continue;
                }
                let group = *append_groups
                    .entry(base)
                    .or_insert_with(|| self.group(base, "Append", "Group"));
                if let Some(parent) = self.nodes[node_id].parent {
                    self.nodes[parent].children.retain(|&id| id != node_id);
                }
                self.nodes[node_id].parent = Some(group);
                self.nodes[group].children.push(node_id);
            }
        }
        for &id in &self.nodes[documents].children {
            self.expanded.insert(id);
        }
        // A populated sequence changes the searchable rows even if the query is unchanged.
        self.query.clear();
    }

    pub fn expand_sequence(&mut self, dossier: &Dossier, name: &str) {
        for (path, expanded) in &mut self.expansion_memory {
            if path
                .iter()
                .any(|(kind, _, _)| matches!(kind, NodeKind::Sequence(seq) if seq == name))
            {
                *expanded = true;
            }
        }
        self.populate(dossier, name);
        if let Some(&id) = self.sequence_nodes.get(name) {
            self.expand_branch(id);
        }
        self.expanded.insert(0);
    }

    pub fn collapse_to_sequences(&mut self) {
        self.expanded.clear();
        self.expanded.insert(0);
        self.expansion_memory
            .values_mut()
            .for_each(|open| *open = false);
    }

    fn expand_branch(&mut self, id: usize) {
        self.expanded.insert(id);
        let children = self.nodes[id].children.clone();
        for child in children {
            self.expand_branch(child);
        }
    }

    pub fn search(&mut self, dossier: &Dossier, query: &str) {
        let query = query.trim().to_lowercase();
        if query == self.query {
            return;
        }
        if !query.is_empty() {
            for seq in &dossier.sequences {
                self.populate(dossier, &seq.name);
            }
        }
        self.matches.clear();
        if !query.is_empty() {
            for (id, node) in self.nodes.iter().enumerate() {
                if node
                    .columns
                    .iter()
                    .any(|c| c.to_lowercase().contains(&query))
                {
                    let mut current = Some(id);
                    while let Some(id) = current {
                        if !self.matches.insert(id) {
                            break;
                        }
                        current = self.nodes[id].parent;
                    }
                }
            }
        }
        self.query = query;
    }

    pub fn visible(&self) -> Vec<(usize, usize)> {
        let mut rows = vec![];
        let mut stack = vec![(0, 0)];
        while let Some((id, depth)) = stack.pop() {
            if !self.query.is_empty() && !self.matches.contains(&id) {
                continue;
            }
            rows.push((id, depth));
            if self.expanded.contains(&id) || !self.query.is_empty() {
                for &child in self.nodes[id].children.iter().rev() {
                    stack.push((child, depth + 1));
                }
            }
        }
        rows
    }
}

fn auxiliary(dossier: &Dossier, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(&dossier.root) else {
        return false;
    };
    let mut parts = rel.components();
    if !parts
        .next()
        .is_some_and(|c| is_sequence(&c.as_os_str().to_string_lossy()))
    {
        return false;
    }
    let name = parts.next().map(|p| p.as_os_str().to_string_lossy());
    matches!(name.as_deref(), Some("util" | "index-md5.txt"))
}

fn filesystem_parts(
    dossier: &Dossier,
    path: Option<&Path>,
    seq: &str,
    consolidated: bool,
) -> Vec<String> {
    let Some(path) = path else {
        return vec!["<no file>".into()];
    };
    let rel = if consolidated {
        path.strip_prefix(&dossier.root)
    } else {
        path.strip_prefix(dossier.root.join(seq))
            .or_else(|_| path.strip_prefix(&dossier.root))
    };
    let mut parts: Vec<_> = rel
        .unwrap_or_else(|_| Path::new(path.file_name().unwrap_or_default()))
        .components()
        .map(|p| p.as_os_str().to_string_lossy().into_owned())
        .collect();
    if consolidated && parts.first().is_some_and(|s| is_sequence(s)) {
        parts.remove(0);
    }
    if parts.is_empty() {
        parts.push("<no file>".into());
    }
    parts
}
