eCTD-Tool

Command-line tool to generate eCTD XML backbones from Excel metadata.

Usage
python ectd-tool.py <base_directory> <sequence_number> [-scan] [-extractXML] [-mapfile PATH] [-eu_mapfile PATH]

Metadata columns (metadata-<seq>.xlsx)
Required: file_path, title, operation, modified-leaf, modified-href
Optional: ctd_toc (override target XML element tag for a file)
EU envelope fields: eu_country, eu_identifier, eu_submission_type, eu_submission_mode,
eu_submission_number, eu_procedure_number, eu_submission_unit_type, eu_agency_code,
eu_procedure_type, eu_invented_name, eu_inn, eu_related_sequence

Mapping file (-mapfile)
CSV columns: item_type (directory|file), relative_path, xml_element
If not provided, ectd_3_2_2_path_to_xml_element_mapping.csv is used.

EU mapping file (-eu_mapfile)
CSV columns: item_type (directory|file), relative_path, xml_element
If not provided, ectd_eu_m1_path_to_xml_element_mapping.csv is used.

Extract from XML (-extractXML)
Fills missing metadata cells in metadata-<seq>.xlsx using index.xml and eu-regional.xml
without overwriting existing values.
