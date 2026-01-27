# eCTD Project Guidelines for AI Agents

## Purpose
This repo contains utilities to generate, validate, and inspect **eCTD (ICH 3.2.2) and EU M1 (3.1.1) XML backbones** from Excel metadata. The primary script generates XML from metadata workbooks, and the viewer inspects/debugs existing dossiers.

---

## Disclaimer and MIT License
THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

MIT License

Copyright (c) 2025 The eCTD Project Authors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

---

## Key Files and Their Roles

| File | Purpose |
|------|---------|
| `ectd-tool.py` | **Primary** generator: generates `index.xml` and `eu-regional.xml` from Excel metadata |
| `ectd-viewer.py` | **Desktop GUI** (PySide6): inspect, navigate, validate eCTD dossiers |
| `metadata-<sequence>.xlsx` | Input Excel workbook with file paths, operations, metadata for sequence |
| `ectd_3_2_2_path_to_xml_element_mapping.csv` | CTD mapping: relative paths → ICH XML elements |
| `ectd_eu_m1_path_to_xml_element_mapping.csv` | EU M1 mapping: relative paths → EU XML elements |
| `util/dtd/` | DTD files for validation (lxml.etree.DTD) |
| `util/style/` | XSL stylesheets for output formatting |

---

## How the Tools Work Together

### 1. **Generator Workflow** (`ectd-tool.py`)
```bash
source ectd-venv/bin/activate
python ectd-tool.py ./Alpro500\ eCTD 0006 [-scan] [-extractXML] [-mapfile PATH] [-eu_mapfile PATH]
```

**Input:**
- Metadata Excel: `metadata-0006.xlsx` with `metadata` sheet (columns: `file_path`, `operation`, `title`, `modified-leaf`, `modified-href`, `ctd_toc`, `attributes`, EU envelope fields)
- Mapping CSVs: `ectd_3_2_2_path_to_xml_element_mapping.csv` (ICH), `ectd_eu_m1_path_to_xml_element_mapping.csv` (EU)
- Previous sequences: scanned for `index.xml` to resolve `modified-leaf` references

**Output:**
- `<dossier_dir>/<seq>/index.xml` (main ICH 3.2 backbone)
- `<dossier_dir>/<seq>/m1/eu/eu-regional.xml` (EU M1 3.1 regional metadata, if EU rows present)

**Key behaviors:**
- Excel input: openpyxl-based, flexible column handling
- Operation semantics: `new` / `replace` / `delete` / `append`
- Delete handling: uses MD5 of empty content (`d41d8cd98f00b204e9800998ecf8427e`) if file missing
- Reference resolution: searches both explicit IDs (`modified-leaf`) and href patterns across previous sequence XMLs
- Mapping-driven hierarchy: `ctd_toc` override or automatic path-to-element mapping
- Directory attributes: rows with directory paths + `attributes` column apply XML attributes without creating leaves
- EU envelope: auto-populated from metadata fields (`eu_country`, `eu_submission_type`, etc.)
- DTD validation: via `lxml.etree.DTD` against `util/dtd/` files
- XSL output: writes with xml-stylesheet PI and DOCTYPE

### 2. **Viewer Workflow** (`ectd-viewer.py`)
```bash
source ectd-venv/bin/activate
python ectd-viewer.py
```
Then select a dossier directory (e.g., `Alpro500 eCTD/`).

**Features:**
- **Lazy tree loading**: Root + sequences built immediately; documents loaded per-sequence on expand
- **Dual views** per sequence:
  - **Filesystem tree**: shows actual folder structure (resolved xlink:href paths)
  - **CTD-TOC tree**: shows XML ancestry hierarchy from backbone structure
- **Consolidated vs. Delta**:
  - Delta (default): shows changes per sequence (lifecycle operations visible)
  - Consolidated: shows cumulative/active documents across all prior sequences
- **Metadata inspection**:
  - Overview tab: computed fields (path, existence, status)
  - Metadata tab: flattened Key/Value list from entire `<leaf>` subtree
  - XML tab: raw `<leaf>` element
  - Versions tab: ancestry chain (modified-file history)
- **QA highlighting**:
  - Red: missing files
  - Yellow: broken modified-file links
  - Gray (delta view only): deleted items
- **Context menu & actions**:
  - Open file / show parent folder
  - Copy path / href / modified-file / leaf ID
  - Jump to predecessor via modified-file

---Excel Metadata Expectations
- **Required columns**: `file_path`, `operation`, `title`
- **Optional columns**: 
  - `modified-leaf` (explicit leaf ID from prior sequence)
  - `modified-href` (explicit href from prior sequence)
  - `ctd_toc` or `ctd-toc` (override CTD-TOC element tag)
  - `attributes` (space-separated `key="value"` pairs for branch elements)
  - EU envelope fields: `applicant_name`, `submission_type`, `sequence_description`, `eu_country`, `eu_identifier`, `eu_submission_type`, `eu_submission_mode`, `eu_submission_number`, `eu_procedure_number`, `eu_submission_unit_type`, `eu_agency_code`, `eu_procedure_type`, `eu_invented_name`, `eu_inn`, `eu_related_sequence`
- **Special rows**: set `file_path` to directory path (ends with `/`) to apply `attributes` without creating leaves
- **Sheet selection**: looks for sheet named `metadata`, falls back to active sheet
  - `operation` (new/replace/delete/append; default: `new`)
  - `title`, `Title`
  - `modified-href/Append Lifecycle
- **new**: creates fresh leaf in sequence
- **replace**: link to predecessor via `modified-file`, removes predecessor from future consolidation
- **delete**: link to predecessor via `modified-file`, leaf marked `operation="delete"` with empty checksum
- **append**: link to predecessor via `modified-file`, adds supplementary leaf to prior document
- **modified-leaf semantics**: 
  - Explicit leaf ID lookup: searches prior sequences' `index.xml` files (and referenced regional XMLs) for exact match
  - Format: leaf ID string without `#` prefix
- **modified-href semantics**: 
  - Href pattern lookup: searches prior sequences for matching file hrefs
  - Fallback to basename match if exact href not found via `PathNormalizer.normalize()`
- Helper: `_norm_href()` wrapper for comparison
- File path matching: exact href match first, then basename fallback
- Must preserve this to match previous leaf IDs correctly and resolve replace/delete/append operationsfId`
  - Points to prior sequence's `<leaf>` element with specific IDbase document (CTD-TOC view groups these)
- **modified-file semantics**: 
  - Format: `path/to/index.xml#leafId`
  - Points to prior sequence's `<leaf>` element
  - Critical for backward compatibility with existing `index.xml` files

### Href & Path NICH 3.2 root backbone (sequence-specific, always generated)
- **m1/eu/eu-regional.xml**: EU M1 3.1 regional backbone (generated only if EU rows exist in metadata)
- Auto-discovery: viewer scans `m1/**/` for any regional XML files
- Separation: ICH index contains references to regional XMLs via leaf `@xlink:href`; regional XMLs contain EU-specific structure
- Must preserve this to match previous leaf IDs correctly

### Validation
- **Method**: DTD-based via `lxml.etree.DTD`
- **EU Regional**: may skip if DTDs missing under `util/dtd/` (warns, doesn't error)
- **No change**: unless you have explicit reason to replace with different validator

### Backbone Structure
- **index.xml**: root backbone (sequence-specific)
- **m1/\*/\*-regional.xml**: regional variants (EU, etc.)
- Auto-discovery: viewer scans `m1/**/` for any regional files

---

## Data Structures (Viewer)

### `BackboneFile`
```python
sequence: str                    # e.g., "0006"
xml_path: Path                   # full path to index.xml or regional XML
kind: str                        # "index", "regional", "other"
region: Optional[str]            # e.g., "eu" for regional files
ns_prefix_to_uri: Dict           # namespace mappings
root_tag: str                    # e.g., "backbone"
leaf_count: int                  # count of <leaf> elements parsed
parse_error: Optional[str]       # if XML parsing failed
```

### `LeafRecord`
```python
sequence: str
backbone: BackboneFile           # reference to parent backbone
leaf_id: Optional[str]           # @ID attribute
operation: str                   # "new", "replace", "delete", "append"
title: str                       # @title attribute
href: Optional[str]              # xlink:href or @href
abs_file_path: Optional[Path]    # resolved file location
modified_file: Optional[str]     # modified-file reference (if replace/append/delete)
prev_ref: Tuple[Path, str]       # parsed modified-file: (xml_path, leaf_id_fragment)
prev_leaf: Optional[LeafRecord]  # resolved predecessor via leaf_index
toc_path: List[str]              # ancestor labels for TOC tree
missing_file: bool               # file exists?
warnings: List[str]              # QA issues
```

---

## Conventions & Style

### Code
- Use `pathlib.Path` for filesystem operations
- Small, testable utility functions (top of file)
- Error handling: raise `FileNotFoundError` / `ValueError` for missing CSV or columns
- Warnings printed for recoverable issues
- XML via `lxml` or `xml.etree.ElementTree`; iterparse for streaming large files

### GUI (Viewer)
- PySide6/Qt6 (cross-platform)
- Custom roles (ROLE_NODEINFO, ROLE_LEAF, ROLE_BACKBONE) for storing metadata on QStandardItem
- Lazy loading: chilookup & checksum semantics**  
   → Breaking change risk: older `index.xml` files won't link correctly
   → Search strategy: explicit ID lookup first, then href matching in prior XMLs

2. **Excel-to-leaf mapping & operation semantics**  
   → Breaking change: workflows depend on new/replace/delete/append meanings
   → Href resolution: attempts direct match, falls back to basename match

3. **Href normalization logic** (PathNormalizer.normalize)  
   → Must preserve exact path comparison algorithm to match previous leaves

4. **Path-to-element mapping files** (`ectd_3_2_2_path_to_xml_element_mapping.csv`, `ectd_eu_m1_path_to_xml_element_mapping.csv`)  
   → Used for CTD hierarchy; changes affect all generated backbones
   → Format: `item_type`, `relative_path`, `xml_element` columns

5. **XML validation method** (currently DTD-based via lxml)  
   → Use explicit migration notes if you switch validators

6. **EU envelope field mapping**  
   → Affects EU M1 generation; preserve all field names and validation enums

2. **CSV-to-leExcel reading/writing for metadata workbooks)
- `PySide6` (GUI, viewer only)

### File Structure
- DTDs: `util/dtd/` (shared; must include `ich-ectd-3-2.dtd`, `eu-regional.dtd`, `eu-envelope.mod`)
- XSLs: `util/style/` (shared; must include `ectd-2-0.xsl`, `eu-regional.xsl`)
- Mapping CSVs: `ectd_3_2_2_path_to_xml_element_mapping.csv`, `ectd_eu_m1_path_to_xml_element_mapping.csv` at repo root
- Metadata Excel: `metadata-<seq>.xlsx` at dossier root
- Sequences: directories matching pattern `\d{4}` (0000–9999) or unpadded `<seq>`

5. **Consolidated-cache algorithm**  
   → Affects how multi-sequence dossiers appear; preserve active-set incrementality

---

## Integration Points & Dependencies

### External Libraries
- `lxml` (XML parsing, DTD validation)
- `openpyxl` (if Excel support needed; currently not used in viewer)
- `PySide6` (GUI, viewer only)
Generator Command (Basic)
```bash
python ectd-tool.py ./Alpro500\ eCTD 0006
```
Reads `./Alpro500 eCTD/metadata-0006.xlsx`, generates `./Alpro500 eCTD/0006/index.xml` and optionally `./Alpro500 eCTD/0006/m1/eu/eu-regional.xml`.

### Generator Command (With Scan)
```bash
python ectd-tool.py ./Alpro500\ eCTD 0006 -scan
```
Scans `./Alpro500 eCTD/0006/` directory structure, appends missing rows to `metadata-0006.xlsx`.

### Generator Command (Extract From XML)
```bash
python ectd-tool.py ./Alpro500\ eCTD 0006 -extractXML
```
Reads existing `index.xml` and referenced regional XMLs, updates `metadata-0006.xlsx` with extracted leaf data.

### Excel Row Format (Minimal)
| file_path | title | operation |
|-----------|-------|-----------|
| m3/quality/documents/tox-study-01.pdf | Toxicology Study | new |
| m3/safety/cv/investigator.pdf | Investigator CV | new |

### Excel Row Format (With Lifecycle)
| file_path | title | operation | modified-leaf |
|-----------|-------|-----------|----------------|
| m3/quality/documents/tox-study-01-v2.pdf | Toxicology Study (Updated) | replace | LEAF-TOX-001 |
| m3/quality/summary.xml | Additional Summary | append | LEAF-TOX-SUMMARY |

### Excel Row Format (With Directory Attributes)
| file_path | title | operation | attributes |
|-----------|-------|-----------|------------|
| m3/quality/ | Quality | new | some_attr="value" |
 or `modified-leaf` lookup? (requires backward-compat review)
- [ ] Does this change how `operation` is interpreted? (check existing Excel workbooks)
- [ ] Does this touch href normalization (`PathNormalizer.normalize`)? (test against real paths with backslashes & spaces)
- [ ] Does this modify DTD validation or structure parsing? (update `util/dtd/` or note externally)
- [ ] If new Excel column: is it gracefully handled if missing?
- [ ] Does this change path-to-element mapping logic? (test with custom mapping files)
- [ ] Did you test with a real multi-sequence dossier (0000→0001→0006)?
- [ ] Does `-scan` still discover files correctly?
- [ ] Does `-extractXML` preserve order and extract all metadata?
- [ ] Does EU regional XML generation work when EU rows are presentitle
m3/quality/documents/tox-study-01.pdf,Toxicology Study
m3/safety/cv/investigator.pdf,Investigator CV
```

### CSV Row Format (With Lifecycle)
```csv
file_path,operation,modified-file,title
m3/quality/documents/tox-study-01-v2.pdf,replace,0005/index.xml#LEAF-TOX-001,Toxicology Study (Updated)
m3/quality/summary.xml,append,0005/index.xml#LEAF-TOX-SUMMARY,Additional Summary
```

### Checking Consolidated vs. Delta
```python
delta_leaves = dossier.get_seq_delta("0006")      # All ops including delete
consolidated = dossier.get_consolidated("0006")   # Only active docs
```

---

## Developer Checklist

When adding features or fixing bugs:

- [ ] Does this affect `modified-file` resolution? (requires backward-compat review)
- [ ] Does this change how `operation` is interpreted? (check existing CSVs)
- [ ] Does this touch href normalization? (test against real paths with backslashes & spaces)
- [ ] Does this modify DTD validation? (update `util/dtd/` or note externally)
- [ ] If new CSV column: is it gracefully handled if missing?
- [ ] Did you test with a real multi-sequence dossier (0000→0001→0006)?
- [ ] Does the viewer still lazy-load correctly?
- [ ] Do missing files still highlight red? Broken links yellow?

---

## Questions or Changes?

Ask for:
- Specific CSV row examples
- Sample sequence directory structure
- Line-by-line code review of critical sections
- Clarification on backward compatibility

This document can be updated iteratively; please let me know what sections need more examples or detail.
