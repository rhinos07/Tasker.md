# Tasker.md

Desktop-Dashboard (Windows, macOS) für einen Obsidian-Vault nach **GTD**, mit Jira- und Kalender-Einblendung.
Der Vault bleibt die einzige Quelle der Wahrheit. Tasker.md ist nur eine Oberfläche darüber.

> Status: frühes Gerüst. Der Vault-Index und die Dashboard-Ansichten laufen. Jira, Kalender und die Schreibaktionen folgen.
> Die vollständige Spezifikation steht in [SPEC.md](SPEC.md).

## Funktionen (Ziel der ersten Version)

- Dashboard mit den Abschnitten Wichtig, Überfällig, Diese Woche, Nächste Aktionen nach Kontext, Warten auf und Blockiert, umschaltbar zwischen Arbeit, Privat und Alle
- Projekte mit Review-Alter, Termin und Warnung „ohne nächste Aktion“
- Aufgaben in der Syntax des [Tasks-Plugins](https://publish.obsidian.md/tasks/), inklusive Abhängigkeiten über 🆔 und ⛔
- Jira (nur lesen): eigene Tickets, Markierung „im Vault erwähnt“ bzw. „→ Projekt“
- Google Kalender (nur lesen), Tages-Zeitleiste, Fokus-Timer, Schnelleingabe, Tastaturbedienung

## Entwicklung

Voraussetzungen: [Rust](https://rustup.rs/), Node.js 22+, die [Tauri-Voraussetzungen](https://tauri.app/start/prerequisites/) des Betriebssystems.

```bash
npm install
npm run tauri dev      # App mit Hot-Reload starten
cargo test -p tasker-core   # Kern-Tests gegen den Demo-Vault
```

Ohne eigene Konfiguration zeigt die App in Debug-Builds den erfundenen [Demo-Vault](examples/demo-vault). Mit dem Schalter „Demo-Vault“ in der Seitenleiste lässt sich jederzeit dorthin wechseln.

### Aufbau

| Pfad | Inhalt |
|---|---|
| `crates/tasker-core` | Rust-Kern ohne UI: Parser, Vault-Scan, Index, GTD-Ansichten, Tests |
| `src-tauri` | Tauri-Backend, stellt den Kern als Befehle bereit |
| `src` | Oberfläche (React, TypeScript, Vite) |
| `examples/demo-vault` | erfundener Vault für Tests und Screenshots |

## Konfiguration

Die Konfiguration liegt **nicht** im Repo, sondern im App-Datenordner:

- Windows: `%APPDATA%\md.tasker.app\config.json`
- macOS: `~/Library/Application Support/md.tasker.app/config.json`

Als Vorlage dient [config.example.json](config.example.json). Zugangsdaten wie der Jira-Token kommen nur in den Schlüsselbund des Betriebssystems.

## Datenschutz

Das Repo enthält keine persönlichen Daten. Vor jedem Commit prüft ein Hook auf Benutzerpfade, konkrete Atlassian-Domains, E-Mail-Adressen und eigene private Begriffe:

```bash
git config core.hooksPath .githooks
```

Eigene Begriffe (Namen, Firmen, Projekte) trägst du zeilenweise in `local/private-terms.txt` ein. Der Ordner `local/` ist von Git ausgeschlossen. Ist [gitleaks](https://github.com/gitleaks/gitleaks) installiert, läuft es zusätzlich. In der CI laufen beide Prüfungen immer.

## Lizenz

[MIT](LICENSE)
