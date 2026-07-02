# CLAUDE.md

Leitfaden für die Arbeit an diesem Repository. `ratada` ist ein
wiederverwendbares **ratatui-Widget-Toolkit** für Rust-Terminal-Apps: ein
generischer Event-Loop-Treiber, Widgets, Modals, Formulare, Picker und eine
framework-agnostische Theming-Schicht über einem schlanken Kern aus `ratatui`,
`crossterm` und wenigen weiteren Crates.

## Projektüberblick

- **Bibliothek, kein Binary.** `ratada` hat keine `main`, keine CLI, keine
  Domänen-/Persistenz-Schicht. Es liefert die generischen TUI-Bausteine, auf
  denen konsumierende Apps (z. B. `clibase`) ihre Views bauen.
- **Kern-Idee:** Der Host implementiert das `Screen`-Trait und übergibt es an
  `run`, das den Event-Loop innerhalb eines `Tui`-Guards (Raw-Mode +
  Alternate-Screen, RAII) fährt. Lifecycle-Hooks kommen über
  `Tui::with_hooks`. Das Toolkit besitzt Terminal, Navigation, Rendering und
  Modal-Bausteine; der Host besitzt den Anwendungszustand.
- **Keine Anwendungstypen.** Die Module hängen ausschließlich von externen
  Crates und dem eigenen `theme`-Submodul ab, nie von Host-Typen. Das hält das
  Toolkit universell einsetzbar.

### Modul-Layout

- **Crate-Root (`src/lib.rs`):** deklariert die Widget-Module flach
  (`pub mod modal;`, `pub mod table;`, …) plus `pub mod theme;` und re-exportiert
  ein kleines Prelude:
  ```rust
  pub use driver::{Flow, Screen, run};
  pub use modal::ModalSignal;
  pub use overlay::{PopupFlow, popup};
  pub use terminal::{Tui, TuiEvent};
  ```
  Die meisten Widgets werden über ihren Modulpfad erreicht (`ratada::table`,
  `ratada::modal`, …), nicht über das Prelude.
- **Widget-Module (flach im Root):** `terminal`, `driver`, `overlay`, `modal`,
  `chrome`, `layout`, `nav`, `scroll`, `style`, Eingabe/Editieren (`input`,
  `textarea`, `autocomplete`, `editor`, `clipboard`), Picker (`color_picker`,
  `date_picker`, `date_range_picker`, `month_picker`, `path_picker`, `slider`),
  Anzeige (`table`, `tree`, `list`, `tabs`, `pager`, `gauge`, `spinner`,
  `toast`, `text`), sowie `form`, `finder`, `fuzzy`, `help`, `header`, `footer`,
  `statusbar`, `double_press`. Querverweise zwischen Modulen laufen über
  `super::` (der Crate-Root).
- **`theme/` (Submodul):** framework-agnostisches Theming, das eine UI-Schicht
  (auch eine reine CLI) teilen kann: `Color` (+ `parse_color`/`dim_color`/
  `lighten`), `Palette` (+ `resolve`, `ColorOverrides`), `Skin` (Bündel aus
  Palette/Glyphs/Mode), `Glyphs`/`GlyphVariant`, `Mode`, sowie
  `ThemeRegistry`/`ThemeColors`/`Surfaces` mit den Built-in-Themes. `style.rs`
  ist die **einzige** Naht, die `theme::Color` auf `ratatui::style::Color`
  abbildet.
- **Abhängigkeiten:** nur `ratatui`, `crossterm`, `unicode-width`,
  `nucleo-matcher`, `chrono`, `log`, `serde` (Letzteres für die persistierbaren
  Enums `Mode`/`GlyphVariant`). Keine weiteren.

### Befehle

```bash
cargo build
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Dieses Crate ist die **SSOT** der unten in §7.10 beschriebenen
TUI-Konventionen. Neue Widgets und konsumierende TUIs bauen darauf auf, statt
sie nachzubauen.

---

Der folgende Style Guide ist bindend. Bei Konflikten gehen spezifischere
(sprachbezogene) Regeln den allgemeinen vor. Diese dokumentierten Regeln haben
Vorrang vor automatischen Formattern/Lintern.

## 1 Clean Code / Design Principles & Patterns

Oberstes Ziel ist leserlicher, wartbarer Code – Verständlichkeit geht im
Zweifel vor Kürze oder Cleverness. Gleichrangig: Robustheit und Sicherheit
(§2.7).

### 1.1 Einfachheit & Wiederholung

- **KISS / YAGNI** – keine spekulative Abstraktion oder Konfigurierbarkeit "für
  später".
- **DRY** (Code/Logik) und **SSOT** (Daten-/Wissensquelle).
- **Konsistenz:** Gleichartige Dinge gleich lösen.
- **Keine Magic Numbers/Strings:** benannte Konstanten.

### 1.2 Funktionen

- **SLAP** – eine Abstraktionsebene pro Funktion.
- **Maximal zwei Verschachtelungsebenen** (mit frühem Return).
- **Keine Flag-Argumente:** statt Boolean-Parameter zwei Funktionen mit
  sprechenden Namen.
- **Command-Query-Separation;** reine Funktionen bevorzugen.

### 1.3 OO & Design

- **Polymorphismus statt Typ-Verzweigung.**
- **Komposition statt Vererbung;** Vererbung nur bei echter "ist ein"-Beziehung
  (LSP).
- **Tell, Don't Ask / Law of Demeter.**
- **SOLID** (DIP via Dependency Injection).
- **Hohe Kohäsion, lose Kopplung.**
- **Design Patterns (GoF):** einsetzen, wo sie ein reales Problem lösen – nie
  als Selbstzweck, KISS/YAGNI gehen vor.

### 1.4 Code Smells

- **Code Smells erkennen & per Refactoring beseitigen** (Long Method, Duplicate
  Code, Feature Envy, Primitive Obsession, …). Ein Smell ist ein Hinweis, kein
  Automatismus. Vorgehen siehe §3.

### 1.5 Namen

- **Booleans/Prädikate** als Ja/Nein-Frage: `is_`, `has_`, `can_`, `should_`.
- **Methoden = Verben, Klassen/Typen = Substantive** – keine Sammelnamen wie
  `Manager`, `Data`, `Helper`.

### 1.6 Fehlerbehandlung

- **Fail fast:** ungültige Zustände/Eingaben so früh wie möglich abfangen.
- **Kein `None`/`null` zurückgeben:** stattdessen leere Collection,
  Special-Case-Objekt oder Exception.
- **Exceptions mit Kontext** (was, wo, warum) – sprachspezifisch siehe
  jeweiliger Abschnitt.

### 1.7 Tests & Performance

- **Tests nach FIRST;** ein Konzept pro Test.
- **Erst messen, dann optimieren:** Optimierung nur mit Profiling-Beweis.

## 2 Allgemeine Regeln (alle Sprachen)

### 2.1 Formatierung & Tools

- **Einrückung:** 4 Spaces als Standard.
- **Zeilenlänge:** Maximal 80 Zeichen. Längere Zeilen leserlich umbrechen. Gilt
  für Code-Dateien (`.rs`, …), nicht für Textdateien (`.txt`, `.md`).
- **Markdown-Fließtext:** Absätze und Listenpunkte NICHT hart umbrechen – jeder
  Absatz und Listenpunkt steht auf genau einer Zeile (Editor-Soft-Wrap nutzen).
  Leerzeilen, Listenstruktur, Überschriften und Code-Blöcke bleiben erhalten.
- **Zeilenumbruch:** Lange Ausdrücke/Aufrufe leserlich umbrechen – Operatoren an
  den Zeilenanfang, bei vielen Argumenten eines pro Zeile; Fortsetzungszeilen
  konsistent einrücken.
- **Signatur- & Aufruf-Umbruch:** Passt eine Signatur/ein Aufruf nicht in 80
  Zeichen, zuerst alle Parameter/Argumente auf eine eingerückte Zeile zwischen
  den Klammern; reicht das nicht, eines pro Zeile. Voll aufgefächerte Signaturen
  sind ein Indikator für zu viele Parameter – dann gruppieren (Struct).
- **Whitespace & Datei-Hygiene:** Nur Spaces (keine Tabs); kein Trailing
  Whitespace; Datei endet mit genau einem Newline; UTF-8; Zeilenenden LF.
- **Zahlenliterale:** Ziffern-Trenner bei großen Zahlen (`1_000_000`); Hex in
  Kleinbuchstaben (`0xff`).
- **Trailing Commas:** In mehrzeiligen Listen, Argumentlisten, Enums und
  Initializern abschließendes Komma; einzeilig nicht. (rustfmt erzwingt das.)
- **Alignment:** Spalten-Alignment mit zusätzlichen Leerzeichen ist zur besseren
  Lesbarkeit erlaubt.
- **Bindestriche / Gedankenstrich:** Niemals den Geviertstrich "—". In Code nur
  das Minuszeichen ("-"). In Markdown-Fließtext den Gedankenstrich "–" als
  Gedankenstrich verwenden, nicht einen von Leerzeichen umgebenen Bindestrich.
  Bindestriche in zusammengesetzten Wörtern bleiben Bindestriche.
- **Anführungszeichen:** Immer gerade Anführungszeichen "…" – nie typografische.
- **Vorrang vor Tools:** Diese Regeln gehen rustfmt/clippy vor.
- **Portabilität/Versionen:** Neueste Standards bevorzugen.

### 2.2 Kommentare & Sprache

- **Kommentare:** Moderat. Funktionen und wichtige Logikblöcke werden
  kommentiert, nicht jede Zeile. Ein Kommentar wiederholt nicht den Bezeichner –
  er erklärt, was der Leser nicht aus dem Namen ableiten kann (Semantik,
  Einheiten, Sentinel-Werte, Invarianten, Beweggründe). Kommentare erklären vor
  allem das **Warum**, nicht das Was.
- **TODO-Kommentare:** Format `// TODO: <text>`.
- **Sprache:** Durchgehend Englisch – Bezeichner, Kommentare, Docstrings und
  sichtbare Texte (Fehler, Logs, TUI-Ausgaben).
- **Einzeilige Kommentare:** Bei ≤ 80 Zeichen hinter einem Befehl erlaubt; dann
  zwei Leerzeichen vor `//`. Besteht der Kommentar nur aus einem Satz, kein
  Punkt am Ende.

### 2.3 Namensgebung

- **Namensgebung:** `snake_case` für Variablen und Funktionen; `UPPER_CASE` für
  Modul-/Klassenkonstanten; `PascalCase` für Typen.
- **Datei-/Modulnamen:** snake_case.
- **Aussagekräftige Variablennamen:** Aus dem Namen geht der Zweck hervor. Keine
  kryptischen Kürzel. Ausnahme: Zählervariable einer einzelnen, nicht
  verschachtelten Schleife darf `i` heißen.
- **Akronyme in Bezeichnern:** wie normale Wörter (`UserId`, `HttpClient`,
  `parse_url`) – nicht `UserID`/`HTTPClient`.
- **Keine negativen Booleans:** positiv benennen; doppelte Verneinung vermeiden.

### 2.4 Typen & Daten

- **Wahrheitswerte:** `bool` verwenden – keine `int`-Flags.
- **Starke Typisierung:** Domäne explizit typisiert – `enum`s statt magischer
  Zahlen/Strings, Structs statt loser Primitiven.
- **Unveränderlichkeit:** Wo sinnvoll unveränderlich halten; Funktionseingaben
  nicht mutieren.

### 2.5 Funktionen & Kontrollfluss

- **Funktionslänge:** Klein halten (Single Responsibility). Größere Funktionen
  zerlegen.
- **Funktionsparameter:** Anzahl gering (möglichst ≤ 3). Zusammengehörige
  Parameter in einem Struct gruppieren. Situative Werte bleiben explizite
  Parameter, nicht als transienter Zustand im Objekt.
- **Explizite Übergabe:** Bei mehreren Werten benannt übergeben statt
  positionsabhängig.
- **Kontrollfluss:** Früher Return (Guard Clauses) statt tiefer Verschachtelung.
- **Lesereihenfolge:** Code von oben nach unten lesbar – extrahierte
  Hilfsfunktionen erscheinen unterhalb ihrer Aufrufer.
- **Deklarationsreihenfolge:** öffentliche vor privaten Membern; zusammen-
  gehörige Member gruppieren.

### 2.6 Architektur & Abhängigkeiten

- **Programmierstil:** Objektorientiert für zustandsbehaftete Komponenten
  (Widgets mit eigenem Zustand); freie Funktionen für zustandslose Utilities
  (Rendering-Helper, Navigation).
- **Schichten-Architektur:** Verantwortungen trennen; von Abstraktionen statt
  Konkretionen abhängen (DIP). Als Bibliothek hängt `ratada` nie von
  Anwendungstypen ab; variables Verhalten kommt über Traits/Callbacks vom Host
  (z. B. das `Screen`-Trait, `Tui::with_hooks`).
- **Externe Bibliotheken:** Bordmittel und Standardbibliothek bevorzugen,
  Abhängigkeiten minimieren. Externe Libraries dürfen vorgeschlagen werden –
  aber vorher nachfragen, bevor eine Dependency eingeführt wird.

### 2.7 Robustheit, Fehler & Logging

- **Robustheit & Sicherheit:** So robust wie möglich gegen Abstürze und so
  sicher wie möglich. Defensiv programmieren: Eingaben validieren, Randfälle
  absichern, Rückgabewerte/Fehler prüfen. Lieber kontrolliert fehlschlagen als
  abstürzen oder still falsch weiterlaufen.
- **Logging:** Für Diagnose strukturiertes Logging über das Logging-Framework
  (`log`) statt direkter Konsolenausgabe (`println!`); Log-Level sinnvoll
  wählen. Sichtbare TUI-Ausgaben sind kein Logging.

### 2.8 Tests

- **Tests:** Werden immer mitgeliefert. Fakes gegenüber Mocks bevorzugen;
  verwandte Tests gruppieren und Testnamen das erwartete Verhalten beschreiben
  lassen.

### 2.9 Verhältnismäßigkeit: kleine und persönliche Skripte

Bei einzelnen Funktionen, kleinen Skripten oder Code nur für den persönlichen
Gebrauch darf der Umfang reduziert werden – aber nie stillschweigend:

- **Rücksprache mit Vorschlag:** Bevor Sicherheitsaspekte oder Tests weggelassen
  werden, nachfragen – mit konkretem Vorschlag (was entfällt, was bleibt,
  Risiko in je einem Satz).
- **Nie verhandelbar:** kein `eval`/`exec` auf fremde Eingaben, keine
  Shell-/Command-Injection, keine Secrets im Code, keine destruktiven
  Operationen auf unvalidierte Pfade.

## 3 Wartung / Refactoring / Code-Anpassungen

- **Lokalen Stil respektieren:** Beim Ändern bestehenden Codes den vorhandenen
  Stil/Idiome übernehmen; den Style Guide nicht mitten in einer Datei halb
  durchdrücken (Konsistenz vor "mein Stil"). Ausnahme: explizit beauftragtes
  Refactoring. Bei großen Abweichungen (Architektur, Tooling, …) keine
  Änderungen ohne vorherige Absprache.
- **Minimale, fokussierte Änderungen:** Nur anfassen, was die Aufgabe erfordert;
  keinen unbeteiligten Code umformatieren, den Umfang nicht ausweiten.
- **Refactoring vom Verhalten trennen:** Reines Refactoring ändert das Verhalten
  nicht; Verhaltensänderungen sind ein separater Schritt. Boy-Scout-Regel.
- **Aufrufer & Tests mitziehen:** Bei Signatur-/Verhaltensänderungen alle
  Aufrufstellen anpassen, Tests aktualisieren/ergänzen und laufen lassen. Als
  Bibliothek mit öffentlicher API: Änderungen an `pub`-Signaturen sind
  Breaking Changes – bewusst und dokumentiert vornehmen.
- **Kommentare/Docs aktuell halten:** Bei Umbenennungen/Änderungen Kommentare
  und Doc-Blöcke mitziehen.
- **Keine Leichen hinterlassen:** Toten und auskommentierten Code entfernen.
- **Ursache statt Symptom:** Bugs an der Wurzel beheben.
- **Dokumentation:** Bei jeder Änderung prüfen, ob Doku (README.md, rustdoc, …)
  angepasst werden muss, und es erledigen.
- **Tests:** Bei jeder Änderung prüfen, ob Tests anzupassen/zu ergänzen sind.
  **Nach jeder Änderung alle Tests neu ausführen und sicherstellen, dass alle
  bestehen.**

## 7 Rust

### 7.1 Toolchain & Standard

- **Edition:** 2024 (neueste stable). `rust-version` nur bei konkreter MSRV.
- **Formatting:** rustfmt mit Default-Einstellungen. Import-Gruppierung über
  `group_imports = "StdExternalCrate"` und `imports_granularity` aktivieren, wo
  eine `rustfmt.toml` vorhanden ist.
- **Linting:** clippy muss warnungsfrei durchlaufen
  (`cargo clippy -- -D warnings`). `clippy::pedantic` optional, projektweit per
  `#![warn(...)]`, nicht durch verstreute `#[allow]`.
- **Logging:** `tracing` oder `log` (mit Implementierung wie `env_logger`) für
  Diagnose statt `println!`/`eprintln!`. Crate vorher abstimmen.
- **Vorrang vor Tools:** Diese Regeln gehen rustfmt/clippy vor.

### 7.2 Projektstruktur & Architektur

- **Modul-Deklaration:** Untermodule über `mod`-Deklarationen in `lib.rs` bzw.
  der übergeordneten Datei; Dateinamen `snake_case`. Querverweise zwischen den
  flach im Root liegenden Widget-Modulen über `super::`.
- **Dependency Injection:** Variables Verhalten über Traits abstrahieren;
  Implementierung per Generic (`fn f<T: Screen>(…)`) oder `dyn Trait`/`Box<dyn
  Trait>` injizieren. Der Host bringt sein Verhalten über das `Screen`-Trait und
  Lifecycle-Hooks ein.
- **Target-Ordner:** `target.nosync` (damit iCloud die Build-Artefakte nicht
  synchronisiert), in `.gitignore` aufnehmen.

### 7.3 Fehlerbehandlung

- **`Result` + `?`:** Fehler über `Result<T, E>` und `?` propagieren; keine
  String-Fehler als dauerhaftes Muster.
- **`thiserror` für Bibliotheken:** Eigene Fehler-Enums mit
  `#[derive(thiserror::Error)]` und sprechenden `#[error("…")]`-Meldungen; pro
  Modul/Domäne ein Error-Typ. Fremdfehler über `#[from]`. `ratada` ist eine
  Bibliothek – **`anyhow` gehört nicht in die Public-API.** Das `Screen`-Trait
  lässt den Host seinen eigenen Fehlertyp wählen (`type Error: From<io::Error>`).
- **`unwrap()` verboten; `expect()` nur an beweisbar unfehlbaren Stellen.** Jedes
  `expect("…")` begründet, warum es nicht fehlschlagen kann. Im normalen Fluss
  `?`.
- **Kein `panic!` im Normalfluss:** nur für echte Programmierfehler/Invarianten.
- **`unsafe`:** Vermeiden. Falls unumgänglich, vorher nachfragen, kapseln, mit
  `// SAFETY: …` begründen.

### 7.4 Dokumentation (rustdoc)

- **`///`-Doc-Comments** über jedem öffentlichen Item. Erste Zeile knappe
  Ein-Satz-Zusammenfassung. Als Bibliothek mit öffentlicher API ist gepflegtes
  rustdoc besonders wichtig.
- **Idiomatisches rustdoc, keine `# Arguments`-Listen:** Parameter/Rückgabe in
  Prosa. Standardabschnitte wo zutreffend: `# Examples` (mit lauffähigem
  Doctest, wenn nicht offensichtlich), `# Errors`, `# Panics`, `# Safety`.
- **Bezeichner in Doc-Comments** in `` `inline code ``; intra-doc-Links wo
  sinnvoll.
- **Modul-Doc:** Jedes Modul oben einen `//!`-Doc-Comment mit Kurzbeschreibung.
- **Private Items:** Kurzer einzeiliger `///`-Kommentar genügt.

### 7.5 Typen & Idiome

- **Starke Typisierung:** `enum`s für Zustände/Varianten statt magischer
  Strings; `struct`s statt loser Tupel. Newtypes für Domänenwerte erwägen.
- **Ableitungen:** Sinnvolle Traits ableiten (`Debug, Clone, PartialEq, …`);
  `Serialize`/`Deserialize` via serde-derive. `Copy` nur bei kleinen Typen.
- **Konstruktoren:** `pub fn new() -> Self`; bei Default-Konstruktoren zusätzlich
  `impl Default`. `new` ohne Argumente nicht doppelt zu `Default` pflegen.
- **Ownership:** Borrows (`&T`/`&mut T`) bevorzugen; unnötiges `.clone()`/
  `.to_string()` vermeiden. Eingaben nicht unnötig mutieren.
- **Optionalität:** `Option<T>` für "kann fehlen"; kein Sentinel-Wert.
- **Kontrollfluss:** `match`/`if let` mit Guard Clauses; tiefe Verschachtelung
  vermeiden.
- **Iteratoren statt manueller Schleifen** für einfache Map/Filter/Fold; bei
  komplexer Logik explizite `for`-Schleife.
- **Sichtbarkeit:** So privat wie möglich; öffentliche API klein halten
  (wichtig für eine Bibliothek). Prelude-Re-Exports über `pub use` in `lib.rs`.

### 7.6 Nebenläufigkeit

- **Synchron als Default,** solange kein realer Bedarf besteht (KISS/YAGNI).
- **`async`/`await` nur bei echtem I/O-Concurrency-Bedarf,** dann mit Runtime
  (`tokio`); Runtime vorher abstimmen.
- **Keine blockierenden Aufrufe im `async`-Kontext;** CPU-Lastiges über
  `spawn_blocking`/dedizierte Threads.
- **Shared State** bevorzugt über Ownership/Channels statt geteilter Sperren;
  nur wo nötig `Arc<Mutex<…>>`.

### 7.7 Externe Crates

- Standardbibliothek bevorzugen, Dependencies minimieren – neue Crates vorher
  abstimmen. Etablierte Crates über dem `use` bzw. in `Cargo.toml` dokumentieren:
  `// https://crates.io/crates/<name>`.

### 7.8 Tests

- **Unit-Tests** in der jeweiligen Datei unter `#[cfg(test)] mod tests { … }`
  mit `use super::*;`; **Integrationstests** im Verzeichnis `tests/`.
- Testfunktionen `#[test]`, Namen beschreiben das erwartete Verhalten;
  Fehlerfälle ggf. mit `#[should_panic]` oder `Result`-Rückgabe. FIRST und
  Fakes-vor-Mocks gelten.
- Doctests in `# Examples` zählen als Tests und müssen laufen.

### 7.9 Sicherheit

- **`unsafe`-Disziplin:** vermeiden, kapseln, `// SAFETY:` + `# Safety`-Doku.
- **Integer-Overflow:** für Werte von außen explizit `checked_*`/`saturating_*`/
  `wrapping_*`. Terminal-Geometrie (u16/usize-Konvertierungen) ist durch die
  Bildschirmgröße begrenzt.
- **Command Injection:** `std::process::Command` mit `.arg()`/`.args()`; kein
  `sh -c` mit zusammengesetzten Strings (relevant für `editor`/`clipboard`, die
  externe Tools aufrufen).
- **Pfad-Traversal:** Pfade von außen mit `canonicalize()` + `starts_with()`
  prüfen (relevant für `path_picker`).
- **Secrets:** keine Secrets in Code/Log; bei Bedarf `zeroize`.
- **Eingaben begrenzen:** Größen-/Längenlimits beim Parsen fremder Daten.
- **Dependencies:** `cargo audit` in CI; Dependencies minimieren (§7.7).

### 7.10 TUI-Konventionen (Rust-Terminal-Apps)

Gilt für ratatui-basierte Terminal-Apps. **Dieses Crate implementiert die
folgenden Konventionen** als wiederverwendbare Bausteine; neue Widgets und
konsumierende Ansichten bauen darauf auf. Punkte ohne Zusatz gelten
grundsätzlich; mit "(optional)" markierte sind bewährte Muster, die übernommen
werden, wo sie passen.

**Listen & Navigation**

- **Zyklisch navigieren:** Auswahllisten wrappen an beiden Enden über einen
  gemeinsamen Helper (`nav::cycle` bzw. `rem_euclid`), nicht per
  `saturating_add/sub`. Leere Liste ergibt Index 0.
- **Seitenweise Navigation:** `PageUp`/`PageDown` bewegen um eine Bildschirm-
  seite (sichtbare Zeilen, mindestens 1); am Rand geklemmt (nicht zyklisch).
- **Sprünge an Anfang/Ende:** `Home`/`End` springen an Listenanfang/-ende und
  klemmen dort. Optional zusätzlich vim (`g`/`G`, `j`/`k`).
- **Direktsprung zu einem Wert (optional):** kleiner Picker, springt auf die
  nächste vorhandene Zeile.
- **Mehrfachauswahl (optional):** `Space` toggelt; `Shift`+Pfeil/`PageUp/Down`
  erweitern einen Bereich von einem Anker aus.

**Scrollen & Scrollbar**

- **Scrollbar bei Überlauf:** vertikale Scrollbar rechts, sobald Inhalt den
  Viewport überläuft, sonst weggelassen. Dim-Stil ohne Pfeile, gemeinsamer
  Helper (`scroll::render_scrollbar`); Positionszahl ist `total - viewport + 1`.
- **Scroll-Offset folgt dem Cursor:** Liste scrollt erst am Rand, nicht
  seitenweise pro Schritt.

**Modals & Widgets**

- **Wiederverwendbare Modal-Widgets:** gemeinsamer Satz – `confirm`, `select`,
  `multi_select`, `number_input`, `message` – nicht pro Aufrufstelle nachgebaut.
  Destruktive Aktionen gehen über `confirm`.
- **Kalender-Date-Picker (optional):** gemeinsames Kalender-Modal mit
  einheitlichem Look/Shortcuts.
- **Fuzzy-Matching:** Filter- und Auswahl-Picker matchen fuzzy (`fuzzy`, backed
  by `nucleo-matcher`).
- **Autocomplete-Dropdown (optional):** Inline-Dropdown für Vorschlagswerte.

**Formulare**

- **Aufbau & Steuerung:** alle Felder gleichzeitig sichtbar; `Tab`/`BackTab`
  steppen (umlaufend), `Ctrl+S` speichert, `Esc` bricht ab; fokussierte Zeile
  per Hintergrund-Tint hervorgehoben.
- **Dirty-Marker & Reset (optional):** geänderte Felder tragen `*`; `r` setzt
  das fokussierte Feld zurück.
- **Externer Editor (optional):** `Ctrl+G` übergibt das Feld an `$EDITOR`.
- **Lese-/Pan-Modus (optional):** `Ctrl`+Pfeile schwenken eine Multiline-Box.

**Textfelder**

- **Vollständige Editier-Shortcuts:** ein- und mehrzeilige Felder teilen
  denselben Satz Shortcuts über die geteilte `text_edit`-Logik (ein Caret mit
  optionalem Selektions-Anker) – einzige Quelle (SSOT/DRY). Der Editor-Kern
  behandelt nur Editier-Tasten; Steuertasten des Feldes (`Esc`, bestätigendes
  `Enter`, andere Chords) gehören dem Aufrufer.
  - **Bewegung:** Pfeile zeichenweise; `Home`/`End`; `Up`/`Down` nur mehrzeilig.
  - **Selektion:** `Shift`+Bewegung erweitert, ohne `Shift` hebt auf; `Ctrl+A`
    alles.
  - **Löschen:** `Backspace`/`Delete`; `Ctrl+U`/`Ctrl+K` bis Zeilenanfang/-ende.
  - **Zwischenablage:** `Ctrl+C`/`X`/`V`; Tippen/Einfügen ersetzen Selektion.
  - **Rendering:** Block-Cursor (Farbe optional via Config); einzeilig
    horizontal scrollend mit `…`-Clipping, mehrzeilig wortweise umgebrochen.

**Darstellung**

- **Farbgebung – dezent statt grell:** ein einziger Akzentton (weiches RGB) für
  Header/aktiven Tab/Hervorhebung, `DIM`/Grau für Sekundärtext, gedämpfte
  Hintergrund-Tints für Selektion/Fokus. Farben tragen Bedeutung und werden als
  benannte Konstanten zentral gehalten (im `theme`-Submodul), nicht über Widgets
  verstreut.
- **Rahmen-Stil:** Boxen/Modals mit abgerundeten Rahmen (`BorderType::Rounded`).
- **Glyphen/Icons – zwei Varianten, config-wählbar:** jedes Icon in zwei Stufen
  (Unicode + ASCII-Fallback), per `GlyphVariant` wählbar. Keine bunten Emojis.
- **Footer-Hint-Line:** aktive Tastenkürzel in einer Fußzeile –
  `(Taste, Beschreibung)`-Tokens, Taste im Akzentton, Beschreibung dim, mit
  ` · ` getrennt, bei zu schmaler Breite umbrechend. Gemeinsamer Helper
  (`footer::lines`).
- **Hilfe-Overlay:** über `?` aufrufbares Voll-Overlay mit allen Shortcuts;
  scrollbarer Fuzzy Finder. Footer weist mit `? help` darauf hin. Beim
  Hinzufügen eines Shortcuts Footer, Hilfe-Overlay und die Doku synchron halten.
- **Transiente Status-Zeile:** Rückmeldungen als kurze Meldung im Footer
  (Akzentfarbe), die beim nächsten Tastendruck verschwindet. Fehler aus Aktionen
  werden so gemeldet und führen nie zum Absturz; nur schwerwiegende Fälle nutzen
  ein Modal.
- **Überlauf kürzen mit `…`:** zu breiter Text auf sichtbare Breite gekürzt
  (gemeinsamer `text::truncate`-Helper).
- **Sticky-Header-Zeile (optional);** Spaltenkopf mit Einheit (optional);
  Tab-Bar (optional); Theming (optional, Farben via `theme` mit
  Default-Fallback).

**Globale Tasten & App-Rahmen**

- **Globale Tasten:** `Ctrl+Q` beendet hart (überall, inkl. Modals, mit
  Speichern der Session). `Ctrl+C` bleibt der Zwischenablage vorbehalten.
  Zahlentasten wählen Top-Level-Views (persistiert). `u` macht die letzte Aktion
  rückgängig (One-Level-Undo), `y` kopiert in die Zwischenablage. (Diese
  Bindungen setzt der Host; das Toolkit liefert die Bausteine.)
- **Terminal-Guard (RAII):** ein Guard-Typ (`Tui`) aktiviert Raw-Mode +
  Alternate-Screen bei Erzeugung und stellt beide beim Drop wieder her; der
  Event-Wrapper liefert Tasten und `Resize`, die Oberfläche zeichnet bei Resize
  neu.
- **Debounced Save (optional):** schnelle, wiederholte Änderungen gebündelt und
  verzögert schreiben.

### 7.11 Sonstiges

- Übrige allgemeine Regeln (Benennung, Zeilenlänge, Kommentare, Robustheit)
  gelten unverändert.

## 11 Git

**FÜHRE KEINE EIGENEN COMMITS DURCH.**

Mache am Ende von Änderungen einen Vorschlag für eine Commit-Nachricht (nur
Titel) – auf Englisch, im Imperativ (z. B. "add X", "update Y"), es sei denn,
etwas anderes ist vorgegeben. Verwende den Stil von
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).
