import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Optional, Dict, Tuple, List

from PySide6.QtCore import Qt, QUrl
from PySide6.QtGui import QAction, QDesktopServices, QStandardItem, QStandardItemModel
from PySide6.QtWidgets import (
    QApplication, QMainWindow, QFileDialog, QTreeView, QWidget, QVBoxLayout,
    QHBoxLayout, QLabel, QPushButton, QComboBox, QCheckBox, QMessageBox, QSplitter
)

import xml.etree.ElementTree as ET


XLINK_HREF = "{http://www.w3.org/1999/xlink}href"


def localname(tag: str) -> str:
    # "{ns}leaf" -> "leaf"
    return tag.split("}", 1)[-1] if "}" in tag else tag


def defrag(s: str) -> Tuple[str, Optional[str]]:
    # "path/to/index.xml#id123" -> ("path/to/index.xml", "id123")
    if "#" in s:
        p, f = s.split("#", 1)
        return p, f
    return s, None


@dataclass
class LeafRecord:
    sequence: str
    backbone_xml: Path
    leaf_id: Optional[str]
    title: str
    operation: str
    href: Optional[str]
    abs_file_path: Optional[Path]
    modified_file: Optional[str]

    prev_ref: Optional[Tuple[Path, str]] = None
    prev_leaf: Optional["LeafRecord"] = None

    # for consolidated view
    active: bool = True

    def display_file_path(self, dossier_root: Path) -> str:
        p = self.abs_file_path
        if p and p.exists():
            try:
                return str(p.relative_to(dossier_root))
            except Exception:
                return str(p)
        return ""


class EctdDossier:
    def __init__(self, root: Path):
        self.root = root
        self.sequences: List[str] = []
        self.leaves_by_seq: Dict[str, List[LeafRecord]] = {}
        self.leaf_index: Dict[Tuple[Path, str], LeafRecord] = {}

    def scan_sequences(self) -> List[str]:
        seqs = []
        for child in self.root.iterdir():
            if child.is_dir() and re.fullmatch(r"\d{4}", child.name):
                seqs.append(child.name)
        seqs.sort()
        self.sequences = seqs
        return seqs

    def _parse_backbone(self, seq: str, xml_path: Path) -> List[LeafRecord]:
        if not xml_path.exists():
            return []
        try:
            tree = ET.parse(xml_path)
        except Exception as e:
            print(f"XML parse error: {xml_path}: {e}")
            return []

        base_dir = xml_path.parent
        leaves: List[LeafRecord] = []

        for el in tree.getroot().iter():
            if localname(el.tag) != "leaf":
                continue

            op = (el.attrib.get("operation") or "new").strip()
            title = (el.attrib.get("title") or "").strip()

            leaf_id = el.attrib.get("ID") or el.attrib.get("id") or el.attrib.get("Id")
            href = el.attrib.get(XLINK_HREF)
            modified = el.attrib.get("modified-file") or el.attrib.get("modified_file")

            abs_file = None
            if href:
                href_path, _frag = defrag(href)
                abs_file = (base_dir / href_path).resolve()

            lr = LeafRecord(
                sequence=seq,
                backbone_xml=xml_path.resolve(),
                leaf_id=leaf_id,
                title=title,
                operation=op,
                href=href,
                abs_file_path=abs_file,
                modified_file=modified,
            )
            leaves.append(lr)

        return leaves

    def load(self) -> None:
        self.scan_sequences()
        all_leaves: List[LeafRecord] = []

        for seq in self.sequences:
            seq_dir = (self.root / seq)

            # 1) Standard backbone
            all_leaves.extend(self._parse_backbone(seq, seq_dir / "index.xml"))

            # 2) EU Module 1 backbone (typisch)
            all_leaves.extend(self._parse_backbone(seq, seq_dir / "m1" / "eu" / "eu-regional.xml"))

        # Index by (xml_path, leaf_id)
        self.leaf_index.clear()
        for lr in all_leaves:
            if lr.leaf_id:
                self.leaf_index[(lr.backbone_xml, lr.leaf_id)] = lr

        # Resolve modified-file references
        for lr in all_leaves:
            if not lr.modified_file:
                continue
            p, frag = defrag(lr.modified_file.strip())
            if not frag:
                continue
            target_xml = (lr.backbone_xml.parent / p).resolve()
            lr.prev_ref = (target_xml, frag)

        for lr in all_leaves:
            if lr.prev_ref:
                lr.prev_leaf = self.leaf_index.get(lr.prev_ref)

        # Group by sequence
        self.leaves_by_seq.clear()
        for lr in all_leaves:
            self.leaves_by_seq.setdefault(lr.sequence, []).append(lr)

    def compute_consolidated(self, upto_seq: str) -> List[LeafRecord]:
        # Reset actives
        for seq in self.sequences:
            for lr in self.leaves_by_seq.get(seq, []):
                lr.active = True

        # Apply lifecycle sequentially
        for seq in self.sequences:
            for lr in self.leaves_by_seq.get(seq, []):
                op = lr.operation.lower()
                if op in ("replace", "delete"):
                    if lr.prev_leaf:
                        lr.prev_leaf.active = False
                    if op == "delete":
                        lr.active = False  # delete event itself is not an active document
                elif op == "append":
                    # keep prev active, new can be treated as additional active piece
                    pass
                else:
                    # new
                    pass

            if seq == upto_seq:
                break

        out: List[LeafRecord] = []
        for seq in self.sequences:
            if seq > upto_seq:
                break
            out.extend(self.leaves_by_seq.get(seq, []))

        # In consolidated view we typically show only active docs (and optionally append parts)
        return [lr for lr in out if lr.active and (lr.operation.lower() != "delete")]


class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("eCTD Viewer (Prototype)")

        self.dossier: Optional[EctdDossier] = None
        self.dossier_root: Optional[Path] = None

        # Controls
        self.seq_combo = QComboBox()
        self.consolidated_cb = QCheckBox("Konsolidiert")
        self.open_btn = QPushButton("Dossier öffnen…")

        self.open_btn.clicked.connect(self.open_dossier)
        self.seq_combo.currentTextChanged.connect(self.refresh_view)
        self.consolidated_cb.stateChanged.connect(self.refresh_view)

        top = QWidget()
        top_layout = QHBoxLayout(top)
        top_layout.addWidget(self.open_btn)
        top_layout.addWidget(QLabel("Sequenz:"))
        top_layout.addWidget(self.seq_combo)
        top_layout.addWidget(self.consolidated_cb)
        top_layout.addStretch(1)

        # Tree
        self.model = QStandardItemModel()
        self.model.setHorizontalHeaderLabels(["Name", "Op", "Seq", "Pfad"])
        self.tree = QTreeView()
        self.tree.setModel(self.model)
        self.tree.doubleClicked.connect(self.on_double_click)
        self.tree.setAlternatingRowColors(True)

        # Detail panel
        self.detail_title = QLabel("—")
        self.detail_info = QLabel("")
        self.detail_info.setTextInteractionFlags(Qt.TextSelectableByMouse)

        self.btn_open = QPushButton("Öffnen")
        self.btn_prev = QPushButton("Vorgänger öffnen")
        self.btn_open.clicked.connect(self.open_selected)
        self.btn_prev.clicked.connect(self.open_prev_selected)

        detail = QWidget()
        dl = QVBoxLayout(detail)
        dl.addWidget(QLabel("Details"))
        dl.addWidget(self.detail_title)
        dl.addWidget(self.detail_info)
        dl.addStretch(1)
        dl.addWidget(self.btn_open)
        dl.addWidget(self.btn_prev)

        splitter = QSplitter()
        splitter.addWidget(self.tree)
        splitter.addWidget(detail)
        splitter.setStretchFactor(0, 3)
        splitter.setStretchFactor(1, 2)

        central = QWidget()
        cl = QVBoxLayout(central)
        cl.addWidget(top)
        cl.addWidget(splitter)
        self.setCentralWidget(central)

        self.tree.selectionModel().selectionChanged.connect(self.update_details)

    def open_dossier(self):
        folder = QFileDialog.getExistingDirectory(self, "eCTD Dossier-Ordner auswählen")
        if not folder:
            return
        root = Path(folder).resolve()
        self.dossier_root = root
        self.dossier = EctdDossier(root)
        self.dossier.load()

        self.seq_combo.blockSignals(True)
        self.seq_combo.clear()
        self.seq_combo.addItems(self.dossier.sequences)
        self.seq_combo.blockSignals(False)

        if not self.dossier.sequences:
            QMessageBox.warning(self, "Keine Sequenzen gefunden", "Keine Ordner wie 0000, 0001, … gefunden.")
            return

        self.seq_combo.setCurrentIndex(len(self.dossier.sequences) - 1)
        self.refresh_view()

    def refresh_view(self):
        if not self.dossier or not self.dossier_root:
            return
        seq = self.seq_combo.currentText().strip()
        if not seq:
            return

        self.model.removeRows(0, self.model.rowCount())

        if self.consolidated_cb.isChecked():
            leaves = self.dossier.compute_consolidated(seq)
        else:
            leaves = self.dossier.leaves_by_seq.get(seq, [])

        # Build folder-like tree based on file paths
        root_nodes: Dict[str, QStandardItem] = {}

        def ensure_path(parts: List[str]) -> QStandardItem:
            # top-level node = first part
            cur_key = ""
            parent_item = None
            for p in parts:
                cur_key = cur_key + "/" + p
                if cur_key in root_nodes:
                    parent_item = root_nodes[cur_key]
                    continue
                item0 = QStandardItem(p)
                item0.setEditable(False)
                row = [item0, QStandardItem(""), QStandardItem(""), QStandardItem("")]
                for it in row:
                    it.setEditable(False)
                if parent_item is None:
                    self.model.appendRow(row)
                else:
                    parent_item.appendRow(row)
                root_nodes[cur_key] = item0
                parent_item = item0
            return parent_item

        for lr in leaves:
            # Determine display-relative path:
            # - in sequence view: path relative to that sequence folder if possible
            # - in consolidated view: path without leading sequence folder if possible
            seq_dir = (self.dossier_root / seq).resolve()

            file_path = lr.abs_file_path
            if lr.operation.lower() == "delete" and lr.prev_leaf and lr.prev_leaf.abs_file_path:
                file_path = lr.prev_leaf.abs_file_path

            if file_path:
                try:
                    rel_to_root = file_path.relative_to(self.dossier_root)
                    parts = list(rel_to_root.parts)
                    if self.consolidated_cb.isChecked():
                        # drop leading sequence if present
                        if parts and re.fullmatch(r"\d{4}", parts[0]):
                            parts = parts[1:]
                    else:
                        # prefer path inside current sequence
                        try:
                            rel_to_seq = file_path.relative_to(seq_dir)
                            parts = list(rel_to_seq.parts)
                        except Exception:
                            pass
                except Exception:
                    parts = [file_path.name]
            else:
                parts = ["<unbekannt>"]

            if len(parts) == 1:
                folder_parts = []
                fname = parts[0]
            else:
                folder_parts = parts[:-1]
                fname = parts[-1]

            parent = ensure_path(folder_parts) if folder_parts else None

            name_item = QStandardItem(fname)
            op_item = QStandardItem(lr.operation)
            seq_item = QStandardItem(lr.sequence)
            path_item = QStandardItem(lr.display_file_path(self.dossier_root))

            for it in (name_item, op_item, seq_item, path_item):
                it.setEditable(False)

            # Grey out deletes in sequence view
            if (not self.consolidated_cb.isChecked()) and lr.operation.lower() == "delete":
                for it in (name_item, op_item, seq_item, path_item):
                    it.setForeground(Qt.gray)

            # store object reference
            name_item.setData(lr, Qt.UserRole)

            row = [name_item, op_item, seq_item, path_item]
            if parent is None:
                self.model.appendRow(row)
            else:
                parent.appendRow(row)

        self.tree.expandToDepth(2)

    def current_leaf(self) -> Optional[LeafRecord]:
        idx = self.tree.currentIndex()
        if not idx.isValid():
            return None
        item = self.model.itemFromIndex(idx.siblingAtColumn(0))
        if not item:
            return None
        lr = item.data(Qt.UserRole)
        return lr if isinstance(lr, LeafRecord) else None

    def update_details(self):
        lr = self.current_leaf()
        if not lr or not self.dossier_root:
            self.detail_title.setText("—")
            self.detail_info.setText("")
            self.btn_open.setEnabled(False)
            self.btn_prev.setEnabled(False)
            return

        self.detail_title.setText(lr.title or "(kein Titel)")
        prev = lr.prev_leaf

        file_path = lr.abs_file_path
        if lr.operation.lower() == "delete" and prev and prev.abs_file_path:
            file_path = prev.abs_file_path

        info_lines = [
            f"Operation: {lr.operation}",
            f"Sequenz: {lr.sequence}",
            f"Backbone: {lr.backbone_xml}",
        ]
        if file_path:
            info_lines.append(f"Datei: {file_path}")
            info_lines.append(f"Existiert: {file_path.exists()}")
        if lr.modified_file:
            info_lines.append(f"modified-file: {lr.modified_file}")
        if prev:
            info_lines.append(f"Vorgänger: {prev.backbone_xml}#{prev.leaf_id}")

        self.detail_info.setText("\n".join(info_lines))

        self.btn_open.setEnabled(bool(file_path and file_path.exists()))
        self.btn_prev.setEnabled(prev is not None and prev.abs_file_path is not None and prev.abs_file_path.exists())

    def open_path(self, p: Path):
        QDesktopServices.openUrl(QUrl.fromLocalFile(str(p)))

    def open_selected(self):
        lr = self.current_leaf()
        if not lr:
            return
        target = lr.abs_file_path
        if lr.operation.lower() == "delete" and lr.prev_leaf and lr.prev_leaf.abs_file_path:
            target = lr.prev_leaf.abs_file_path
        if target and target.exists():
            self.open_path(target)

    def open_prev_selected(self):
        lr = self.current_leaf()
        if not lr or not lr.prev_leaf:
            return
        p = lr.prev_leaf.abs_file_path
        if p and p.exists():
            self.open_path(p)

    def on_double_click(self, _idx):
        # double click = open file (or original if delete)
        self.open_selected()


if __name__ == "__main__":
    app = QApplication(sys.argv)
    w = MainWindow()
    w.resize(1200, 700)
    w.show()
    sys.exit(app.exec())
