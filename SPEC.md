# Tasker.md – Spezifikation

> Desktop-Dashboard für einen Obsidian-Vault nach GTD, mit Jira- und Kalender-Einblendung.
> Status: Entwurf · Sprache der App: Deutsch (Englisch vorbereitet)

## 1. Ziel und Nicht-Ziele

### Ziel
Eine Desktop-App für **Windows und macOS**. Sie fasst an einer Stelle zusammen, was für einen einzelnen Nutzer gerade wichtig ist:

- Aufgaben und Projekte aus einem Obsidian-Vault (Markdown, Syntax des Tasks-Plugins)
- die eigenen Jira-Tickets (nur Anzeige und Links)
- Termine aus dem Google Kalender (nur lesen)

Mit den Aufgaben kann der Nutzer direkt arbeiten: abhaken, planen, anlegen und in Obsidian öffnen. Der Vault bleibt die einzige Quelle der Wahrheit, die App ist nur eine Oberfläche darüber.

Die App ist für eine bestimmte Arbeitsweise gebaut: GTD mit PARA-Ordnern, Kontexten, „Warten auf“, Abhängigkeiten zwischen Aufgaben und Projekt-Reviews. Sie ist kein Werkzeug für jede beliebige Vault-Struktur. Diese **Logik** ist fest eingebaut. Alle **konkreten Werte** wie Pfade, Ordnernamen, Bereiche, Kontexte, Jira-Instanz und Kalender stehen in einer lokalen Konfiguration (siehe 11).

### Nicht-Ziele
- Mobile Apps
- Mehrbenutzer- oder Team-Funktionen
- Allgemeine Unterstützung anderer Aufgaben-Syntaxen oder Vault-Konventionen
- Jira-Daten in den Vault kopieren
- In Jira schreiben (v1; Worklog auf Knopfdruck ist für P2 vorgesehen)
- In den Kalender schreiben (v1)
- Eigene Synchronisation. Der Vault wird mit den Mitteln des Nutzers synchronisiert, z. B. Cloud-Speicher oder Obsidian Sync.

## 2. Abgleich mit bestehenden Werkzeugen

Untersucht wurden TaskForge, Super Productivity und Todoist. Keines deckt die Kombination ab aus Vault als Quelle, GTD-Logik mit Abhängigkeiten und Jira-Einblendung.

Einstufung: **MVP** = erste Version · **P2** = Phase 2 · **–** = nicht geplant

| Funktion | TaskForge | Super Productivity | Todoist | Tasker.md |
|---|---|---|---|---|
| Vault (Markdown) ist die Quelle | ✅ | ❌ eigene DB (Obsidian-Brücke nur per Plugin FlowTask, `#sp`-Tag, Marker in der Zeile) | ❌ Cloud (Obsidian nur per Community-Sync-Plugin) | ✅ **MVP** |
| Tasks-Syntax inkl. 🆔/⛔ Abhängigkeiten | ✅ | ❌ | ❌ | ✅ **MVP** |
| Heute / Überfällig / Wichtig / Diese Woche | ✅ | 🟡 | ✅ | ✅ **MVP** |
| Nächste Aktionen nach Kontext, Warten auf, Blockiert | 🟡 Filter | ❌ | 🟡 Labels und Filter | ✅ **MVP** |
| Projekt-Review („Review fällig“, „ohne nächste Aktion“) | ❌ | ❌ | ❌ | ✅ **MVP** |
| Inbox | ✅ | ✅ | ✅ | ✅ **MVP** (Zähler, Liste, Schnellerfassung) |
| Kanban / Boards | ✅ (Premium) | ✅ | ✅ | **P2** |
| Spaces / eigene Ansichten | ✅ (Premium) | ✅ | ✅ Filtersprache | ✅ **MVP** als eigene Filter (6.3) |
| Kalender lesen | ✅ | ✅ Google, CalDAV | ✅ Google | ✅ **MVP** Google · **P2** Outlook/M365 |
| Kalender schreiben / Timeboxing im Kalender | 🟡 (Premium) | ✅ | ✅ | **P2** (nur nach Bestätigung) |
| Tages-Zeitleiste / Timeboxing | ❌ | ✅ | 🟡 Dauer und Kalender | ✅ **MVP** (5.1) |
| Zeiterfassung / Pomodoro | ❌ | ✅ (Stärke) | ❌ | ✅ **MVP** Fokus-Timer, Zeiten lokal und im Vault-Log (6.5) |
| Zeitberichte (CSV/JSON) | ❌ | ✅ | ❌ | **P2** |
| Jira | ❌ | ✅ Import per JQL, Worklog, Status | ❌ nur über Drittanbieter | ✅ **MVP** Anzeige, Links, Vault-Markierung |
| Zeiten für Jira-Buchung | ❌ | ✅ Worklog | ❌ | ✅ **MVP** Tageszusammenfassung zum Kopieren (6.5) · **P2** Buchen auf Knopfdruck |
| Status zurück nach Jira | ❌ | ✅ | ❌ | – |
| Schnelleingabe in natürlicher Sprache | 🟡 | 🟡 | ✅ (Stärke) | ✅ **MVP** (6.2) |
| Umplanen per Drag & Drop | ✅ | ✅ | ✅ | ✅ **MVP** (5.5) |
| Globales Tastenkürzel, Tastaturbedienung | 🟡 | ✅ | ✅ | ✅ **MVP** (6.6) |
| Wiederkehrende Aufgaben 🔁 | ✅ | ✅ | ✅ | ✅ **MVP** (einfache Regeln) |
| Meeting-Notizen | ✅ (Premium) | ❌ | ❌ | – |
| Widgets, Erinnerungen per Push | ✅ | ✅ | ✅ | – |
| Mobile | ✅ | ✅ | ✅ | – |
| CLI / MCP-Server | ✅ | 🟡 REST-API | ✅ API | **P2** |
| Datenschutz | lokal | lokal | Cloud | lokal, keine Telemetrie |

**Gegen eine Übernahme spricht jeweils:**
- **TaskForge:** kein Jira, kein Dashboard über mehrere Quellen
- **Super Productivity:** nicht Markdown-basiert, keine GTD-Abhängigkeiten
- **Todoist:** Cloud statt Vault, 🆔/⛔ gehen verloren, kein Jira, Firmendaten liegen bei einem Drittanbieter

## 3. Datenquellen und Regeln

### 3.1 Vault-Scan
- Gescannt wird der Vault-Ordner aus `vault.path`, alle `*.md`-Dateien.
- **Ausschlüsse** sind konfigurierbar (`scan.exclude`, Glob-Muster). Standard: Vorlagen, Archiv, Wissensordner, `.obsidian/`, `.claude/` sowie beliebige weitere Muster (z. B. kopierte Fremd-Dokumentation).
- Die Tagesnotizen (`folders.journal`) fließen nur in die Ansicht „Heute“ ein, nicht ins Dashboard.
- Dateinamen und Pfade werden **NFC-normalisiert**, weil macOS Umlaute zerlegt (NFD) speichern kann.
- Ein Datei-Watcher aktualisiert den Index live.

### 3.2 Aufgaben-Parser (Tasks-Plugin-Syntax)
| Element | Bedeutung |
|---|---|
| `- [ ]` / `- [x]` | offen / erledigt (andere Status-Zeichen werden als offen gelesen und unverändert gelassen) |
| `📅 YYYY-MM-DD` | fällig (nur echte Termine) |
| `⏳ YYYY-MM-DD` | geplant / Wiedervorlage |
| `🛫 YYYY-MM-DD` | Start |
| `✅ YYYY-MM-DD` | erledigt am |
| `➕ YYYY-MM-DD` | erstellt am |
| `🔺 ⏫ 🔼 🔽 ⏬` | Priorität (höchste … niedrigste) |
| `🔁 every …` | Wiederholung |
| `🆔 kurz-id` | ID für Abhängigkeiten |
| `⛔ id1,id2` | hängt ab von (mehrere mit Komma) |
| `#@kontext` | Kontext (Liste in `contexts`) |
| `#warten` | wartet auf jemand anderen (Tag in `waitingTag`) |

- Eine Aufgabe ist **blockiert**, wenn mindestens eine ⛔-ID auf eine **offene** Aufgabe zeigt. Zeigt eine ⛔-ID auf eine unbekannte ID, erscheint ein Hinweis.
- Leere Checkboxen (`- [ ] ` ohne Text) werden ignoriert.
- Eingerückte Aufgaben werden als Unteraufgaben ihres Elternpunkts erkannt.

### 3.3 Projekte und Bereiche
- **Projekt:** eine Notiz mit Frontmatter `typ: projekt`, Feldnamen konfigurierbar. Felder: `status` (`aktiv | wartend | irgendwann | erledigt`), `bereich`, `start`, `review`, optional `termin`.
- **Optionales Feld `jira`** (Name über `frontmatter.jira`): eine Liste von Ticket-Schlüsseln, die zu diesem Projekt **gehören**, z. B. `jira: [ABC-123, ABC-456]`.
  - Es stehen nur Schlüssel drin, keine URLs. Die URL ergibt sich aus `jira.baseUrl`.
  - Beliebig viele Tickets pro Projekt sind möglich. Ein Ticket darf auch mehreren Projekten zugeordnet sein, dann zeigt das Badge alle Projekte.
  - Erlaubt sind auch ein einzelner Wert (`jira: ABC-123`) und die YAML-Listenform.
  - Ungültige Einträge erzeugen einen Hinweis, ohne Abbruch.
  - Die App **liest** das Feld in v1 nur. Gepflegt wird es in Obsidian (Eigenschaften) oder von Hand.
- Abgeleitete Werte: Tage seit dem letzten Review, Tage bis zum Termin, **Review fällig** (Schwelle `projects.reviewAfterDays`, Standard 14) und **ohne offene nächste Aktion**.
- **Bereich:** eine Notiz mit `typ: bereich`. Aufgaben stehen dort unter einer konfigurierbaren Überschrift (Standard `## Aufgaben ohne Projekt`), in Projekten unter `## Nächste Schritte`.
- **Arbeit/Privat:** Eine Aufgabe gehört zu „Arbeit“, wenn ihr Projekt oder Bereich in `areas.work` steht oder ihr Pfad unter einem der `areas.workPaths` liegt. Alles andere ist „Privat“.

## 4. Jira-Anbindung (nur lesen)
- Atlassian Cloud REST API v3, Endpunkt `/rest/api/3/search/jql`.
- **Auth (MVP): E-Mail plus persönlicher API-Token.** Damit liest die App mit den Rechten des Nutzers.
  - Der Token wird im Einrichtungsdialog eingegeben und **nur im OS-Schlüsselbund** gespeichert, nie in der Konfiguration, in Logs oder im Repo.
  - **Empfohlen ist ein Token mit eingeschränkten Rechten** (Scopes), nur lesend, z. B. `read:jira-work` und `read:jira-user`, mit Ablaufdatum. Für P2-Worklogs kommt `write:jira-work` dazu.
    - Solche Tokens laufen über `https://api.atlassian.com/ex/jira/<cloudId>/…`. Die `cloudId` ermittelt die App einmalig über `<jira.baseUrl>/_edge/tenant_info`.
    - Klassische Tokens ohne Scopes gehen direkt gegen `jira.baseUrl`.
    - Welcher Typ vorliegt, steht in `jira.tokenType` (`scoped` | `classic`, Standard `scoped`). Vor der Umsetzung ist zu prüfen, welche Scopes die Suche wirklich braucht.
  - **Läuft der Token ab** oder ist er ungültig (401/403), zeigt die App den Cache-Stand mit einem deutlichen Hinweis „Jira-Token erneuern“. Sie bricht nicht ab.
  - Verbindungstest im Einrichtungsdialog: `GET /rest/api/3/myself` zeigt den angemeldeten Namen.
- **P2, alternative Quellen** über `jira.source` (`api-token` ist Standard):
  - `cache-file`: Die App liest eine JSON-Datei in dokumentiertem Format, die ein externes Programm befüllt (z. B. eine geplante KI-Aufgabe mit Jira-Konnektor oder ein Skript). Das ist für Umgebungen ohne API-Tokens gedacht.
  - OAuth 2.0 (3LO) oder ein MCP-Server, falls der Admin das freischaltet
- **Standard-JQL** (konfigurierbar, weitere Abfragen speicherbar):
  `assignee = currentUser() AND statusCategory != Done ORDER BY priority DESC, updated DESC`
- Angezeigt werden Schlüssel, Typ, Priorität, Status, zuletzt geändert und Titel, gruppiert nach Status. Jeder Schlüssel ist ein Link nach `<jira.baseUrl>/browse/<KEY>`.
- **Cache** nur im App-Datenordner, nie im Vault. Beim Aktualisieren und alle `jira.refreshMinutes` wird neu geladen.
- **Vault-Markierung:**
  - Der Index sucht Ticket-Schlüssel in allen Notizen (Regex `\b[A-Z][A-Z0-9]+-\d+\b`).
  - Als Treffer zählen nur Schlüssel, die in den Jira-Ergebnissen vorkommen oder deren Präfix in `jira.projectKeys` steht. So fallen Fehltreffer wie `UTF-8`, `DR-05` oder `ADR-009` weg.
  - Ein Ticket mit Fundstellen bekommt ein Badge mit der Zahl der Notizen. Ein Klick listet die Fundstellen mit Kontextzeile, von dort öffnet sich die Notiz in Obsidian.
  - Umgekehrt zeigen Projekte die Tickets an, die in ihnen erwähnt werden, mit Live-Status aus Jira.
  - **Zwei Stufen der Verknüpfung:**
    - **zugeordnet:** Das Ticket steht im Feld `jira` eines Projekts (3.3).
    - **erwähnt:** Der Schlüssel kommt irgendwo im Text einer Notiz vor.
    - Zugeordnete Tickets zeigen in der Jira-Ansicht ein eigenes Badge „→ <Projekt>“, Erwähnungen weiterhin „in N Notizen“.
    - Ohne das Feld funktioniert alles über die Erwähnungen, nur weniger genau.

## 5. Ansichten

### 5.1 Heute
- Termine des Tages, Aufgaben mit ⏳ bis heute (Liegengebliebenes rutscht mit), 📅 bis heute, Wichtig (Priorität ⏫/🔼, nicht blockiert).
- **Tages-Zeitleiste:**
  - Die Termine stehen als feste Blöcke auf einer Zeitachse (`timeline.start` bis `timeline.end`, Standard 07–20 Uhr).
  - Daneben gibt es die Spalte „Heute geplant, ohne Uhrzeit“.
  - Eine Aufgabe wird per Drag & Drop auf die Zeitachse gezogen und bekommt damit einen Zeitblock (Standard 30 min, Länge ziehbar).
  - Freie Lücken zwischen Terminen sind hervorgehoben, eine Jetzt-Linie zeigt die aktuelle Uhrzeit.
  - Am Block lässt sich die Aufgabe erledigen, verschieben oder der Fokus-Timer starten.
  - **Zeitblöcke liegen nur im App-Datenordner**, weil die Tasks-Syntax keine Uhrzeit kennt. Der Vault bleibt unverändert.
  - Liegt eine Aufgabe nicht mehr auf heute oder ist sie erledigt, verschwindet ihr Block. Nicht erledigte Blöcke von gestern erscheinen als „liegengeblieben“.

### 5.2 Dashboard
Mit dem Umschalter **Arbeit / Privat / Alle** gibt es folgende Abschnitte:
1. Wichtig: Priorität gesetzt, nicht blockiert, nach Priorität sortiert
2. Überfällig: 📅 vor heute
3. Diese Woche fällig: 📅 heute bis +7 Tage
4. Nächste Aktionen nach Kontext: offen, ohne 📅, ohne Priorität, nicht blockiert, nicht wartend, gruppiert nach Kontext
5. Warten auf: Tag `#warten`, gruppiert nach Notiz
6. Später dran (blockiert): gruppiert nach Notiz

### 5.3 Projekte
- Aktive Projekte gruppiert nach Bereich, mit Termin, Tagen bis zum Termin, letztem Review und Tagen seit dem Review
- Eigene Abschnitte für „Review fällig“ und „Wartend & Irgendwann“
- Warnung bei „Ohne nächste Aktion“
- Pro Projekt die offenen Aufgaben, darunter **zugeordnete** Jira-Tickets (Feld `jira`) mit Live-Status oben und **erwähnte** Tickets darunter

### 5.4 Jira
Siehe 4. Die Gruppen ergeben sich aus dem Status (z. B. In Arbeit, Testing, Review, Offen). Gespeicherte JQL-Abfragen stehen als Reiter bereit.

### 5.5 Demnächst
- Heute plus 13 Tage als Spalten, Termine eingeblendet
- Eine Aufgabe auf einen Tag zu ziehen **setzt ⏳ neu**. Ziehen auf „ohne Datum“ entfernt ⏳.
- **📅 wird nie per Drag & Drop geändert**, nur bewusst im Dialog.

### 5.6 Global
Seitenleiste mit den Ansichten und den gespeicherten Filtern. Dazu Suche, Inbox-Zähler, Aktualisieren-Knopf und Statusanzeige für Jira und Kalender (verbunden, Cache-Stand). Hell und dunkel.

## 6. Interaktionen

### 6.1 Aufgaben bearbeiten
- **Abhaken:** `[x]` setzen und ` ✅ YYYY-MM-DD` anhängen. Bei 🔁 wird die nächste Instanz eingefügt, wie es das Tasks-Plugin macht. Rückgängig ist möglich.
- **Planen:** ⏳ auf heute oder ein Datum setzen bzw. entfernen. Priorität auf ⏫, 🔼 oder keine setzen.
- **Anlegen:** Dialog mit Text, Kontext, Priorität, ⏳, 📅, 🔁 und Ziel. Ziel ist ein Projekt (Abschnitt „Nächste Schritte“) oder ein Bereich (Abschnitt „Aufgaben ohne Projekt“). Fehlt der Abschnitt, wird er am Ende der Notiz angelegt.
- **Inbox-Erfassung:** legt eine neue Notiz in `folders.inbox` an.
- **In Obsidian öffnen** (Klick oder `o`), zweistufig:
  - **Mit Plugin „Advanced URI“** (optional, nicht vorausgesetzt): Sprung genau auf die Zeile der Aufgabe über `obsidian://adv-uri?vault=<vault.name>&filepath=<pfad>&line=<zeile>`. Die Zeilennummer stammt aus dem live aktualisierten Index.
    - Ob das Plugin vorhanden ist, prüft die App **nur lesend**: Plugin-Ordner unter `.obsidian/plugins/` und Eintrag in `community-plugins.json`. Sie installiert und ändert dort nichts.
    - Abschaltbar über `obsidian.useAdvancedUri` (`auto` | `never`, Standard `auto`)
  - **Ohne Plugin:**
    - Die App öffnet die Notiz über `obsidian://open?vault=<vault.name>&file=<pfad>`, wenn möglich mit Sprung zur Überschrift des Abschnitts. Das muss vor der Umsetzung gegen die aktuelle Obsidian-Version geprüft werden.
    - Zusätzlich kopiert sie den Aufgabentext in die Zwischenablage, damit `Ctrl/Cmd+F`, `Ctrl/Cmd+V` direkt zur Aufgabe führt.
    - Einmalig erscheint ein Hinweis, dass der Zeilensprung mit dem Plugin genauer ginge. Der Hinweis nennt auch das Sicherheitsrisiko: Das Plugin erlaubt, über Links Notizen zu verändern und Befehle auszuführen.
  - Block-IDs (`^id`) werden **nicht** an Aufgaben angehängt, weil das jede Zeile im Vault verändern würde.
- Die Emoji-Reihenfolge beim Schreiben folgt dem Tasks-Plugin.

### 6.2 Schnelleingabe
Ein Eingabefeld übersetzt natürliche Sprache in Tasks-Syntax:

| Eingabe | Ergebnis |
|---|---|
| `heute`, `morgen`, `übermorgen`, `montag` … `sonntag`, `in 3 tagen`, `YYYY-MM-DD` | Datum, standardmäßig als `⏳` |
| `fällig …` / `due …` / `bis …` vor dem Datum | `📅` statt `⏳` |
| `24.12.`, `am mo`, `in 2 wochen` | weitere Datumsformen |
| `!1` / `!2` / `!3` | `⏫` / `🔼` / `🔽` |
| `@telefon` | `#@telefon` (Vorschläge aus `contexts`) |
| `warten` | `#warten` |
| `jeden montag`, `jede woche`, `täglich` | `🔁 every monday` usw. |
| `>Name` (am Ende, Leerzeichen erlaubt) | Ziel-Projekt oder -Bereich, mit Autovervollständigung aus dem Index |

Beispiel: `Müller anrufen morgen @telefon !1 >Gartenhaus` ergibt `- [ ] Müller anrufen #@telefon ⏫ ⏳ <morgen>` unter „Nächste Schritte“ im Projekt Gartenhaus.

- Ohne `>Ziel` landet der Text als neue Notiz in der Inbox.
- Vor dem Speichern zeigt eine **Vorschau** die erzeugte Zeile und die Zieldatei.
- Erkannt werden Deutsch und Englisch.

### 6.3 Eigene Filter
Gespeicherte Ansichten werden mit einer kleinen Abfragesprache definiert, z. B. `kontext:@telefon & bereich:arbeit & !blockiert`.

- Operatoren: `!` vor `&` vor `|`, dazu Klammern. Zwei Begriffe ohne Operator sind mit „und“ verknüpft. Groß- und Kleinschreibung zählt nicht.
- Felder:
  - `kontext`, `tag`, `bereich`, `projekt`, `pfad`
  - `prio` (`hoch | mittel | niedrig | keine`, ohne Wert: Priorität gesetzt)
  - `fällig` und `geplant` (`heute | überfällig | <7d | keine | JJJJ-MM-TT`, ohne Wert: Datum gesetzt)
  - `blockiert`, `warten`, `jira` (Aufgabe oder Notiz erwähnt ein Ticket), `jira:ABC-123` (Zeile oder Notiz erwähnt das Ticket, oder das Projekt hat es im Feld `jira`)
  - `bereich:arbeit` bzw. `bereich:privat` passt auch auf den Bereichsschalter (SPEC 3.2), sonst auf den Namen des Bereichs
  - Werte mit Leerzeichen in Anführungszeichen, z. B. `projekt:"Release 2.0"`; ein Wort ohne Feld sucht in der Beschreibung
- Ein Filter zeigt offene Aufgaben ohne Journal; der Schalter Arbeit/Privat/Alle gilt zusätzlich. Fehler werden mit Position angezeigt, ein fehlerhafter Filter lässt sich nicht speichern.
- Die festen Abschnitte aus 5.2 lassen sich als solche Filter ausdrücken; ein Akzeptanztest sichert, dass beide dieselbe Menge liefern (Kriterium 7).
- Gespeicherte Filter erscheinen in der Seitenleiste und liegen in der lokalen Konfiguration.

### 6.4 Schreibregeln (verbindlich)
- Vor jedem Schreiben wird die Zielzeile neu gelesen und mit dem Index verglichen. Bei Abweichung wird abgebrochen, neu geladen und ein Hinweis gezeigt.
- Geändert wird nur die betroffene Zeile, bzw. es wird eine neue Zeile eingefügt. Die App **löscht nie** Dateien oder Inhalte.
- Zeilenenden, Encoding (UTF-8) und abschließende Leerzeile bleiben erhalten.
- Konfliktdateien des Cloud-Syncs werden erkannt und gemeldet, nicht verarbeitet.
- Der Obsidian-Konfigurationsordner `.obsidian/` bleibt unangetastet.
- Ein Rückgängig-Verlauf deckt die letzten Änderungen der Sitzung ab.

### 6.5 Fokus-Timer (in den Einstellungen abschaltbar)
- Wird an einer Aufgabe oder einem Zeitblock gestartet, als Pomodoro (Standard 25/5 min, einstellbar) oder als freie Stoppuhr. Pause und Stopp sind möglich.
- Sichtbar im Fenster und im Tray bzw. in der Menüleiste. Am Ende kommt eine System-Benachrichtigung mit der Frage „Erledigt abhaken?“.
- Die erfasste Zeit wird lokal im App-Datenordner gespeichert: pro Aufgabe sowie als Tages- und Wochensumme. Für Jira gibt es eine Tageszusammenfassung zum Kopieren. Gebucht wird in v1 nicht, siehe unten.
- Die Zuordnung zur Aufgabe läuft über Datei plus 🆔, sonst über einen Hash des Aufgabentexts. Ändert sich der Text, kann die Zuordnung verloren gehen, das ist akzeptiert.

#### Fokus-Zeiten im Vault (Log)
Die Fokus-Zeiten werden zusätzlich in die Notiz geschrieben, zu der die Aufgabe gehört. So bleibt im Vault nachvollziehbar, wie viel Zeit in ein Projekt geflossen ist.

- **Ziel:**
  - Bei Projektaufgaben ist das die Projektnotiz, bei Aufgaben ohne Projekt die Bereichsnotiz.
  - Geschrieben wird jeweils unter die Überschrift `focus.logSection` (Standard `## Log`).
  - Fehlt die Überschrift, wird sie am Ende der Notiz angelegt.
  - Aufgaben aus Tagesnotizen oder der Inbox haben kein Ziel. Ihre Zeit bleibt nur lokal.
- **Format:** **eine Zeile pro Aufgabe und Tag**, nicht eine Zeile pro Sitzung. Beispiel:
  `- 2026-11-11 ⏱ 50 min (2 Sitzungen) · Testszenarien prüfen`
  - Weitere Sitzungen am selben Tag aktualisieren diese Zeile (Minuten und Zahl der Sitzungen). Es kommt keine neue Zeile dazu.
  - Erkannt wird die Zeile über Datum, das Zeichen ⏱ und den Aufgabentext.
  - Das Format ist über `focus.logFormat` mit den Platzhaltern `{date}`, `{minutes}`, `{sessions}` (wird zu „1 Sitzung“ bzw. „2 Sitzungen“) und `{task}` einstellbar.
- **Position:** Neue Zeilen kommen ans Ende des Log-Abschnitts, passend zu den bestehenden chronologischen Log-Einträgen der Vorlage.
- **Zeitpunkt:** beim Beenden einer Sitzung (Pomodoro abgelaufen oder Stopp)
  - Danach erscheint eine Meldung „Ins Log geschrieben“ mit der Möglichkeit, es rückgängig zu machen.
  - Sitzungen unter `focus.minMinutes` (Standard 5) werden nicht geschrieben.
  - Pausen zählen nicht mit.
- **Schreibregeln** wie in 6.4: Die Zeile wird vorher neu gelesen, nur diese eine Zeile wird geändert oder eingefügt, gelöscht wird nie.
  - Ist die Datei zwischenzeitlich gesperrt oder in Konflikt, bleibt die Zeit lokal vorgemerkt. Sie wird beim nächsten Schreibvorgang nachgetragen.
- **Abschaltbar** über `focus.writeToVault` (Standard `true`)
- Die Projektansicht (5.3) zeigt die Fokus-Summe pro Projekt: diese Woche und gesamt. Die Gesamtsumme wird aus den Log-Zeilen gelesen, damit sie auch Einträge erfasst, die von Hand geschrieben wurden.

#### Zeiten für Jira (Tageszusammenfassung)
In v1 schreibt die App **nicht** in Jira. Sie bereitet die Fokus-Zeiten aber so auf, dass das Buchen in Jira nur noch Abschreiben oder Kopieren ist.

- **Zuordnung Aufgabe → Ticket**, in dieser Reihenfolge:
  1. Ticket-Schlüssel in der Aufgabenzeile (z. B. `… ABC-123`)
  2. Frontmatter `jira` des Projekts mit genau einem Eintrag
     - Bei **mehreren Einträgen** fragt die App nicht nach, sondern ordnet die Zeit der Gruppe „Projekt mit mehreren Tickets“ zu. Dort ist eine Auswahl nur aus den Tickets dieses Projekts vorbelegt.
     - Die gewählte Zuordnung merkt sich die App lokal pro Aufgabe. Weitere Sitzungen an derselben Aufgabe gehen dann automatisch an dasselbe Ticket.
     - Tipp für Nutzer: Steht der Schlüssel in der Aufgabenzeile, ist die Zuordnung eindeutig (Regel 1).
  3. Das Projekt erwähnt genau ein Ticket. Die Zuordnung gilt dann nur als **vermutet** und wird markiert.
  4. Sonst landet die Zeit unter „ohne Ticket“. Dort lässt sich per Auswahl ein Ticket aus der Jira-Liste zuordnen. Diese Zuordnung wird nur lokal gemerkt, im Vault ändert sich nichts.
- **Ansicht „Zeiten heute“**, erreichbar aus „Heute“, mit Datumswahl für frühere Tage:
  - je Ticket: Schlüssel, Titel, Summe, Zahl der Sitzungen, erste Startzeit und die beteiligten Aufgaben
  - Rundung einstellbar über `jira.worklog.roundToMinutes` (Standard 15, `0` = aus), auf das nächstgelegene Vielfache, mindestens ein Vielfaches. Gerundet wird pro Ticket und Tag, die Rohwerte stehen daneben.
  - Pro Zeile gibt es „Kopieren“ (z. B. `ABC-123 · 0h 45m · Testszenarien prüfen`) und „In Jira öffnen“. Dazu kommt „Alles kopieren“ für den ganzen Tag.
  - Der Status „gebucht“ lässt sich von Hand setzen und wird lokal gemerkt. So sieht der Nutzer, was noch offen ist.
- Hinweis in der Ansicht: Fokus-Zeit erfasst nur konzentrierte Blöcke. Meetings und Abstimmungen fehlen, die Werte sind deshalb nicht der volle Aufwand.

**P2: Buchen auf Knopfdruck** (nur wenn die Organisation native Jira-Worklogs nutzt und erlaubt):
- „Buchen“ öffnet pro Ticket einen Prüfdialog mit Dauer, Startzeit und Kommentar, alle Felder änderbar. Erst die Bestätigung schickt den Eintrag über `POST /rest/api/3/issue/{key}/worklog`. Es gibt **nie automatische Buchungen**.
- Dafür ist ein Token mit Schreibrecht für Worklogs nötig. Diese Funktion bleibt standardmäßig aus (`jira.worklog.enabled = false`).
- Schutz vor Doppelbuchungen: Gebuchte Einträge werden lokal mit der Worklog-ID gemerkt, ein zweites Buchen wird blockiert. „Rückgängig“ löscht nur den eigenen, gerade angelegten Worklog, und das nur nach Rückfrage.

### 6.6 Tastatur und globales Tastenkürzel
- **Globales Tastenkürzel** (Standard `Ctrl+Alt+Space` bzw. `Cmd+Option+Space`, einstellbar): öffnet die Schnellerfassung als kleines Fenster, auch wenn die App im Hintergrund läuft.
- **In der App:**

| Taste | Aktion |
|---|---|
| `j` / `k` | Auswahl nach unten / oben |
| `x` | abhaken |
| `t` | für heute planen |
| `d` | Datumsdialog |
| `1` / `2` / `0` | Priorität ⏫ / 🔼 / keine |
| `o` | in Obsidian öffnen |
| `f` | Fokus-Timer starten |
| `/` | Suche |
| `q` | Schnelleingabe |
| `g h` · `g d` · `g p` · `g j` · `g n` | Heute · Dashboard · Projekte · Jira · Demnächst |
| `?` | Übersicht der Tastenkürzel |

## 7. Google Kalender (nur lesen)
- OAuth 2.0 Desktop-Flow (Loopback-Redirect), Scope `calendar.readonly`
- Client-ID und Secret stammen aus einem eigenen Google-Cloud-Projekt des Nutzers. Sie werden beim Einrichten eingetragen und nicht mit der App ausgeliefert. Die Anleitung steht in der README.
- Die Kalender werden in den Einstellungen ausgewählt (`calendar.google.calendarIds`).
- Termine des Tages und der nächsten 14 Tage. Ganztägige Termine erscheinen als Leiste.
- **P2:** Outlook/Microsoft 365 über Microsoft Graph, z. B. für Firmentermine

## 8. Technik
- **Tauri 2**, Zielsysteme Windows 10/11 und macOS (Apple Silicon und Intel)
- **Rust-Backend:**
  - Vault-Scan und Index, Parser
  - Datei-Watcher (`notify`), zeilengenaues Schreiben
  - HTTP-Clients für Jira und Google
  - Schlüsselbund (`keyring`), globales Tastenkürzel (`tauri-plugin-global-shortcut`), Tray und Benachrichtigungen
- **Frontend:** React, TypeScript und Vite, Drag & Drop für Zeitleiste und „Demnächst“
- **Persistenz:**
  - JSON-Konfiguration im App-Datenordner
  - Caches und lokale Daten (Jira-Cache, Zeitblöcke, Fokus-Zeiten) als JSON oder SQLite im App-Datenordner
- **Tests:**
  - Parser- und Schreib-Tests gegen `examples/demo-vault/` (Golden Files)
  - Jira- und Kalender-Antworten gemockt

### Nicht-funktionale Anforderungen
- Start und erster Index in unter 2 s bei etwa 2 000 Notizen
- Offline nutzbar: Vault plus zuletzt gecachte Jira- und Kalenderdaten
- Keine Telemetrie, keine Absturzberichte an Dritte

## 9. Veröffentlichung und Datenschutz (verbindlich)
Das Repository ist öffentlich. **Es enthält keine persönlichen Daten.**

- **Konfiguration** liegt nur im App-Datenordner (`%APPDATA%\md.tasker.app\config.json` bzw. `~/Library/Application Support/md.tasker.app/config.json`, abgeleitet vom App-Identifier), nie im Repo. Beim ersten Start legt ein Einrichtungsdialog sie an. `config.example.json` im Repo enthält nur Platzhalter.
- **Geheimnisse** (Jira-Token, Google-OAuth-Tokens, Google-Client-Secret) liegen nur im OS-Schlüsselbund.
- **Lokale Daten** (Caches, Zeitblöcke, Fokus-Zeiten) liegen nur im App-Datenordner.
- **Logs** enthalten keine Aufgabentexte, Dateipfade, Ticket-Titel oder Kalenderinhalte, sondern nur Zähler und Fehlercodes. Einen ausführlichen Debug-Modus gibt es nur lokal und nur nach ausdrücklichem Einschalten.
- **Tests, Beispiele und Screenshots** verwenden ausschließlich den erfundenen `examples/demo-vault/` und erfundene Jira- und Kalenderdaten.
- **Private Arbeitsnotizen** liegen im Ordner `local/`, der per `.gitignore` ausgeschlossen ist.
- **Scan vor jedem Commit:** `gitleaks` als Pre-commit-Hook mit eigenen Regeln. Erkannt werden u. a. konkrete Atlassian-Domains (alles außer dem Platzhalter `<firma>`), Benutzer-Heimatverzeichnisse unter Windows und macOS, Pfade in Cloud-Sync-Ordnern und E-Mail-Adressen. Dieselbe Prüfung läuft als GitHub Action.

## 10. Phasen
- **MVP:** Abschnitte 3 bis 7 und 9
- **P2:**
  - Outlook/M365-Kalender
  - Zeitblöcke als Termine in Google anlegen (nur nach Bestätigung von Datum und Titel)
  - Worklog nach Jira buchen, auf Knopfdruck mit Prüfdialog (6.5)
  - Aus einem Jira-Ticket eine Vault-Aufgabe mit Link erzeugen
  - Ansicht für das Weekly Review: Inbox, Projekte ohne Aktion, alte `#warten`-Einträge, Review fällig. Dazu Hinweise aus dem Feld `jira`:
    - „zugeordnetes Ticket ist erledigt, Projekt noch aktiv?“
    - „dir zugewiesenes Ticket gehört zu keinem Projekt“
  - Aktion „Ticket einem Projekt zuordnen“ in der Jira-Ansicht: ergänzt das Feld `jira` nach Rückfrage. Dabei wird nur dieses eine Feld im Frontmatter geändert, das übrige YAML bleibt byte-gleich.
  - Tagesnotiz aus Vorlage erzeugen
  - Kanban
  - Zeitberichte
  - CLI und MCP-Server
  - Linux
- **Nicht geplant:** Mobile, Mehrbenutzer, Meeting-Notizen

## 11. Konfiguration
Die Schlüssel stehen in `config.example.json`:

| Schlüssel | Zweck |
|---|---|
| `vault.path`, `vault.name` | Vault-Ordner und Name für `obsidian://`-Links |
| `folders.inbox`, `folders.projects`, `folders.areas`, `folders.archive`, `folders.journal`, `folders.templates` | Ordnernamen (Standard: neutrale PARA-Namen) |
| `scan.exclude` | Glob-Muster, die nicht gescannt werden |
| `sections.projectTasks`, `sections.areaTasks` | Überschriften, unter denen neue Aufgaben eingefügt werden |
| `frontmatter.*` | Feldnamen und Werte für Projekt und Bereich (`typ`, `status`, `bereich`, `review`, `termin`, `jira`) |
| `projects.reviewAfterDays` | Schwelle für „Review fällig“ |
| `areas.work`, `areas.workPaths` | was als „Arbeit“ zählt |
| `contexts`, `waitingTag` | Kontext-Tags und Warten-Tag |
| `jira.baseUrl`, `jira.email`, `jira.jql`, `jira.savedQueries`, `jira.projectKeys`, `jira.refreshMinutes` | Jira |
| `jira.source`, `jira.tokenType` | Zugangsweg (`api-token`; P2: `cache-file`, OAuth) und Token-Typ (`scoped` / `classic`) |
| `jira.worklog.roundToMinutes`, `jira.worklog.enabled` (P2) | Tageszusammenfassung und Buchen |
| `obsidian.useAdvancedUri` | Zeilensprung über das Plugin Advanced URI (`auto` / `never`) |
| `calendar.google.calendarIds`, `calendar.refreshMinutes`, `calendar.daysAhead` | angezeigte Kalender, Aktualisierung, Zeitraum (Standard 14 Tage) |
| `timeline.start`, `timeline.end`, `timeline.defaultBlockMinutes` | Zeitleiste |
| `focus.enabled`, `focus.pomodoro.work`, `focus.pomodoro.break` | Fokus-Timer |
| `focus.writeToVault`, `focus.logSection`, `focus.logFormat`, `focus.minMinutes` | Fokus-Zeiten im Vault-Log |
| `hotkeys.quickCapture` | globales Tastenkürzel |
| `filters` | gespeicherte eigene Filter |

## 12. Akzeptanzkriterien
Alle Kriterien werden gegen `examples/demo-vault/` geprüft.

1. **Abhängigkeiten:** Im Demo-Projekt „Gartenhaus“ hat die Aufgabe `🆔 gh-a` den Nachfolger `⛔ gh-a`. Dieser Nachfolger erscheint unter „Blockiert“, nicht unter „Nächste Aktionen“. Nach dem Abhaken von `gh-a` wechselt er in „Nächste Aktionen“.
2. **Jira-Markierung:** `ABC-123` wird im Demo-Projekt erwähnt und bekommt in der Jira-Ansicht ein Badge „1 Notiz“. `UTF-8` und `DR-05` im selben Text werden nicht als Tickets erkannt. Ein zweites Demo-Projekt hat `jira: [ABC-124]` im Frontmatter. `ABC-124` zeigt das Badge „→ <Projekt>“, und eine Zeitsitzung an einer Aufgabe dieses Projekts ohne Schlüssel in der Zeile wird `ABC-124` eindeutig zugeordnet (nicht „vermutet“).
3. **Abhaken:** Eine abgehakte Aufgabe erhält genau `[x]` und ` ✅ <heute>`. Alle anderen Bytes der Datei bleiben gleich.
4. **Zeitleiste:** Wird eine Aufgabe mit ⏳ heute auf die Zeitleiste gezogen, entsteht ein Zeitblock. Die Quelldatei bleibt byte-gleich.
5. **Fokus-Timer:** Ein 25-min-Pomodoro erzeugt einen lokalen Zeiteintrag an der richtigen Aufgabe und genau eine Zeile `- <heute> ⏱ 25 min (1 Sitzung) · <Aufgabe>` unter `## Log` der Demo-Projektnotiz. Ein zweiter Pomodoro am selben Tag ändert diese Zeile auf `50 min (2 Sitzungen)`, ohne eine neue Zeile anzulegen. Eine Sitzung von 3 min schreibt nichts in den Vault.
6. **Schnelleingabe:** `Test morgen @telefon !1 >Gartenhaus` erzeugt genau eine Zeile `- [ ] Test #@telefon ⏫ ⏳ <morgen>` unter „Nächste Schritte“ der Demo-Projektnotiz.
7. **Filter:** `kontext:@computer & !blockiert & !warten & prio:keine & fällig:keine` liefert dieselbe Menge wie „Nächste Aktionen“ in der Gruppe `@computer`.
8. **Drag & Drop:** Eine Aufgabe in „Demnächst“ auf übermorgen zu ziehen ändert in der Zeile nur das ⏳-Datum.
9. **Jira-Zeiten:** Zwei Pomodoros (25 + 25 min) an einer Aufgabe mit `ABC-123` in der Zeile ergeben in „Zeiten heute“ genau eine Zeile `ABC-123 · 0h 45m` (Rohwert 50 min, Rundung 15 min). Die App sendet dabei keine schreibende Anfrage an Jira.
10. **Konflikt:** Wird die Zeile zwischen Laden und Schreiben extern geändert, schreibt die App nicht und meldet den Konflikt.
11. **In Obsidian öffnen:** Ist Advanced URI im Demo-Vault aktiv, enthält der erzeugte Link die richtige Zeilennummer. Ohne das Plugin wird ein `obsidian://open`-Link erzeugt und der Aufgabentext liegt in der Zwischenablage. In beiden Fällen bleibt `.obsidian/` byte-gleich.
12. **Datenschutz:** Der Commit-Scan schlägt bei einer Test-Datei mit einer konkreten Atlassian-Domain oder einem Benutzerpfad an.

## 13. Offene Fragen
1. ~~Lizenz~~ → entschieden: **MIT** (`LICENSE`, Inhaber „rhinos07 and Tasker.md contributors“)
2. ~~Fokus-Zeiten in den Vault~~ → entschieden: **ja**, eine Zeile pro Aufgabe und Tag unter `## Log` (siehe 6.5)
3. ~~Worklog nach Jira~~ → entschieden: **MVP** Tageszusammenfassung zum Kopieren, Jira bleibt nur lesend. **P2** Buchen auf Knopfdruck mit Prüfdialog, falls native Worklogs erlaubt sind.
4. ~~Jira-API-Tokens erlaubt?~~ → entschieden: **ja**, das Erstellen eines Tokens ist möglich. MVP nutzt E-Mail plus API-Token, empfohlen mit Scopes und nur lesend (siehe 4).
5. ~~Plugin „Advanced URI“~~ → entschieden: **optional**. Wird es erkannt, springt die App auf die Zeile, sonst öffnet sie die Notiz und kopiert den Aufgabentext in die Zwischenablage (siehe 6.1).
6. ~~Frontmatter-Feld `jira:`~~ → entschieden: **optional**. In v1 liest die App das Feld nur, in P2 gibt es die Aktion „Ticket zuordnen“ mit Rückfrage (siehe 3.3, 4, 10).
