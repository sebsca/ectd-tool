#!/usr/bin/env python3
"""
ema_ectd_backbone_generator.py
Generates and validates eCTD 3.2 and EU M1 3.1 compliant XML backbones.
"""

import os
import uuid
import hashlib
from pathlib import Path
from lxml import etree

# =========================================================
# Utility functions
# =========================================================

def md5_checksum(filepath: Path) -> str:
    m = hashlib.md5()
    with open(filepath, "rb") as f:
        for chunk in iter(lambda: f.read(4096), b""):
            m.update(chunk)
    return m.hexdigest()

METADATA_FIELDS = [
    "file_path",
    "title",
    "operation",
    "modified-leaf",
    "modified-href",
    "applicant_name",
    "submission_type",
    "sequence_description",
    "eu_country",
    "eu_identifier",
    "eu_submission_type",
    "eu_submission_mode",
    "eu_submission_number",
    "eu_procedure_number",
    "eu_submission_unit_type",
    "eu_agency_code",
    "eu_procedure_type",
    "eu_invented_name",
    "eu_inn",
    "eu_related_sequence",
]

def checksum_for_path(seq_dir: Path, file_path: str) -> tuple[str, bool]:
    """Return checksum for file_path or empty string if missing (with warnings)."""
    file_abs = seq_dir / file_path
    if not file_abs.exists():
        if not file_abs.parent.exists():
            print(f"⚠ Missing directory for leaf: {file_abs.parent}")
        print(f"⚠ Missing file for leaf: {file_path}")
        return "", False
    return md5_checksum(file_abs), True

def write_xml_with_doctype_and_xsl(xml_path: Path, root, doctype: str, xsl_href: str) -> None:
    """Write XML with a DOCTYPE and xml-stylesheet PI."""
    etree.indent(root, space="  ")
    body = etree.tostring(root, encoding="utf-8", xml_declaration=False)
    header = b'<?xml version="1.0" encoding="utf-8"?>\n'
    doctype_line = (doctype + "\n").encode("utf-8")
    pi_line = (f'<?xml-stylesheet href="{xsl_href}" type="text/xsl"?>\n').encode("utf-8")
    with open(xml_path, "wb") as f:
        f.write(header)
        f.write(doctype_line)
        f.write(pi_line)
        f.write(body)

def write_index_md5(seq_dir: Path, index_path: Path) -> None:
    """Write MD5 checksum for index.xml to index-md5.txt in sequence root."""
    if not index_path.exists():
        return
    checksum = md5_checksum(index_path)
    rel = index_path.relative_to(seq_dir).as_posix()
    out_path = seq_dir / "index-md5.txt"
    out_path.write_text(f"{checksum}  {rel}\n", encoding="utf-8")

def resolve_previous_leaf_ref(
    base_dir: Path,
    seq_num: int,
    modified_leaf: str | None,
    modified_href: str | None,
    file_path: str,
    fallback_paths: list[str] | None = None,
) -> tuple[int, str, str, str] | None:
    """Resolve prior leaf by explicit ID or by href/path lookup."""
    if modified_leaf:
        return find_previous_leaf_by_id(base_dir, seq_num, modified_leaf)
    if modified_href:
        return find_previous_leaf_id(base_dir, seq_num, [modified_href])
    if fallback_paths:
        return find_previous_leaf_id(base_dir, seq_num, fallback_paths)
    return find_previous_leaf_id(base_dir, seq_num, [file_path])

def load_metadata(xlsx_path: Path, sheet_name: str | None = None) -> list[dict]:
    try:
        import openpyxl
    except ImportError as exc:
        raise ImportError(
            "openpyxl is required to read Excel metadata files. "
            "Install it in the current environment."
        ) from exc

    wb = openpyxl.load_workbook(xlsx_path, data_only=True)
    ws = wb[sheet_name] if sheet_name else wb.active

    rows = list(ws.iter_rows(values_only=True))
    if not rows:
        return []

    headers = [str(h).strip() if h is not None else "" for h in rows[0]]
    out: list[dict] = []
    for row in rows[1:]:
        if row is None or all(cell is None or str(cell).strip() == "" for cell in row):
            continue
        record: dict[str, str] = {}
        for idx, header in enumerate(headers):
            if not header:
                continue
            value = row[idx] if idx < len(row) else None
            record[header] = "" if value is None else str(value).strip()
        out.append(record)
    return out

def write_metadata_from_scan(seq_dir: Path, xlsx_path: Path) -> int:
    """Scan sequence directory and append missing rows to metadata Excel."""
    try:
        import openpyxl
    except ImportError as exc:
        raise ImportError(
            "openpyxl is required to write Excel metadata files. "
            "Install it in the current environment."
        ) from exc

    if xlsx_path.exists():
        wb = openpyxl.load_workbook(xlsx_path)
        ws = wb.active
        rows = list(ws.iter_rows(values_only=True))
        headers = [str(h).strip() if h is not None else "" for h in (rows[0] if rows else [])]
        if not headers or "file_path" not in headers:
            ws.delete_rows(1, ws.max_row)
            ws.append(METADATA_FIELDS)
            headers = METADATA_FIELDS[:]
        existing = set()
        for row in rows[1:]:
            if not row:
                continue
            idx = headers.index("file_path")
            value = row[idx] if idx < len(row) else None
            if value:
                existing.add(str(value).strip())
    else:
        wb = openpyxl.Workbook()
        ws = wb.active
        ws.title = "metadata"
        ws.append(METADATA_FIELDS)
        headers = METADATA_FIELDS[:]
        existing = set()

    new_rows = 0
    for path in sorted(seq_dir.rglob("*")):
        if not path.is_file():
            continue
        rel_path = path.relative_to(seq_dir).as_posix()
        if "/" not in rel_path:
            continue
        if rel_path == "index.xml" or rel_path == "m1/eu/eu-regional.xml":
            continue
        if rel_path.startswith("util/"):
            continue
        if rel_path in existing:
            continue
        title = path.stem.replace("-", " ").replace("_", " ").strip() or path.name
        row = {
            "file_path": rel_path,
            "title": title,
            "operation": "new",
        }
        ws.append([row.get(field, "") for field in METADATA_FIELDS])
        new_rows += 1

    if new_rows > 0 or not xlsx_path.exists():
        wb.save(xlsx_path)
    return new_rows

def _norm_href(href: str) -> str:
    """Normalize hrefs for comparison (strip, unify slashes)."""
    href = (href or "").strip()
    return href.replace("\\", "/")

def _leaf_href(leaf) -> str:
    """Extract href from a leaf element (xlink:href or href)."""
    # Prefer explicit xlink href if present
    for k in (
        "{http://www.w3.org/1999/xlink}href",
        "{http://www.w3c.org/1999/xlink}href",
        "href",
    ):
        if k in leaf.attrib:
            return _norm_href(leaf.attrib.get(k) or "")
    # Fallback: any attribute whose localname is 'href'
    for k, v in leaf.attrib.items():
        if k.endswith("}href") or k == "href":
            return _norm_href(v or "")
    return ""


def _referenced_xmls_from_index(prev_dir: Path) -> list[Path]:
    """Return all XML files referenced by index.xml in a sequence.

    We treat a referenced file as an XML candidate if its href ends with '.xml' (case-insensitive)
    and the file exists on disk. Always includes index.xml itself.
    """
    candidates: list[Path] = []

    index_path = prev_dir / "index.xml"
    if index_path.exists():
        candidates.append(index_path)
        try:
            ix = etree.parse(str(index_path))
            leaves = ix.xpath("//*[local-name()='leaf']")
            for leaf in leaves:
                href = _leaf_href(leaf)
                if not href:
                    continue
                if not href.lower().endswith(".xml"):
                    continue
                p = (prev_dir / href).resolve()
                # Only include files that are actually inside the sequence folder
                # (avoid accidental directory traversal)
                try:
                    p.relative_to(prev_dir.resolve())
                except Exception:
                    continue
                if p.exists() and p.is_file():
                    candidates.append(p)
        except Exception:
            # If index.xml is malformed, we at least search it as-is later.
            pass

    # De-duplicate while preserving order
    seen = set()
    out: list[Path] = []
    for p in candidates:
        if p not in seen:
            out.append(p)
            seen.add(p)
    return out


def find_previous_leaf_id(base_dir: Path, current_seq: int, file_paths: list[str]) -> tuple[int, str, str, str] | None:
    """Walk back through previous sequences to locate the most recent matching leaf ID.

    Searches index.xml AND every XML file that is referenced by index.xml (via leaf @xlink:href)
    within each previous sequence.

    Matching strategy (in order):
      1) Exact href match against any of `file_paths`
      2) Basename match (last path component) against any of `file_paths`
         (useful when replace uses a new filename but same document basename).
    """
    wanted = [_norm_href(p) for p in (file_paths or []) if p]
    wanted_basenames = {Path(p).name for p in wanted if p}

    for prev_seq in range(current_seq - 1, -1, -1):
        prev_dir = base_dir / f"{prev_seq:04d}"
        prev_dir_abs = prev_dir.resolve()
        xml_paths = _referenced_xmls_from_index(prev_dir)
        if not xml_paths:
            continue

        for xml_path in xml_paths:
            try:
                xml = etree.parse(str(xml_path))
            except Exception:
                # Skip malformed or non-XML files
                continue
            try:
                rel_xml_path = xml_path.resolve().relative_to(prev_dir_abs).as_posix()
            except Exception:
                rel_xml_path = "index.xml"

            # Use local-name() to be namespace-agnostic
            leaves = xml.xpath("//*[local-name()='leaf']")

            # 1) exact href match
            for leaf in leaves:
                href = _leaf_href(leaf)
                if href and href in wanted:
                    leaf_id = leaf.get("ID")
                    if leaf_id:
                        return prev_seq, rel_xml_path, leaf_id, href

            # 2) basename match (best-effort fallback)
            if wanted_basenames:
                for leaf in leaves:
                    href = _leaf_href(leaf)
                    if href and Path(href).name in wanted_basenames:
                        leaf_id = leaf.get("ID")
                        if leaf_id:
                            return prev_seq, rel_xml_path, leaf_id, href

    return None

def find_previous_leaf_by_id(base_dir: Path, current_seq: int, leaf_id: str) -> tuple[int, str, str, str] | None:
    """Walk back through previous sequences to locate a specific leaf ID."""
    if not leaf_id:
        return None

    for prev_seq in range(current_seq - 1, -1, -1):
        prev_dir = base_dir / f"{prev_seq:04d}"
        prev_dir_abs = prev_dir.resolve()
        xml_paths = _referenced_xmls_from_index(prev_dir)
        if not xml_paths:
            continue

        for xml_path in xml_paths:
            try:
                xml = etree.parse(str(xml_path))
            except Exception:
                continue
            try:
                rel_xml_path = xml_path.resolve().relative_to(prev_dir_abs).as_posix()
            except Exception:
                rel_xml_path = "index.xml"

            leaves = xml.xpath("//*[local-name()='leaf']")
            for leaf in leaves:
                if leaf.get("ID") == leaf_id:
                    href = _leaf_href(leaf)
                    return prev_seq, rel_xml_path, leaf_id, href

    return None
def validate_xml(xml_path: Path, dtd_path: Path):
    """Validate XML against provided DTD, raise if invalid."""
    dtd = etree.DTD(str(dtd_path))
    xml = etree.parse(str(xml_path))
    if not dtd.validate(xml):
        raise ValueError(f"DTD validation failed for {xml_path.name}:\n{dtd.error_log.filter_from_errors()}")
    print(f"✔ Validated {xml_path.name} against {dtd_path.name}")

# =========================================================
# XML generation
# =========================================================

def create_index_xml(seq_dir: Path, metadata_rows: list[dict], base_dir: Path, seq_num: int):
    """Build index.xml (ICH eCTD 3.2) with support for new/replace/delete/append."""
    NS = "http://www.ich.org/ectd"
    XLINK = "http://www.w3c.org/1999/xlink"

    root = etree.Element(
        "{%s}ectd" % NS,
        nsmap={"xlink": XLINK, "ectd": NS},
        attrib={"dtd-version": "3.2"}
    )

    module_map = {
        "m1": "m1-administrative-information-and-prescribing-information",
        "m2": "m2-common-technical-document-summaries",
        "m3": "m3-quality",
        "m4": "m4-nonclinical-study-reports",
        "m5": "m5-clinical-study-reports",
    }

    def _new_leaf_id() -> str:
        return f"N{uuid.uuid4().hex}"

    def _add_leaf(parent, row, file_path: str, href_override: str | None = None):
        operation = (row.get("operation") or "new").strip().lower()
        modified_href = (row.get("modified-href") or "").strip() or None
        modified_leaf = (row.get("modified-leaf") or "").strip() or None
        leaf_id = _new_leaf_id()

        if operation == "delete":
            checksum = ""
        else:
            checksum, _ = checksum_for_path(seq_dir, file_path)

        href_value = href_override or file_path or modified_href or ""

        attrs = {
            "checksum": checksum,
            "checksum-type": "MD5",
            "operation": operation,
            "ID": leaf_id,
        }
        if operation != "delete":
            if not href_value:
                raise ValueError("Non-delete operation requires 'file_path' or 'modified-href'.")
            attrs["{%s}href" % XLINK] = href_value

        if operation in ("replace", "delete", "append"):
            prev_leaf_ref = resolve_previous_leaf_ref(
                base_dir,
                seq_num,
                modified_leaf,
                modified_href,
                file_path,
            )

            if prev_leaf_ref:
                prev_seq, prev_xml_path, prev_id, _prev_href = prev_leaf_ref
                attrs["modified-file"] = f"../{prev_seq:04d}/{prev_xml_path}#{prev_id}"
            else:
                hint = modified_leaf or modified_href or file_path
                print(f"⚠ No previous leaf found for {operation} operation on {hint}")

        leaf = etree.SubElement(parent, "leaf", attrib=attrs)
        title = etree.SubElement(leaf, "title")
        title.text = (row.get("title") or os.path.basename(file_path))

    eu_rows = []
    rows_by_module: dict[str, list[dict]] = {k: [] for k in module_map}
    for row in metadata_rows:
        operation = (row.get("operation") or "new").strip().lower()
        file_path = (row.get("file_path") or "").strip()
        if not file_path:
            if operation == "delete":
                modified_href = (row.get("modified-href") or "").strip()
                modified_leaf = (row.get("modified-leaf") or "").strip()
                if modified_href:
                    file_path = modified_href
                elif modified_leaf:
                    prev_leaf_ref = find_previous_leaf_by_id(base_dir, seq_num, modified_leaf)
                    if prev_leaf_ref and prev_leaf_ref[3]:
                        file_path = prev_leaf_ref[3]
                if not file_path:
                    raise ValueError("Delete operation requires file_path, modified-href, or modified-leaf.")
            else:
                raise ValueError("CSV row is missing required column 'file_path'")
        row_copy = dict(row)
        row_copy["file_path"] = file_path
        if file_path.startswith("m1/eu/") and file_path != "m1/eu/eu-regional.xml":
            eu_rows.append(row_copy)
            continue
        module_key = file_path.split("/", 1)[0]
        if module_key in rows_by_module:
            rows_by_module[module_key].append(row_copy)

    if eu_rows and not any(
        (row.get("file_path") or "").strip() == "m1/eu/eu-regional.xml"
        for row in rows_by_module["m1"]
    ):
        rows_by_module["m1"].append({
            "file_path": "m1/eu/eu-regional.xml",
            "title": "European Union",
            "operation": "new",
        })

    for module_key in ("m1", "m2", "m3", "m4", "m5"):
        rows = rows_by_module.get(module_key) or []
        if not rows:
            continue
        module_elem = etree.SubElement(root, module_map[module_key])
        for row in rows:
            file_path = (row.get("file_path") or "").strip()
            if not file_path:
                continue
            _add_leaf(module_elem, row, file_path)

    xml_path = seq_dir / "index.xml"
    write_xml_with_doctype_and_xsl(
        xml_path,
        root,
        '<!DOCTYPE ectd:ectd SYSTEM "util/dtd/ich-ectd-3-2.dtd">',
        "util/style/ectd-2-0.xsl",
    )

    validate_xml(xml_path, seq_dir / "util" / "dtd" / "ich-ectd-3-2.dtd")

# =========================================================

def create_eu_regional_xml(seq_dir: Path, metadata_rows: list[dict], sequence_num: str) -> Path | None:
    """Build eu-regional.xml (EU M1 3.1) and map EU files into proper sections."""
    eu_rows: list[dict] = []
    for row in metadata_rows:
        operation = (row.get("operation") or "new").strip().lower()
        file_path = (row.get("file_path") or "").strip()
        if not file_path and operation == "delete":
            modified_href = (row.get("modified-href") or "").strip()
            modified_leaf = (row.get("modified-leaf") or "").strip()
            resolved_href = ""
            if modified_href:
                resolved_href = modified_href
            elif modified_leaf:
                prev_leaf_ref = find_previous_leaf_by_id(seq_dir.parent, int(sequence_num), modified_leaf)
                if prev_leaf_ref and prev_leaf_ref[3]:
                    resolved_href = prev_leaf_ref[3]
            if resolved_href:
                if resolved_href.startswith("m1/eu/"):
                    file_path = resolved_href
                else:
                    file_path = f"m1/eu/{resolved_href}"
        if file_path.startswith("m1/eu/") and file_path != "m1/eu/eu-regional.xml":
            row_copy = dict(row)
            row_copy["file_path"] = file_path
            eu_rows.append(row_copy)
    if not eu_rows:
        return None

    NS_EU = "http://europa.eu.int"
    XLINK = "http://www.w3c.org/1999/xlink"
    root = etree.Element(
        "{%s}eu-backbone" % NS_EU,
        nsmap={"eu": NS_EU, "xlink": XLINK},
        attrib={"dtd-version": "3.1"}
    )

    def _first_value(keys, default=""):
        for key in keys:
            val = (metadata_rows[0].get(key) or "").strip()
            if val:
                return val
        return default

    def _new_leaf_id() -> str:
        return f"N{uuid.uuid4().hex}"

    def _add_leaf(parent, row, file_path: str, href_override: str):
        operation = (row.get("operation") or "new").strip().lower()
        modified_href = (row.get("modified-href") or "").strip() or None
        modified_leaf = (row.get("modified-leaf") or "").strip() or None
        leaf_id = _new_leaf_id()

        if operation == "delete":
            checksum = ""
        else:
            checksum, _ = checksum_for_path(seq_dir, file_path)

        href_value = href_override or file_path or modified_href or ""

        attrs = {
            "checksum": checksum,
            "checksum-type": "MD5",
            "operation": operation,
            "ID": leaf_id,
        }
        if operation != "delete":
            if not href_value:
                raise ValueError("Non-delete operation requires 'file_path' or 'modified-href'.")
            attrs["{%s}href" % XLINK] = href_value

        if operation in ("replace", "delete", "append"):
            prev_leaf_ref = resolve_previous_leaf_ref(
                seq_dir.parent,
                int(sequence_num),
                modified_leaf,
                modified_href,
                file_path,
                fallback_paths=[file_path, href_override],
            )

            if prev_leaf_ref:
                prev_seq, prev_xml_path, prev_id, _prev_href = prev_leaf_ref
                attrs["modified-file"] = f"../{prev_seq:04d}/{prev_xml_path}#{prev_id}"
            else:
                hint = modified_leaf or modified_href or file_path
                print(f"⚠ No previous leaf found for {operation} operation on {hint}")

        leaf = etree.SubElement(parent, "leaf", attrib=attrs)
        title = etree.SubElement(leaf, "title")
        title.text = (row.get("title") or os.path.basename(file_path))

    def _ensure_child(parent, tag, attrib=None, order_map=None):
        attrib = attrib or {}
        for child in parent:
            if child.tag == tag and all(child.get(k) == v for k, v in attrib.items()):
                return child
        new_elem = etree.Element(tag, attrib=attrib)
        if order_map and tag in order_map:
            insert_at = len(parent)
            new_order = order_map[tag]
            for idx, child in enumerate(list(parent)):
                child_order = order_map.get(child.tag)
                if child_order is not None and child_order > new_order:
                    insert_at = idx
                    break
            parent.insert(insert_at, new_elem)
        else:
            parent.append(new_elem)
        return new_elem

    countries = {
        "at", "be", "bg", "common", "cy", "cz", "de", "dk", "edqm", "ee", "el", "es", "ema",
        "fi", "fr", "hr", "hu", "ie", "is", "it", "li", "lt", "lu", "lv", "mt", "nl", "no",
        "pl", "pt", "ro", "se", "si", "sk", "uk", "xi",
    }
    languages = {
        "bg", "cs", "da", "de", "el", "en", "es", "et", "fi", "fr", "ga", "hr", "hu", "is",
        "it", "lt", "lv", "mt", "nl", "no", "pl", "pt", "ro", "sk", "sl", "sv",
    }
    pi_types = {"spc", "annex2", "outer", "interpack", "impack", "other", "pl", "combined"}

    eu_envelope = etree.SubElement(root, "eu-envelope")
    env_country = _first_value(["eu_country"], "at")
    if env_country not in countries:
        env_country = "at"
    envelope = etree.SubElement(eu_envelope, "envelope", attrib={"country": env_country})
    etree.SubElement(envelope, "identifier").text = _first_value(["eu_identifier"], str(uuid.uuid4()))

    submission = etree.SubElement(
        envelope,
        "submission",
        attrib={
            "type": _first_value(["eu_submission_type"], "none"),
            "mode": _first_value(["eu_submission_mode"], "single"),
        },
    )
    submission_number = _first_value(["eu_submission_number"], "")
    if submission_number:
        etree.SubElement(submission, "number").text = submission_number
    proc_tracking = etree.SubElement(submission, "procedure-tracking")
    etree.SubElement(proc_tracking, "number").text = _first_value(["eu_procedure_number"], "1")

    etree.SubElement(
        envelope,
        "submission-unit",
        attrib={"type": _first_value(["eu_submission_unit_type"], "initial")},
    )
    etree.SubElement(envelope, "applicant").text = _first_value(["applicant_name"], "Unknown applicant")
    etree.SubElement(
        envelope,
        "agency",
        attrib={"code": _first_value(["eu_agency_code"], "EU-EMA")},
    )
    etree.SubElement(
        envelope,
        "procedure",
        attrib={"type": _first_value(["eu_procedure_type"], "national")},
    )
    etree.SubElement(envelope, "invented-name").text = _first_value(
        ["eu_invented_name"], "Unknown product"
    )
    inn_value = _first_value(["eu_inn"], "")
    if inn_value:
        etree.SubElement(envelope, "inn").text = inn_value
    etree.SubElement(envelope, "sequence").text = sequence_num
    etree.SubElement(envelope, "related-sequence").text = _first_value(
        ["eu_related_sequence"], sequence_num
    )
    etree.SubElement(envelope, "submission-description").text = _first_value(
        ["sequence_description"], "Not provided"
    )

    m1_eu = etree.SubElement(root, "m1-eu")
    m1_order = [
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
    ]
    m1_order_map = {tag: idx for idx, tag in enumerate(m1_order)}
    child_order_maps = {
        "m1-3-pi": {
            "m1-3-1-spc-label-pl": 0,
            "m1-3-2-mockup": 1,
            "m1-3-3-specimen": 2,
            "m1-3-4-consultation": 3,
            "m1-3-5-approved": 4,
            "m1-3-6-braille": 5,
        },
        "m1-4-expert": {
            "m1-4-1-quality": 0,
            "m1-4-2-non-clinical": 1,
            "m1-4-3-clinical": 2,
        },
        "m1-5-specific": {
            "m1-5-1-bibliographic": 0,
            "m1-5-2-generic-hybrid-bio-similar": 1,
            "m1-5-3-data-market-exclusivity": 2,
            "m1-5-4-exceptional-circumstances": 3,
            "m1-5-5-conditional-ma": 4,
        },
        "m1-6-environrisk": {
            "m1-6-1-non-gmo": 0,
            "m1-6-2-gmo": 1,
        },
        "m1-7-orphan": {
            "m1-7-1-similarity": 0,
            "m1-7-2-market-exclusivity": 1,
        },
        "m1-8-pharmacovigilance": {
            "m1-8-1-pharmacovigilance-system": 0,
            "m1-8-2-risk-management-system": 1,
        },
    }

    def _find_token(parts, allowed, default):
        for part in parts:
            if part in allowed:
                return part
        return default

    def _map_m1_section(section: str, parts: list[str]):
        if section == "additional-data":
            return "m1-additional-data", None, "specific"
        if section == "responses":
            return "m1-responses", None, "specific"
        if section.startswith("110"):
            return "m1-10-paediatrics", None, "leaf"
        if section.startswith("10"):
            return "m1-0-cover", None, "specific"
        if section.startswith("12"):
            return "m1-2-form", None, "specific"
        if section.startswith("13"):
            pi_child_map = {
                "31": "m1-3-1-spc-label-pl",
                "32": "m1-3-2-mockup",
                "33": "m1-3-3-specimen",
                "34": "m1-3-4-consultation",
                "35": "m1-3-5-approved",
                "36": "m1-3-6-braille",
            }
            child_key = parts[1][:2] if len(parts) > 1 else ""
            return "m1-3-pi", pi_child_map.get(child_key, "m1-3-1-spc-label-pl"), "pi-doc"
        if section.startswith("14"):
            expert_map = {
                "41": "m1-4-1-quality",
                "42": "m1-4-2-non-clinical",
                "43": "m1-4-3-clinical",
            }
            child_key = parts[1][:2] if len(parts) > 1 else ""
            return "m1-4-expert", expert_map.get(child_key, "m1-4-1-quality"), "leaf"
        if section.startswith("15"):
            specific_map = {
                "51": "m1-5-1-bibliographic",
                "52": "m1-5-2-generic-hybrid-bio-similar",
                "53": "m1-5-3-data-market-exclusivity",
                "54": "m1-5-4-exceptional-circumstances",
                "55": "m1-5-5-conditional-ma",
            }
            child_key = parts[1][:2] if len(parts) > 1 else ""
            return "m1-5-specific", specific_map.get(child_key, "m1-5-1-bibliographic"), "leaf"
        if section.startswith("16"):
            env_map = {
                "61": "m1-6-1-non-gmo",
                "62": "m1-6-2-gmo",
            }
            child_key = parts[1][:2] if len(parts) > 1 else ""
            return "m1-6-environrisk", env_map.get(child_key, "m1-6-1-non-gmo"), "leaf"
        if section.startswith("17"):
            orphan_map = {
                "71": "m1-7-1-similarity",
                "72": "m1-7-2-market-exclusivity",
            }
            child_key = parts[1][:2] if len(parts) > 1 else ""
            return "m1-7-orphan", orphan_map.get(child_key, "m1-7-1-similarity"), "leaf"
        if section.startswith("18"):
            pv_map = {
                "81": "m1-8-1-pharmacovigilance-system",
                "82": "m1-8-2-risk-management-system",
            }
            child_key = parts[1][:2] if len(parts) > 1 else ""
            return "m1-8-pharmacovigilance", pv_map.get(child_key, "m1-8-1-pharmacovigilance-system"), "leaf"
        if section.startswith("19"):
            return "m1-9-clinical-trials", None, "leaf"
        return "m1-additional-data", None, "specific"

    for row in eu_rows:
        file_path = (row.get("file_path") or "").strip()
        rel_path = file_path[len("m1/eu/"):]
        parts = [p for p in rel_path.split("/") if p]
        if not parts:
            continue

        section = parts[0]
        section_tag, child_tag, container_type = _map_m1_section(section, parts)
        section_elem = _ensure_child(m1_eu, section_tag, order_map=m1_order_map)
        if child_tag:
            section_elem = _ensure_child(
                section_elem,
                child_tag,
                order_map=child_order_maps.get(section_tag),
            )

        if container_type == "specific":
            country = parts[1] if len(parts) > 1 else "common"
            if country not in countries:
                country = "common"
            container = _ensure_child(section_elem, "specific", {"country": country})
        elif container_type == "pi-doc":
            pi_country = _find_token(parts, countries, "common")
            pi_lang = _find_token(parts, languages, "en")
            pi_type = _find_token(parts, pi_types, "other")
            container = _ensure_child(
                section_elem,
                "pi-doc",
                {"country": pi_country, "type": pi_type, "{http://www.w3.org/XML/1998/namespace}lang": pi_lang},
            )
        else:
            container = section_elem

        _add_leaf(container, row, file_path, rel_path)

    m1_eu_dir = seq_dir / "m1" / "eu"
    m1_eu_dir.mkdir(parents=True, exist_ok=True)
    xml_path = m1_eu_dir / "eu-regional.xml"

    write_xml_with_doctype_and_xsl(
        xml_path,
        root,
        '<!DOCTYPE eu:eu-backbone SYSTEM "../../util/dtd/eu-regional.dtd">',
        "../../util/style/eu-regional.xsl",
    )

    dtd_path = seq_dir / "util" / "dtd" / "eu-regional.dtd"
    if dtd_path.exists():
        validate_xml(xml_path, dtd_path)
    else:
        print(f"⚠️  EU regional DTD not found at {dtd_path}; skipped validation for eu-regional.xml")
    return xml_path

# =========================================================
# Main entry
# =========================================================

def main(base_dir: str, sequence_num: str, scan: bool = False):
    base = Path(base_dir)
    seq_dir = base / sequence_num
    xlsx_file = base / f"metadata-{sequence_num}.xlsx"

    if scan:
        count = write_metadata_from_scan(seq_dir, xlsx_file)
        print(f"✔ Wrote {count} rows to {xlsx_file}")

    if not xlsx_file.exists():
        raise FileNotFoundError(f"Metadata Excel missing: {xlsx_file}")

    metadata = load_metadata(xlsx_file)
    seq_int = int(sequence_num)

    print(f"Starting backbone generation for sequence {sequence_num} …")
    create_eu_regional_xml(seq_dir, metadata, sequence_num)
    create_index_xml(seq_dir, metadata, base, seq_int)
    write_index_md5(seq_dir, seq_dir / "index.xml")
    print("✅ All XML backbones created and validated!")

if __name__ == "__main__":
    import argparse

    ap = argparse.ArgumentParser(
        description=(
            "Generate eCTD XML backbones from Excel metadata (metadata-<seq>.xlsx). "
            "Excel columns must include file_path, title, operation, modified-leaf, modified-href "
            "plus EU envelope fields (eu_country, eu_identifier, eu_submission_type, eu_submission_mode, "
            "eu_submission_number, eu_procedure_number, eu_submission_unit_type, eu_agency_code, "
            "eu_procedure_type, eu_invented_name, eu_inn, eu_related_sequence)."
        )
    )
    ap.add_argument("base_directory")
    ap.add_argument("sequence_number")
    ap.add_argument(
        "-scan",
        action="store_true",
        help="Scan sequence folder and append missing file_path rows to metadata-<seq>.xlsx.",
    )
    args = ap.parse_args()
    main(args.base_directory, args.sequence_number, scan=args.scan)
