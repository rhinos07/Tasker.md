//! Aufgaben anlegen (SPEC 6.1): in einen Abschnitt einer Projekt- oder Bereichsnotiz einfügen
//! oder als neue Notiz in der Inbox. Es wird nur eingefügt, nie etwas anderes geändert oder gelöscht.

use crate::edit::{resolve_in_vault, write_atomic, EditError};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

/// Ergebnis einer Einfügung, für Rückgängig.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InsertResult {
    pub path: String,
    /// Zeilennummer (0-basiert) der neuen Zeile
    pub line: usize,
    pub new: String,
    pub added_heading: bool,
    /// neue Datei angelegt (Inbox)
    pub created_file: bool,
}

fn heading_level(line: &str) -> Option<usize> {
    let n = line.chars().take_while(|c| *c == '#').count();
    (n > 0 && line[n..].starts_with(' ')).then_some(n)
}

/// Fügt `new_line` am Ende des Abschnitts `heading` ein (nach dem letzten nicht-leeren Eintrag).
/// Fehlt die Überschrift, kommt sie mit der Zeile ans Ende. Zeilenenden und BOM bleiben erhalten.
pub fn insert_in_section(text: &str, heading: &str, new_line: &str) -> (String, usize, bool) {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let trailing = text.ends_with('\n') || text.is_empty();
    let mut lines: Vec<String> = text.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
    if text.ends_with('\n') || text.is_empty() {
        lines.pop();
    }
    let clean = |s: &str| s.trim_start_matches('\u{FEFF}').trim_end().to_string();
    let level = heading_level(heading.trim()).unwrap_or(2);
    let (at, added) = match lines.iter().position(|l| clean(l) == heading.trim()) {
        Some(h) => {
            let end = (h + 1..lines.len()).find(|&i| heading_level(&lines[i]).is_some_and(|lv| lv <= level)).unwrap_or(lines.len());
            let last = (h + 1..end).rev().find(|&i| !lines[i].trim().is_empty()).unwrap_or(h);
            lines.insert(last + 1, new_line.to_string());
            (last + 1, false)
        }
        None => {
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                lines.pop();
            }
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(heading.trim().to_string());
            lines.push(new_line.to_string());
            (lines.len() - 1, true)
        }
    };
    let mut out = lines.join(eol);
    if trailing || added {
        out.push_str(eol);
    }
    (out, at, added)
}

/// Fügt eine Aufgabe in eine vorhandene Notiz ein.
pub fn add_task(root: &Path, rel: &str, heading: &str, line: &str) -> Result<InsertResult, EditError> {
    if line.contains('\n') || line.contains('\r') {
        return Err(EditError::NotApplicable);
    }
    let path = resolve_in_vault(root, rel)?;
    let text = std::fs::read_to_string(&path).map_err(|_| EditError::Io)?;
    let (out, at, added_heading) = insert_in_section(&text, heading, line);
    write_atomic(&path, &out).map_err(|_| EditError::Io)?;
    Ok(InsertResult { path: rel.into(), line: at, new: line.into(), added_heading, created_file: false })
}

/// Legt eine neue Notiz in der Inbox an. Vorhandene Dateien werden nie überschrieben;
/// bei Namensgleichheit wird durchnummeriert.
pub fn create_inbox_note(root: &Path, inbox: &str, title: &str, content: &str) -> Result<InsertResult, EditError> {
    for n in 1..100 {
        let name = if n == 1 { format!("{title}.md") } else { format!("{title} {n}.md") };
        let rel = format!("{}/{name}", inbox.trim_end_matches('/'));
        let path = resolve_in_vault(root, &rel)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|_| EditError::Io)?;
        }
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                f.write_all(format!("{content}\n").as_bytes()).map_err(|_| EditError::Io)?;
                return Ok(InsertResult { path: rel, line: 0, new: content.into(), added_heading: false, created_file: true });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(EditError::Io),
        }
    }
    Err(EditError::Io)
}

/// Entfernt eine eingefügte Zeile wieder, aber nur, wenn sie unverändert ist.
/// Eine neu angelegte Inbox-Notiz wird nicht gelöscht (die App löscht nie Dateien); dafür gibt es kein Rückgängig.
pub fn undo_insert(root: &Path, r: &InsertResult) -> Result<(), EditError> {
    if r.created_file {
        return Err(EditError::NotApplicable);
    }
    let path = resolve_in_vault(root, &r.path)?;
    let text = std::fs::read_to_string(&path).map_err(|_| EditError::Io)?;
    let mut parts: Vec<&str> = text.split_inclusive('\n').collect();
    let content = |p: &str| p.trim_end_matches('\n').trim_end_matches('\r').to_string();
    // bevorzugt an der gemerkten Stelle, sonst eindeutig im Text
    let idx = if parts.get(r.line).is_some_and(|p| content(p) == r.new) {
        r.line
    } else {
        let hits: Vec<usize> = parts.iter().enumerate().filter(|(_, p)| content(p) == r.new).map(|(i, _)| i).collect();
        match hits[..] {
            [i] => i,
            _ => return Err(EditError::Conflict),
        }
    };
    parts.remove(idx);
    write_atomic(&path, &parts.concat()).map_err(|_| EditError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_after_last_entry_of_section() {
        let t = "# P\r\n\r\n## Nächste Schritte\r\n- [ ] A\r\n- [ ] B\r\n\r\n## Log\r\n- x\r\n";
        let (out, at, added) = insert_in_section(t, "## Nächste Schritte", "- [ ] C");
        assert_eq!(out, "# P\r\n\r\n## Nächste Schritte\r\n- [ ] A\r\n- [ ] B\r\n- [ ] C\r\n\r\n## Log\r\n- x\r\n");
        assert_eq!((at, added), (5, false));
    }

    #[test]
    fn subheadings_stay_in_section() {
        let t = "## Nächste Schritte\n- [ ] A\n### Später\n- [ ] B\n## Log\n";
        let (out, at, _) = insert_in_section(t, "## Nächste Schritte", "- [ ] C");
        assert_eq!(out, "## Nächste Schritte\n- [ ] A\n### Später\n- [ ] B\n- [ ] C\n## Log\n");
        assert_eq!(at, 4);
    }

    #[test]
    fn adds_missing_heading() {
        let (out, at, added) = insert_in_section("# Bereich\nText", "## Aufgaben ohne Projekt", "- [ ] C");
        assert_eq!(out, "# Bereich\nText\n\n## Aufgaben ohne Projekt\n- [ ] C\n");
        assert_eq!((at, added), (4, true));
    }

    #[test]
    fn add_and_undo_on_disk() {
        let dir = std::env::temp_dir().join(format!("tasker-create-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("P")).unwrap();
        let original = "\u{FEFF}# P\n## Nächste Schritte\n- [ ] A\n";
        std::fs::write(dir.join("P/P.md"), original).unwrap();
        let r = add_task(&dir, "P/P.md", "## Nächste Schritte", "- [ ] Neu").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("P/P.md")).unwrap(), "\u{FEFF}# P\n## Nächste Schritte\n- [ ] A\n- [ ] Neu\n");
        undo_insert(&dir, &r).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("P/P.md")).unwrap(), original);
        assert_eq!(undo_insert(&dir, &r), Err(EditError::Conflict));

        let a = create_inbox_note(&dir, "00 Inbox", "Idee", "- [ ] Idee").unwrap();
        let b = create_inbox_note(&dir, "00 Inbox", "Idee", "- [ ] Idee").unwrap();
        assert_eq!((a.path.as_str(), b.path.as_str()), ("00 Inbox/Idee.md", "00 Inbox/Idee 2.md"));
        assert_eq!(std::fs::read_to_string(dir.join("00 Inbox/Idee.md")).unwrap(), "- [ ] Idee\n");
        assert!(create_inbox_note(&dir, "../aussen", "x", "x").is_err(), "Pfad außerhalb des Vaults");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
