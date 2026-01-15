# eCTD Tools

Utilities for generating and viewing eCTD backbone XMLs from Excel metadata.

## Tools

### `ectd-tool.py`
Command-line tool to generate `index.xml` (ICH 3.2) and `m1/eu/eu-regional.xml` (EU M1 3.1).

#### Usage
```bash
python ectd-tool.py <base_directory> <sequence_number> [-scan] [-extractXML] [-mapfile PATH] [-eu_mapfile PATH]
```

#### Metadata (metadata-<seq>.xlsx)
- Required columns: `file_path`, `title`, `operation`, `modified-leaf`, `modified-href`
- Optional columns: `ctd_toc` (override target XML element tag for a file)
- EU envelope fields: `eu_country`, `eu_identifier`, `eu_submission_type`, `eu_submission_mode`,
  `eu_submission_number`, `eu_procedure_number`, `eu_submission_unit_type`, `eu_agency_code`,
  `eu_procedure_type`, `eu_invented_name`, `eu_inn`, `eu_related_sequence`

#### Mapping files
- `-mapfile` (default: `ectd_3_2_2_path_to_xml_element_mapping.csv`)
  - CSV columns: `item_type` (`directory|file`), `relative_path`, `xml_element`
- `-eu_mapfile` (default: `ectd_eu_m1_path_to_xml_element_mapping.csv`)
  - CSV columns: `item_type` (`directory|file`), `relative_path`, `xml_element`

#### Extract from XML
`-extractXML` fills missing metadata cells from `index.xml` and `m1/eu/eu-regional.xml` without
overwriting existing values.

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
