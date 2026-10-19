# Gemeinsame Ressourcen

- `ectd_3_2_2_path_to_xml_element_mapping.csv`: ICH-Zuordnung von Dateipfaden zu XML-Elementen.
- `ectd_eu_m1_path_to_xml_element_mapping.csv`: EU-M1-Zuordnung von Dateipfaden zu XML-Elementen.
- `metadata-0001.xlsx`: Excel-Vorlage für die Metadaten einer Sequenz.

Der Generator bettet beide CSV-Dateien beim Kompilieren über `include_str!` ein.
Nach Änderungen an den Standard-Mappings muss das Programm neu gebaut werden.
Eigene CSV-Dateien können zur Laufzeit über `-mapfile` und `-eu_mapfile` geladen werden.
Die CSV-Spalten heißen `item_type`, `relative_path` und `xml_element`.

Die Excel-Vorlage in den Dossierordner kopieren, gegebenenfalls in
`metadata-<sequenz>.xlsx` umbenennen und vor der Erzeugung an das Dossier anpassen.
Alternativ mit `-metadata PATH` eine andere Datei auswählen oder mit `-scan` eine
neue Metadatendatei aus den Dokumentpfaden erzeugen.

DTD- und XSL-Dateien werden aus dem jeweiligen Dossier gelesen und sind hier nicht
enthalten. Die synthetischen DTD-Dateien unter `tests/fixtures/tool/` dienen nur den Tests.
Weitere Informationen stehen in [TOOL.md](../TOOL.md).
