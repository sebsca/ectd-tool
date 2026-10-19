// SPDX-FileCopyrightText: 2026 Sebastian Scala
// SPDX-License-Identifier: GPL-3.0-or-later

use ectd_rust::{
    dossier::Dossier,
    tool::{
        self, Options, OutputPolicy, Row, checksum,
        dtd::{Schema, attributes},
        extract, generate,
        mapping::Mapping,
        workbook::{self, MetadataBook},
        xml::{Xml, local},
    },
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

fn put(root: &Path, name: &str, data: impl AsRef<[u8]>) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, data).unwrap();
}
fn setup(root: &Path, seq: &str) -> PathBuf {
    let dir = root.join(seq);
    for (name, data) in [
        (
            "ich-ectd-3-2.dtd",
            include_str!("fixtures/tool/ich-ectd-3-2.dtd"),
        ),
        (
            "eu-regional.dtd",
            include_str!("fixtures/tool/eu-regional.dtd"),
        ),
        (
            "eu-envelope.mod",
            include_str!("fixtures/tool/eu-envelope.mod"),
        ),
    ] {
        put(&dir, &format!("util/dtd/{name}"), data);
    }
    dir
}
fn row(path: &str, title: &str, op: &str) -> Row {
    Row::from([
        ("file_path".into(), path.into()),
        ("title".into(), title.into()),
        ("operation".into(), op.into()),
    ])
}
fn write_rows(root: &Path, seq: &str, rows: &[Row]) {
    workbook::replace(
        &root.join(format!("metadata-{seq}.xlsx")),
        rows,
        &mut OutputPolicy::new(true),
    )
    .unwrap();
}
fn run(root: &Path, seq: &str) -> tool::Result<()> {
    tool::run(Options {
        base: root.into(),
        sequence: seq.into(),
        scan: false,
        extract: false,
        metadata: None,
        mapping: None,
        eu_mapping: None,
        yes: true,
    })
}
fn leaves(xml: &Xml) -> Vec<usize> {
    xml.descendants(0)
        .into_iter()
        .filter(|&id| local(&xml.nodes[id].name) == "leaf")
        .collect()
}

#[test]
fn scan_preserves_reordered_headers_existing_rows_and_other_sheets() {
    let temp = TempDir::new().unwrap();
    let seq = setup(temp.path(), "0001");
    put(&seq, "m2/new-report.pdf", "new");
    put(&seq, "m2/existing.pdf", "old");
    put(&seq, "root.pdf", "skip");
    put(&seq, "m1/eu/eu-regional.xml", "skip");
    let path = temp.path().join("metadata-0001.xlsx");
    let mut book = umya_spreadsheet::new_file();
    let ws = book.active_sheet_mut();
    ws.set_name("metadata");
    for (cell, v) in [
        ("A1", "title"),
        ("B1", "operation"),
        ("C1", "file_path"),
        ("D1", "custom"),
        ("A2", "Keep title"),
        ("B2", "replace"),
        ("C2", "m2/existing.pdf"),
        ("D2", "Keep value"),
    ] {
        ws.cell_mut(cell).set_value_string(v);
    }
    book.new_sheet("notes")
        .unwrap()
        .cell_mut("A1")
        .set_value_string("Preserved");
    umya_spreadsheet::writer::xlsx::write(&book, &path).unwrap();
    assert_eq!(
        workbook::scan(&seq, &path, &mut OutputPolicy::new(true)).unwrap(),
        1
    );
    let book = MetadataBook::read(&path).unwrap();
    let rows = book.rows().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["title"], "Keep title");
    assert_eq!(rows[0]["custom"], "Keep value");
    assert_eq!(rows[1]["file_path"], "m2/new-report.pdf");
    assert_eq!(rows[1]["title"], "new report");
    assert_eq!(rows[1]["operation"], "new");
    assert_eq!(
        book.book.sheet_by_name("notes").unwrap().value("A1"),
        "Preserved"
    );
    assert_eq!(
        workbook::scan(&seq, &path, &mut OutputPolicy::new(true)).unwrap(),
        0
    );
}

#[test]
fn generation_validates_namespaced_backbones_envelope_and_checksums() {
    let temp = TempDir::new().unwrap();
    let seq = setup(temp.path(), "0000");
    let mut rows = vec![
        row("m2/23-qos/summary.pdf", "Quality & safety", "new"),
        row("m1/eu/10-cover/at/at-cover.pdf", "Cover letter", "new"),
        row(
            "m1/eu/13-pi/131-spclabelpl/at/de/spc/pi.pdf",
            "Produktinformation",
            "new",
        ),
    ];
    rows[0].extend(Row::from([
        ("applicant_name".into(), "Test Pharma".into()),
        ("eu_submission_type".into(), "var-type2".into()),
        ("eu_submission_mode".into(), "grouping".into()),
        ("eu_identifier".into(), "fixed-id".into()),
        ("eu_country".into(), "at".into()),
        ("eu_inn".into(), "Sugar".into()),
        (
            "sequence_description".into(),
            "Change of manufacturer".into(),
        ),
    ]));
    for r in &rows {
        put(&seq, &r["file_path"], r["title"].as_bytes());
    }
    write_rows(temp.path(), "0000", &rows);
    run(temp.path(), "0000").unwrap();
    let index = Xml::read(&seq.join("index.xml")).unwrap();
    let eu = Xml::read(&seq.join("m1/eu/eu-regional.xml")).unwrap();
    assert_eq!(index.nodes[0].name, "ectd:ectd");
    assert_eq!(
        index.nodes[index.nodes[0].children[0]].name,
        tool::MODULES[0]
    );
    let regional = leaves(&index)
        .into_iter()
        .find(|&id| index.href(id).ends_with(".xml"))
        .unwrap();
    assert_eq!(
        index.nodes[regional].attrs["checksum"],
        checksum(&seq.join("m1/eu/eu-regional.xml")).unwrap()
    );
    assert_eq!(
        eu.nodes[eu.descendant(0, "applicant").unwrap()].text,
        "Test Pharma"
    );
    let submission = eu.descendant(0, "submission").unwrap();
    assert_eq!(eu.nodes[submission].attrs["mode"], "grouping");
    let pi = eu.descendant(0, "pi-doc").unwrap();
    assert_eq!(eu.nodes[pi].attrs["xml:lang"], "de");
    assert_eq!(eu.nodes[pi].attrs["type"], "spc");
    assert_eq!(
        fs::read_to_string(seq.join("index-md5.txt")).unwrap(),
        format!("{}  index.xml\n", checksum(&seq.join("index.xml")).unwrap())
    );
    let d = Dossier::load(temp.path()).unwrap();
    assert!(
        d.leaves.iter().all(|l| l.warnings.is_empty()),
        "{:?}",
        d.leaves
    );
}

#[test]
fn lifecycle_roundtrip_resolves_regional_modified_files_from_their_xml_directory() {
    let temp = TempDir::new().unwrap();
    let seq0 = setup(temp.path(), "0000");
    let rows0 = vec![
        row("m2/23-qos/summary.pdf", "Summary", "new"),
        row("m1/eu/10-cover/at/at-cover.pdf", "Cover", "new"),
    ];
    for r in &rows0 {
        put(&seq0, &r["file_path"], "v1");
    }
    write_rows(temp.path(), "0000", &rows0);
    run(temp.path(), "0000").unwrap();
    let ix = Xml::read(&seq0.join("index.xml")).unwrap();
    let eu = Xml::read(&seq0.join("m1/eu/eu-regional.xml")).unwrap();
    let summary = leaves(&ix)
        .into_iter()
        .find(|&id| ix.href(id).ends_with("summary.pdf"))
        .unwrap();
    let cover = leaves(&eu)[0];
    let seq1 = setup(temp.path(), "0001");
    put(&seq1, "m1/eu/10-cover/at/new-cover.pdf", "v2");
    let mut delete = row("", "Deleted summary", "delete");
    delete.insert(
        "modified-leaf".into(),
        ix.nodes[summary].attrs["ID"].clone(),
    );
    let mut replace = row("m1/eu/10-cover/at/new-cover.pdf", "New cover", "replace");
    replace.insert("modified-leaf".into(), eu.nodes[cover].attrs["ID"].clone());
    write_rows(temp.path(), "0001", &[delete, replace]);
    run(temp.path(), "0001").unwrap();
    let d = Dossier::load(temp.path()).unwrap();
    for leaf in &d.leaves {
        assert!(leaf.warnings.is_empty(), "{:?}", leaf);
    }
    assert_eq!(
        d.sequences[1]
            .consolidated
            .iter()
            .filter(|&&i| !d.leaves[i].href.ends_with(".xml"))
            .count(),
        1
    );
    let extracted = extract::extract(&seq1).unwrap();
    let deleted = extracted
        .iter()
        .find(|r| tool::value(r, "operation") == "delete")
        .unwrap();
    assert_eq!(deleted["file_path"], "m2/23-qos/summary.pdf");
    let replacement = extracted
        .iter()
        .find(|r| tool::value(r, "operation") == "replace")
        .unwrap();
    assert_eq!(replacement["modified-leaf"], eu.nodes[cover].attrs["ID"]);
}

#[test]
fn extraction_keeps_document_order_branch_attributes_and_envelope() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/demo-dossier/0000");
    let rows = extract::extract(&root).unwrap();
    let docs: Vec<_> = rows
        .iter()
        .filter(|r| !tool::value(r, "operation").is_empty())
        .map(|r| r["title"].as_str())
        .collect();
    assert_eq!(
        docs,
        [
            "Austrian cover letter",
            "Product information",
            "Quality & safety overview",
            "Drug substance"
        ]
    );
    assert!(
        rows.iter()
            .any(|r| tool::value(r, "ctd_toc") == "m3-2-s-drug-substance"
                && tool::value(r, "attributes").contains("manufacturer=\"Example manufacturer\""))
    );
    assert!(rows.iter().any(|r| tool::value(r, "ctd_toc") == "pi-doc"
        && tool::value(r, "attributes").contains("xml:lang=\"de\"")));
}

#[test]
fn explicit_toc_rows_create_separate_contexts_and_filter_deletions() {
    let temp = TempDir::new().unwrap();
    let seq = setup(temp.path(), "0000");
    put(&seq, "m3/shared.pdf", "same document");
    let tocrow = |name: &str| {
        Row::from([
            ("ctd_toc".into(), "m3-2-s-drug-substance".into()),
            (
                "attributes".into(),
                format!("substance=\"{name}\" manufacturer=\"Example\""),
            ),
        ])
    };
    let mut doc = row("m3/shared.pdf", "Shared", "new");
    doc.insert("ctd_toc".into(), "m3-2-s-drug-substance".into());
    let rows = vec![tocrow("A"), doc.clone(), tocrow("B"), doc];
    write_rows(temp.path(), "0000", &rows);
    assert_eq!(
        MetadataBook::read(&temp.path().join("metadata-0000.xlsx"))
            .unwrap()
            .rows()
            .unwrap()
            .len(),
        4
    );
    run(temp.path(), "0000").unwrap();
    let xml = Xml::read(&seq.join("index.xml")).unwrap();
    assert_eq!(leaves(&xml).len(), 2);
    let seq1 = setup(temp.path(), "0001");
    let mut delete = row("m3/shared.pdf", "Delete A only", "delete");
    delete.insert("ctd_toc".into(), "m3-2-s-drug-substance".into());
    write_rows(temp.path(), "0001", &[tocrow("A"), delete]);
    run(temp.path(), "0001").unwrap();
    let xml1 = Xml::read(&seq1.join("index.xml")).unwrap();
    assert_eq!(leaves(&xml1).len(), 1);
    let old = leaves(&xml)[0];
    assert!(
        xml1.nodes[leaves(&xml1)[0]].attrs["modified-file"].ends_with(&xml.nodes[old].attrs["ID"])
    );
}

#[test]
fn mappings_custom_attributes_and_dtd_validation_errors() {
    let mapping=Mapping::parse("item_type,relative_path,xml_element\ndirectory,m3/drug-1,m3-generic\ndirectory,m3/drug-special,m3-specific\nfile,m3/drug-1/report.pdf,m3-report\n").unwrap();
    assert_eq!(mapping.matches("m3/drug-7", false), Some("m3-generic"));
    assert_eq!(
        mapping.matches("m3/drug-special", false),
        Some("m3-specific")
    );
    assert_eq!(
        mapping.matches("m3/drug-99/report-v2.pdf", true),
        Some("m3-report")
    );
    assert!(Mapping::parse("wrong,headers\na,b\n").is_err());
    let attrs = attributes("substance=“A &amp; B” manufacturer='Example'");
    assert_eq!(attrs["substance"], "A & B");
    assert_eq!(attrs["manufacturer"], "Example");
    let temp = TempDir::new().unwrap();
    let seq = setup(temp.path(), "0000");
    let schema = Schema::load(&seq.join("util/dtd/ich-ectd-3-2.dtd"), "ectd:ectd").unwrap();
    assert!(schema.validate("<ectd:ectd xmlns:ectd=\"http://www.ich.org/ectd\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" dtd-version=\"wrong\"/>","invalid").is_err());
    assert!(schema.validate("<ectd:ectd xmlns:ectd=\"http://www.ich.org/ectd\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" dtd-version=\"3.2\"><unknown/></ectd:ectd>","unknown").is_err());
    let schema = Schema::load(&seq.join("util/dtd/eu-regional.dtd"), "eu:eu-backbone").unwrap();
    assert_eq!(
        schema.enumeration("envelope", "country"),
        ["at", "de", "common", "ema"]
    );
}

#[test]
fn invalid_generation_keeps_existing_outputs_and_metadata_untouched() {
    let temp = TempDir::new().unwrap();
    let seq = setup(temp.path(), "0000");
    put(&seq, "index.xml", "keep existing index");
    put(&seq, "index-md5.txt", "keep checksum");
    let mut r = row("m2/file.pdf", "Invalid TOC", "new");
    r.insert("ctd_toc".into(), "not-in-dtd".into());
    write_rows(temp.path(), "0000", &[r]);
    let path = temp.path().join("metadata-0000.xlsx");
    let before = fs::read(&path).unwrap();
    assert!(run(temp.path(), "0000").is_err());
    assert_eq!(
        fs::read_to_string(seq.join("index.xml")).unwrap(),
        "keep existing index"
    );
    assert_eq!(
        fs::read_to_string(seq.join("index-md5.txt")).unwrap(),
        "keep checksum"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn cli_padded_sequence_custom_metadata_and_overwrite_policy() {
    let temp = TempDir::new().unwrap();
    let seq = setup(temp.path(), "0001");
    put(&seq, "m2/23-qos/summary.pdf", "hello");
    let bin = env!("CARGO_BIN_EXE_ectd-tool");
    let runcli = |args: &[&str]| {
        Command::new(bin)
            .arg(temp.path())
            .args(args)
            .output()
            .unwrap()
    };
    assert!(
        runcli(&["1", "-scan", "-metadata", "custom.xlsx"])
            .status
            .success()
    );
    assert!(temp.path().join("custom.xlsx").is_file());
    assert!(!seq.join("index.xml").exists());
    let result = runcli(&["1", "-metadata", "custom.xlsx"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = runcli(&["1", "-metadata", "custom.xlsx"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("--yes"));
    assert!(
        runcli(&["1", "-metadata", "custom.xlsx", "--yes"])
            .status
            .success()
    );
    assert!(
        runcli(&["1", "-extractXML", "-metadata", "extracted.xlsx"])
            .status
            .success()
    );
    let rows = MetadataBook::read(&temp.path().join("extracted.xlsx"))
        .unwrap()
        .rows()
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["title"], "summary");
    assert!(!runcli(&["nonnumeric", "-scan"]).status.success());
    let help = Command::new(bin).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("-extractXML"));
}

#[test]
fn missing_dtd_is_an_error_and_custom_mapping_is_used() {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join("0000")).unwrap();
    let r = row("m2/unusual.pdf", "Custom mapping", "new");
    assert!(
        generate::generate(
            temp.path(),
            &temp.path().join("0000"),
            0,
            "0000",
            std::slice::from_ref(&r),
            None,
            None
        )
        .is_err()
    );
    let seq = setup(temp.path(), "0000");
    put(&seq, "m2/unusual.pdf", "custom");
    put(
        temp.path(),
        "custom.csv",
        "item_type,relative_path,xml_element\nfile,m2/unusual.pdf,m2-2-introduction\n",
    );
    let output = generate::generate(
        temp.path(),
        &seq,
        0,
        "0000",
        &[r],
        Some(&temp.path().join("custom.csv")),
        None,
    )
    .unwrap();
    let data = &output
        .iter()
        .find(|(p, _)| p.ends_with("index.xml"))
        .unwrap()
        .1;
    let xml = Xml::parse(data).unwrap();
    assert!(xml.descendant(0, "m2-2-introduction").is_some());
}

#[test]
fn supplied_workbook_and_active_sheet_fallback_are_supported() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/metadata-0001.xlsx");
    let rows = MetadataBook::read(&source).unwrap().rows().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["file_path"], "m1/eu/10-cover/at/at-cover.pdf");
    assert_eq!(rows[0]["applicant_name"], "Test Pharma");
    assert!(rows[0]["title"].contains('–'));
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("active.xlsx");
    let mut book = umya_spreadsheet::new_file();
    book.active_sheet_mut()
        .cell_mut("A1")
        .set_value_string("Previous contents");
    umya_spreadsheet::writer::xlsx::write(&book, &path).unwrap();
    workbook::replace(&path, &rows, &mut OutputPolicy::new(true)).unwrap();
    let book = MetadataBook::read(&path).unwrap();
    assert!(book.book.sheet_by_name("metadata").is_ok());
    assert_eq!(book.rows().unwrap(), rows);
}

#[test]
fn dtd_constraints_and_local_parameter_entities_are_enforced() {
    let temp = TempDir::new().unwrap();
    put(
        temp.path(),
        "root.dtd",
        "<!ENTITY % include SYSTEM 'parts.mod'>%include;<!ELEMENT root (item+)><!ATTLIST root version CDATA #FIXED '1'><!ELEMENT item EMPTY><!ATTLIST item id ID #REQUIRED kind (a|b) #REQUIRED>",
    );
    put(temp.path(), "parts.mod", "<!ENTITY % unused 'value'>");
    let schema = Schema::load(&temp.path().join("root.dtd"), "root").unwrap();
    assert!(
        schema
            .validate("<root version='1'><item id='N1' kind='a'/></root>", "valid")
            .is_ok()
    );
    for invalid in [
        "<root version='1'/>",
        "<root version='2'><item id='N1' kind='a'/></root>",
        "<root><item kind='a'/></root>",
        "<root><item id='N1' kind='c'/></root>",
        "<root><item id='N1' kind='a'/><item id='N1' kind='b'/></root>",
        "<root><item id='N1' kind='a' unknown='x'/></root>",
        "<root><item id='N1' kind='a'>text</item></root>",
    ] {
        assert!(schema.validate(invalid, "invalid").is_err(), "{invalid}");
    }
    put(
        temp.path(),
        "cycle.dtd",
        "<!ENTITY % loop SYSTEM 'cycle.dtd'>%loop;",
    );
    assert!(Schema::load(&temp.path().join("cycle.dtd"), "root").is_err());
    put(
        temp.path(),
        "remote.dtd",
        "<!ENTITY % remote SYSTEM 'https://example.org/schema.dtd'>%remote;",
    );
    assert!(Schema::load(&temp.path().join("remote.dtd"), "root").is_err());
}

#[test]
fn eu_toc_override_missing_eu_dtd_and_directory_attributes() {
    let temp = TempDir::new().unwrap();
    let seq = setup(temp.path(), "0000");
    put(&seq, "m1/eu/custom/at/form.pdf", "form");
    let mut r = row("m1/eu/custom/at/form.pdf", "Form override", "new");
    r.insert("ctd_toc".into(), "m1-2-form".into());
    let output = generate::generate(temp.path(), &seq, 0, "0000", &[r], None, None).unwrap();
    let xml = Xml::parse(
        &output
            .iter()
            .find(|(p, _)| p.ends_with("eu-regional.xml"))
            .unwrap()
            .1,
    )
    .unwrap();
    assert!(xml.descendant(0, "m1-2-form").is_some());
    let id = xml.descendant(0, "specific").unwrap();
    assert_eq!(xml.nodes[id].attrs["country"], "at");
    fs::remove_file(seq.join("util/dtd/eu-regional.dtd")).unwrap();
    put(&seq, "m1/eu/10-cover/at/cover.pdf", "cover");
    let output = generate::generate(
        temp.path(),
        &seq,
        0,
        "0000",
        &[row("m1/eu/10-cover/at/cover.pdf", "Cover", "new")],
        None,
        None,
    )
    .unwrap();
    assert_eq!(output.len(), 3);
    let seq1 = setup(temp.path(), "0001");
    let prefix = "m3/32-body-data/32s-drug-sub/substance-1-manufacturer-1";
    put(
        &seq1,
        &format!("{prefix}/32s1-gen-info/report.pdf"),
        "report",
    );
    let dir = Row::from([
        ("file_path".into(), format!("{prefix}/")),
        (
            "attributes".into(),
            "substance=\"A\" manufacturer=\"B\"".into(),
        ),
    ]);
    let output = generate::generate(
        temp.path(),
        &seq1,
        1,
        "0001",
        &[
            dir,
            row(
                &format!("{prefix}/32s1-gen-info/report.pdf"),
                "Report",
                "new",
            ),
        ],
        None,
        None,
    )
    .unwrap();
    let xml = Xml::parse(
        &output
            .iter()
            .find(|(p, _)| p.ends_with("index.xml"))
            .unwrap()
            .1,
    )
    .unwrap();
    let id = xml.descendant(0, "leaf").unwrap();
    assert!(xml.ancestors(id).iter().any(|&p| {
        xml.nodes[p]
            .attrs
            .get("substance")
            .is_some_and(|s| s == "A")
    }));
}
