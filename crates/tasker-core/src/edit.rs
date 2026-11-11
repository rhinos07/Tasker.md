//! Zeilengenaue Änderungen an Vault-Dateien mit den Schreibregeln aus SPEC 6.4:
//! - die Zielzeile wird vor dem Schreiben neu gelesen und mit dem erwarteten Inhalt verglichen
//! - nur diese eine Zeile ändert sich, alle anderen Bytes bleiben gleich
//!   (Zeilenenden, BOM, abschließender Zeilenumbruch)
//! - es wird nie etwas gelöscht

use crate::task::{complete_line, parse_task_line, set_scheduled_line};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub enum LineEdit {
    /// `[ ]` → `[x]` plus ✅ Datum
    Complete,
    /// ⏳ setzen (`Some`) oder entfernen (`None`)
    SetScheduled(Option<NaiveDate>),
    /// Zeile durch einen festen Text ersetzen (für Rückgängig)
    Replace(String),
}

/// Ergebnis einer Änderung, damit die Oberfläche Rückgängig anbieten kann.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EditResult {
    pub path: String,
    pub line: usize,
    pub old: String,
    pub new: String,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum EditError {
    #[error("Ungültiger Pfad")]
    InvalidPath,
    #[error("Datei nicht lesbar oder nicht schreibbar")]
    Io,
    #[error("Die Zeile wurde inzwischen geändert. Bitte neu laden.")]
    Conflict,
    #[error("Die Zeile ist keine passende Aufgabe")]
    NotApplicable,
    #[error("Wiederkehrende Aufgaben (🔁) bitte vorerst in Obsidian abhaken")]
    RecurringNotSupported,
}

/// Prüft, dass `rel` innerhalb des Vaults liegt und eine Markdown-Datei außerhalb von `.obsidian/` ist.
pub fn resolve_in_vault(root: &Path, rel: &str) -> Result<PathBuf, EditError> {
    let rel_path = Path::new(rel);
    let ok = rel.ends_with(".md")
        && rel_path.components().all(|c| matches!(c, Component::Normal(_)))
        && rel_path.components().next().is_some_and(|c| c.as_os_str() != ".obsidian");
    if !ok {
        return Err(EditError::InvalidPath);
    }
    Ok(root.join(rel_path))
}

/// Wendet die Änderung auf den Text an. Reine Funktion, gut testbar.
pub fn apply_to_text(
    text: &str,
    line: usize,
    expected: &str,
    edit: &LineEdit,
    today: NaiveDate,
) -> Result<(String, String, String), EditError> {
    // Zeilen mit ihren Endungen behalten, damit alle anderen Bytes identisch bleiben.
    let mut parts: Vec<&str> = text.split_inclusive('\n').collect();
    let raw_with_end = *parts.get(line).ok_or(EditError::Conflict)?;
    let ending = if raw_with_end.ends_with("\r\n") {
        "\r\n"
    } else if raw_with_end.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    let mut current = &raw_with_end[..raw_with_end.len() - ending.len()];
    // BOM gehört nicht zum Zeileninhalt
    let bom = if line == 0 && current.starts_with('\u{FEFF}') { "\u{FEFF}" } else { "" };
    current = &current[bom.len()..];
    if current != expected {
        return Err(EditError::Conflict);
    }

    let new_line = match edit {
        LineEdit::Complete => {
            let task = parse_task_line(line, current).ok_or(EditError::NotApplicable)?;
            if task.recurrence.is_some() {
                return Err(EditError::RecurringNotSupported);
            }
            complete_line(current, today).ok_or(EditError::NotApplicable)?
        }
        LineEdit::SetScheduled(d) => {
            let task = parse_task_line(line, current).ok_or(EditError::NotApplicable)?;
            if task.done {
                return Err(EditError::NotApplicable);
            }
            set_scheduled_line(current, *d).ok_or(EditError::NotApplicable)?
        }
        LineEdit::Replace(s) => {
            if s.contains('\n') || s.contains('\r') {
                return Err(EditError::NotApplicable);
            }
            s.clone()
        }
    };

    let replaced = format!("{bom}{new_line}{ending}");
    let old = current.to_string();
    parts[line] = &replaced;
    Ok((parts.concat(), old, new_line))
}

/// Liest die Datei, prüft die Zeile, schreibt die Änderung (über eine temporäre Datei plus Umbenennen).
pub fn edit_line(
    root: &Path,
    rel: &str,
    line: usize,
    expected: &str,
    edit: &LineEdit,
    today: NaiveDate,
) -> Result<EditResult, EditError> {
    let path = resolve_in_vault(root, rel)?;
    let text = std::fs::read_to_string(&path).map_err(|_| EditError::Io)?;
    let (new_text, old, new) = apply_to_text(&text, line, expected, edit, today)?;
    write_atomic(&path, &new_text).map_err(|_| EditError::Io)?;
    Ok(EditResult { path: rel.to_string(), line, old, new })
}

pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path.parent().ok_or(std::io::ErrorKind::NotFound)?;
    let name = path.file_name().ok_or(std::io::ErrorKind::NotFound)?.to_string_lossy();
    let tmp = dir.join(format!(".{name}.tasker-tmp"));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    const TODAY: &str = "2026-11-11";

    #[test]
    fn complete_changes_only_target_line_crlf() {
        let text = "# Titel\r\n- [ ] Eins #@haus\r\n- [ ] Zwei\r\n";
        let (out, old, new) =
            apply_to_text(text, 1, "- [ ] Eins #@haus", &LineEdit::Complete, d(TODAY)).unwrap();
        assert_eq!(out, "# Titel\r\n- [x] Eins #@haus ✅ 2026-11-11\r\n- [ ] Zwei\r\n");
        assert_eq!(old, "- [ ] Eins #@haus");
        assert_eq!(new, "- [x] Eins #@haus ✅ 2026-11-11");
    }

    #[test]
    fn keeps_missing_final_newline_and_bom() {
        let text = "\u{FEFF}- [ ] Eins";
        let (out, ..) = apply_to_text(text, 0, "- [ ] Eins", &LineEdit::Complete, d(TODAY)).unwrap();
        assert_eq!(out, "\u{FEFF}- [x] Eins ✅ 2026-11-11");
    }

    #[test]
    fn detects_conflict() {
        let text = "- [ ] Eins geändert\n";
        let r = apply_to_text(text, 0, "- [ ] Eins", &LineEdit::Complete, d(TODAY));
        assert_eq!(r.unwrap_err(), EditError::Conflict);
        let r = apply_to_text(text, 5, "- [ ] Eins", &LineEdit::Complete, d(TODAY));
        assert_eq!(r.unwrap_err(), EditError::Conflict);
    }

    #[test]
    fn schedule_today_and_undo_restore_bytes() {
        let text = "a\n- [ ] Eins #@haus ⏳ 2026-11-01\nb\n";
        let (out, old, new) = apply_to_text(
            text,
            1,
            "- [ ] Eins #@haus ⏳ 2026-11-01",
            &LineEdit::SetScheduled(Some(d(TODAY))),
            d(TODAY),
        )
        .unwrap();
        assert_eq!(new, "- [ ] Eins #@haus ⏳ 2026-11-11");
        let (back, ..) = apply_to_text(&out, 1, &new, &LineEdit::Replace(old), d(TODAY)).unwrap();
        assert_eq!(back, text, "Rückgängig stellt den Originaltext byte-genau wieder her");
    }

    #[test]
    fn refuses_recurring_done_and_non_tasks() {
        let r = apply_to_text("- [ ] Test 🔁 every week\n", 0, "- [ ] Test 🔁 every week", &LineEdit::Complete, d(TODAY));
        assert_eq!(r.unwrap_err(), EditError::RecurringNotSupported);
        let r = apply_to_text("- [x] Fertig\n", 0, "- [x] Fertig", &LineEdit::SetScheduled(None), d(TODAY));
        assert_eq!(r.unwrap_err(), EditError::NotApplicable);
        let r = apply_to_text("Text\n", 0, "Text", &LineEdit::Complete, d(TODAY));
        assert_eq!(r.unwrap_err(), EditError::NotApplicable);
    }

    #[test]
    fn rejects_paths_outside_vault() {
        let root = Path::new("/vault");
        assert!(resolve_in_vault(root, "10 Projekte/A/A.md").is_ok());
        for bad in ["../x.md", "/etc/x.md", ".obsidian/x.md", "a/../../x.md", "a.txt"] {
            assert_eq!(resolve_in_vault(root, bad).unwrap_err(), EditError::InvalidPath, "{bad}");
        }
    }

    #[test]
    fn edit_line_on_disk() {
        let dir = std::env::temp_dir().join(format!("tasker-edit-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("P")).unwrap();
        std::fs::write(dir.join("P/P.md"), "x\n- [ ] Tun\n").unwrap();
        let r = edit_line(&dir, "P/P.md", 1, "- [ ] Tun", &LineEdit::Complete, d(TODAY)).unwrap();
        assert_eq!(r.new, "- [x] Tun ✅ 2026-11-11");
        assert_eq!(std::fs::read_to_string(dir.join("P/P.md")).unwrap(), "x\n- [x] Tun ✅ 2026-11-11\n");
        assert!(!dir.join("P/.P.md.tasker-tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
