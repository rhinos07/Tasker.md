//! Fokus-Zeiten im Vault-Log (SPEC 6.5): eine Zeile pro Aufgabe und Tag unter `focus.logSection`.
//!
//! Schreibregeln wie in SPEC 6.4: Datei frisch lesen, nur eine Zeile ändern oder einfügen
//! (fehlt die Überschrift, kommt sie mit ans Ende), Zeilenenden und BOM bleiben erhalten.

use crate::edit::{resolve_in_vault, write_atomic, EditError};
use chrono::NaiveDate;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::LazyLock;

pub const DEFAULT_FORMAT: &str = "- {date} ⏱ {minutes} min ({sessions}) · {task}";
const MARK: char = '⏱';

/// Log-Zeilen im Standardformat, für Summen pro Projekt (auch von Hand geschriebene Einträge).
static LOG_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*[-*] (\d{4}-\d{2}-\d{2}) ⏱ (\d+) min").expect("gültige Regex"));

pub fn sessions_label(n: u32) -> String {
    if n == 1 { "1 Sitzung".into() } else { format!("{n} Sitzungen") }
}

pub fn format_line(fmt: &str, date: NaiveDate, minutes: u32, sessions: u32, task: &str) -> String {
    fmt.replace("{date}", &date.format("%Y-%m-%d").to_string())
        .replace("{minutes}", &minutes.to_string())
        .replace("{sessions}", &sessions_label(sessions))
        .replace("{task}", task)
}

/// Fokus-Einträge (Datum, Minuten) einer Notiz.
pub fn parse_log_minutes(text: &str) -> Vec<(NaiveDate, u32)> {
    text.lines()
        .filter_map(|l| {
            let c = LOG_LINE.captures(l)?;
            Some((NaiveDate::parse_from_str(&c[1], "%Y-%m-%d").ok()?, c[2].parse().ok()?))
        })
        .collect()
}

/// Was geändert wurde, damit es rückgängig gemacht werden kann.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogChange {
    pub path: String,
    /// vorherige Zeile, `None` = neu eingefügt
    pub old: Option<String>,
    pub new: String,
    /// Überschrift wurde neu angelegt
    pub added_heading: bool,
}

fn heading_level(line: &str) -> Option<usize> {
    let n = line.chars().take_while(|c| *c == '#').count();
    (n > 0 && line[n..].starts_with(' ')).then_some(n)
}

/// Aktualisiert oder ergänzt die Log-Zeile im Text. Gibt den neuen Text, die alte Zeile (falls vorhanden)
/// und ob die Überschrift ergänzt wurde zurück.
pub fn upsert_in_text(text: &str, section: &str, date: NaiveDate, task: &str, new_line: &str) -> (String, Option<String>, bool) {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
    let trailing_newline = text.ends_with('\n');
    if trailing_newline {
        lines.pop();
    }
    let strip_bom = |s: &str| s.trim_start_matches('\u{FEFF}').trim_end().to_string();
    let date_s = date.format("%Y-%m-%d").to_string();
    let section_level = heading_level(section.trim()).unwrap_or(2);

    let head = lines.iter().position(|l| strip_bom(l) == section.trim());
    let (old, added_heading) = match head {
        Some(h) => {
            let end = lines
                .iter()
                .enumerate()
                .skip(h + 1)
                .find(|(_, l)| heading_level(l).is_some_and(|lv| lv <= section_level))
                .map_or(lines.len(), |(i, _)| i);
            let existing = (h + 1..end).find(|&i| {
                let l = &lines[i];
                l.contains(&date_s) && l.contains(MARK) && l.trim_end().ends_with(task.trim())
            });
            match existing {
                Some(i) => {
                    let old = std::mem::replace(&mut lines[i], new_line.to_string());
                    (Some(old), false)
                }
                None => {
                    // nach dem letzten nicht-leeren Eintrag des Abschnitts
                    let last = (h + 1..end).rev().find(|&i| !lines[i].trim().is_empty()).unwrap_or(h);
                    lines.insert(last + 1, new_line.to_string());
                    (None, false)
                }
            }
        }
        None => {
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                lines.pop();
            }
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(section.trim().to_string());
            lines.push(new_line.to_string());
            (None, true)
        }
    };
    let mut out = lines.join(eol);
    if trailing_newline || head.is_none() {
        out.push_str(eol);
    }
    (out, old, added_heading)
}

/// Schreibt die Log-Zeile für `task` am `date` mit den Gesamtwerten des Tages.
#[allow(clippy::too_many_arguments)]
pub fn write_log(
    root: &Path,
    rel: &str,
    section: &str,
    fmt: &str,
    date: NaiveDate,
    task: &str,
    minutes: u32,
    sessions: u32,
) -> Result<LogChange, EditError> {
    let path = resolve_in_vault(root, rel)?;
    let text = std::fs::read_to_string(&path).map_err(|_| EditError::Io)?;
    let new = format_line(fmt, date, minutes, sessions, task);
    if new.contains('\n') {
        return Err(EditError::NotApplicable);
    }
    let (out, old, added_heading) = upsert_in_text(&text, section, date, task, &new);
    if old.as_deref() != Some(new.as_str()) {
        write_atomic(&path, &out).map_err(|_| EditError::Io)?;
    }
    Ok(LogChange { path: rel.to_string(), old, new, added_heading })
}

/// Macht eine Log-Änderung rückgängig, aber nur, wenn die Zeile noch genau so dasteht.
/// Eine neu eingefügte Zeile wird wieder entfernt; eine dabei angelegte Überschrift bleibt stehen.
pub fn revert_log(root: &Path, change: &LogChange) -> Result<(), EditError> {
    let path = resolve_in_vault(root, &change.path)?;
    let text = std::fs::read_to_string(&path).map_err(|_| EditError::Io)?;
    let mut parts: Vec<&str> = text.split_inclusive('\n').collect();
    let content = |p: &str| p.trim_end_matches('\n').trim_end_matches('\r').trim_start_matches('\u{FEFF}').to_string();
    let hits: Vec<usize> = parts.iter().enumerate().filter(|(_, p)| content(p) == change.new).map(|(i, _)| i).collect();
    let [i] = hits[..] else { return Err(EditError::Conflict) };
    let replaced;
    match &change.old {
        Some(old) => {
            let p = parts[i];
            let ending = &p[p.trim_end_matches('\n').trim_end_matches('\r').len()..];
            let bom = if p.starts_with('\u{FEFF}') { "\u{FEFF}" } else { "" };
            replaced = format!("{bom}{old}{ending}");
            parts[i] = &replaced;
        }
        None => {
            parts.remove(i);
        }
    }
    write_atomic(&path, &parts.concat()).map_err(|_| EditError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    const DAY: &str = "2026-11-11";

    fn line(min: u32, n: u32, task: &str) -> String {
        format_line(DEFAULT_FORMAT, d(DAY), min, n, task)
    }

    #[test]
    fn formats_line() {
        assert_eq!(line(25, 1, "Fundament"), "- 2026-11-11 ⏱ 25 min (1 Sitzung) · Fundament");
        assert_eq!(line(50, 2, "Fundament"), "- 2026-11-11 ⏱ 50 min (2 Sitzungen) · Fundament");
    }

    #[test]
    fn inserts_at_end_of_section_then_updates() {
        let text = "# P\r\n\r\n## Log\r\n- 2026-11-01 Start\r\n\r\n## Notizen\r\nx\r\n";
        let (t1, old, added) = upsert_in_text(text, "## Log", d(DAY), "Fundament", &line(25, 1, "Fundament"));
        assert_eq!((old, added), (None, false));
        assert_eq!(
            t1,
            "# P\r\n\r\n## Log\r\n- 2026-11-01 Start\r\n- 2026-11-11 ⏱ 25 min (1 Sitzung) · Fundament\r\n\r\n## Notizen\r\nx\r\n"
        );
        let (t2, old, _) = upsert_in_text(&t1, "## Log", d(DAY), "Fundament", &line(50, 2, "Fundament"));
        assert_eq!(old.as_deref(), Some("- 2026-11-11 ⏱ 25 min (1 Sitzung) · Fundament"));
        assert_eq!(t2, t1.replace("25 min (1 Sitzung)", "50 min (2 Sitzungen)"));
        // andere Aufgabe am selben Tag → eigene Zeile
        let (t3, old, _) = upsert_in_text(&t2, "## Log", d(DAY), "Dach", &line(30, 1, "Dach"));
        assert!(old.is_none());
        assert_eq!(t3.matches('⏱').count(), 2);
    }

    #[test]
    fn adds_missing_heading_at_end() {
        let (t, old, added) = upsert_in_text("# P\n- [ ] A\n\n", "## Log", d(DAY), "A", &line(10, 1, "A"));
        assert_eq!((old, added), (None, true));
        assert_eq!(t, "# P\n- [ ] A\n\n## Log\n- 2026-11-11 ⏱ 10 min (1 Sitzung) · A\n");
    }

    #[test]
    fn parses_log_minutes() {
        let t = "## Log\n- 2026-11-10 ⏱ 25 min (1 Sitzung) · A\n* 2026-11-11 ⏱ 90 min · von Hand\n- 2026-11-11 Notiz\n";
        assert_eq!(parse_log_minutes(t), vec![(d("2026-11-10"), 25), (d("2026-11-11"), 90)]);
    }

    #[test]
    fn write_and_revert_on_disk() {
        let dir = std::env::temp_dir().join(format!("tasker-focuslog-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("P")).unwrap();
        let original = "\u{FEFF}# P\n\n## Log\n- alt\n";
        std::fs::write(dir.join("P/P.md"), original).unwrap();

        let c1 = write_log(&dir, "P/P.md", "## Log", DEFAULT_FORMAT, d(DAY), "A", 25, 1).unwrap();
        let c2 = write_log(&dir, "P/P.md", "## Log", DEFAULT_FORMAT, d(DAY), "A", 50, 2).unwrap();
        let now = std::fs::read_to_string(dir.join("P/P.md")).unwrap();
        assert_eq!(now, "\u{FEFF}# P\n\n## Log\n- alt\n- 2026-11-11 ⏱ 50 min (2 Sitzungen) · A\n");

        revert_log(&dir, &c2).unwrap();
        revert_log(&dir, &c1).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("P/P.md")).unwrap(), original, "byte-gleich zurück");
        assert_eq!(revert_log(&dir, &c1), Err(EditError::Conflict), "zweites Rückgängig greift nicht");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
