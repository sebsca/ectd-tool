# eCTD Project Guidelines for AI Agents

## Purpose
This repo contains utilities to generate, validate, and inspect **eCTD (ICH 3.2.2) and EU M1 (3.1.1) XML backbones** from CSV metadata. The primary scripts generate XML, and the viewer inspects/debugs existing dossiers.

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
| `ectd_fixed.py` | **Preferred** generator: generates `index.xml` from CSV, robust leaf lookup & delete handling |
| `ectd.py` | Original generator (reference only) |
| `ectd-viewer.py` | **Desktop GUI** (PySide6): inspect, navigate, validate eCTD dossiers |
| `metadata-<sequence>.csv` | Input CSV with file paths, operations, metadata for sequence |
| `util/dtd/` | DTD files for validation (lxml.etree.DTD) |
| `util/style/` | XSL stylesheets for output formatting |

---

## How the Tools Work Together

### 1. **Generator Workflow** (`ectd_fixed.py`)
```bash
source ectd-venv/bin/activate
python ectd_fixed.py ./Alpro500\ eCTD 0006
```

**Input:**
- Metadata CSV: `metadata-0006.csv` (columns: `file_path`, `operation`, `title`, `modified-*` variants)
- Previous sequences: scanned for `index.xml` to resolve `modified-file` references

**Output:**
- `<dossier_dir>/0006/index.xml` (main backbone)
- `<dossier_dir>/0006/m1/eu/eu-regional.xml` (EU regional metadata)

**Key behaviors:**
- CSV tolerant: missing optional columns handled gracefully
- Operation semantics: `new` / `replace` / `append` / `delete`
- Delete handling: uses MD5 of empty content (`d41d8cd98f00b204e9800998ecf8427e`) if file missing
- Reference resolution: scans prior `index.xml` files to infer `modified-leaf` IDs
- Href normalization: unified path separators before matching previous leaves
- DTD validation: via `lxml.etree.DTD` against `util/dtd/` files

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

---

## Important Patterns & Behaviors to Preserve

### CSV Expectations
- **Required column**: `file_path`
- **Optional columns**: 
  - `operation` (new/replace/delete/append; default: `new`)
  - `title`, `Title`
  - `modified-href`, `modified-file`, `modified_file_path`
  - Any `modified-*` variant (generator uses tolerant lookup)

### Replace/Delete Lifecycle
- **replace**: removes predecessor from consolidated view, adds new version
- **delete**: removes from consolidated view; in delta view, shows grayed-out and opens predecessor on double-click
- **append**: adds supplementary content under base document (CTD-TOC view groups these)
- **modified-file semantics**: 
  - Format: `path/to/index.xml#leafId`
  - Points to prior sequence's `<leaf>` element
  - Critical for backward compatibility with existing `index.xml` files

### Href & Path Normalization
- All path comparisons normalize slashes (Windows `\` → Unix `/`)
- Helper: `_norm_href()` style normalization
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
- Lazy loading: children only populated on expand
- Proxy model for filtering/search

### Performance
- Iterparse streaming for XML (keeps memory low)
- Lazy tree population per sequence
- Consolidated cache built once on load (incremental via operation semantics)

---

## What NOT to Change Without Discussion

1. **Modified-leaf inference & checksum semantics**  
   → Breaking change risk: older `index.xml` files won't link correctly

2. **CSV-to-leaf mapping & operation semantics**  
   → Breaking change: workflows depend on new/replace/delete/append meanings

3. **Href normalization logic**  
   → Must preserve exact path comparison algorithm

4. **XML validation method** (currently DTD-based via lxml)  
   → Use explicit migration notes if you switch validators

5. **Consolidated-cache algorithm**  
   → Affects how multi-sequence dossiers appear; preserve active-set incrementality

---

## Integration Points & Dependencies

### External Libraries
- `lxml` (XML parsing, DTD validation)
- `openpyxl` (if Excel support needed; currently not used in viewer)
- `PySide6` (GUI, viewer only)

### File Structure
- DTDs: `<seq_dir>/util/dtd/` (or shared location)
- XSLs: `<seq_dir>/util/style/`
- Metadata CSVs: `metadata-<seq>.csv` at dossier root
- Sequences: directories matching pattern `\d{4}` (0000–9999)

### Virtual Environment
- Located at `ectd-venv/`
- Standard Python 3.13+ (check `pyvenv.cfg`)
- Install: `python -m venv ectd-venv && source ectd-venv/bin/activate && pip install -r requirements.txt`

---

## Concrete Examples

### Scanning a Dossier
```python
dossier = EctdDossier(Path("./Alpro500 eCTD"))
dossier.load()
# Now: dossier.sequences = ["0000", "0001", "0006"]
# And: dossier.leaves_by_seq["0006"] = [LeafRecord, LeafRecord, ...]
```

### CSV Row Format (Minimal)
```csv
file_path,title
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
