# eCTD Tools — Rust

Native Rust-Werkzeuge zum Anzeigen von eCTD-Dossiers und zum Erzeugen von
ICH-/EU-Backbone-XMLs aus Excel-Metadaten.
Der Viewer startet mit `cargo run`. Der Generator startet mit
`cargo run --bin ectd-tool -- <basisordner> <sequenz>` und unterstützt `-scan`,
`-extractXML`, Excel-Metadaten, ICH-/EU-Backbones und DTD-Prüfung.
Die vollständige Generator-Anleitung steht in [TOOL.md](TOOL.md).

Die Oberfläche verwendet [egui/eframe](https://github.com/emilk/egui), der XML-Reader
[quick-xml](https://github.com/tafia/quick-xml). Die Oberfläche ist englischsprachig.
Das Repository enthält die Rust-Implementierung, Tests und Beispieldaten.

## Starten

Die Release-Archive enthalten beide Programme, Dokumentation und Ressourcen.
Nach dem Entpacken den Viewer mit `./ectd-viewer` unter Linux oder mit
`ectd-viewer.exe` unter Windows starten. Die Generator-Aufrufe stehen in [TOOL.md](TOOL.md).

Für die Entwicklung eine aktuelle stabile Rust-Toolchain über [rustup](https://rustup.rs/) installieren.
`rust-toolchain.toml` wählt `stable` mit `rustfmt` und `clippy` aus.
Im Projektordner:

```bash
cargo run
```

Direkt mit einem Dossier oder dem mitgelieferten Beispiel starten:

```bash
cargo run -- /pfad/zum/dossier
cargo run -- examples/demo-dossier
```

In VS Code unter Linux die empfohlenen Erweiterungen **rust-analyzer** und **C/C++**
installieren. Der Debugger verwendet GDB über den Rust-Wrapper `rust-gdb`; falls GDB
fehlt, unter Ubuntu/Debian mit `sudo apt install gdb` installieren.
Mit **F5** startet **eCTD Viewer (Debug)** im Debugger; **Strg+F5** startet ihn ohne
Debugger. Cargo baut das Programm automatisch. **eCTD Viewer (Release)** verwendet
den optimierten Build. **eCTD Tool (Debug)** fragt nach Dossierordner und Sequenz und
startet den Generator. **Strg+Umschalt+B** baut beide Programme im Debug-Profil.
Die ausgewählte Startkonfiguration wird für F5 und Strg+F5 verwendet und baut das
zugehörige Profil vor jedem Start. Für Haltepunkte die Debug-Konfiguration auswählen;
der Release-Build ist optimiert und enthält standardmäßig keine Debug-Symbole.

Eine optimierte ausführbare Datei bauen:

```bash
cargo build --release --locked --bins
./target/release/ectd-viewer
```

Unter Windows heißt die Datei `target\release\ectd-viewer.exe`.

### Windows-EXE unter Linux bauen

Für eine Windows-x86-64-Datei unter Ubuntu/Debian werden die Rust-Zielbibliothek
und der MinGW-Crosscompiler benötigt:

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64
cargo rustc --release --locked --target x86_64-pc-windows-gnu --bin ectd-viewer -- -C link-arg=-static
```

Die ausführbare Datei liegt anschließend unter
`target/x86_64-pc-windows-gnu/release/ectd-viewer.exe`.
`-static` bindet die MinGW-Laufzeitbibliotheken ein; die normalen Windows-System-DLLs
werden weiterhin verwendet. Rust muss auf dem Zielrechner nicht installiert sein.
Das Windows-Ziel setzt Windows 10 oder neuer voraus
([Rust-Plattformunterstützung](https://doc.rust-lang.org/rustc/platform-support.html)).
Der GitHub-Actions-Workflow baut und testet beide Programme direkt unter Windows
und Linux. Die grafische Bedienung muss auf der jeweiligen Zielplattform manuell
geprüft werden.

Unter Linux werden ein C-Linker, `pkg-config`, eine grafische Sitzung (Wayland oder X11),
OpenGL und die üblichen Desktop-Bibliotheken benötigt. Unter Ubuntu/Debian beispielsweise:

```bash
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev libgl1-mesa-dev
```

Der native Ordnerdialog verwendet unter Linux `xdg-desktop-portal` und einen zur
Desktop-Umgebung passenden Portal-Backend. Alternativ lässt sich der Dossierpfad direkt
im Fenster eingeben. Unter Windows werden die C++ Build Tools benötigt, unter macOS
die Xcode Command Line Tools. macOS ist derzeit nicht Teil der CI-Matrix.

## Bedienung

1. **Open Dossier…** öffnet den übergeordneten Dossierordner mit den Sequenzverzeichnissen
   `0000`, `0001`, usw. Alternativ einen Pfad eingeben oder den Ordner ins Fenster ziehen.
2. Eine Sequenz aufklappen. **Backbones** zeigt `index.xml` und automatisch entdeckte
   regionale XML-Dateien unter `m1/**`, deren Dateiname „regional“ enthält.
3. **Documents** zeigt Dokumente mit **Files** als Dateibaum oder mit **CTD-TOC** nach XML-Struktur.
4. **Consolidated** zeigt den aktiven Dokumentbestand bis einschließlich der gewählten
   Sequenz. Ohne diese Option werden die Änderungen der jeweiligen Sequenz angezeigt.
5. Eine Zeile auswählen, um **Overview**, vollständige **Metadata**, **XML** oder
   die Vorgängerkette unter **Versions** anzusehen. Doppelklick öffnet ein Dokument
   in der Standardanwendung. PDFs werden extern geöffnet.

Weitere Funktionen:

- Suche ohne Beachtung der Groß-/Kleinschreibung über alle zwölf Spalten und Sequenzen,
  einschließlich noch nicht aufgeklappter Sequenzen. Treffer werden mit ihren Eltern angezeigt.
- Verschiebbare Trennlinie zwischen Baum und Inspector, veränderbare Spaltenbreiten
  und horizontaler Bildlauf. Die Standardansicht zeigt Name, Operation, Sequenz und
  Status; zusätzliche Spalten stehen im Menü **Display** zur Auswahl.
- **Expand** klappt die gewählte Sequenz vollständig auf; bei ausgewähltem Dossier
  werden alle Sequenzen aufgeklappt. **Collapse** klappt den Baum bis zur Sequenzliste
  zu und leert die Suche. Beide Buttons stehen direkt in der Werkzeugleiste.
- Beim Wechsel zwischen **Files** und **CTD-TOC** bleiben die Aufklappzustände der
  jeweiligen Struktur erhalten. Beim ersten Wechsel werden offene Dokumentbereiche
  übernommen. Ein ausgewähltes Dokument bleibt ausgewählt, sofern es in der Zielansicht vorhanden ist.
- Das Neuladen-Symbol im Kopfbereich lädt das aktuelle Dossier erneut.
- **Display → Show backbone XMLs in Documents** blendet zusätzlich Backbones, `util/`-Dateien
  und `index-md5.txt` im Dateibaum ein.
- Kontextmenü zum Öffnen, Öffnen des Vorgängers und Kopieren von Pfad, Href,
  modified-file oder Leaf-ID. Auch nicht referenzierte Dateien lassen sich öffnen.
- Fehlende Dateien erscheinen rot, defekte Verweise/Warnungen gelbbraun,
  gelöschte Einträge grau und nicht in XML referenzierte Dateien blau (`ORPHAN`).
  XML-Lesefehler bleiben im Backbone-Eintrag und der Statusleiste sichtbar.

## Lebenszyklus und Export

Die konsolidierte Ansicht verwendet diese Lebenszyklusregeln:

| Operation | Wirkung |
| --- | --- |
| `new` | Dokument zum aktiven Bestand hinzufügen |
| `replace` | Direkt referenzierten Vorgänger entfernen, neue Version hinzufügen |
| `append` | Vorgänger behalten und Ergänzung hinzufügen |
| `delete` | Direkt referenzierten Vorgänger entfernen; Lösch-Leaf nicht aktivieren |

In der konsolidierten CTD-TOC-Ansicht werden Ergänzungen unter **Append** beim aktiven
Vorgänger gruppiert. Bei einem Lösch-Leaf öffnet ein Doppelklick das Vorgängerdokument.
Pfade werden relativ zur jeweiligen Backbone-Datei aufgelöst; URL-kodierte Pfade,
Backslashes, regionale Referenzen und `#leafId` innerhalb derselben XML-Datei werden unterstützt.
Zyklen und nicht auflösbare Vorgänger werden gemeldet.

Für **Export Consolidated…** zuerst eine Sequenz oder einen Eintrag innerhalb dieser
Sequenz auswählen. Im Dialog den Ziel-Elternordner wählen; darin entsteht
`<sequenz>-consolidated`. Exportiert werden die aktiven Dokumente aller bisherigen
Sequenzen mit ihren Modulpfaden, ohne Backbone-XMLs. Namenskollisionen erhalten
Sequenzsuffixe, beispielsweise `report-0001.pdf` und `report-0001-2.pdf`.
Fehlende Dateien und Kopierfehler werden im Exportbericht aufgeführt.

Das Ziel muss neu oder leer sein und außerhalb der Quellsequenzen liegen.
Der Export erstellt eine Dokumentensammlung, **kein neues
einreichbares eCTD mit neu erzeugten XMLs**. Für die XML-Erzeugung aus Excel gibt es
das separate Rust-Programm [ectd-tool](TOOL.md).

## Ohne grafische Oberfläche

Dossier als JSON untersuchen, mit Backbones, Leafs, Metadaten, Vorgängern,
Warnungen und konsolidiertem Bestand pro Sequenz:

```bash
cargo run -- --inspect examples/demo-dossier
```

Einen konsolidierten Dokumentbestand exportieren:

```bash
cargo run -- --export examples/demo-dossier 0002 /tmp/ectd-demo-export
```

Bei unvollständigem Export endet der CLI-Befehl mit einem Fehlerstatus.
XML-Lesefehler werden bei `--inspect` als `parse_error` pro Backbone ausgegeben;
ein ladbares Dossier mit XML-Problemen kann weiterhin untersucht werden.

## Beispiel und Tests

`examples/demo-dossier` enthält drei synthetische Sequenzen, einen EU-Backbone,
alle vier Lebenszyklusoperationen, verschachtelte Metadaten und eine nicht
referenzierte Datei. Die Textdateien dienen der Demonstration; das Beispiel
ist keine regulatorische Einreichung.

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

Die Tests prüfen XML-Inhalte und Zeichenkodierungen, regionale TOC-Beschriftungen,
Lebenszyklen, fehlende Dateien, defekte Referenzen, Zyklen, Suche bei zugeklappten
Sequenzen, Append-Gruppierung, nicht referenzierte Dateien und Export-Kollisionen.

## Aufbau

| Datei | Aufgabe |
| --- | --- |
| `src/main.rs` | Desktop-Start und CLI |
| `src/app.rs` | Oberfläche, Hintergrundladen und Dateiaktionen |
| `src/app/theme.rs` | Farben, Typografie, Statuskennzeichnungen und Vektorsymbole |
| `src/dossier.rs` | Dossiermodell, Pfadauflösung, Versionen und Konsolidierung |
| `src/parser.rs` | Streaming-XML-Reader und vollständige Leaf-Metadaten |
| `src/tree.rs` | Bei Bedarf aufgebaute Baumstruktur, Filter und Ansichten |
| `src/export.rs` | Konsolidierter Dokumentexport mit Kollisionsbehandlung |
| `src/bin/ectd-tool.rs` | Kommandozeile des Backbone-Generators |
| `src/tool/` | Excel, Scan, XML-Extraktion, CSV-Mapping, DTDs und Backbone-Erzeugung |
| `resources/` | Eingebettete CSV-Mappings und Excel-Metadatenvorlage |
| `tests/` | Rust-Tests und synthetische DTD-Testdateien |
| `examples/demo-dossier/` | Synthetisches Dossier zum Ausprobieren des Viewers |

Die beiden CSV-Dateien unter [resources/](resources/README.md) werden beim Build in
den Generator eingebettet. Die Excel-Vorlage `resources/metadata-0001.xlsx` kann in
den Dossierordner kopiert und für die gewünschte Sequenz angepasst werden.
Produktive DTD- und XSL-Dateien gehören weiterhin zum jeweiligen Dossier; die
Dateien unter `tests/fixtures/` sind ausschließlich für Tests bestimmt.

Der Workflow `.github/workflows/build-binaries.yml` prüft Formatierung, Clippy und
Tests und erstellt Release-Builds für Windows und Linux. Die Archive enthalten
beide Programme, Dokumentation, Lizenz und Ressourcen. Pushes auf `main` und `dev`
sowie Pull Requests auf diese Branches starten den Workflow. Tags mit Präfix `v`
veröffentlichen die Archive zusätzlich als GitHub Release.

Das Lesen des Dossiers und der Export laufen im Hintergrund. XML wird als Stream
gelesen; gespeichert werden die Leaf-Inhalte und die für die Anzeige benötigten Daten.
Der Baum wird je Sequenz beim Aufklappen aufgebaut, Tabellen rendern nur sichtbare Zeilen.
Die Suche baut bei Bedarf alle Sequenzen auf. Metadaten und XML werden vollständig
angezeigt; externe DTDs werden nicht geladen. Eine DTD- oder Prüfsummenvalidierung
ist im Viewer nicht enthalten; die DTD-Prüfung erfolgt im Generator.

Verschachtelte XML-Inhalte bleiben vollständig erhalten; die Suche erreicht
ungeöffnete Sequenzen. Bereits ersetzte oder gelöschte, aber im XML
referenzierte Dateien werden nicht irrtümlich als `ORPHAN` wieder eingeblendet.
Die ausgewählte Sequenz bestimmt auch dann den Exportstand, wenn ein darin gezeigtes
Dokument aus einer früheren Sequenz stammt.

## Lizenz

Copyright © 2026 Sebastian Scala. GPL-3.0-or-later.
Siehe [LICENSE](LICENSE).
