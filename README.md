# Tasker.md

Desktop-Dashboard (Windows, macOS) für einen Obsidian-Vault nach **GTD**, mit Jira- und Kalender-Einblendung.
Der Vault bleibt die einzige Quelle der Wahrheit. Tasker.md ist nur eine Oberfläche darüber.

> Status: in Arbeit. Es laufen: Heute mit Zeitleiste, Dashboard, Projekte, Jira (nur lesen), Google Kalender (nur lesen), Fokus-Timer mit Vault-Log und Jira-Tageszusammenfassung, Abhaken und „⏳ heute“, Schnelleingabe (Taste `q`), Einstellungen. Es folgen: Demnächst, Filter, Tastatur.
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

### Jira

In der App unter **Jira** E-Mail und [API-Token](https://id.atlassian.com/manage-profile/security/api-tokens) eintragen. Bei einem Token mit Bereichen genügen `read:jira-work` und `read:jira-user`. Die Site-Adresse steht in `jira.baseUrl`.

### Google Kalender

Tasker.md liefert keine eigenen Google-Zugangsdaten mit. Jeder Nutzer legt einmalig ein eigenes Google-Cloud-Projekt an:

1. In der [Google Cloud Console](https://console.cloud.google.com/) ein neues Projekt anlegen.
2. Unter „APIs & Dienste → Bibliothek“ die **Google Calendar API** aktivieren.
3. Unter „APIs & Dienste → OAuth-Zustimmungsbildschirm“ (bzw. „Google Auth Platform“) die App einrichten: Typ **Extern**, App-Name frei wählbar, eigene E-Mail als Support- und Entwickler-Kontakt. Unter „Datenzugriff“ den Bereich `.../auth/calendar.readonly` hinzufügen.
4. Unter „Zielgruppe“ das eigene Google-Konto als **Testnutzer** eintragen.
5. Unter „Clients“ (bzw. „Anmeldedaten“) einen **OAuth-Client vom Typ „Desktop-App“** erstellen und Client-ID sowie Client-Secret kopieren.
6. In Tasker.md unter **Heute → Verbinden …** Client-ID und -Secret eintragen, **Mit Google verbinden** klicken und im Browser den Lesezugriff erlauben. Danach die angezeigten Kalender auswählen.

Hinweise:

- Im Status „Testen“ läuft die Verbindung nach **7 Tagen** ab, danach einfach neu verbinden. Wer das vermeiden will, stellt die App im Zustimmungsbildschirm auf „In Produktion“. Für die eigene Nutzung ist keine Prüfung durch Google nötig; Google zeigt dann beim Verbinden einen Hinweis „App nicht überprüft“, den man über „Erweitert“ bestätigt.
- Tasker.md fordert nur Lesezugriff an und ändert keine Termine. Client-Daten und Token liegen im Schlüsselbund des Betriebssystems, die Termine der nächsten 14 Tage im Cache-Ordner der App.

## Datenschutz

Das Repo enthält keine persönlichen Daten. Vor jedem Commit prüft ein Hook auf Benutzerpfade, konkrete Atlassian-Domains, E-Mail-Adressen und eigene private Begriffe:

```bash
git config core.hooksPath .githooks
```

Eigene Begriffe (Namen, Firmen, Projekte) trägst du zeilenweise in `local/private-terms.txt` ein. Der Ordner `local/` ist von Git ausgeschlossen. Ist [gitleaks](https://github.com/gitleaks/gitleaks) installiert, läuft es zusätzlich. In der CI laufen beide Prüfungen immer.

## Lizenz

[MIT](LICENSE)
