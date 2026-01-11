## Purpose
This repo contains utilities to generate and validate eCTD (ICH 3.2.2) and EU M1 (3.1.1) XML backbones from CSV metadata. The primary scripts are `ectd.py` and `ectd_fixed.py` which are CLI tools that expect a base directory and a four-digit sequence (e.g. `0006`).

**Key files**
- [ectd_fixed.py](ectd_fixed.py#L1): preferred, more robust implementation (leaf lookup, delete handling).
- [ectd.py](ectd.py#L1): original implementation to reference for intent.
- `util/dtd/` and `util/style/`: contains DTDs and XSLs used for validation and output formatting.
- Example submission tree: `Alpro500 eCTD/0006/` — contains `index.xml`, `m1/`, `m3/`, and `util/` directories.

## How the tool is organized (big picture)
- Input: a metadata CSV named `metadata-<sequence>.csv` placed in the base dir alongside sequence folders.
- Output: writes `index.xml` into the sequence dir and `m1/eu/eu-regional.xml` for EU metadata.
- Validation: uses `lxml.etree.DTD` to validate generated XML against DTD files under `util/dtd/`.
- Sequence handling: scripts infer references to previous sequences by scanning prior `index.xml` files (see `find_previous_leaf_id`).

## Important patterns and behaviors agents must preserve
- CSV expectations: rows must contain `file_path`; optional columns handled include `operation` (new/replace/delete/append), `title`, and various `modified-*` column variants (e.g. `modified-href`, `modified_file_path`). Agents should follow `ectd_fixed.py`'s tolerant lookup logic when inferring `modified-leaf`.
- Replace/delete logic: when operation is `delete` the fixed script uses the MD5 for empty content (`d41d8cd98f00b204e9800998ecf8427e`) rather than reading a missing file.
- Href normalization: path comparison is normalized (slashes unified) before matching previous leaves — preserve `_norm_href` style normalization.
- Validation is DTD-based using `lxml`; do not change to a different validator without explicit reason.

## Developer workflows and commands
- Virtualenv is included at `ectd-venv/`. Typical workflow:

```bash
source ectd-venv/bin/activate
python ectd_fixed.py ./"Alpro500 eCTD" 0006
```

- If DTDs are missing for EU regional validation, `ectd_fixed.py` skips that validation and prints a warning (see eu-regional DTD lookup logic).

## Concrete examples to reference
- CSV name pattern: `metadata-0006.csv` (located at repo root alongside `Alpro500 eCTD/`).
- Example code locations:
  - `create_index_xml` and DTD call: [ectd_fixed.py](ectd_fixed.py#L1).
  - Previous-leaf inference and normalization: [ectd_fixed.py](ectd_fixed.py#L1).

## Conventions and style
- Use `pathlib.Path` for filesystem operations.
- Prefer small, testable functions (utilities live at top of the main scripts).
- Error handling: scripts raise `FileNotFoundError` / `ValueError` for missing CSV or required columns; printing warnings is acceptable for recoverable issues.

## Integration points / extensibility
- External dependency: `lxml` (used for XML handling and DTD validation). Any change to XML handling should keep `lxml` or provide explicit migration notes.
- DTDs/XSLs live under each sequence `util/dtd/` and `util/style/` — changing validation requires updating files in those folders or referencing a new path.

## What to avoid changing without discussion
- Algorithms that infer `modified-leaf` ids and checksum semantics — these are sensitive to backward compatibility with previous sequence `index.xml` files.
- The CSV-to-leaf mapping and the `operation` semantics (new/replace/delete/append).

## If you need more detail
- Ask for examples of specific CSV rows or a sample sequence to inspect (e.g. `Alpro500 eCTD/0006/`).

---
Please review and tell me which sections need more examples or line-level pointers; I can iterate the file accordingly.