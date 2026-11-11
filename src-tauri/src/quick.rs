//! Schnelleingabe (SPEC 6.2): Vorschau, Anlegen und Rückgängig.

use crate::{resolve_config, today, writable_config};
use serde::Serialize;
use tasker_core::create::{add_task, create_inbox_note, undo_insert, InsertResult};
use tasker_core::quick::{note_title, parse_quick, QuickTask};
use tasker_core::{load_index, Config, Index};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub path: String,
    pub name: String,
    /// `project` oder `area`
    pub kind: &'static str,
    pub heading: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    line: String,
    task: QuickTask,
    /// aufgelöstes Ziel; `None` = Inbox
    target: Option<Target>,
    /// Pfad der neuen Inbox-Notiz, falls kein Ziel
    inbox_path: Option<String>,
    /// Vorschläge zum Text nach `>`
    candidates: Vec<Target>,
    contexts: Vec<String>,
    /// Hinweis, warum nicht gespeichert werden kann
    problem: Option<String>,
}

fn targets(index: &Index, cfg: &Config) -> Vec<Target> {
    let mut v: Vec<Target> = index
        .projects
        .iter()
        .map(|p| Target {
            path: p.path.clone(),
            name: p.name.clone(),
            kind: "project",
            heading: cfg.sections.project_tasks.clone(),
        })
        .collect();
    // aktive Projekte zuerst
    v.sort_by_key(|t| index.projects.iter().find(|p| p.path == t.path).and_then(|p| p.status.clone()).as_deref() != Some("aktiv"));
    v.extend(index.areas.iter().map(|a| Target {
        path: a.path.clone(),
        name: a.name.clone(),
        kind: "area",
        heading: cfg.sections.area_tasks.clone(),
    }));
    v
}

/// Exakt (ohne Groß/klein) vor eindeutigem Anfang vor eindeutigem Teilstring.
fn resolve_target(all: &[Target], q: &str) -> (Option<Target>, Vec<Target>) {
    let ql = q.to_lowercase();
    let starts: Vec<Target> = all.iter().filter(|t| t.name.to_lowercase().starts_with(&ql)).cloned().collect();
    let contains: Vec<Target> = all.iter().filter(|t| t.name.to_lowercase().contains(&ql)).cloned().collect();
    let candidates: Vec<Target> = starts.iter().chain(contains.iter().filter(|c| !starts.contains(c))).take(8).cloned().collect();
    let exact = all.iter().find(|t| t.name.to_lowercase() == ql).cloned();
    let chosen = exact
        .or_else(|| (starts.len() == 1).then(|| starts[0].clone()))
        .or_else(|| (contains.len() == 1).then(|| contains[0].clone()));
    (chosen, candidates)
}

fn build(cfg: &Config, index: &Index, input: &str) -> Preview {
    let task = parse_quick(input, today(), &cfg.waiting_tag);
    let all = targets(index, cfg);
    let (target, candidates) = match task.target.as_deref() {
        Some(q) => resolve_target(&all, q),
        None => (None, vec![]),
    };
    let problem = if task.description.trim().is_empty() {
        Some("Bitte einen Aufgabentext eingeben.".to_string())
    } else if task.target.is_some() && target.is_none() {
        Some(if candidates.is_empty() {
            "Kein Projekt oder Bereich mit diesem Namen.".to_string()
        } else {
            "Mehrere Treffer, bitte genauer angeben oder auswählen.".to_string()
        })
    } else {
        None
    };
    let inbox_path = (task.target.is_none()).then(|| format!("{}/{}.md", cfg.folders.inbox, note_title(&task.description)));
    Preview { line: task.line(), task, target, inbox_path, candidates, contexts: cfg.contexts.clone(), problem }
}

#[tauri::command]
pub fn quick_preview(app: tauri::AppHandle, input: String, demo: Option<bool>) -> Result<Preview, String> {
    let (cfg, _) = resolve_config(&app, demo.unwrap_or(false))?;
    let index = crate::index_cache::get(&cfg, today())?;
    Ok(build(&cfg, &index, &input))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Added {
    #[serde(flatten)]
    result: InsertResult,
    target_name: String,
}

#[tauri::command]
pub fn quick_add(app: tauri::AppHandle, input: String, demo: Option<bool>) -> Result<Added, String> {
    let cfg = writable_config(&app, demo.unwrap_or(false))?;
    let index = load_index(&cfg, today()).map_err(|e| e.to_string())?;
    let p = build(&cfg, &index, &input);
    if let Some(problem) = p.problem {
        return Err(problem);
    }
    match p.target {
        Some(t) => add_task(&cfg.vault.path, &t.path, &t.heading, &p.line)
            .map(|result| Added { result, target_name: t.name })
            .map_err(|e| e.to_string()),
        None => create_inbox_note(&cfg.vault.path, &cfg.folders.inbox, &note_title(&p.task.description), &p.line)
            .map(|result| Added { result, target_name: "Inbox".into() })
            .map_err(|e| e.to_string()),
    }
}

#[tauri::command]
pub fn quick_undo(app: tauri::AppHandle, result: InsertResult) -> Result<(), String> {
    let cfg = writable_config(&app, false)?;
    undo_insert(&cfg.vault.path, &result).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str) -> Target {
        Target { path: format!("{name}.md"), name: name.into(), kind: "project", heading: "## X".into() }
    }

    #[test]
    fn resolves_targets() {
        let all = vec![t("Gartenhaus"), t("Garage"), t("Release 2.0"), t("Haus")];
        assert_eq!(resolve_target(&all, "gartenhaus").0.unwrap().name, "Gartenhaus");
        assert_eq!(resolve_target(&all, "Gartenh").0.unwrap().name, "Gartenhaus");
        assert!(resolve_target(&all, "Gar").0.is_none(), "mehrdeutig");
        assert_eq!(resolve_target(&all, "Gar").1.len(), 2);
        assert_eq!(resolve_target(&all, "haus").0.unwrap().name, "Haus", "exakt vor Teilstring");
        assert_eq!(resolve_target(&all, "2.0").0.unwrap().name, "Release 2.0");
    }

    #[test]
    fn acceptance_quick_entry_on_demo_vault() {
        // SPEC 12, Kriterium 6 (Ziel und Abschnitt), gegen den Demo-Vault
        let cfg = crate::demo_config().expect("Demo-Vault im Debug-Build");
        let index = load_index(&cfg, today()).unwrap();
        let p = build(&cfg, &index, "Test morgen @telefon !1 >Gartenhaus");
        let target = p.target.expect("Ziel gefunden");
        assert_eq!(target.path, "10 Projekte/Gartenhaus/Gartenhaus.md");
        assert_eq!(target.heading, "## Nächste Schritte");
        assert!(p.line.starts_with("- [ ] Test #@telefon ⏫ ⏳ "));
        assert!(p.problem.is_none());
        let area = build(&cfg, &index, "Licht >Haus");
        assert_eq!(area.target.map(|t| t.kind), Some("area"));
        let inbox = build(&cfg, &index, "Idee notieren");
        assert_eq!(inbox.inbox_path.as_deref(), Some("00 Inbox/Idee notieren.md"));
    }
}
