# Code-Walkthrough & Aufräumen (Checkliste zum Abhaken)

## Context

Das Repo ist nach mehreren Feature-Runden stabil und sauber (`cargo fmt --check` grün, `cargo clippy --all-targets -- -D warnings` grün, 142 Unit-/Integrationstests + 7 Doctests, `clippy::pedantic` crate-weit, nur wenige begründete `#[allow]`, keine offenen TODOs, jedes Modul hat einen `//!`-Doc). `ratada` ist die **Bibliothek selbst** – das wiederverwendbare ratatui-Widget-Toolkit plus die framework-agnostische `theme`-Schicht; es gibt kein Binary, keine Domänen-/Persistenz-Schicht. Konsumierende Apps (z. B. `clibase`) hängen als Pfad-Dependency daran. Diese Checkliste betrifft daher das **gesamte Crate**; da es eine öffentliche API ist, sind Sichtbarkeit und Signatur-Stabilität hier besonders wichtig (`pub`-Änderungen sind Breaking Changes).

Reihenfolge-Prinzip: zuerst Baseline herstellen, dann Schicht für Schicht von den abhängigkeitsfreien Fundamenten (`theme`) nach außen zu den zusammengesetzten Widgets (so baut sich das Verständnis bottom-up auf und jede Schicht wird nach ihren Abhängigkeiten geprüft), zum Schluss ein Querschnitts-Durchlauf.

## Generische Prüfpunkte (gelten bei JEDEM Modul)

Beim Durchgehen jeder Datei jeweils prüfen (CLAUDE.md §1, §2, §7):
- **Namen:** Prädikate `is_/has_/can_/should_`; Methoden = Verben, Typen = Substantive; keine `Manager/Helper/Data`-Sammelnamen; keine negativen Booleans; Akronyme wie normale Wörter (`UserId`, nicht `UserID`).
- **Funktionen:** SLAP (eine Abstraktionsebene), max. 2 Verschachtelungen mit frühem Return, ≤ 3 Parameter (sonst Struct), Command-Query-Separation.
- **Sichtbarkeit:** so privat wie möglich; `pub` nur, wo für die öffentliche API wirklich nötig – Internes auf `pub(crate)`; Prelude-Re-Exports schlank halten (`lib.rs`).
- **Fehler:** `Result`/`?`, kein `unwrap/expect/panic` im Normalfluss; jedes `expect` begründet. Das `Screen`-Trait überlässt dem Host den Fehlertyp – keine `anyhow` in der Public-API.
- **Magic Numbers/Strings:** durch benannte Konstanten/`enum`s ersetzt (Glyphen, Farben, Tastenkürzel, Layout-Maße).
- **Hygiene:** kein toter/auskommentierter Code; Kommentare erklären das *Warum*; Doc-Comments je öffentlichem Item, erste Zeile Ein-Satz-Summary, Prosa statt `# Arguments`; 80-Spalten; gerade Anführungszeichen; kein Geviertstrich.
- **Tests:** logiktragender Code hat Tests; Testnamen beschreiben Verhalten; Doctests in `# Examples` müssen laufen.
- **TUI-Konventionen (§7.10):** zyklische Navigation über `nav::cycle`/`rem_euclid` (nicht `saturating_add/sub`); Scrollbar bei Überlauf über `scroll::render_scrollbar`; Überlauf-Kürzung über `text::truncate`; abgerundete Rahmen; Glyphen in beiden Varianten; Farben zentral im `theme`-Submodul.

---

## Orientierung – Lesedurchgang (vor Phase 0, ohne Änderungen)

Bottom-up nur *lesen*, um die mentale Landkarte aufzubauen, bevor aufgeräumt wird. Hier wird nichts geändert – nur Verdrahtung und Modulstruktur erfassen.

- [ ] `lib.rs`: Modulbaum, crate-weite `#![warn/allow]`, öffentliche Re-Exports und `prelude` überfliegen – was ist nach außen sichtbar, welche Schichten gibt es?
- [ ] `theme/mod.rs` → `style.rs`: die eine Naht `theme::Color → ratatui::style::Color` nachvollziehen; alles Weitere baut darauf auf.
- [ ] Den Abhängigkeiten von innen nach außen folgen (`theme` → Primitives `nav/scroll/text` → `terminal/driver` → `overlay/chrome` → Eingabe/Anzeige/Picker → zusammengesetzte Widgets `modal/form/finder/help`). Auffälligkeiten notieren, aber noch nicht anfassen – das passiert bottom-up ab Phase 1.
- [ ] Quer-Referenz zur Doku: `API.md`, `DEVELOPMENT.md`, `README.md` überfliegen und mit dem tatsächlichen Modulbaum abgleichen.

## Phase 0 – Baseline & Scope

- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` laufen lassen – grüner Ausgangszustand bestätigt.
- [ ] Sauberen Branch (`clean-up`) nutzen (kein Commit auf `main`); Arbeitsstand sichern.
- [ ] Entscheiden: reiner Review (nur lesen + Mini-Fixes) vs. echte Refactors – Umfang abstecken. Bei `pub`-Signaturänderungen bewusst als Breaking Change behandeln und dokumentieren.

## Phase 1 – theme (`src/theme/`)

Das abhängigkeitsfreie Fundament (nur `serde` für die persistierbaren Enums).

- [ ] `color.rs` (`Color`, `parse_color`, `hex`, OKLCH-Varianten `darken`/`lighten`/`vivid`/`dim`/`shade`/`mix`/`readable_on`, `distance`, Modell-Konvertierungen `to_hsl`/`from_hsl`/`to_oklch`/`from_oklch`): generische Checks; Parsing-Fehlerpfade robust (kein `unwrap`); Wertebereiche/Clamping.
- [ ] `glyphs.rs` (`Glyphs`, `GlyphVariant`): zwei Icon-Varianten (Unicode + ASCII-Fallback), keine Emojis; `serde`-Ableitungen bewusst.
- [ ] `palette.rs` (`Palette`, `resolve`, `ColorOverrides`, `define_palette!`): SSOT der Akzent-/Dim-/Tint-Farben (Palette-Felder einmalig im Makro deklariert); Override-Merge klar; benannte Konstanten statt verstreuter RGB-Literale.
- [ ] `skin.rs` (`Skin`): Bündel aus Palette/Glyphs – schlanke Konstruktion.
- [ ] `theme_set.rs` (`ThemeRegistry`, `ThemeColors`, Built-in-Themes `default`/`monochrome`): Registry-Struktur, Default-Fallback; keine Magic-Strings für Theme-Namen.
- [ ] `mod.rs`: Re-Exports minimal und konsistent.

## Phase 2 – style (`src/style.rs`)

- [ ] `style.rs`: die **einzige** Naht `theme::Color → ratatui::style::Color`. Prüfen, dass diese Abbildung nirgends sonst dupliziert ist (DRY/SSOT); Konvertierungen vollständig und ohne Panics.

## Phase 3 – Primitives & Utilities (`nav`, `scroll`, `layout`, `text`, `fuzzy`, `double_press`)

Zustandslose Helfer, auf denen die Widgets aufsetzen – freie Funktionen (CLAUDE.md §2.6).

- [ ] `nav.rs` (`cycle`/`rem_euclid`): zyklische Navigation als SSOT; leere Liste ergibt Index 0; Rand-Klemmung für Seiten/Sprünge korrekt.
- [ ] `scroll.rs` (`render_scrollbar`): sichtbarer Stil ohne Pfeile (Thumb `foreground_dim`, Track `border`), nimmt `skin`; Positionszahl `total - viewport + 1`; nur bei Überlauf.
- [ ] `layout.rs`, `text.rs` (`truncate`): Überlauf-Kürzung mit `…` auf sichtbare Breite; `unicode-width`-korrekt (keine Byte-/Char-Verwechslung bei breiten Glyphen).
- [ ] `fuzzy.rs` (backed by `nucleo-matcher`): Match-/Ranking-Schnittstelle klar; Eingaben begrenzt.
- [ ] `double_press.rs`: Zeitfenster-Logik; `Instant`-Nutzung; generische Checks.

## Phase 4 – terminal & driver (`src/terminal.rs`, `src/driver.rs`)

Der App-Rahmen: RAII-Guard und Event-Loop.

- [ ] `terminal.rs` (`Tui`, `TuiEvent`, `with_hooks`, `suspend`): Raw-Mode + Alternate-Screen bei Erzeugung, sauberes Restore im `Drop` (auch auf Fehlerpfaden/Panic); `Resize` liefert Neuzeichnen; Lifecycle-Hooks korrekt einbezogen.
- [ ] `driver.rs` (`Screen`, `Flow`, `run`, `TICK`): generischer Loop; `type Error: From<io::Error>` lässt Host den Fehlertyp wählen; `# Errors`-Doc; `tick`-Kadenz begründet (`TICK`-Konstante).

## Phase 5 – chrome & overlay (`src/chrome.rs`, `src/overlay.rs`)

- [ ] `overlay.rs` (`popup`, `PopupFlow`, Dim-Backdrop): das eine Overlay-Primitive – zentriertes Box + `Clear` + Key-Routing als SSOT für jedes blockierende Widget. Prüfen, dass Picker/Modals wirklich darüber laufen (keine Nachbauten).
- [ ] `chrome.rs` (`panel`/`menu_panel`/`modal_block`/`BoxDecor`/`framed_decor`): zentralisiert das Rahmen-Chrome (Caption in der Top-Border, Badge unten rechts über `framed_decor`); abgerundete Rahmen (`BorderType::Rounded`); Views/Widgets bauen Blöcke nicht inline.

## Phase 6 – Text-Eingabe & Editieren (`input`, `textarea`, `autocomplete`, `clipboard`, `editor`)

- [ ] `input.rs` (**geteilter Editier-Kern**: `apply_edit_key`, `TextCursor`, `render_line`): SSOT/DRY der Editier-Shortcuts (ein Caret + optionaler Selektions-Anker). Kern behandelt nur Editier-Tasten, Steuertasten gehören dem Aufrufer; horizontales Scrollen mit `…`-Clipping; `unicode-width`-korrekte Caret-Position; Sichtbarkeit (`pub(crate)`) prüfen.
- [ ] `textarea.rs`: mehrzeilig, teilt `input::TextCursor` – prüfen, dass die Editier-Logik nicht dupliziert ist; wortweiser Umbruch; Block-Cursor; `Up/Down` nur mehrzeilig.
- [ ] `autocomplete.rs`: Inline-Dropdown für Vorschläge; Navigation zyklisch über `nav`; Scrollbar über `scroll`.
- [ ] `clipboard.rs`: externe Tools über `Command` mit `.arg()`/`.args()` – **kein `sh -c` mit zusammengesetzten Strings** (§7.9 Command-Injection); Fehlerpfade kontrolliert.
- [ ] `editor.rs`: `$EDITOR` via Temp-Datei, Terminal um den Prozess herum via `Tui::suspend` ausgesetzt/wiederhergestellt; Command-Injection-Disziplin; Temp-Datei-Handling robust.

## Phase 7 – Anzeige-Widgets (`table`, `tree`, `list`, `sidebar`, `tabs`, `pager`, `gauge`, `spinner`, `toast`, `header`, `statusbar`, `shortcut_hints`, `theme_preview`)

- [ ] `table.rs` (**größte Datei, ~1170 Zeilen**): dichte Render-/Navigations-Funktionen gezielt auf SLAP und Verschachtelungstiefe prüfen; Navigations-Helper über `nav`; Sticky-Header/Spaltenkopf; keine Magic-Strings. Kandidat für Zerlegung in kleinere Einheiten (siehe konkrete Kandidaten).
- [ ] `tree.rs`, `list.rs`: Navigation/Selektion/Scroll-Offset generisch; `list.rs` trägt das eine `#[allow(too_many_arguments)]` – prüfen (siehe Kandidaten).
- [ ] `sidebar.rs`: sektionierte Menü-Spalte (Header + Items, optionaler `/`-Fuzzy-Filter, `Overflow::Truncate`/`Scroll` mit horizontaler Scrollbar); Selektion überspringt Header, `selected_id`-Mapping; Highlight = Pointer + Akzent + `selection`-Tint; nutzt `nav`/`text`/`scroll`/`chrome::menu_panel`.
- [ ] `tabs.rs`: Tab-Bar, aktiver Tab im Akzentton; zyklisch.
- [ ] `pager.rs`: Scroll/Seiten-Navigation; Scrollbar bei Überlauf; `PageUp/Down` geklemmt.
- [ ] `gauge.rs`, `spinner.rs`, `toast.rs`: kleine Anzeige-Widgets; Animation über `tick`; benannte Konstanten für Frames/Timings. `gauge.rs`: Prozent-Label über dem gefüllten Balken in Kontrastfarbe (`readable_on`).
- [ ] `theme_preview.rs`: rendert die Farb-/Varianten-Vorschau (OKLCH-Stufen) für die Gallery – keine Magic-RGB, Farben aus `palette`.
- [ ] `header.rs`, `statusbar.rs`, `shortcut_hints.rs`: `shortcut_hints::lines`/`group_lines` als gemeinsamer Hint-Helfer (`(Taste, Beschreibung)`-Tokens, Taste im Akzentton, ` · `-getrennt, umbrechend); `statusbar` als transiente Status-Zeile; Sekundärtext dim.

## Phase 8 – Picker (`color_picker`, `swatches`, `date_picker`, `date_range_picker`, `month_picker`, `path_picker`, `slider`)

Alle sollten dünne Wrapper über `overlay::popup` sein – gemeinsamer Look/Shortcuts.

- [ ] `date_picker.rs`, `date_range_picker.rs`, `month_picker.rs`: gemeinsames Kalender-Modal-Muster; `chrono`-Nutzung (kein `unwrap` außerhalb von Tests); einheitliche Shortcuts; Rand-/Monatswechsel-Logik.
- [ ] `color_picker.rs`, `slider.rs`: Wertebereiche/Clamping; Schrittweiten als benannte Konstanten. `color_picker.rs`: RGB/HSL/OKLCH-Modelle (Umschaltung via `m`), Gradient-Slider mit Marker, editierbares Hex-Feld, Palette-Presets, Hell/Dunkel-Vorschau; Rückgabe `ColorExit` (`Enter`=Done, `Esc`=Back, `s`=Swatches, Ctrl+Q=Quit); Modell-Konvertierungen als SSOT in `theme::color` (`to_hsl`/`from_hsl`/`to_oklch`/`from_oklch`).
- [ ] `swatches.rs`: Multi-Mode-Farb-Picker (`m` cyclet Names/Grid/Grays/Palette; Farbe via `Color::distance` mitgenommen); Names/Palette als Liste über `list::render`, Grid (Hue×Sättigung, `[`/`]` = Helligkeit) und Grays als Farbraster; `/`-Filter in Names, Fokus-Vorschau. `color_chooser` verbindet Swatch- und Picker-Ansicht (Wechsel-Schleife): `Enter` Swatch→Picker, `Esc`/`s` Picker→Swatch (Modus/Helligkeit bleiben erhalten), `Space` = direkt, `y` = kopieren; `swatch_picker` ist der Wrapper mit Start in der Swatch-Ansicht.
- [ ] `path_picker.rs`: **Pfad-Traversal absichern** – Pfade von außen mit `canonicalize()` + `starts_with()` prüfen (§7.9); Verzeichnis-Navigation robust; Scrollbar bei Überlauf.

## Phase 9 – Zusammengesetzte Widgets (`modal`, `form`, `finder`, `help`)

- [ ] `modal.rs` (`ModalSignal`, `confirm`/`select`/`multi_select`/`number_input`/`message`): der gemeinsame Modal-Satz als SSOT – nicht pro Aufrufstelle nachgebaut; destruktive Aktionen über `confirm`; `ModalSignal::Quit`-Propagation konsistent.
- [ ] `form.rs`: alle Felder sichtbar; `Tab/BackTab` umlaufend, `Ctrl+S`/`Esc`; Fokus-Tint; Dirty-Marker `*`/Reset `r`; externer Editor `Ctrl+G`; Pan-Modus. Dichte Dispatch-Funktion auf SLAP prüfen.
- [ ] `finder.rs`: Fuzzy-Filter über `fuzzy`; scrollbare Liste; Auswahl-Rückgabe.
- [ ] `help.rs`: Voll-Overlay mit allen Shortcuts, scrollbarer Fuzzy Finder; Footer weist mit `? help` darauf hin. Beim Ändern von Shortcuts Footer/Hilfe/Doku synchron halten.

## Phase 10 – Crate-Root (`src/lib.rs`, `tests/render.rs`)

Zuletzt, weil hier alles zusammenläuft:

- [ ] `lib.rs`: Modul-Deklarationen vollständig/konsistent; öffentliche Re-Exports und `prelude` minimal und bewusst (Breaking-Change-Fläche); crate-weite `#![warn(clippy::pedantic)]` und die drei `#![allow(...)]`-Blöcke (cast-Lints, `must_use_candidate`/`missing_errors_doc`) mit aktueller Begründung bestätigen; Modul-Doc mit lauffähigem Beispiel-Doctest aktuell.
- [ ] `tests/render.rs`: Integrations-Render-Tests decken die zentralen Widgets ab; ggf. Lücken benennen (bewerten, nicht zwingend erweitern – YAGNI).

## Phase 11 – Querschnitt & Abschluss

- [ ] **`#[allow]`-Inventur:** die crate-weiten Allows in `lib.rs` (cast-Lints, `must_use_candidate`, `missing_errors_doc`) bewusst bestätigen; das lokale `#[allow(clippy::too_many_arguments)]` in `list.rs:39` (auf `render_boxed`) nach Phase 7 möglichst reduzieren (Parameter in Struct gruppieren) oder bewusst belassen + Begründung aktuell.
- [ ] **Doku-Sync:** `README.md` / `DEVELOPMENT.md` / `API.md` und die rustdoc-Kommentare gegen den aufgeräumten Stand; Footer/Hilfe/Shortcuts-Verweise konsistent; `prelude`-Beschreibung stimmt.
- [ ] **Tests:** durch Refactors berührte Pfade getestet; alle grün (inkl. Doctests).
- [ ] **Abschluss-Gates:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` – alles grün.
- [ ] Commit-Nachricht(en) im Conventional-Commits-Stil vorschlagen (kein Auto-Commit gemäß CLAUDE.md §11).

Konkrete Kandidaten:
- [ ] **`table.rs` (~1170 Zeilen):** mit Abstand die größte Datei. Render-/Navigations-Verantwortungen auf SLAP prüfen und ggf. in kohärente Einheiten zerlegen (Sticky-Header, Spalten-Layout, Body-Render, Navigation). Reines Refactoring, Verhalten identisch – Render-Tests müssen ohne Neu-Generierung bestehen.
- [ ] **`list.rs:39` `#[allow(clippy::too_many_arguments)]` (auf `render_boxed`):** die aufgefächerte Signatur ist ein Indikator für zu viele Parameter (§2.5). Prüfen, ob zusammengehörige Parameter in ein Struct gruppiert werden können, sodass das `#[allow]` entfällt.
- [ ] **Editier-Kern-Duplizierung:** gegenchecken, dass `textarea.rs` die Editier-Logik wirklich aus `input.rs` bezieht und nichts parallel nachbaut (SSOT/DRY der Textfeld-Shortcuts, §7.10).

## Verifikation

Nach jeder Schicht und am Ende: `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` + `cargo test` grün. Reine Refactorings dürfen das Verhalten nicht ändern – die Render-/Integrationstests (`tests/`) und Doctests müssen ohne Neu-Generierung bestehen; nur bei bewusster Verhaltens-/Layout-Änderung Snapshots/Erwartungen gezielt aktualisieren.

## Hinweise / Nicht-Ziele

- **`ratada` ist die Bibliothek:** Änderungen an `pub`-Signaturen sind Breaking Changes für konsumierende Apps – bewusst und dokumentiert vornehmen; die öffentliche API klein halten.
- **Kein Binary/keine Domäne:** es gibt bewusst keine `main`, keine CLI, keine Persistenz – nur die generischen TUI-Bausteine. Nichts davon „nachrüsten" (YAGNI).
- Bekanntes, separat: CI-Workflow (`cargo audit`, §7.9) – bewusst außerhalb dieses Aufräum-Durchlaufs gelassen (sofern nicht vorhanden).
- KISS/YAGNI vor „mein Stil": lokalen Stil respektieren, nur anfassen was die Aufgabe erfordert, Refactoring von Verhalten trennen (CLAUDE.md §3).
