# eCTD-Tool — Rust

Separates Kommandozeilenprogramm zum Erzeugen und Extrahieren von eCTD-Backbones.
Der Desktop-Viewer heißt `ectd-viewer`. Beide Programme verwenden die Version
aus `Cargo.toml` (derzeit 0.1.0). Auf dem Zielrechner werden weder Excel noch
LibreOffice, eine Rust-Toolchain oder eine externe XML-Bibliothek benötigt.

## Aufruf

```bash
cargo run --bin ectd-tool -- /pfad/zum/dossier 0001
cargo run --bin ectd-tool -- /pfad/zum/dossier 0001 -scan
cargo run --bin ectd-tool -- /pfad/zum/dossier 0001 -extractXML
cargo run --bin ectd-tool -- --help
```

Verfügbare Optionen:

| Option | Wirkung |
| --- | --- |
| `-scan` / `--scan` | Dateibaum scannen und fehlende Dokumentzeilen in Excel ergänzen; keine XML-Erzeugung |
| `-extractXML` / `--extract-xml` | Metadaten aus `index.xml` und referenzierten XMLs nach Excel übertragen |
| `-metadata PATH` / `--metadata PATH` | Andere Metadatendatei; relative Pfade gelten zum Basisordner |
| `-mapfile PATH` / `--mapfile PATH` | Eigene ICH-CSV für Pfad-zu-XML-Element-Zuordnung |
| `-eu_mapfile PATH` / `--eu-mapfile PATH` | Eigene EU-M1-CSV |
| `-version` / `--version` | Versionsnummer ausgeben |
| `--yes` / `-y` | Bestehende Ausgabedateien ohne interaktive Rückfrage überschreiben |

Ohne `-metadata` wird `<basisordner>/metadata-<sequenz>.xlsx` verwendet.
Wenn beispielsweise `1` angegeben wird und nur der Ordner `0001` existiert,
verwendet das Tool `0001` und `metadata-0001.xlsx`.

Bestehende Dateien werden nur nach Bestätigung überschrieben. In Skripten oder
umgeleiteten Terminals ohne interaktive Eingabe muss dafür `--yes` angegeben werden.
Ein abgelehntes Überschreiben beendet den Aufruf mit einem Fehlerstatus.

## Metadaten und Scan

Das Tool bevorzugt ein Tabellenblatt namens `metadata`, andernfalls das aktive Blatt.
Die Vorlage [resources/metadata-0001.xlsx](resources/metadata-0001.xlsx) kann in den
Dossierordner kopiert und für die jeweilige Sequenz angepasst werden.
Erwartete Spalten:

```text
file_path, title, attributes, operation, modified-leaf, modified-href, ctd_toc,
applicant_name, submission_type, sequence_description, eu_country, eu_identifier,
eu_submission_type, eu_submission_mode, eu_submission_number, eu_procedure_number,
eu_submission_unit_type, eu_agency_code, eu_procedure_type, eu_invented_name,
eu_inn, eu_related_sequence
```

`CTD-TOC` und `ctd-toc` werden ebenfalls als TOC-Spalten gelesen.
`file_path` ist relativ zum Sequenzverzeichnis, beispielsweise
`m2/23-qos/summary.pdf`. Vorhandene Unterverzeichnisse und Pfade mit abschließendem
`/` werden als Verzeichniszeilen behandelt und erzeugen kein Leaf.
`attributes` enthält Attribute wie `substance="Example" manufacturer="Company"`;
zugelassen sind die in der jeweiligen DTD deklarierten Attribute.
Smart Quotes aus Excel werden unterstützt.

Verzeichnisattribute gelten für den entsprechenden Teilbaum. Explizite TOC-Zeilen
mit `ctd_toc` und leerem Dateipfad eröffnen einen Kontext für die nachfolgenden Zeilen,
beispielsweise für wiederholte Drug-Substance-Zweige. Dabei werden Attribute und
DTD-Reihenfolge berücksichtigt. Solche TOC-Zeilen bleiben beim Einlesen erhalten.

`-scan` ergänzt Dokumente aus Unterverzeichnissen, ohne bestehende Einträge zu
duplizieren. Dateien direkt im Sequenzwurzelverzeichnis, `util/` und der EU-Backbone
werden übersprungen. Neue Titel werden aus dem Dateinamen
abgeleitet. Vorhandene Spaltenreihenfolgen, zusätzliche Spalten und andere
Tabellenblätter bleiben erhalten. Neue Pfade werden unter die tatsächlich passende
Spaltenüberschrift geschrieben. Formeln werden nicht neu berechnet; gelesen werden
die gespeicherten Zellwerte.

## Backbone-Erzeugung

Die Standard-CSV-Mappings aus `resources/` sind in die ausführbare Datei eingebettet.
Sie müssen nicht neben der EXE liegen. Eine eigene Mapping-Datei benötigt die
Spalten `item_type`, `relative_path`, `xml_element`.
Spezifische Pfade haben Vorrang vor dynamischen Segmenten wie `substance-1-manufacturer-1`.

Erzeugt werden:

- `0001/index.xml` für Module 1–5 mit neuen eindeutigen Leaf-IDs und MD5-Prüfsummen.
- `0001/m1/eu/eu-regional.xml`, wenn EU-Dokumentzeilen vorhanden sind, einschließlich
  Envelope, Ländergruppen und Produktinformationen nach Sprache und Dokumenttyp.
- `0001/index-md5.txt` mit der Prüfsumme des tatsächlich geschriebenen `index.xml`.

Die EU-Envelope-Werte stammen aus der ersten Metadatenzeile. Nicht gesetzte Angaben
erhalten Standardwerte; Enum-Werte werden aus vorhandenen
DTDs übernommen. DOCTYPE und Stylesheet-Verweise werden mitgeschrieben.
DTD- und XSL-Dateien müssen im Dossier vorliegen; das Tool lädt sie nicht aus dem Internet.

Unterstützt werden `new`, `replace`, `append` und `delete`. Vorgänger werden zuerst
über eine explizite `modified-leaf`-ID gesucht, ansonsten anhand von `modified-href`
oder Dateipfaden, mit Dateinamen-Fallback. Berücksichtigt werden frühere Sequenzen
sowie deren referenzierte regionale XMLs. Bei mehreren passenden Leafs im letzten
passenden Index werden mehrere Lifecycle-Leafs erzeugt. Löschungen innerhalb expliziter
TOC-Kontexte werden anhand der Zweigattribute eingegrenzt.

`modified-file` wird relativ zur erzeugenden XML-Datei geschrieben, auch bei
regionalen Backbones. Löschungen benötigen keine neue Datei
und bekommen keinen `xlink:href`. Nicht gefundene Vorgänger und fehlende Dokumente
werden als Warnung ausgegeben; fehlende Dokumente haben eine leere Prüfsumme.

## DTD-Prüfung und Dateischutz

`util/dtd/ich-ectd-3-2.dtd` ist für die Erzeugung erforderlich. Ein vorhandenes
`util/dtd/eu-regional.dtd` wird ebenfalls geprüft; referenzierte lokale Module wie
`eu-envelope.mod` werden relativ zur DTD geladen. Fehlt die EU-DTD, wird deren
Validierung mit einer ausdrücklichen Warnung übersprungen.
Remote-DTD-Module werden nicht geladen.

Die Prüfung verwendet [xmloxide](https://github.com/jonwiggins/xmloxide) in Rust und
prüft unter anderem Elementreihenfolge, Vorkommen, Pflichtattribute, feste Werte,
Enums und eindeutige IDs. XML wird vor dem Schreiben im Speicher aufgebaut und
validiert. Bei einem Validierungsfehler bleiben vorhandene Backbones unverändert.
Einzelne Dateien werden über temporäre Dateien ersetzt; das gesamte Dateiset ist
keine gemeinsame Dateisystemtransaktion.

Excel-Dateien werden mit [umya-spreadsheet](https://github.com/MathNya/umya-spreadsheet)
gelesen und geschrieben. Benutzerdefinierte Excel-Erweiterungen, die diese Bibliothek
nicht unterstützt, sind nicht durch die Tests abgedeckt.

## XML nach Excel extrahieren

```bash
cargo run --bin ectd-tool -- /pfad/zum/dossier 0001 -extractXML -metadata extracted.xlsx
```

Die Extraktion schreibt Dokumentzeilen und Verzeichnis-/TOC-Zeilen für Zweigattribute
in der XML-Reihenfolge. Referenzierte regionale Inhalte erscheinen an der Stelle
ihres Verweises im Index. Auch verschachtelte XML-Verweise werden verfolgt;
Zyklen werden erkannt. Verweise auf XMLs außerhalb der aktuellen Sequenz werden
nicht als regionale Inhalte importiert. Andere Tabellenblätter bleiben erhalten.

EU-Envelope-Felder werden in die erste extrahierte Zeile übernommen. Lifecycle-Zeilen
enthalten `modified-leaf` und `modified-href`; bei Löschungen wird der frühere Dateipfad
aus dem Vorgänger ermittelt. Der ausgelesene TOC-Tag steht in `ctd_toc`.

## Ausführbare Dateien

Linux:

```bash
cargo build --release --locked --bin ectd-tool
./target/release/ectd-tool /pfad/zum/dossier 0001 -scan
```

Windows, direkt unter Windows gebaut:

```powershell
cargo build --release --locked --bin ectd-tool
.\target\release\ectd-tool.exe C:\Dossier 0001 -scan
```

Windows-x86-64-Crossbuild unter Linux mit MinGW:

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64
cargo rustc --release --locked --target x86_64-pc-windows-gnu --bin ectd-tool -- -C link-arg=-static
```

Ausgabe: `target/x86_64-pc-windows-gnu/release/ectd-tool.exe`.
Der GitHub-Actions-Workflow baut und testet den Generator zusätzlich direkt unter Windows.

## Tests

```bash
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
```

Die Tool-Tests prüfen Scan und Spaltenzuordnung, die mitgelieferte
Metadatendatei, ICH/EU-Erzeugung, Prüfsummen, Envelope, Vorgänger, regionale Verweise,
TOC-Kontexte, XML-Extraktion, eigene Mappings, DTD-Fehler und Überschreibschutz.
Die DTD-Dateien unter `tests/fixtures/tool` sind synthetische Testausschnitte,
keine Produktions-DTDs.
