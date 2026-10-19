// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use ectd_rust::{
    dossier::{Dossier, is_sequence, resolve_path},
    export::export_consolidated,
    tree::{NodeKind, Tree, ViewOptions},
};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

fn demo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/demo-dossier")
}
fn leaf(dossier: &Dossier, id: &str) -> usize {
    dossier.leaves.iter().position(|l| l.id == id).unwrap()
}
fn active(dossier: &Dossier, seq: &str) -> Vec<String> {
    let mut ids: Vec<_> = dossier
        .sequence(seq)
        .unwrap()
        .consolidated
        .iter()
        .map(|&id| dossier.leaves[id].id.clone())
        .collect();
    ids.sort();
    ids
}
fn put(root: &Path, name: &str, value: impl AsRef<[u8]>) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, value).unwrap();
}
fn xml(body: &str) -> String {
    format!(r#"<ectd xmlns:xlink="http://www.w3.org/1999/xlink"><m2>{body}</m2></ectd>"#)
}

#[test]
fn discovery_and_lifecycle_snapshots() {
    let d = Dossier::load(&demo()).unwrap();
    assert_eq!(
        d.sequences
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["0000", "0001", "0002"]
    );
    assert_eq!(d.backbones.len(), 4);
    assert!(
        d.backbones.iter().all(|bb| bb.parse_error.is_none()),
        "{:?}",
        d.backbones
    );
    assert!(
        d.leaves.iter().all(|l| l.warnings.is_empty()),
        "{:?}",
        d.leaves
    );
    assert_eq!(
        active(&d, "0000"),
        [
            "cover-at",
            "product-information",
            "regional-0000",
            "substance-v1",
            "summary-v1"
        ]
    );
    assert_eq!(
        active(&d, "0001"),
        [
            "cover-at",
            "product-information",
            "regional-0000",
            "substance-addendum",
            "substance-v1",
            "summary-v2"
        ]
    );
    assert_eq!(
        active(&d, "0002"),
        [
            "cover-at",
            "product-information",
            "regional-0000",
            "substance-addendum",
            "summary-v2"
        ]
    );
    let deleted = leaf(&d, "substance-deletion");
    assert_eq!(d.open_path(deleted), d.open_path(leaf(&d, "substance-v1")));
    assert_eq!(d.status(deleted, false), "DELETED");
    let (versions, cycle) = d.versions(leaf(&d, "summary-v2"));
    assert_eq!(versions, [leaf(&d, "summary-v2"), leaf(&d, "summary-v1")]);
    assert!(!cycle);
}

#[test]
fn complete_leaf_xml_metadata_and_regional_toc() {
    let d = Dossier::load(&demo()).unwrap();
    let summary = &d.leaves[leaf(&d, "summary-v1")];
    assert_eq!(summary.title, "Quality & safety overview");
    assert!(
        summary
            .xml
            .contains("<note language=\"en\">Complete nested metadata</note>")
    );
    assert!(summary.xml.contains("Quality &amp; safety overview"));
    assert_eq!(summary.metadata["leaf[1]/extension[0]/note@language"], "en");
    assert_eq!(
        summary.metadata["leaf[1]/extension[0]/note#text"],
        "Complete nested metadata"
    );
    assert_eq!(summary.attributes["checksum-type"], "md5");
    assert_eq!(
        &d.leaves[leaf(&d, "product-information")].toc[2..],
        ["de", "sm-pc", "at"]
    );
    assert_eq!(d.leaves[leaf(&d, "cover-at")].toc.last().unwrap(), "at");
    assert!(
        d.leaves[leaf(&d, "substance-v1")]
            .toc
            .last()
            .unwrap()
            .contains("manufacturer=Example manufacturer")
    );
    assert_eq!(
        d.backbones[0].namespaces["xlink"],
        "http://www.w3.org/1999/xlink"
    );
}

#[test]
fn empty_leaves_cdata_entities_prefixed_names_and_local_references() {
    let tmp = TempDir::new().unwrap();
    put(tmp.path(), "0000/m2/file name.txt", "exists");
    put(
        tmp.path(),
        "0000/index.xml",
        r##"<?xml version="1.0"?><e:ectd xmlns:e="urn:ectd" xmlns:link="http://www.w3.org/1999/xlink"><m2><leaf Id="a" link:href="m2\file%20name.txt#page=2"><title><![CDATA[A <title>]]> &amp; &#x42;</title><meta value="a &amp; b">Text</meta></leaf><leaf ID="b" title="Empty leaf" href="m2/file%20name.txt"/><leaf ID="c" operation="delete" modified-file="#a"/></m2></e:ectd>"##,
    );
    let d = Dossier::load(tmp.path()).unwrap();
    assert!(d.backbones[0].parse_error.is_none(), "{:?}", d.backbones[0]);
    assert_eq!(d.leaves.len(), 3);
    assert_eq!(d.leaves[0].title, "A <title> & B");
    assert_eq!(d.leaves[0].metadata["leaf[1]/meta@value"], "a & b");
    assert!(d.leaves.iter().all(|l| !l.missing));
    assert_eq!(d.leaves[2].predecessor, Some(0));
    assert!(d.leaves[1].xml.ends_with("/>"));
    assert_eq!(active(&d, "0000"), ["b"]);
}

#[test]
fn parse_errors_and_broken_links_remain_visible() {
    let tmp = TempDir::new().unwrap();
    put(
        tmp.path(),
        "0000/index.xml",
        xml(
            r#"<leaf ID="missing" xlink:href="absent.pdf"/><leaf ID="broken" operation="replace" modified-file="../9999/index.xml#nope"/><leaf ID="no-fragment" operation="append" modified-file="index.xml"/><leaf ID="no-ref" operation="delete"/>"#,
        ),
    );
    put(
        tmp.path(),
        "0000/m1/us/us-regional.xml",
        "<root><leaf ID=\"bad\"></root>",
    );
    let d = Dossier::load(tmp.path()).unwrap();
    assert!(d.backbones.iter().any(|bb| bb.parse_error.is_some()));
    assert!(d.leaves[leaf(&d, "missing")].missing);
    assert!(d.status(leaf(&d, "broken"), false).contains("BROKEN-LINK"));
    assert!(
        d.leaves[leaf(&d, "no-fragment")]
            .warnings
            .iter()
            .any(|s| s.contains("fragment"))
    );
    assert!(d.status(leaf(&d, "no-ref"), false).contains("BROKEN-LINK"));
    let mut tree = Tree::new(&d, ViewOptions::default());
    tree.expand_sequence(&d, "0000");
    assert!(tree.nodes.iter().any(|n| n.columns[4] == "ERROR"));
}

#[test]
fn predecessor_cycles_are_finite_in_model_and_toc_tree() {
    let tmp = TempDir::new().unwrap();
    put(
        tmp.path(),
        "0000/index.xml",
        xml(
            r##"<leaf ID="a" operation="append" modified-file="#b"/><leaf ID="b" operation="append" modified-file="#a"/><leaf ID="c" operation="delete" modified-file="#d"/><leaf ID="d" operation="delete" modified-file="#c"/>"##,
        ),
    );
    let d = Dossier::load(tmp.path()).unwrap();
    assert!(d.versions(0).1);
    assert!(d.open_path(2).is_none());
    assert!(
        d.leaves
            .iter()
            .all(|l| l.warnings.iter().any(|w| w.contains("Cycle")))
    );
    let mut tree = Tree::new(
        &d,
        ViewOptions {
            toc: true,
            consolidated: true,
            show_backbones: false,
        },
    );
    tree.expand_sequence(&d, "0000");
    let rows = tree.visible();
    let unique: std::collections::HashSet<_> = rows.iter().map(|r| r.0).collect();
    assert_eq!(rows.len(), unique.len());
    assert!(rows.len() <= tree.nodes.len());
}

#[test]
fn search_loads_collapsed_sequences_and_includes_ancestors() {
    let d = Dossier::load(&demo()).unwrap();
    let mut tree = Tree::new(&d, ViewOptions::default());
    assert_eq!(tree.visible().len(), 4);
    assert!(
        tree.sequence_nodes
            .values()
            .all(|&id| !tree.nodes[id].populated)
    );
    tree.search(&d, "REVISED QUALITY");
    let visible: Vec<_> = tree
        .visible()
        .iter()
        .map(|(id, _)| tree.nodes[*id].columns[0].clone())
        .collect();
    assert!(visible.contains(&"summary.txt".into()));
    assert!(visible.contains(&"0001".into()));
    assert!(!visible.contains(&"0002".into()));
    tree.search(&d, "cannot-match-any-document");
    assert!(tree.visible().is_empty());
    tree.search(&d, "");
    assert_eq!(tree.visible().len(), 4);
}

fn branch_states(tree: &Tree) -> std::collections::HashMap<Vec<String>, bool> {
    tree.nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| !node.children.is_empty() || !node.populated)
        .map(|(id, _)| {
            let mut path = Vec::new();
            let mut current = Some(id);
            while let Some(parent) = current {
                path.push(tree.nodes[parent].columns[0].clone());
                current = tree.nodes[parent].parent;
            }
            (path, tree.expanded.contains(&id))
        })
        .collect()
}

fn visible_leaves(tree: &Tree) -> std::collections::HashSet<usize> {
    tree.visible()
        .into_iter()
        .filter_map(|(id, _)| match tree.nodes[id].kind {
            NodeKind::Leaf(leaf) => Some(leaf),
            _ => None,
        })
        .collect()
}

#[test]
fn view_switch_preserves_fully_expanded_hierarchies_with_reordered_nodes() {
    let d = Dossier::load(&demo()).unwrap();
    let options = ViewOptions::default();
    let mut tree = Tree::new(&d, options);
    // Populate out of sequence order: numeric row IDs change during the rebuild.
    tree.expand_sequence(&d, "0001");
    tree.expand_sequence(&d, "0000");
    let branches = branch_states(&tree);
    let documents = visible_leaves(&tree);
    tree.set_options(
        &d,
        ViewOptions {
            toc: true,
            ..options
        },
    );
    assert_eq!(visible_leaves(&tree), documents);
    tree.set_options(&d, options);
    assert_eq!(visible_leaves(&tree), documents);
    assert_eq!(branch_states(&tree), branches);
    assert!(!tree.nodes[tree.sequence_nodes["0002"]].populated);
}

#[test]
fn view_switch_remembers_partial_branches_and_closed_parents() {
    let d = Dossier::load(&demo()).unwrap();
    let options = ViewOptions::default();
    let mut tree = Tree::new(&d, options);
    tree.expand_sequence(&d, "0001");
    let m2 = tree
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Group && n.columns[0] == "m2")
        .unwrap();
    tree.expanded.remove(&m2);
    let files = branch_states(&tree);
    let sequence = tree.sequence_nodes["0001"];
    tree.expanded.remove(&sequence);

    let toc_options = ViewOptions {
        toc: true,
        ..options
    };
    tree.set_options(&d, toc_options);
    assert!(!tree.expanded.contains(&tree.sequence_nodes["0001"]));
    tree.expanded.insert(tree.sequence_nodes["0001"]);
    assert!(!visible_leaves(&tree).contains(&leaf(&d, "summary-v2")));
    assert!(visible_leaves(&tree).contains(&leaf(&d, "substance-addendum")));
    let section = tree
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Group && n.columns[0] == "m3-2-body-of-data")
        .unwrap();
    tree.expanded.remove(&section);
    let toc = branch_states(&tree);

    tree.expanded.remove(&0);
    tree.set_options(&d, options);
    assert_eq!(tree.visible(), vec![(0, 0)]);
    tree.expanded.insert(0);
    assert_eq!(branch_states(&tree), files);
    tree.set_options(&d, toc_options);
    assert_eq!(branch_states(&tree), toc);
    for name in ["0000", "0002"] {
        assert!(!tree.nodes[tree.sequence_nodes[name]].populated);
    }
}

#[test]
fn expand_and_collapse_update_remembered_views() {
    let d = Dossier::load(&demo()).unwrap();
    let options = ViewOptions::default();
    let toc_options = ViewOptions {
        toc: true,
        ..options
    };
    let mut tree = Tree::new(&d, options);
    tree.expand_sequence(&d, "0001");
    let documents = visible_leaves(&tree);
    tree.set_options(&d, toc_options);
    tree.collapse_to_sequences();
    tree.set_options(&d, options);
    assert_eq!(tree.visible().len(), d.sequences.len() + 1);
    tree.expand_sequence(&d, "0001");
    tree.set_options(&d, toc_options);
    assert_eq!(visible_leaves(&tree), documents);
}

#[test]
fn search_during_view_switch_does_not_replace_branch_state() {
    let d = Dossier::load(&demo()).unwrap();
    let options = ViewOptions::default();
    let mut tree = Tree::new(&d, options);
    tree.expand_sequence(&d, "0001");
    let m2 = tree
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Group && n.columns[0] == "m2")
        .unwrap();
    tree.expanded.remove(&m2);
    tree.search(&d, "Revised quality");
    tree.set_options(
        &d,
        ViewOptions {
            toc: true,
            ..options
        },
    );
    tree.search(&d, "Revised quality");
    assert!(visible_leaves(&tree).contains(&leaf(&d, "summary-v2")));
    tree.search(&d, "");
    assert!(!visible_leaves(&tree).contains(&leaf(&d, "summary-v2")));
    tree.set_options(&d, options);
    assert!(!visible_leaves(&tree).contains(&leaf(&d, "summary-v2")));
}

#[test]
fn append_grouping_and_orphan_detection() {
    let d = Dossier::load(&demo()).unwrap();
    let mut tree = Tree::new(
        &d,
        ViewOptions {
            toc: true,
            consolidated: true,
            show_backbones: false,
        },
    );
    tree.expand_sequence(&d, "0001");
    let node = tree
        .nodes
        .iter()
        .position(|n| n.kind == NodeKind::Leaf(leaf(&d, "substance-addendum")))
        .unwrap();
    let parent = tree.nodes[node].parent.unwrap();
    assert_eq!(tree.nodes[parent].columns[0], "Append");
    let base = tree.nodes[parent].parent.unwrap();
    assert_eq!(
        tree.nodes[base].kind,
        NodeKind::Leaf(leaf(&d, "substance-v1"))
    );
    // The row belongs to the selected snapshot even for documents inherited from 0000.
    assert_eq!(tree.nodes[base].sequence.as_deref(), Some("0001"));
    let mut tree = Tree::new(
        &d,
        ViewOptions {
            consolidated: true,
            ..Default::default()
        },
    );
    tree.expand_sequence(&d, "0001");
    let orphans: Vec<_> = tree
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Orphan(_)))
        .collect();
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].columns[0], "unreferenced.txt");
    assert!(
        !tree
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Leaf(leaf(&d, "regional-0000")))
    );
    let mut tree = Tree::new(
        &d,
        ViewOptions {
            show_backbones: true,
            ..Default::default()
        },
    );
    tree.populate(&d, "0000");
    assert!(
        tree.nodes
            .iter()
            .any(|n| n.kind == NodeKind::Leaf(leaf(&d, "regional-0000")))
    );
}

#[test]
fn export_active_documents_and_preserve_sources() {
    let d = Dossier::load(&demo()).unwrap();
    let tmp = TempDir::new().unwrap();
    let output = tmp.path().join("export");
    let report = export_consolidated(&d, "0002", &output).unwrap();
    assert_eq!(report.copied, 4);
    assert_eq!(report.skipped_backbones, 1);
    assert_eq!(report.missing, 0);
    assert!(report.errors.is_empty());
    assert_eq!(
        fs::read_to_string(output.join("m2/summary.txt")).unwrap(),
        fs::read_to_string(demo().join("0001/m2/summary.txt")).unwrap()
    );
    assert!(output.join("m3/addendum.txt").is_file());
    assert!(!output.join("m3/substance.txt").exists());
    assert!(!output.join("m1/eu/eu-regional.xml").exists());
    assert!(!output.join("unreferenced.txt").exists());
    assert!(demo().join("0000/m3/substance.txt").is_file());
    assert!(export_consolidated(&d, "0002", &output).is_err());
    assert!(export_consolidated(&d, "0002", &d.root.join("0000/export")).is_err());
    assert!(export_consolidated(&d, "0002", &d.root).is_err());
}

#[test]
fn export_collisions_are_renamed_without_losing_documents() {
    let tmp = TempDir::new().unwrap();
    put(
        tmp.path(),
        "source/0000/index.xml",
        xml(r#"<leaf ID="a" xlink:href="m2/report.txt"/>"#),
    );
    put(
        tmp.path(),
        "source/0001/index.xml",
        xml(
            r##"<leaf ID="b" operation="append" modified-file="../0000/index.xml#a" xlink:href="m2/report.txt"/><leaf ID="c" xlink:href="m2/report.txt"/>"##,
        ),
    );
    put(tmp.path(), "source/0000/m2/report.txt", "original");
    put(tmp.path(), "source/0001/m2/report.txt", "appendix");
    let d = Dossier::load(&tmp.path().join("source")).unwrap();
    let output = tmp.path().join("export");
    let report = export_consolidated(&d, "0001", &output).unwrap();
    assert_eq!(report.copied, 3);
    assert_eq!(report.renamed, 2);
    assert_eq!(
        fs::read_to_string(output.join("m2/report.txt")).unwrap(),
        "original"
    );
    assert_eq!(
        fs::read_to_string(output.join("m2/report-0001.txt")).unwrap(),
        "appendix"
    );
    assert_eq!(
        fs::read_to_string(output.join("m2/report-0001-2.txt")).unwrap(),
        "appendix"
    );
}

#[test]
fn errors_empty_sequences_and_path_resolution() {
    let tmp = TempDir::new().unwrap();
    assert!(Dossier::load(tmp.path()).is_err());
    fs::create_dir(tmp.path().join("0007")).unwrap();
    fs::create_dir(tmp.path().join("007")).unwrap();
    let d = Dossier::load(tmp.path()).unwrap();
    assert_eq!(d.sequences.len(), 1);
    assert!(d.warnings.iter().any(|w| w.contains("no backbone")));
    assert!(is_sequence("0007"));
    assert!(!is_sequence("00a7"));
    assert!(!is_sequence("１２３４"));
    let root = tmp.path().canonicalize().unwrap();
    assert_eq!(
        resolve_path(
            &root.join("0007/index.xml"),
            "../0007/m2/file%20name.pdf#page=1"
        )
        .unwrap(),
        root.join("0007/m2/file name.pdf")
    );
    assert!(resolve_path(&root.join("index.xml"), "https://example.org/doc.pdf").is_err());
    assert!(resolve_path(&root.join("index.xml"), "bad%00path").is_err());
}

#[test]
fn legacy_xml_encoding_and_malformed_documents() {
    let tmp = TempDir::new().unwrap();
    let mut bytes =
        b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><ectd><leaf ID=\"latin\"><title>Pr"
            .to_vec();
    bytes.push(0xfc);
    bytes.extend_from_slice(b"fung</title></leaf></ectd>");
    put(tmp.path(), "0000/index.xml", bytes);
    let d = Dossier::load(tmp.path()).unwrap();
    assert!(d.backbones[0].parse_error.is_none());
    assert_eq!(d.leaves[0].title, "Prüfung");
    assert!(d.leaves[0].xml.contains("Prüfung"));
    for invalid in [
        "",
        "<root>",
        "<root/><other/>",
        "<root><leaf></root>",
        "<root><leaf>&undefined;</leaf></root>",
    ] {
        put(tmp.path(), "0000/index.xml", invalid);
        assert!(
            Dossier::load(tmp.path()).unwrap().backbones[0]
                .parse_error
                .is_some(),
            "{invalid}"
        );
    }
}

#[cfg(unix)]
#[test]
fn symlink_directory_cycles_are_not_followed() {
    use std::os::unix::fs::symlink;
    let tmp = TempDir::new().unwrap();
    put(tmp.path(), "source/0000/index.xml", "<ectd/>");
    symlink(
        tmp.path().join("source/0000"),
        tmp.path().join("source/0000/loop"),
    )
    .unwrap();
    let d = Dossier::load(&tmp.path().join("source")).unwrap();
    assert_eq!(d.sequences[0].files.len(), 1);
    symlink(
        tmp.path().join("source/0000"),
        tmp.path().join("export-link"),
    )
    .unwrap();
    assert!(export_consolidated(&d, "0000", &tmp.path().join("export-link/out")).is_err());
}
