
"""
eCTD Viewer (Extended)
=====================

Desktop viewer for eCTD Dossiers (v3.x Backbones) with cross-platform GUI (PySide6/Qt).

Features
--------
- Tree starts with dossier directory name, containing all sequences (0000, 0001, ...).
- Per sequence: optionally
  * Filesystem tree (resolved xlink:href paths) or
  * CTD-TOC tree (XML ancestor path / "TOC" from backbone structure)
- Checkbox "Consolidated": shows per sequence the consolidated state (active documents) instead of just delta.
- Lifecycle operations: new/replace/append/delete
  * delete is shown grayed out in delta view and opens the original (predecessor) on double-click.
  * replace/append/delete: jump to predecessor version via modified-file.
- Metadata:
  * Detail view shows computed fields (path, existence, status, etc.)
  * AND "all metadata" as flattened Key/Value list from complete <leaf> subtree
  * AND Raw leaf XML
- Auto-discovery of regional backbones under m1/** (not just EU).
- Performance:
  * XML parsing via iterparse (streaming)
  * GUI builds root + sequences immediately; contents loaded per sequence on expand (lazy).
- QA:
  * missing files marked in red
  * broken modified-file links marked in yellow
- Search: recursive filter over the tree.

Installation
------------
pip install PySide6

Start
-----
python ectd_viewer.py
"""

from __future__ import annotations

import re
import html
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple
from urllib.parse import unquote

import xml.etree.ElementTree as ET

from PySide6.QtCore import Qt, QUrl, QModelIndex, QSortFilterProxyModel
from PySide6.QtGui import QAction, QDesktopServices, QStandardItem, QStandardItemModel
from PySide6.QtWidgets import (
    QApplication, QMainWindow, QFileDialog, QTreeView, QWidget, QVBoxLayout,
    QHBoxLayout, QLabel, QPushButton, QCheckBox, QMessageBox,
    QSplitter, QLineEdit, QTabWidget, QTableWidget, QTableWidgetItem, QPlainTextEdit,
    QTextEdit, QMenu, QAbstractItemView, QHeaderView
)

XLINK_NS = "http://www.w3.org/1999/xlink"
XLINK_HREF = f"{{{XLINK_NS}}}href"

ROLE_NODEINFO = Qt.UserRole + 1   # dict: kind, seq, populated, etc.
ROLE_LEAF = Qt.UserRole + 2       # LeafRecord
ROLE_BACKBONE = Qt.UserRole + 3   # BackboneFile


def localname(tag: str) -> str:
    return tag.split("}", 1)[-1] if "}" in tag else tag


def defrag(s: str) -> Tuple[str, Optional[str]]:
    if "#" in s:
        p, f = s.split("#", 1)
        return p, f
    return s, None


def is_seq_dir_name(name: str) -> bool:
    return bool(re.fullmatch(r"\d{4}", name))


def safe_relpath(path: Optional[Path], base: Path) -> str:
    if not path:
        return ""
    try:
        return str(path.relative_to(base))
    except Exception:
        return str(path)


def to_prefixed_name(name: str, uri_to_prefix: Dict[str, str]) -> str:
    """
    Convert ElementTree expanded name: "{uri}local" -> "prefix:local" if possible.
    If no prefix is known, returns "local".
    """
    if name.startswith("{") and "}" in name:
        uri, local = name[1:].split("}", 1)
        pref = uri_to_prefix.get(uri, "")
        return f"{pref}:{local}" if pref else local
    return name


def element_outer_xml(elem: ET.Element) -> str:
    try:
        return ET.tostring(elem, encoding="unicode", method="xml")
    except Exception:
        return "<leaf/>"


def flatten_element(elem: ET.Element, uri_to_prefix: Dict[str, str], prefix: str = "") -> Dict[str, str]:
    """
    Flatten element subtree into key->value strings (for metadata table).
    Keys include attributes and text nodes.
    """
    out: Dict[str, str] = {}
    tag = to_prefixed_name(elem.tag, uri_to_prefix)
    key_base = f"{prefix}/{tag}" if prefix else tag

    for k, v in (elem.attrib or {}).items():
        kk = to_prefixed_name(k, uri_to_prefix)
        out[f"{key_base}@{kk}"] = v

    txt = (elem.text or "").strip()
    if txt:
        out[f"{key_base}#text"] = txt

    for i, ch in enumerate(list(elem)):
        child_prefix = f"{key_base}[{i}]"
        out.update(flatten_element(ch, uri_to_prefix, prefix=child_prefix))

    return out


def derive_toc_label(elem: ET.Element) -> str:
    """
    Human-ish label for TOC tree from an XML element.
    """
    tag = localname(elem.tag)

    # Prefer recognizable CTD module tags
    if tag.lower().startswith("m") and re.fullmatch(r"m\d+", tag.lower()):
        return tag.lower()

    title = elem.attrib.get("title") or elem.attrib.get("Title")
    name = elem.attrib.get("name") or elem.attrib.get("Name")
    number = elem.attrib.get("number") or elem.attrib.get("Number") or elem.attrib.get("num")

    if title:
        return f"{tag}: {title}"
    if name:
        return f"{tag}: {name}"
    if number:
        return f"{tag} {number}"

    return tag


def derive_toc_label_m1(elem: ET.Element) -> str:
    tag = localname(elem.tag)
    if tag == "specific":
        country = elem.attrib.get("country") or elem.attrib.get("Country")
        if country:
            return country
        if elem.attrib:
            return next(iter(elem.attrib.values()))
    return derive_toc_label(elem)


def derive_toc_labels_m1(elem: ET.Element) -> List[str]:
    tag = localname(elem.tag)
    if tag == "pi-doc":
        lang = (
            elem.attrib.get("{http://www.w3.org/XML/1998/namespace}lang")
            or elem.attrib.get("xml:lang")
            or elem.attrib.get("lang")
        )
        pi_type = elem.attrib.get("type")
        country = elem.attrib.get("country")
        labels = []
        if lang:
            labels.append(lang)
        if pi_type:
            labels.append(pi_type)
        if country:
            labels.append(country)
        if labels:
            return labels
    return [derive_toc_label_m1(elem)]


def derive_toc_labels(elem: ET.Element, module_key: Optional[str]) -> List[str]:
    if module_key == "m1":
        return derive_toc_labels_m1(elem)
    if elem.attrib:
        tag = localname(elem.tag)
        parts = []
        for key, value in sorted(elem.attrib.items(), key=lambda kv: localname(kv[0])):
            attr_name = localname(key)
            parts.append(f"{attr_name}={value}")
        attr_text = ", ".join(parts)
        return [f"{tag} ({attr_text})"]
    return [derive_toc_label(elem)]


@dataclass(eq=False)
class BackboneFile:
    sequence: str
    xml_path: Path
    kind: str  # "index" | "regional" | "other"
    region: Optional[str] = None

    ns_prefix_to_uri: Dict[str, str] = field(default_factory=dict)
    uri_to_prefix: Dict[str, str] = field(default_factory=dict)

    root_tag: str = ""
    parse_error: Optional[str] = None
    leaf_count: int = 0

    def rel_to_seq(self, dossier_root: Path) -> str:
        return safe_relpath(self.xml_path, dossier_root / self.sequence)


@dataclass(eq=False)
class LeafRecord:
    sequence: str
    backbone: BackboneFile
    leaf_id: Optional[str]
    operation: str
    title: str
    href: Optional[str]
    abs_file_path: Optional[Path]
    modified_file: Optional[str]

    prev_ref: Optional[Tuple[Path, str]] = None
    prev_leaf: Optional["LeafRecord"] = None

    toc_path: List[str] = field(default_factory=list)

    leaf_xml: str = ""
    leaf_meta_flat: Dict[str, str] = field(default_factory=dict)
    leaf_attrib_prefixed: Dict[str, str] = field(default_factory=dict)

    missing_file: bool = False
    warnings: List[str] = field(default_factory=list)

    def op_lower(self) -> str:
        return (self.operation or "new").strip().lower()

    def resolved_open_path(self) -> Optional[Path]:
        if self.op_lower() == "delete":
            if self.prev_leaf and self.prev_leaf.abs_file_path:
                return self.prev_leaf.abs_file_path
        return self.abs_file_path

    def display_name_filesystem(self) -> str:
        p = self.resolved_open_path()
        if p:
            return p.name
        return self.title or "<leaf>"

    def display_name_toc(self) -> str:
        return self.title or self.display_name_filesystem()


class EctdDossier:
    def __init__(self, root: Path):
        self.root = root
        self.sequences: List[str] = []
        self.backbones_by_seq: Dict[str, List[BackboneFile]] = {}
        self.leaves_by_seq: Dict[str, List[LeafRecord]] = {}
        self.leaf_index: Dict[Tuple[Path, str], LeafRecord] = {}
        self.consolidated_cache: Dict[str, List[LeafRecord]] = {}

    def scan_sequences(self) -> List[str]:
        seqs = []
        for child in self.root.iterdir():
            if child.is_dir() and is_seq_dir_name(child.name):
                seqs.append(child.name)
        seqs.sort()
        self.sequences = seqs
        return seqs

    def discover_backbones(self, seq: str) -> List[BackboneFile]:
        seq_dir = self.root / seq
        out: List[BackboneFile] = []

        idx = seq_dir / "index.xml"
        if idx.exists():
            out.append(BackboneFile(sequence=seq, xml_path=idx.resolve(), kind="index"))

        m1_dir = seq_dir / "m1"
        if m1_dir.exists():
            for p in m1_dir.rglob("*.xml"):
                name = p.name.lower()
                if "regional" in name:
                    region = None
                    try:
                        rel = p.relative_to(m1_dir)
                        if rel.parts:
                            region = rel.parts[0]
                    except Exception:
                        pass
                    out.append(BackboneFile(sequence=seq, xml_path=p.resolve(), kind="regional", region=region))

        # Deduplicate
        uniq: Dict[Path, BackboneFile] = {}
        for b in out:
            uniq[b.xml_path] = b
        return list(uniq.values())

    def _parse_backbone(self, backbone: BackboneFile) -> List[LeafRecord]:
        leaves: List[LeafRecord] = []
        xml_path = backbone.xml_path

        prefix_to_uri: Dict[str, str] = {}
        uri_to_prefix: Dict[str, str] = {}

        # iterparse streaming
        try:
            it = ET.iterparse(str(xml_path), events=("start-ns", "start", "end"))
        except Exception as e:
            backbone.parse_error = f"iterparse failed: {e}"
            return leaves

        stack: List[ET.Element] = []
        leaf_title_map: Dict[int, str] = {}
        root_seen = False

        try:
            for event, obj in it:
                if event == "start-ns":
                    pref, uri = obj
                    pref = pref or ""
                    prefix_to_uri.setdefault(pref, uri)

                elif event == "start":
                    elem: ET.Element = obj  # type: ignore
                    if not root_seen:
                        backbone.root_tag = localname(elem.tag)
                        root_seen = True
                    stack.append(elem)

                elif event == "end":
                    elem: ET.Element = obj  # type: ignore

                    if localname(elem.tag) == "title" and len(stack) >= 2:
                        parent = stack[-2]
                        if localname(parent.tag) == "leaf":
                            text = (elem.text or "").strip()
                            if text:
                                leaf_title_map[id(parent)] = text

                    if localname(elem.tag) == "leaf":
                        # build uri->prefix map each time (cheap) to make attribute keys readable
                        uri_to_prefix = {}
                        for pref, uri in prefix_to_uri.items():
                            if uri not in uri_to_prefix or (pref and not uri_to_prefix[uri]):
                                uri_to_prefix[uri] = pref
                        if XLINK_NS not in uri_to_prefix:
                            uri_to_prefix[XLINK_NS] = "xlink"

                        leaf_id = elem.attrib.get("ID") or elem.attrib.get("id") or elem.attrib.get("Id") or None
                        operation = (elem.attrib.get("operation") or "new").strip()
                        title = (elem.attrib.get("title") or "").strip()
                        if not title:
                            title = leaf_title_map.pop(id(elem), "")
                        if title:
                            title_elem = None
                            for child in list(elem):
                                if localname(child.tag) == "title":
                                    title_elem = child
                                    break
                            if title_elem is None:
                                ns = ""
                                if elem.tag.startswith("{") and "}" in elem.tag:
                                    ns = elem.tag[1:].split("}", 1)[0]
                                title_tag = f"{{{ns}}}title" if ns else "title"
                                title_elem = ET.SubElement(elem, title_tag)
                            title_elem.text = title

                        href = elem.attrib.get(XLINK_HREF)
                        if not href:
                            for k, v in elem.attrib.items():
                                if k.endswith("}href") or k == "href":
                                    href = v
                                    break

                        modified = elem.attrib.get("modified-file") or elem.attrib.get("modified_file")

                        abs_file = None
                        if href:
                            href_path, _ = defrag(href)
                            href_path = unquote(href_path).replace("\\", "/")
                            abs_file = (xml_path.parent / Path(href_path)).resolve()

                        toc_labels: List[str] = []
                        module_key = None
                        if len(stack) >= 2:
                            for anc in stack[1:-1]:
                                if module_key is None:
                                    anc_name = localname(anc.tag).lower()
                                    m = re.match(r"m[1-5]", anc_name)
                                    if m:
                                        module_key = m.group(0)
                                toc_labels.extend(derive_toc_labels(anc, module_key))

                        leaf_xml = element_outer_xml(elem)
                        attrib_prefixed = {to_prefixed_name(k, uri_to_prefix): v for k, v in (elem.attrib or {}).items()}
                        meta_flat = flatten_element(elem, uri_to_prefix)

                        lr = LeafRecord(
                            sequence=backbone.sequence,
                            backbone=backbone,
                            leaf_id=leaf_id,
                            operation=operation,
                            title=title,
                            href=href,
                            abs_file_path=abs_file,
                            modified_file=modified,
                            toc_path=toc_labels,
                            leaf_xml=leaf_xml,
                            leaf_meta_flat=meta_flat,
                            leaf_attrib_prefixed=attrib_prefixed,
                        )
                        leaves.append(lr)
                        backbone.leaf_count += 1

                    # pop and clear to keep memory low
                    if stack and stack[-1] is elem:
                        stack.pop()
                    elem.clear()

        except ET.ParseError as e:
            backbone.parse_error = f"XML parse error: {e}"
        except Exception as e:
            backbone.parse_error = f"Unexpected parse error: {e}"

        backbone.ns_prefix_to_uri = prefix_to_uri
        backbone.uri_to_prefix = {uri: pref for pref, uri in prefix_to_uri.items()}
        if XLINK_NS not in backbone.uri_to_prefix:
            backbone.uri_to_prefix[XLINK_NS] = "xlink"

        return leaves

    def load(self) -> None:
        self.scan_sequences()
        self.backbones_by_seq.clear()
        self.leaves_by_seq.clear()
        self.leaf_index.clear()
        self.consolidated_cache.clear()

        all_leaves: List[LeafRecord] = []

        for seq in self.sequences:
            bbs = self.discover_backbones(seq)
            self.backbones_by_seq[seq] = bbs
            for bb in bbs:
                leaves = self._parse_backbone(bb)
                all_leaves.extend(leaves)
                self.leaves_by_seq.setdefault(seq, []).extend(leaves)

        # index
        for lr in all_leaves:
            if lr.leaf_id:
                self.leaf_index[(lr.backbone.xml_path, lr.leaf_id)] = lr

        # resolve modified-file
        for lr in all_leaves:
            if not lr.modified_file:
                continue
            p, frag = defrag(lr.modified_file.strip())
            if not frag:
                lr.warnings.append("modified-file without fragment (#leafId) – predecessor not resolvable.")
                continue
            p = unquote(p).replace("\\", "/")
            target_xml = (lr.backbone.xml_path.parent / Path(p)).resolve()
            lr.prev_ref = (target_xml, frag)

        for lr in all_leaves:
            if lr.prev_ref:
                lr.prev_leaf = self.leaf_index.get(lr.prev_ref)
                if lr.prev_leaf is None:
                    lr.warnings.append("modified-file points to nothing (leaf not found).")

        # missing file checks
        for lr in all_leaves:
            p = lr.resolved_open_path()
            if not p:
                lr.missing_file = True
                if lr.op_lower() != "delete":
                    lr.warnings.append("No xlink:href / no file resolvable.")
            else:
                lr.missing_file = not p.exists()
                if lr.missing_file:
                    lr.warnings.append("File does not exist at resolved path.")

        # consolidated cache incremental
        active_set = set()  # set[LeafRecord], LeafRecord is hashable by identity (eq=False)
        for seq in self.sequences:
            for lr in self.leaves_by_seq.get(seq, []):
                op = lr.op_lower()
                if op in ("replace", "delete"):
                    if lr.prev_leaf:
                        active_set.discard(lr.prev_leaf)
                if op != "delete":
                    active_set.add(lr)
            self.consolidated_cache[seq] = list(active_set)

    def get_seq_delta(self, seq: str) -> List[LeafRecord]:
        return self.leaves_by_seq.get(seq, [])

    def get_consolidated(self, seq: str) -> List[LeafRecord]:
        return self.consolidated_cache.get(seq, [])

    def backbones_for_seq(self, seq: str) -> List[BackboneFile]:
        return self.backbones_by_seq.get(seq, [])

    def backbone_summary_for_seq(self, seq: str) -> str:
        bbs = self.backbones_by_seq.get(seq, [])
        if not bbs:
            return "No backbone XMLs found."
        lines = []
        for b in bbs:
            kind = b.kind
            if b.kind == "regional" and b.region:
                kind = f"regional ({b.region})"
            rel = safe_relpath(b.xml_path, self.root / seq)
            status = "OK" if not b.parse_error else f"FEHLER: {b.parse_error}"
            lines.append(f"- {kind}: {rel}  (leafs={b.leaf_count})  [{status}]")
        return "\n".join(lines)


class MainWindow(QMainWindow):
    COL_NAME = 0
    COL_KIND = 1
    COL_OP = 2
    COL_SEQ = 3
    COL_STATUS = 4
    COL_BACKBONE = 5
    COL_LEAFID = 6
    COL_TITLE = 7
    COL_HREF = 8
    COL_MODIFIED = 9
    COL_FILEPATH = 10
    COL_CTD_TOC = 11

    def __init__(self):
        super().__init__()
        self.setWindowTitle("eCTD Viewer (Extended)")

        self.dossier: Optional[EctdDossier] = None
        self.dossier_root: Optional[Path] = None

        # Controls
        self.open_btn = QPushButton("Open Dossier…")
        self.open_btn.clicked.connect(self.open_dossier)

        self.collapse_btn = QPushButton("Collapse to Sequences")
        self.collapse_btn.clicked.connect(self.collapse_to_sequences)

        self.expand_seq_btn = QPushButton("Expand Sequence")
        self.expand_seq_btn.clicked.connect(self.expand_selected_sequence)

        self.toc_cb = QCheckBox("CTD-TOC Tree (instead of Filesystem)")
        self.toc_cb.stateChanged.connect(self.on_toc_toggle)

        self.consolidated_cb = QCheckBox("Consolidated (state per sequence)")
        self.consolidated_cb.stateChanged.connect(self.on_consolidated_changed)

        self.show_backbone_docs_cb = QCheckBox("Show backbone XMLs in Documents")
        self.show_backbone_docs_cb.stateChanged.connect(self.on_show_backbone_toggle)

        top = QWidget()
        tl = QHBoxLayout(top)
        tl.setContentsMargins(4, 4, 4, 4)
        tl.setSpacing(6)
        tl.addWidget(self.open_btn)
        tl.addWidget(self.expand_seq_btn)
        tl.addWidget(self.collapse_btn)
        tl.addWidget(self.toc_cb)
        tl.addWidget(self.consolidated_cb)
        tl.addWidget(self.show_backbone_docs_cb)
        tl.addStretch(1)
        top.setMaximumHeight(40)

        # Model + Proxy filter
        self.model = QStandardItemModel()
        self.model.setHorizontalHeaderLabels([
            "Name", "Type", "Op", "Seq", "Status", "Backbone", "Leaf ID",
            "Title", "Href", "modified-file", "File Path", "CTD-TOC"
        ])

        self.proxy = QSortFilterProxyModel(self)
        self.proxy.setSourceModel(self.model)
        self.proxy.setFilterCaseSensitivity(Qt.CaseInsensitive)
        self.proxy.setRecursiveFilteringEnabled(True)
        self.proxy.setFilterKeyColumn(-1)

        # Tree
        self.tree = QTreeView()
        self.tree.setModel(self.proxy)
        self.tree.setAlternatingRowColors(True)
        self.tree.setUniformRowHeights(True)
        self.tree.expanded.connect(self.on_expanded)
        self.tree.doubleClicked.connect(self.on_tree_double_click)
        self.tree.setContextMenuPolicy(Qt.CustomContextMenu)
        self.tree.customContextMenuRequested.connect(self.on_context_menu)
        self.tree.header().setSectionResizeMode(self.COL_NAME, QHeaderView.ResizeToContents)
        self.tree.setColumnHidden(self.COL_CTD_TOC, self.toc_cb.isChecked())
        self._set_title_column_label()

        # Detail panel
        self.detail_title = QLabel("—")
        self.detail_title.setTextInteractionFlags(Qt.TextSelectableByMouse)
        self.detail_title.setMaximumHeight(24)

        self.detail_overview = QTextEdit()
        self.detail_overview.setReadOnly(True)

        self.meta_table = QTableWidget()
        self.meta_table.setColumnCount(2)
        self.meta_table.setHorizontalHeaderLabels(["Key", "Value"])
        self.meta_table.horizontalHeader().setStretchLastSection(True)

        self.xml_view = QPlainTextEdit()
        self.xml_view.setReadOnly(True)

        self.versions_table = QTableWidget()
        self.versions_table.setColumnCount(5)
        self.versions_table.setHorizontalHeaderLabels(["Depth", "Seq", "Op", "Exists", "File"])
        self.versions_table.horizontalHeader().setStretchLastSection(True)
        self.versions_table.setEditTriggers(QAbstractItemView.NoEditTriggers)
        self.versions_table.cellDoubleClicked.connect(self.on_versions_double_click)

        self.btn_open = QPushButton("Open")
        self.btn_prev = QPushButton("Open Predecessor")
        self.btn_show = QPushButton("Show in File Manager")
        self.btn_copy = QPushButton("Copy Path")

        self.btn_open.clicked.connect(self.open_selected)
        self.btn_prev.clicked.connect(self.open_prev_selected)
        self.btn_show.clicked.connect(self.show_in_file_manager)
        self.btn_copy.clicked.connect(self.copy_selected_path)

        btn_row = QWidget()
        brl = QHBoxLayout(btn_row)
        brl.setContentsMargins(0, 0, 0, 0)
        brl.addWidget(self.btn_open)
        brl.addWidget(self.btn_prev)
        brl.addWidget(self.btn_show)
        brl.addWidget(self.btn_copy)
        brl.addStretch(1)

        tabs = QTabWidget()
        tab_over = QWidget()
        tab_over_l = QVBoxLayout(tab_over)
        tab_over_l.setContentsMargins(4, 4, 4, 4)
        tab_over_l.setSpacing(4)
        tab_over_l.addWidget(self.detail_title)
        tab_over_l.addWidget(self.detail_overview)
        tab_over_l.addWidget(btn_row)

        tab_meta = QWidget()
        tab_meta_l = QVBoxLayout(tab_meta)
        tab_meta_l.addWidget(self.meta_table)

        tab_xml = QWidget()
        tab_xml_l = QVBoxLayout(tab_xml)
        tab_xml_l.addWidget(self.xml_view)

        tab_ver = QWidget()
        tab_ver_l = QVBoxLayout(tab_ver)
        tab_ver_l.addWidget(self.versions_table)

        tabs.addTab(tab_over, "Overview")
        tabs.addTab(tab_meta, "Metadata")
        tabs.addTab(tab_xml, "XML")
        tabs.addTab(tab_ver, "Versions")

        splitter = QSplitter()
        splitter.addWidget(self.tree)
        splitter.addWidget(tabs)
        splitter.setStretchFactor(0, 4)
        splitter.setStretchFactor(1, 1)

        central = QWidget()
        cl = QVBoxLayout(central)
        cl.setContentsMargins(0, 0, 0, 0)
        cl.setSpacing(0)
        cl.addWidget(top)
        cl.addWidget(splitter)
        self.setCentralWidget(central)

        self.tree.selectionModel().selectionChanged.connect(self.update_details)

    # ---------------- Loading ----------------

    def open_dossier(self):
        folder = QFileDialog.getExistingDirectory(self, "Select eCTD Dossier Directory")
        if not folder:
            return
        root = Path(folder).resolve()
        if not root.exists():
            QMessageBox.critical(self, "Invalid Directory", "Directory does not exist.")
            return

        self.dossier_root = root
        self.dossier = EctdDossier(root)
        self.dossier.load()

        if not self.dossier.sequences:
            QMessageBox.warning(self, "No Sequences Found", "No directories like 0000, 0001, … found.")
            return

        self.rebuild_root_tree(expand_root=True)

    # ---------------- Tree build (lazy) ----------------

    def rebuild_root_tree(self, _=None, expand_root: bool = False):
        if not self.dossier or not self.dossier_root:
            self.model.removeRows(0, self.model.rowCount())
            return

        self.model.removeRows(0, self.model.rowCount())

        dossier_name = self.dossier_root.name
        root_row = self._make_row(
            name=dossier_name, kind="Dossier", op="", seq="", status="",
            backbone="", leaf_id="", title="", href="", modified="",
            filepath=str(self.dossier_root)
        )
        root_item = root_row[self.COL_NAME]
        root_item.setData({"kind": "dossier"}, ROLE_NODEINFO)
        self.model.appendRow(root_row)

        for seq in self.dossier.sequences:
            seq_row = self._make_row(
                name=seq, kind="Sequence", op="", seq=seq, status="",
                backbone="", leaf_id="", title="", href="", modified="",
                filepath=str(self.dossier_root / seq)
            )
            seq_item = seq_row[self.COL_NAME]
            seq_item.setData({"kind": "sequence", "seq": seq, "populated": False}, ROLE_NODEINFO)
            root_item.appendRow(seq_row)

            placeholder = self._make_row(
                name="⏳ (expand to load)", kind="", op="", seq="", status="",
                backbone="", leaf_id="", title="", href="", modified="", filepath=""
            )
            seq_item.appendRow(placeholder)

        if expand_root:
            src_root_idx = self.model.index(0, 0)
            proxy_root_idx = self.proxy.mapFromSource(src_root_idx)
            self.tree.expand(proxy_root_idx)

        self.tree.resizeColumnToContents(self.COL_NAME)

    def on_consolidated_changed(self, _=None):
        expanded_paths = self._expanded_name_paths()
        self.rebuild_root_tree(expand_root=True)
        self._restore_expanded_paths(expanded_paths)

    def on_show_backbone_toggle(self, _=None):
        expanded_paths = self._expanded_name_paths()
        self.rebuild_root_tree(expand_root=True)
        self._restore_expanded_paths(expanded_paths)

    def on_toc_toggle(self, _=None):
        self.tree.setColumnHidden(self.COL_CTD_TOC, self.toc_cb.isChecked())
        self._set_title_column_label()
        self.rebuild_root_tree(expand_root=True)

    def collapse_to_sequences(self):
        if not self.dossier or not self.dossier_root:
            return
        self.tree.collapseAll()
        src_root_idx = self.model.index(0, 0)
        if not src_root_idx.isValid():
            return
        proxy_root_idx = self.proxy.mapFromSource(src_root_idx)
        if proxy_root_idx.isValid():
            self.tree.expand(proxy_root_idx)

    def _expanded_name_paths(self) -> List[List[Tuple[str, str, str, str, str, str]]]:
        paths: List[List[Tuple[str, str, str, str, str, str]]] = []
        root = self.model.invisibleRootItem()
        for r in range(root.rowCount()):
            item = root.child(r, 0)
            if item:
                self._collect_expanded_paths(item, [], paths)
        return paths

    def _collect_expanded_paths(
        self,
        item: QStandardItem,
        prefix: List[Tuple[str, str, str, str, str, str]],
        out: List[List[Tuple[str, str, str, str, str, str]]],
    ):
        key = self._node_key(item)
        path = prefix + [key]
        proxy_idx = self.proxy.mapFromSource(item.index())
        if proxy_idx.isValid() and self.tree.isExpanded(proxy_idx):
            out.append(path)
        for r in range(item.rowCount()):
            ch = item.child(r, 0)
            if ch:
                self._collect_expanded_paths(ch, path, out)

    def _restore_expanded_paths(self, paths: List[List[Tuple[str, str, str, str, str, str]]]):
        if not paths:
            return
        for path in sorted(paths, key=len):
            cur = self.model.invisibleRootItem()
            for key in path:
                child = self._find_child_by_key(cur, key)
                if not child:
                    break
                info = child.data(ROLE_NODEINFO) or {}
                if info.get("kind") == "sequence" and not info.get("populated"):
                    seq = info.get("seq")
                    if seq:
                        self.populate_sequence(child, seq)
                proxy_idx = self.proxy.mapFromSource(child.index())
                if proxy_idx.isValid():
                    self.tree.expand(proxy_idx)
                cur = child

    def _find_child_by_key(
        self,
        parent: QStandardItem,
        key: Tuple[str, str, str, str, str, str],
    ) -> Optional[QStandardItem]:
        for r in range(parent.rowCount()):
            ch = parent.child(r, 0)
            if not ch:
                continue
            if self._node_key(ch) == key:
                return ch
        return None

    def _node_key(self, item: QStandardItem) -> Tuple[str, str, str, str, str, str]:
        info = item.data(ROLE_NODEINFO) or {}
        kind = info.get("kind", "")
        seq = info.get("seq", "")
        name = item.text() or ""
        leaf_id = ""
        href = ""
        filepath = ""
        lr = item.data(ROLE_LEAF)
        if isinstance(lr, LeafRecord):
            leaf_id = lr.leaf_id or ""
            href = lr.href or ""
            filepath = str(lr.resolved_open_path() or "")
        return (name, kind, seq, leaf_id, href, filepath)

    def expand_selected_sequence(self):
        if not self.dossier or not self.dossier_root:
            return
        idx = self.tree.currentIndex()
        if not idx.isValid():
            return
        src_idx = self.proxy.mapToSource(idx)
        if not src_idx.isValid():
            return
        name_src_idx = src_idx.sibling(src_idx.row(), self.COL_NAME)
        item = self.model.itemFromIndex(name_src_idx)
        if not item:
            return
        seq_item = self._find_sequence_item(item)
        if not seq_item:
            return
        info = seq_item.data(ROLE_NODEINFO) or {}
        if info.get("kind") == "sequence" and not info.get("populated"):
            seq = info.get("seq")
            if seq:
                self.populate_sequence(seq_item, seq)
        proxy_idx = self.proxy.mapFromSource(seq_item.index())
        if proxy_idx.isValid():
            self.tree.expandRecursively(proxy_idx)

    def _find_sequence_item(self, item: QStandardItem) -> Optional[QStandardItem]:
        cur = item
        while cur is not None:
            info = cur.data(ROLE_NODEINFO) or {}
            if info.get("kind") == "sequence":
                return cur
            cur = cur.parent()
        return None

    def _set_title_column_label(self):
        label = "Filename" if self.toc_cb.isChecked() else "Title"
        self.model.setHeaderData(self.COL_TITLE, Qt.Horizontal, label)

    def _leaf_filename(self, lr: LeafRecord) -> str:
        if lr.href:
            href_path, _ = defrag(lr.href)
            return Path(href_path).name
        p = lr.resolved_open_path()
        return p.name if p else ""

    def _make_row(
        self,
        name: str, kind: str, op: str, seq: str, status: str,
        backbone: str, leaf_id: str, title: str, href: str, modified: str, filepath: str, ctd_toc: str = ""
    ) -> List[QStandardItem]:
        items = [
            QStandardItem(name),
            QStandardItem(kind),
            QStandardItem(op),
            QStandardItem(seq),
            QStandardItem(status),
            QStandardItem(backbone),
            QStandardItem(leaf_id),
            QStandardItem(title),
            QStandardItem(href),
            QStandardItem(modified),
            QStandardItem(filepath),
            QStandardItem(ctd_toc),
        ]
        for it in items:
            it.setEditable(False)
        return items

    def on_expanded(self, proxy_index: QModelIndex):
        if not proxy_index.isValid():
            return
        src_index = self.proxy.mapToSource(proxy_index)
        item = self.model.itemFromIndex(src_index)
        if not item:
            return
        info = item.data(ROLE_NODEINFO) or {}
        if info.get("kind") != "sequence":
            return
        if info.get("populated"):
            return
        seq = info.get("seq")
        if not seq:
            return
        self.populate_sequence(item, seq)

    def populate_sequence(self, seq_item: QStandardItem, seq: str):
        if not self.dossier or not self.dossier_root:
            return

        seq_item.removeRows(0, seq_item.rowCount())

        # Backbones group
        bb_group_row = self._make_row(
            name="Backbones", kind="Group", op="", seq=seq, status="",
            backbone="", leaf_id="", title="", href="", modified="", filepath=""
        )
        bb_group_item = bb_group_row[self.COL_NAME]
        bb_group_item.setData({"kind": "bb_group"}, ROLE_NODEINFO)
        seq_item.appendRow(bb_group_row)

        for bb in self.dossier.backbones_for_seq(seq):
            kind = bb.kind
            if bb.kind == "regional" and bb.region:
                kind = f"regional ({bb.region})"
            status = "OK" if not bb.parse_error else "ERROR"
            row = self._make_row(
                name=bb.xml_path.name,
                kind="Backbone",
                op="",
                seq=seq,
                status=status,
                backbone=safe_relpath(bb.xml_path, self.dossier_root / seq),
                leaf_id="",
                title=f"{kind}, leafs={bb.leaf_count}",
                href="",
                modified=bb.parse_error or "",
                filepath=str(bb.xml_path),
            )
            row[self.COL_NAME].setData({"kind": "backbone"}, ROLE_NODEINFO)
            row[self.COL_NAME].setData(bb, ROLE_BACKBONE)
            bb_group_item.appendRow(row)

        # Documents group (main)
        doc_group_row = self._make_row(
            name="Documents", kind="Group", op="", seq=seq, status="",
            backbone="", leaf_id="", title="", href="", modified="", filepath=""
        )
        doc_group_item = doc_group_row[self.COL_NAME]
        doc_group_item.setData({"kind": "doc_group"}, ROLE_NODEINFO)
        seq_item.appendRow(doc_group_row)

        consolidated = self.consolidated_cb.isChecked()
        leaves = self.dossier.get_consolidated(seq) if consolidated else self.dossier.get_seq_delta(seq)
        leaves = self._filter_document_leaves(seq, leaves)

        if self.toc_cb.isChecked():
            self._build_toc_tree(parent=doc_group_item, seq=seq, leaves=list(leaves), consolidated=consolidated)
        else:
            self._build_filesystem_tree(parent=doc_group_item, seq=seq, leaves=list(leaves), consolidated=consolidated)

        seq_item.setData({"kind": "sequence", "seq": seq, "populated": True}, ROLE_NODEINFO)

        self._expand_sequence_defaults(seq_item)
        self.tree.resizeColumnToContents(self.COL_NAME)

    # ---------------- Tree building helpers ----------------

    def _expand_sequence_defaults(self, seq_item: QStandardItem):
        seq_src_idx = seq_item.index()
        seq_proxy_idx = self.proxy.mapFromSource(seq_src_idx)
        if not seq_proxy_idx.isValid():
            return
        self.tree.expand(seq_proxy_idx)

        for r in range(seq_item.rowCount()):
            child = seq_item.child(r, 0)
            if not child:
                continue
            info = child.data(ROLE_NODEINFO) or {}
            kind = info.get("kind")
            child_proxy_idx = self.proxy.mapFromSource(child.index())
            if child_proxy_idx.isValid():
                self.tree.expand(child_proxy_idx)
            if kind != "doc_group":
                continue
            if self.toc_cb.isChecked():
                for i in range(child.rowCount()):
                    sec = child.child(i, 0)
                    if not sec:
                        continue
                    sec_proxy_idx = self.proxy.mapFromSource(sec.index())
                    if sec_proxy_idx.isValid():
                        self.tree.expand(sec_proxy_idx)
                continue
            for i in range(child.rowCount()):
                folder = child.child(i, 0)
                if not folder:
                    continue
                folder_proxy_idx = self.proxy.mapFromSource(folder.index())
                if folder_proxy_idx.isValid():
                    self.tree.expand(folder_proxy_idx)
                for j in range(folder.rowCount()):
                    sub = folder.child(j, 0)
                    if not sub:
                        continue
                    sub_proxy_idx = self.proxy.mapFromSource(sub.index())
                    if sub_proxy_idx.isValid():
                        self.tree.expand(sub_proxy_idx)

    def _filter_document_leaves(self, seq: str, leaves: List[LeafRecord]) -> List[LeafRecord]:
        if self.show_backbone_docs_cb.isChecked():
            return leaves
        if not self.dossier or not self.dossier_root:
            return leaves
        seq_dir = (self.dossier_root / seq).resolve()
        backbone_paths: Set[Path] = set()
        for bbs in self.dossier.backbones_by_seq.values():
            for bb in bbs:
                backbone_paths.add(bb.xml_path.resolve())
        filtered: List[LeafRecord] = []
        for lr in leaves:
            target = lr.resolved_open_path()
            rel = None
            if target:
                target = target.resolve()
                if target in backbone_paths:
                    continue
                try:
                    rel = target.relative_to(seq_dir).as_posix()
                except Exception:
                    rel = None
            if rel is None and lr.href:
                href_path, _ = defrag(lr.href)
                rel = href_path.replace("\\", "/")
            if rel:
                if rel == "index-md5.txt":
                    continue
                if rel.startswith("util/"):
                    continue
            filtered.append(lr)
        return filtered

    def _leaf_status(self, lr: LeafRecord, consolidated: bool) -> str:
        op = lr.op_lower()
        if (not consolidated) and op == "delete":
            return "DELETED"
        if lr.missing_file:
            return "MISSING"
        if lr.modified_file and lr.prev_leaf is None and op in ("replace", "append", "delete"):
            return "BROKEN-LINK"
        return "OK"

    def _apply_leaf_styling(self, row: List[QStandardItem], lr: LeafRecord, consolidated: bool):
        op = lr.op_lower()
        if (not consolidated) and op == "delete":
            for it in row:
                it.setForeground(Qt.gray)
        if lr.missing_file:
            for it in row:
                it.setForeground(Qt.darkRed)
        if lr.modified_file and lr.prev_leaf is None and op in ("replace", "append", "delete"):
            for it in row:
                it.setForeground(Qt.darkYellow)
        if lr.warnings:
            tip = "\n".join(lr.warnings)
            for it in row:
                it.setToolTip(tip)

    def _filesystem_parts(self, lr: LeafRecord, seq: str, consolidated: bool) -> List[str]:
        if not self.dossier_root:
            return [lr.display_name_filesystem()]
        target = lr.resolved_open_path()
        if not target:
            return ["<no file>"]
        try:
            rel = target.relative_to(self.dossier_root)
            parts = list(rel.parts)
        except Exception:
            return [target.name]

        if consolidated:
            if parts and is_seq_dir_name(parts[0]):
                parts = parts[1:]
        else:
            seq_dir = (self.dossier_root / seq).resolve()
            try:
                rel2 = target.relative_to(seq_dir)
                parts = list(rel2.parts)
            except Exception:
                pass
        return parts if parts else [target.name]

    def _filesystem_parts_for_path(self, p: Path, seq: str, consolidated: bool) -> List[str]:
        if not self.dossier_root:
            return [p.name]
        target = p.resolve()
        if consolidated:
            try:
                rel = target.relative_to(self.dossier_root)
                parts = list(rel.parts)
            except Exception:
                parts = [target.name]
            if parts and is_seq_dir_name(parts[0]):
                parts = parts[1:]
        else:
            seq_dir = (self.dossier_root / seq).resolve()
            try:
                rel2 = target.relative_to(seq_dir)
                parts = list(rel2.parts)
            except Exception:
                parts = [target.name]
        return parts if parts else [target.name]

    def _build_filesystem_tree(self, parent: QStandardItem, seq: str, leaves: List[LeafRecord], consolidated: bool):
        def sort_key(parts: List[str], filepath: str):
            return tuple(parts + [filepath])

        node_map: Dict[Tuple[str, ...], QStandardItem] = {}

        def ensure_folder(path_parts: List[str]) -> QStandardItem:
            cur: List[str] = []
            parent_item = parent
            for p in path_parts:
                cur.append(p)
                key = tuple(cur)
                if key in node_map:
                    parent_item = node_map[key]
                    continue
                row = self._make_row(
                    name=p, kind="Folder", op="", seq=seq, status="",
                    backbone="", leaf_id="", title="", href="", modified="", filepath=""
                )
                folder_item = row[self.COL_NAME]
                folder_item.setData({"kind": "folder"}, ROLE_NODEINFO)
                parent_item.appendRow(row)
                node_map[key] = folder_item
                parent_item = folder_item
            return parent_item

        entries: List[Tuple[List[str], List[QStandardItem], Optional[LeafRecord], bool]] = []
        for lr in leaves:
            parts = self._filesystem_parts(lr, seq, consolidated)
            bb_rel = safe_relpath(lr.backbone.xml_path, self.dossier_root / lr.sequence)
            row = self._make_row(
                name=parts[-1],
                kind="File",
                op=lr.operation,
                seq=lr.sequence,
                status=self._leaf_status(lr, consolidated),
                backbone=bb_rel,
                leaf_id=lr.leaf_id or "",
                title=lr.title or "",
                href=lr.href or "",
                modified=lr.modified_file or "",
                filepath=safe_relpath(lr.resolved_open_path(), self.dossier_root),
                ctd_toc=lr.toc_path[-1] if lr.toc_path else "",
            )
            entries.append((parts, row, lr, False))

        if self.dossier_root:
            seq_dir = (self.dossier_root / seq).resolve()
            referenced: Set[Path] = set()
            for lr in leaves:
                p = lr.resolved_open_path()
                if p and p.exists():
                    referenced.add(p.resolve())
            backbone_paths = {bb.xml_path for bb in self.dossier.backbones_for_seq(seq)} if self.dossier else set()
            for p in seq_dir.rglob("*"):
                if not p.is_file():
                    continue
                rp = p.resolve()
                rel = None
                try:
                    rel = rp.relative_to(seq_dir).as_posix()
                except Exception:
                    rel = None
                if not self.show_backbone_docs_cb.isChecked():
                    if rel == "index-md5.txt":
                        continue
                    if rel and rel.startswith("util/"):
                        continue
                    if rp in backbone_paths:
                        continue
                if rp in referenced:
                    continue
                parts = self._filesystem_parts_for_path(rp, seq, consolidated)
                row = self._make_row(
                    name=parts[-1],
                    kind="File",
                    op="",
                    seq=seq,
                    status="ORPHAN",
                    backbone="",
                    leaf_id="",
                    title="(not referenced in XML)",
                    href="",
                    modified="",
                    filepath=safe_relpath(rp, self.dossier_root),
                )
                entries.append((parts, row, None, True))

        entries_sorted = sorted(entries, key=lambda e: sort_key(e[0], e[1][self.COL_FILEPATH].text()))

        for parts, row, lr, is_orphan in entries_sorted:
            folder_parts = parts[:-1]
            folder_item = ensure_folder(folder_parts) if folder_parts else parent
            if lr is not None:
                row[self.COL_NAME].setData({"kind": "leaf"}, ROLE_NODEINFO)
                row[self.COL_NAME].setData(lr, ROLE_LEAF)
                self._apply_leaf_styling(row, lr, consolidated)
            else:
                row[self.COL_NAME].setData({"kind": "orphan_file"}, ROLE_NODEINFO)
                if is_orphan:
                    for it in row:
                        it.setForeground(Qt.blue)
            folder_item.appendRow(row)

    def _build_toc_tree(self, parent: QStandardItem, seq: str, leaves: List[LeafRecord], consolidated: bool):
        def sort_key(lr: LeafRecord):
            return tuple(lr.toc_path + [lr.display_name_toc()])
        leaves_sorted = sorted(leaves, key=sort_key)

        node_map: Dict[Tuple[str, ...], QStandardItem] = {}
        leaf_item_map: Dict[LeafRecord, QStandardItem] = {}

        def ensure_section(path_parts: List[str]) -> QStandardItem:
            cur: List[str] = []
            parent_item = parent
            for p in path_parts:
                cur.append(p)
                key = tuple(cur)
                if key in node_map:
                    parent_item = node_map[key]
                    continue
                row = self._make_row(
                    name=p, kind="TOC", op="", seq=seq, status="",
                    backbone="", leaf_id="", title="", href="", modified="", filepath=""
                )
                sec_item = row[self.COL_NAME]
                sec_item.setData({"kind": "toc_section"}, ROLE_NODEINFO)
                parent_item.appendRow(row)
                node_map[key] = sec_item
                parent_item = sec_item
            return parent_item

        for lr in leaves_sorted:
            section_item = ensure_section(lr.toc_path) if lr.toc_path else parent
            bb_rel = safe_relpath(lr.backbone.xml_path, self.dossier_root / lr.sequence)

            row = self._make_row(
                name=lr.display_name_toc(),
                kind="Document",
                op=lr.operation,
                seq=lr.sequence,
                status=self._leaf_status(lr, consolidated),
                backbone=bb_rel,
                leaf_id=lr.leaf_id or "",
                title=self._leaf_filename(lr),
                href=lr.href or "",
                modified=lr.modified_file or "",
                filepath=safe_relpath(lr.resolved_open_path(), self.dossier_root),
            )
            row[self.COL_NAME].setData({"kind": "leaf"}, ROLE_NODEINFO)
            row[self.COL_NAME].setData(lr, ROLE_LEAF)
            if not lr.title:
                name_item = row[self.COL_NAME]
                font = name_item.font()
                font.setItalic(True)
                name_item.setFont(font)
            self._apply_leaf_styling(row, lr, consolidated)
            section_item.appendRow(row)
            leaf_item_map[lr] = row[self.COL_NAME]

        # Append grouping (TOC + consolidated): move append under its base leaf
        if consolidated:
            for lr, item in list(leaf_item_map.items()):
                if lr.op_lower() != "append":
                    continue
                if not lr.prev_leaf:
                    continue
                if lr.prev_leaf not in leaf_item_map:
                    continue
                base_item = leaf_item_map[lr.prev_leaf]
                old_parent = item.parent()
                if old_parent is None:
                    continue
                row_idx = item.row()
                row_items = [old_parent.child(row_idx, c) for c in range(self.model.columnCount())]
                old_parent.removeRow(row_idx)

                # container under base
                append_container = None
                for r in range(base_item.rowCount()):
                    ch = base_item.child(r, 0)
                    if ch and ch.text() == "Append":
                        append_container = ch
                        break
                if append_container is None:
                    cont_row = self._make_row(
                        name="Append", kind="Group", op="", seq="", status="",
                        backbone="", leaf_id="", title="", href="", modified="", filepath=""
                    )
                    append_container = cont_row[self.COL_NAME]
                    append_container.setData({"kind": "append_group"}, ROLE_NODEINFO)
                    base_item.appendRow(cont_row)

                append_container.appendRow(row_items)

    # ---------------- Selection / details ----------------

    def _selected_source_item(self) -> Optional[QStandardItem]:
        idx = self.tree.currentIndex()
        if not idx.isValid():
            return None
        src_idx = self.proxy.mapToSource(idx)
        if src_idx.isValid() and src_idx.column() != 0:
            src_idx = src_idx.sibling(src_idx.row(), 0)
        return self.model.itemFromIndex(src_idx)

    def _selected_leaf(self) -> Optional[LeafRecord]:
        it = self._selected_source_item()
        if not it:
            return None
        lr = it.data(ROLE_LEAF)
        return lr if isinstance(lr, LeafRecord) else None

    def _overview_html(self, text: str) -> str:
        lines = text.splitlines()
        html_lines: List[str] = []
        for line in lines:
            if line == "":
                html_lines.append("<br/>")
                continue
            lead = len(line) - len(line.lstrip(" "))
            indent = "&nbsp;" * lead
            trimmed = line.lstrip(" ")
            if ":" in trimmed:
                head, rest = trimmed.split(":", 1)
                rest_lead = len(rest) - len(rest.lstrip(" "))
                rest_html = ("&nbsp;" * rest_lead) + html.escape(rest.lstrip(" "))
                html_lines.append(f"{indent}<u>{html.escape(head)}</u>:{rest_html}")
            elif trimmed.endswith(":"):
                html_lines.append(f"{indent}<u>{html.escape(trimmed)}</u>")
            else:
                html_lines.append(f"{indent}{html.escape(trimmed)}")
        return "<br/>".join(html_lines)

    def _set_overview_text(self, text: str):
        self.detail_overview.setHtml(self._overview_html(text))

    def update_details(self):
        it = self._selected_source_item()
        if not it:
            self._clear_details()
            return

        # Backbone node?
        bb = it.data(ROLE_BACKBONE)
        if isinstance(bb, BackboneFile):
            self.detail_title.setText(f"Backbone: {bb.xml_path.name}")
            overview = [
                f"Sequence: {bb.sequence}",
                f"Path: {bb.xml_path}",
                f"Kind: {bb.kind}",
                f"Region: {bb.region or ''}",
                f"Root Tag: {bb.root_tag}",
                f"Leaf count: {bb.leaf_count}",
                f"Parse status: {'OK' if not bb.parse_error else 'ERROR'}",
                f"Parse error: {bb.parse_error or ''}",
                "",
                "Namespaces (prefix -> uri):",
            ]
            for pref, uri in sorted(bb.ns_prefix_to_uri.items()):
                overview.append(f"  {pref or '(default)'} -> {uri}")
            self._set_overview_text("\n".join(overview))
            meta = {
                "sequence": bb.sequence,
                "xml_path": str(bb.xml_path),
                "kind": bb.kind,
                "region": bb.region or "",
                "root_tag": bb.root_tag,
                "leaf_count": str(bb.leaf_count),
                "parse_error": bb.parse_error or "",
            }
            for pref, uri in sorted(bb.ns_prefix_to_uri.items()):
                meta[f"ns:{pref or '(default)'}"] = uri
            self._fill_meta_table(meta)
            # show raw backbone xml (could be large; still ok for typical)
            try:
                self.xml_view.setPlainText(bb.xml_path.read_text(encoding="utf-8", errors="replace"))
            except Exception as e:
                self.xml_view.setPlainText(f"(Could not read XML: {e})")
            self.versions_table.setRowCount(0)
            self._set_buttons_for_leaf(None)
            return

        lr = it.data(ROLE_LEAF)
        if not isinstance(lr, LeafRecord):
            info = it.data(ROLE_NODEINFO) or {}
            kind = info.get("kind")
            self.detail_title.setText(it.text() or "—")
            if kind == "sequence" and self.dossier:
                self._set_overview_text(self.dossier.backbone_summary_for_seq(info.get("seq", "")))
            else:
                self._set_overview_text("")
            self.meta_table.setRowCount(0)
            self.xml_view.setPlainText("")
            self.versions_table.setRowCount(0)
            self._set_buttons_for_leaf(None)
            return

        # Leaf details
        self.detail_title.setText(lr.display_name_toc() or lr.display_name_filesystem())
        open_path = lr.resolved_open_path()
        overview_lines = [
            f"Sequence: {lr.sequence}",
            f"Operation: {lr.operation}",
            f"Status: {self._leaf_status(lr, consolidated=self.consolidated_cb.isChecked())}",
            f"Backbone: {lr.backbone.xml_path}",
            f"Leaf ID: {lr.leaf_id or ''}",
            f"Title: {lr.title or ''}",
            f"Href: {lr.href or ''}",
            f"modified-file: {lr.modified_file or ''}",
            f"File (opens): {str(open_path) if open_path else ''}",
            f"Exists: {open_path.exists() if open_path else False}",
        ]
        if lr.warnings:
            overview_lines.append("")
            overview_lines.append("Warnings:")
            overview_lines.extend([f"- {w}" for w in lr.warnings])
        self._set_overview_text("\n".join(overview_lines))

        meta: Dict[str, str] = {
            "sequence": lr.sequence,
            "operation": lr.operation,
            "leaf_id": lr.leaf_id or "",
            "title": lr.title or "",
            "href": lr.href or "",
            "resolved_open_path": str(open_path) if open_path else "",
            "modified_file": lr.modified_file or "",
            "backbone_xml": str(lr.backbone.xml_path),
            "backbone_kind": lr.backbone.kind,
            "backbone_region": lr.backbone.region or "",
            "missing_file": str(lr.missing_file),
        }
        for k, v in sorted(lr.leaf_attrib_prefixed.items()):
            meta[f"leaf_attr:{k}"] = v
        for k, v in sorted(lr.leaf_meta_flat.items()):
            meta[f"leaf_xml:{k}"] = v

        self._fill_meta_table(meta)
        self.xml_view.setPlainText(lr.leaf_xml or "")
        self._fill_versions_table(lr)
        self._set_buttons_for_leaf(lr)

    def _fill_versions_table(self, lr: LeafRecord):
        self.versions_table.setRowCount(0)
        rows: List[List[str]] = []
        cur = lr
        seen = set()
        depth = 0
        while cur and cur not in seen and depth < 200:
            seen.add(cur)
            p = cur.resolved_open_path()
            exists = p.exists() if p else False
            rows.append([f"{depth:02d}", cur.sequence, cur.operation, str(exists), str(p or "")])
            cur = cur.prev_leaf
            depth += 1
        if cur in seen:
            rows.append(["", "", "", "", "WARNING: Cycle detected in modified-file chain."])
        self.versions_table.setRowCount(len(rows))
        for r, row in enumerate(rows):
            for c, val in enumerate(row):
                self.versions_table.setItem(r, c, QTableWidgetItem(val))
        self.versions_table.resizeColumnToContents(0)

    def on_versions_double_click(self, row: int, column: int):
        if column != 4:
            return
        item = self.versions_table.item(row, column)
        if not item:
            return
        path_str = (item.text() or "").strip()
        if not path_str:
            return
        path = Path(path_str)
        if not path.exists():
            QMessageBox.warning(self, "File Not Found", f"File does not exist:\n{path}")
            return
        QDesktopServices.openUrl(QUrl.fromLocalFile(str(path)))

    def on_tree_double_click(self, index: QModelIndex):
        if not index.isValid():
            return
        src_index = self.proxy.mapToSource(index)
        item = self.model.itemFromIndex(src_index)
        if not item:
            return
        lr = item.data(ROLE_LEAF)
        if not isinstance(lr, LeafRecord):
            return
        p = lr.resolved_open_path()
        if p and p.exists():
            self.open_path(p)

    def _fill_meta_table(self, meta: Dict[str, str]):
        self.meta_table.setRowCount(0)
        if not meta:
            return
        keys = sorted(meta.keys())
        self.meta_table.setRowCount(len(keys))
        for r, k in enumerate(keys):
            self.meta_table.setItem(r, 0, QTableWidgetItem(str(k)))
            self.meta_table.setItem(r, 1, QTableWidgetItem(str(meta[k])))
        self.meta_table.resizeColumnToContents(0)

    def _clear_details(self):
        self.detail_title.setText("—")
        self.detail_overview.setHtml("")
        self.meta_table.setRowCount(0)
        self.xml_view.setPlainText("")
        self.versions_table.setRowCount(0)
        self._set_buttons_for_leaf(None)

    def _set_buttons_for_leaf(self, lr: Optional[LeafRecord]):
        if lr is None:
            self.btn_open.setEnabled(False)
            self.btn_prev.setEnabled(False)
            self.btn_show.setEnabled(False)
            self.btn_copy.setEnabled(False)
            return
        p = lr.resolved_open_path()
        self.btn_open.setEnabled(bool(p and p.exists()))
        prev = lr.prev_leaf.abs_file_path if (lr.prev_leaf and lr.prev_leaf.abs_file_path) else None
        self.btn_prev.setEnabled(bool(prev and prev.exists()))
        self.btn_show.setEnabled(bool(p and p.exists()))
        self.btn_copy.setEnabled(bool(p))

    # ---------------- Actions ----------------

    def open_path(self, p: Path):
        QDesktopServices.openUrl(QUrl.fromLocalFile(str(p)))

    def open_selected(self):
        lr = self._selected_leaf()
        if not lr:
            return
        p = lr.resolved_open_path()
        if p and p.exists():
            self.open_path(p)

    def open_prev_selected(self):
        lr = self._selected_leaf()
        if not lr or not lr.prev_leaf:
            return
        p = lr.prev_leaf.abs_file_path
        if p and p.exists():
            self.open_path(p)

    def show_in_file_manager(self):
        lr = self._selected_leaf()
        if not lr:
            return
        p = lr.resolved_open_path()
        if not p:
            return
        folder = p if p.is_dir() else p.parent
        if folder.exists():
            self.open_path(folder)

    def copy_selected_path(self):
        lr = self._selected_leaf()
        if not lr:
            return
        p = lr.resolved_open_path()
        if p:
            QApplication.clipboard().setText(str(p))

    # ---------------- Context menu ----------------

    def on_context_menu(self, pos):
        idx = self.tree.indexAt(pos)
        if not idx.isValid():
            return
        self.tree.setCurrentIndex(idx)
        lr = self._selected_leaf()

        menu = QMenu(self)
        act_open = menu.addAction("Open")
        act_prev = menu.addAction("Open Predecessor")
        act_show = menu.addAction("Show in File Manager")
        menu.addSeparator()
        act_copy_path = menu.addAction("Copy Path")
        act_copy_href = menu.addAction("Copy Href")
        act_copy_mod = menu.addAction("Copy modified-file")
        act_copy_leafid = menu.addAction("Copy Leaf ID")

        if not lr:
            for a in (act_open, act_prev, act_show, act_copy_path, act_copy_href, act_copy_mod, act_copy_leafid):
                a.setEnabled(False)
        else:
            p = lr.resolved_open_path()
            act_open.setEnabled(bool(p and p.exists()))
            prev = lr.prev_leaf.abs_file_path if (lr.prev_leaf and lr.prev_leaf.abs_file_path) else None
            act_prev.setEnabled(bool(prev and prev.exists()))
            act_show.setEnabled(bool(p and p.exists()))
            act_copy_path.setEnabled(bool(p))
            act_copy_href.setEnabled(bool(lr.href))
            act_copy_mod.setEnabled(bool(lr.modified_file))
            act_copy_leafid.setEnabled(bool(lr.leaf_id))

        chosen = menu.exec(self.tree.viewport().mapToGlobal(pos))
        if not chosen:
            return

        if chosen == act_open:
            self.open_selected()
        elif chosen == act_prev:
            self.open_prev_selected()
        elif chosen == act_show:
            self.show_in_file_manager()
        elif chosen == act_copy_path and lr:
            p = lr.resolved_open_path()
            if p:
                QApplication.clipboard().setText(str(p))
        elif chosen == act_copy_href and lr:
            QApplication.clipboard().setText(lr.href or "")
        elif chosen == act_copy_mod and lr:
            QApplication.clipboard().setText(lr.modified_file or "")
        elif chosen == act_copy_leafid and lr:
            QApplication.clipboard().setText(lr.leaf_id or "")


def main():
    import sys
    app = QApplication(sys.argv)
    w = MainWindow()
    w.resize(1500, 850)
    w.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
