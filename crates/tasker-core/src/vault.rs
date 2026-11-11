//! Vault-Scan: Notizen lesen, Frontmatter und Aufgaben extrahieren (SPEC 3.1, 3.3).

use crate::config::Config;
use crate::jira::{find_keys, is_valid_key, KeyFilter};
use crate::task::{parse_task_line, Task};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Serialize;
use serde_yaml_ng::Value;
use std::path::Path;
use unicode_normalization::UnicodeNormalization;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteKind {
    Project,
    Area,
    Journal,
    Inbox,
    Other,
}

/// Eine gelesene Notiz. `path` ist relativ zum Vault, NFC-normalisiert, mit `/`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub path: String,
    pub name: String,
    pub kind: NoteKind,
    #[serde(skip)]
    pub frontmatter: Option<Value>,
    pub tasks: Vec<Task>,
    /// im Text erwähnte Ticket-Schlüssel
    pub jira_mentions: Vec<String>,
    /// im Frontmatter-Feld `jira` zugeordnete Schlüssel (nur Projekte)
    pub jira_assigned: Vec<String>,
    /// Fokus-Einträge aus dem Log (Datum, Minuten)
    #[serde(skip)]
    pub focus_log: Vec<(chrono::NaiveDate, u32)>,
    pub warnings: Vec<String>,
}

impl Note {
    pub fn fm_str(&self, key: &str) -> Option<String> {
        match self.frontmatter.as_ref()?.get(key)? {
            Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("Vault-Ordner nicht gefunden")]
    VaultMissing,
    #[error("Ungültiges Ausschlussmuster: {0}")]
    Glob(#[from] globset::Error),
}

pub fn nfc(s: &str) -> String {
    s.nfc().collect()
}

/// Trennt Frontmatter (`---` … `---`) vom Inhalt. Gibt (YAML, Startzeile des Inhalts) zurück.
pub fn split_frontmatter(text: &str) -> (Option<&str>, usize) {
    let Some(first_end) = text.find('\n') else { return (None, 0) };
    if text[..first_end].trim_end_matches('\r') != "---" {
        return (None, 0);
    }
    let mut pos = first_end + 1;
    let mut line_no = 1;
    while pos < text.len() {
        let end = text[pos..].find('\n').map_or(text.len(), |e| pos + e);
        if text[pos..end].trim_end_matches('\r') == "---" {
            let yaml = text[first_end + 1..pos].trim_end_matches(['\n', '\r']);
            return (Some(yaml), line_no + 1);
        }
        pos = end + 1;
        line_no += 1;
    }
    (None, 0)
}

fn build_globs(patterns: &[String]) -> Result<GlobSet, ScanError> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p)?);
    }
    Ok(b.build()?)
}

fn jira_list(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
        Value::Sequence(seq) => seq.iter().filter_map(|x| x.as_str().map(|s| s.trim().to_string())).collect(),
        _ => vec![],
    }
}

/// Liest eine einzelne Notiz.
pub fn parse_note(rel_path: &str, text: &str, cfg: &Config, filter: &KeyFilter) -> Note {
    let path = nfc(&rel_path.replace('\\', "/"));
    let name = path.rsplit('/').next().unwrap_or(&path).trim_end_matches(".md").to_string();
    let (yaml, _) = split_frontmatter(text);
    let mut warnings = vec![];
    let frontmatter = yaml.and_then(|y| match serde_yaml_ng::from_str::<Value>(y) {
        Ok(v) => Some(v),
        Err(_) => {
            warnings.push("Frontmatter nicht lesbar".into());
            None
        }
    });

    let fm = &cfg.frontmatter;
    let typ = frontmatter
        .as_ref()
        .and_then(|v| v.get(&fm.type_key))
        .and_then(Value::as_str)
        .map(str::trim);
    let kind = if typ == Some(fm.project_type.as_str()) {
        NoteKind::Project
    } else if typ == Some(fm.area_type.as_str()) {
        NoteKind::Area
    } else if path.starts_with(&format!("{}/", cfg.folders.journal)) {
        NoteKind::Journal
    } else if path.starts_with(&format!("{}/", cfg.folders.inbox)) {
        NoteKind::Inbox
    } else {
        NoteKind::Other
    };

    let mut jira_assigned = vec![];
    if kind == NoteKind::Project {
        if let Some(v) = frontmatter.as_ref().and_then(|f| f.get(&fm.jira)) {
            for k in jira_list(v) {
                if is_valid_key(&k) {
                    jira_assigned.push(k);
                } else {
                    warnings.push(format!("Ungültiger Eintrag im Feld {}", fm.jira));
                }
            }
        }
    }

    let tasks = text
        .split('\n')
        .enumerate()
        .filter_map(|(i, l)| parse_task_line(i, l.trim_end_matches('\r')))
        .collect();

    Note {
        jira_mentions: find_keys(text, filter),
        focus_log: crate::focuslog::parse_log_minutes(text),
        path,
        name,
        kind,
        frontmatter,
        tasks,
        jira_assigned,
        warnings,
    }
}

/// Scannt den Vault gemäß Konfiguration.
pub fn scan_vault(cfg: &Config, filter: &KeyFilter) -> Result<Vec<Note>, ScanError> {
    let root = &cfg.vault.path;
    if !root.is_dir() {
        return Err(ScanError::VaultMissing);
    }
    let excludes = build_globs(&cfg.scan.exclude)?;
    let mut notes = vec![];
    // Ausgeschlossene Ordner gar nicht erst betreten (spart bei großen Wissensordnern viel Zeit).
    let walker = WalkDir::new(root).into_iter().filter_entry(|e| {
        if e.depth() == 0 || !e.file_type().is_dir() {
            return true;
        }
        rel_path(root, e.path()).is_none_or(|rel| !excludes.is_match(format!("{rel}/_")))
    });
    for entry in walker.filter_map(Result::ok) {
        let p = entry.path();
        if !entry.file_type().is_file() || p.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(rel) = rel_path(root, p) else { continue };
        if excludes.is_match(&rel) || is_sync_conflict(&rel) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(p) else { continue };
        notes.push(parse_note(&rel, &text, cfg, filter));
    }
    notes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(notes)
}

fn rel_path(root: &Path, p: &Path) -> Option<String> {
    let rel = p.strip_prefix(root).ok()?;
    Some(nfc(&rel.to_string_lossy().replace('\\', "/")))
}

/// Entscheidet für den Datei-Watcher, ob eine geänderte Datei einen Neuaufbau des Index auslöst.
/// Vorkompiliert, weil der Watcher sie für jedes Ereignis aufruft.
pub struct ChangeFilter {
    root: std::path::PathBuf,
    excludes: GlobSet,
}

impl ChangeFilter {
    pub fn new(cfg: &Config) -> Result<Self, ScanError> {
        Ok(Self { root: cfg.vault.path.clone(), excludes: build_globs(&cfg.scan.exclude)? })
    }

    /// Relevant sind Markdown-Dateien im Vault, die nicht ausgeschlossen sind.
    /// Das Löschen einer Datei zählt auch (die Datei existiert dann nicht mehr).
    pub fn is_relevant(&self, abs: &Path) -> bool {
        if abs.extension().and_then(|e| e.to_str()) != Some("md") {
            return false;
        }
        let Some(rel) = rel_path(&self.root, abs) else { return false };
        !(rel.starts_with(".obsidian/") || self.excludes.is_match(&rel) || is_sync_conflict(&rel))
    }
}

/// Erkennt typische Konfliktdateien von Cloud-Sync-Diensten (SPEC 6.4).
pub fn is_sync_conflict(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel).to_lowercase();
    name.contains("conflicted copy") || name.contains("konflikt") || name.contains("(conflict")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_filter() {
        let cfg = Config::for_vault("/v");
        let f = ChangeFilter::new(&cfg).unwrap();
        let p = |s: &str| Path::new("/v").join(s);
        assert!(f.is_relevant(&p("10 Projekte/A/A.md")));
        assert!(f.is_relevant(&p("Journal/2026-11-11.md")));
        assert!(!f.is_relevant(&p("30 Wissen/X.md")), "ausgeschlossen");
        assert!(!f.is_relevant(&p(".obsidian/workspace.json")));
        assert!(!f.is_relevant(&p("10 Projekte/A/.A.md.tasker-tmp")), "eigene Temp-Datei");
        assert!(!f.is_relevant(&p("A (conflicted copy).md")));
        assert!(!f.is_relevant(Path::new("/anders/A.md")), "außerhalb des Vaults");
    }

    #[test]
    fn splits_frontmatter() {
        let text = "---\ntyp: projekt\n---\n# Titel\n";
        let (y, start) = split_frontmatter(text);
        assert_eq!(y, Some("typ: projekt"));
        assert_eq!(start, 3);
        assert_eq!(split_frontmatter("# ohne\n").0, None);
    }

    #[test]
    fn reads_project_with_jira_field() {
        let cfg = Config::for_vault(".");
        let f = KeyFilter::new(["ABC".to_string()]);
        let text = "---\ntyp: projekt\nstatus: aktiv\njira: [ABC-1, ABC-2, kaputt]\n---\n- [ ] Tun ABC-9\n";
        let n = parse_note("10 Projekte/X/X.md", text, &cfg, &f);
        assert_eq!(n.kind, NoteKind::Project);
        assert_eq!(n.jira_assigned, vec!["ABC-1", "ABC-2"]);
        assert_eq!(n.warnings.len(), 1);
        assert_eq!(n.tasks.len(), 1);
        assert_eq!(n.tasks[0].line, 5);
        assert_eq!(n.fm_str("status").as_deref(), Some("aktiv"));
        assert!(n.jira_mentions.contains(&"ABC-9".to_string()));
    }

    #[test]
    fn detects_conflict_files() {
        assert!(is_sync_conflict("a/Notiz (conflicted copy 2026-11-11).md"));
        assert!(!is_sync_conflict("a/Notiz.md"));
    }
}
