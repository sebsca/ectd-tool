# eCTD Tools

Utilities for generating and viewing eCTD backbone XMLs from Excel metadata.

## Tools

### `ectd-tool.py`
Command-line tool to generate `index.xml` (ICH 3.2) and `m1/eu/eu-regional.xml` (EU M1 3.1).

#### Usage
```bash
python ectd-tool.py <base_directory> <sequence_number> [-scan] [-extractXML] [-metadata PATH] [-mapfile PATH] [-eu_mapfile PATH]
```

#### Metadata (metadata-\<seq>.xlsx)
- By default, the tool expects an Excel workbook named `metadata-<seq>.xlsx` in the base directory,
  with a `metadata` sheet as the first/active sheet.
- Use `-metadata PATH` to override the metadata file path or filename (relative paths are resolved
  against the base directory).
- `file_path` values are relative to the sequence directory (for example: `m1/12/cover.pdf`).
- Required columns: `file_path`, `title`, `operation`, `modified-leaf`, `modified-href`
- Optional columns: `ctd_toc` (override target XML element tag for a file),
  `attributes` (backbone element attributes in `key="value"` form)
- EU envelope fields: `applicant_name`, `submission_type`, `sequence_description`,
  `eu_country`, `eu_identifier`, `eu_submission_type`, `eu_submission_mode`,
  `eu_submission_number`, `eu_procedure_number`, `eu_submission_unit_type`, `eu_agency_code`,
  `eu_procedure_type`, `eu_invented_name`, `eu_inn`, `eu_related_sequence`
- Directory rows: set `file_path` to a folder path to apply `attributes` to the backbone element
  for that subtree without creating a leaf (all child files are grouped under that element).
- You can generate the file with `-scan` and extract metadata from backbone XMLs with `-extractXML`.

#### Mapping files
- `-mapfile` (default: `ectd_3_2_2_path_to_xml_element_mapping.csv`)
  - CSV columns: `item_type` (`directory|file`), `relative_path`, `xml_element`
- `-eu_mapfile` (default: `ectd_eu_m1_path_to_xml_element_mapping.csv`)
  - CSV columns: `item_type` (`directory|file`), `relative_path`, `xml_element`

#### Extract from XML
`-extractXML` reads `index.xml` and any referenced regional XML files (for example
`m1/eu/eu-regional.xml`) and transfers the information into the metadata Excel file
(default: `metadata-<seq>.xlsx`, or the path provided by `-metadata`).
It creates rows for every leaf and for every branch that has attributes, and preserves
the XML order (regional XML content appears where it is referenced in `index.xml`).

### `ectd-viewer.py`
GUI viewer for eCTD dossiers (PySide6).

#### Usage
```bash
python ectd-viewer.py
```

#### Features
- Filesystem tree or CTD-TOC tree view
- Metadata panel and raw XML view per leaf
- Optional display of backbone XML files in Documents
