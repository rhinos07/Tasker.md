//! Schnelleingabe (SPEC 6.2): Vorschau, Anlegen und Rückgängig.

use crate::today::{delete_block, put_block, valid_block, BlockLock, MAX_BLOCK_MINUTES};
use crate::{resolve_config, today, writable_config};
use chrono::NaiveDate;
use serde::Serialize;
use tasker_core::create::{add_task, create_inbox_note, undo_insert, InsertResult};
use tasker_core::quick::{note_title, parse_quick, QuickTask};
use tasker_core::today::{parse_hm, TimeBlock};
use tasker_core::task::parse_task_line;
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

/// Geplanter Zeitblock aus Uhrzeit und Dauer, Dauer schon aufgelöst.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BlockPlan {
    date: NaiveDate,
    start: String,
    end: String,
    minutes: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    line: String,
    block: Option<BlockPlan>,
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
    let block = task.block.as_ref().zip(task.scheduled).map(|(b, date)| {
        let minutes = b.minutes.unwrap_or(cfg.timeline.default_block_minutes);
        let end = parse_hm(&b.start).map(|t| (t + chrono::Duration::minutes(minutes as i64)).format("%H:%M").to_string()).unwrap_or_default();
        BlockPlan { date, start: b.start.clone(), end, minutes }
    });
    let problem = if task.description.trim().is_empty() {
        Some("Bitte einen Aufgabentext eingeben.".to_string())
    } else if block.as_ref().is_some_and(|b| !valid_block(&b.start, b.minutes)) {
        Some(format!("Zeitblock: Dauer 5 bis {} Minuten.", MAX_BLOCK_MINUTES))
    } else if block.as_ref().is_some_and(|b| b.date < today()) {
        Some("Zeitblock liegt in der Vergangenheit.".to_string())
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
    Preview { line: task.line(), block, task, target, inbox_path, candidates, contexts: cfg.contexts.clone(), problem }
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
    /// angelegter Zeitblock
    block: Option<TimeBlock>,
    /// Aufgabe angelegt, Block aber nicht
    block_error: Option<String>,
}

#[tauri::command]
pub fn quick_add(app: tauri::AppHandle, lock: tauri::State<'_, BlockLock>, input: String, demo: Option<bool>) -> Result<Added, String> {
    let cfg = writable_config(&app, demo.unwrap_or(false))?;
    let index = load_index(&cfg, today()).map_err(|e| e.to_string())?;
    let p = build(&cfg, &index, &input);
    if let Some(problem) = p.problem {
        return Err(problem);
    }
    let (result, target_name) = match &p.target {
        Some(t) => (add_task(&cfg.vault.path, &t.path, &t.heading, &p.line).map_err(|e| e.to_string())?, t.name.clone()),
        None => (
            create_inbox_note(&cfg.vault.path, &cfg.folders.inbox, &note_title(&p.task.description), &p.line).map_err(|e| e.to_string())?,
            "Inbox".into(),
        ),
    };
    let mut added = Added { result, target_name, block: None, block_error: None };
    if let Some(plan) = p.block {
        // Der Block hängt an Pfad und Beschreibung, wie sie der Index aus der neuen Zeile liest
        let made = parse_task_line(0, &added.result.new)
            .ok_or_else(|| "Neue Zeile nicht lesbar.".to_string())
            .and_then(|t| {
                let index = load_index(&cfg, today()).map_err(|e| e.to_string())?;
                let block = TimeBlock {
                    id: String::new(),
                    date: plan.date,
                    start: plan.start,
                    minutes: plan.minutes,
                    path: added.result.path.clone(),
                    description: t.description,
                };
                put_block(&app, &lock, false, &index, block)
            });
        match made {
            Ok(b) => added.block = Some(b),
            Err(e) => added.block_error = Some(e),
        }
    }
    Ok(added)
}

#[tauri::command]
pub fn quick_undo(app: tauri::AppHandle, lock: tauri::State<'_, BlockLock>, result: InsertResult, block_id: Option<String>) -> Result<(), String> {
    let cfg = writable_config(&app, false)?;
    undo_insert(&cfg.vault.path, &result).map_err(|e| e.to_string())?;
    match block_id {
        Some(id) => delete_block(&app, &lock, false, &id),
        None => Ok(()),
    }
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
        assert!(inbox.block.is_none());

        // Uhrzeit und Dauer: Block am geplanten Tag, Zeile ohne Uhrzeit
        let timed = build(&cfg, &index, "Konzept schreiben 14:00 90m >Gartenhaus");
        let b = timed.block.expect("Zeitblock");
        assert_eq!((b.date, b.start.as_str(), b.end.as_str(), b.minutes), (today(), "14:00", "15:30", 90));
        assert_eq!(timed.line, format!("- [ ] Konzept schreiben ⏳ {}", today()));
        assert!(timed.problem.is_none());
        let default = build(&cfg, &index, "Kurz 9:00").block.unwrap();
        assert_eq!((default.minutes, default.end.as_str()), (cfg.timeline.default_block_minutes, "09:30"));
        assert!(build(&cfg, &index, "Lang 8:00 13h").problem.is_some(), "zu lang");
    }
}
