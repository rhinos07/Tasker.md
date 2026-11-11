//! Befehle für den Fokus-Timer (SPEC 6.5).
//! Die Sitzungen liegen in `focus.json` im App-Datenordner; das Vault-Log bekommt eine Zeile pro Aufgabe und Tag.
//! Der Timer selbst läuft in der Oberfläche, hier wird nur die abgeschlossene Sitzung verbucht.

use crate::{jira, resolve_config, today};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tasker_core::config::FocusConfig;
use tasker_core::focus::{booked_key, day_summary, hm, task_day_total, Session, TicketRow};
use tasker_core::focuslog::{revert_log, write_log, LogChange};
use tasker_core::jira::{find_keys, KeyFilter};
use tasker_core::vault::NoteKind;
use tasker_core::{load_index_with_keys, Config};
use tauri::Manager;

#[derive(Default)]
pub struct FocusLock(Mutex<()>);

/// Noch nicht ins Vault-Log geschriebene Tagessumme (z. B. Datei war gesperrt).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct PendingLog {
    path: String,
    date: chrono::NaiveDate,
    task_key: String,
    description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct FocusStore {
    sessions: Vec<Session>,
    /// Aufgabe → lokal gewähltes Ticket
    choices: BTreeMap<String, String>,
    /// `datum|ticket` bzw. `datum|aufgabe`, von Hand als gebucht markiert
    booked: BTreeSet<String>,
    pending_logs: Vec<PendingLog>,
}

fn store_path(app: &tauri::AppHandle, demo: bool) -> Result<PathBuf, String> {
    let name = if demo { "focus-demo.json" } else { "focus.json" };
    app.path().app_data_dir().map(|d| d.join(name)).map_err(|_| "App-Datenordner unbekannt.".into())
}

fn read_store(path: &Path) -> FocusStore {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn write_store(path: &Path, s: &FocusStore) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|_| "App-Datenordner nicht anlegbar.".to_string())?;
    }
    let text = serde_json::to_string_pretty(s).map_err(|_| "Fokus-Daten nicht schreibbar.".to_string())?;
    tasker_core::edit::write_atomic(path, &text).map_err(|_| "Fokus-Daten nicht schreibbar.".into())
}

fn jira_cache(app: &tauri::AppHandle) -> jira::JiraCache {
    crate::cache_path(app).map(|p| jira::read_cache(&p)).unwrap_or_default()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusSettings {
    #[serde(flatten)]
    focus: FocusConfig,
    round_to_minutes: u32,
}

#[tauri::command]
pub fn focus_settings(app: tauri::AppHandle, demo: Option<bool>) -> Result<FocusSettings, String> {
    let (cfg, _) = resolve_config(&app, demo.unwrap_or(false))?;
    Ok(FocusSettings { focus: cfg.focus.clone(), round_to_minutes: cfg.jira.worklog.round_to_minutes })
}

/// Die Aufgabe, an der die Sitzung lief.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusTarget {
    path: String,
    description: String,
    raw: String,
    task_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishResult {
    minutes: u32,
    /// Änderung im Vault-Log (für Rückgängig)
    log: Option<LogChange>,
    /// Warum nichts ins Log geschrieben wurde, falls zutreffend
    log_note: Option<String>,
}

fn write_task_log(cfg: &Config, store: &FocusStore, p: &PendingLog) -> Result<Option<LogChange>, String> {
    let (minutes, sessions) = task_day_total(&store.sessions, p.date, &p.task_key, cfg.focus.min_minutes);
    if sessions == 0 {
        return Ok(None);
    }
    crate::index_cache::invalidate();
    write_log(&cfg.vault.path, &p.path, &cfg.focus.log_section, &cfg.focus.log_format, p.date, &p.description, minutes, sessions)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// Verbucht eine abgeschlossene Sitzung: lokal speichern und (wenn eingestellt) ins Log der Projekt- bzw. Bereichsnotiz.
#[tauri::command]
pub fn focus_finish(
    app: tauri::AppHandle,
    lock: tauri::State<'_, FocusLock>,
    target: FocusTarget,
    start: String,
    minutes: u32,
    demo: Option<bool>,
) -> Result<FinishResult, String> {
    if minutes == 0 {
        return Ok(FinishResult { minutes, log: None, log_note: Some("Kürzer als eine Minute, nichts erfasst.".into()) });
    }
    let (cfg, source) = resolve_config(&app, demo.unwrap_or(false))?;
    let cache = jira_cache(&app);
    let index = load_index_with_keys(&cfg, today(), cache.issues.iter().map(|i| i.key.clone())).map_err(|e| e.to_string())?;
    let mut filter = KeyFilter::new(cfg.jira.project_keys.iter().cloned());
    filter.known_keys.extend(cache.issues.iter().map(|i| i.key.clone()));

    let note_kind = index.tasks.iter().find(|t| t.note_path == target.path).map(|t| t.note_kind);
    let project = index.projects.iter().find(|p| p.path == target.path);
    let session = Session {
        id: format!("s{}", chrono::Local::now().timestamp_nanos_opt().unwrap_or_default()),
        date: today(),
        start,
        minutes,
        path: target.path.clone(),
        description: target.description.clone(),
        task_id: target.task_id.clone(),
        line_keys: find_keys(&target.raw, &filter),
        project_keys: project.map(|p| p.jira_assigned.clone()).unwrap_or_default(),
        project_mentions: project.map(|p| p.jira_mentions.clone()).unwrap_or_default(),
    };
    let key = session.task_key();

    let _g = lock.0.lock().map_err(|_| "Fokus-Daten gesperrt.".to_string())?;
    let path = store_path(&app, source == "demo")?;
    let mut store = read_store(&path);
    store.sessions.push(session.clone());

    let (log, log_note) = if !cfg.focus.write_to_vault {
        (None, None)
    } else if source == "demo" {
        (None, Some("Demo-Vault ist schreibgeschützt, Zeit nur lokal erfasst.".into()))
    } else if minutes < cfg.focus.min_minutes {
        (None, Some(format!("Unter {} min, nicht ins Log geschrieben.", cfg.focus.min_minutes)))
    } else if !matches!(note_kind, Some(NoteKind::Project | NoteKind::Area)) {
        (None, Some("Aufgabe gehört zu keinem Projekt oder Bereich, Zeit nur lokal erfasst.".into()))
    } else {
        let p = PendingLog { path: target.path.clone(), date: session.date, task_key: key.clone(), description: target.description.clone() };
        match write_task_log(&cfg, &store, &p) {
            Ok(change) => {
                store.pending_logs.retain(|x| x != &p);
                (change, None)
            }
            Err(e) => {
                if !store.pending_logs.contains(&p) {
                    store.pending_logs.push(p);
                }
                (None, Some(format!("Log nicht geschrieben ({e}). Wird beim nächsten Mal nachgetragen.")))
            }
        }
    };
    // vorgemerkte Einträge nachtragen
    if source != "demo" && cfg.focus.write_to_vault {
        let pending = std::mem::take(&mut store.pending_logs);
        for p in pending {
            if write_task_log(&cfg, &store, &p).is_err() {
                store.pending_logs.push(p);
            }
        }
    }
    write_store(&path, &store)?;
    Ok(FinishResult { minutes, log, log_note })
}

#[tauri::command]
pub fn focus_undo_log(app: tauri::AppHandle, change: LogChange) -> Result<(), String> {
    let cfg = crate::writable_config(&app, false)?;
    revert_log(&cfg.vault.path, &change).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayRow {
    #[serde(flatten)]
    row: TicketRow,
    /// Bezeichner für „gebucht“ und Zuordnung: Ticket oder Aufgabe
    row_id: String,
    title: Option<String>,
    url: Option<String>,
    /// fertige Zeile zum Kopieren, z. B. `ABC-123 · 0h 45m · Aufgabe`
    copy_text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySummary {
    date: chrono::NaiveDate,
    rows: Vec<DayRow>,
    total_minutes: u32,
    round_to_minutes: u32,
    /// Tickets aus dem Jira-Cache zur Auswahl bei „ohne Ticket“
    known_tickets: Vec<(String, String)>,
}

#[tauri::command]
pub fn focus_day(app: tauri::AppHandle, date: Option<String>, demo: Option<bool>) -> Result<DaySummary, String> {
    let (cfg, source) = resolve_config(&app, demo.unwrap_or(false))?;
    let date = match date {
        Some(d) => chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").map_err(|_| "Ungültiges Datum.".to_string())?,
        None => today(),
    };
    let store = read_store(&store_path(&app, source == "demo")?);
    let cache = jira_cache(&app);
    let step = cfg.jira.worklog.round_to_minutes;
    let base = cfg.jira.base_url.trim_end_matches('/');
    let rows: Vec<DayRow> = day_summary(&store.sessions, date, &store.choices, &store.booked, step)
        .into_iter()
        .map(|row| {
            let row_id = row.key.clone().unwrap_or_else(|| row.tasks[0].task_key.clone());
            let title = row.key.as_ref().and_then(|k| cache.issues.iter().find(|i| &i.key == k).map(|i| i.summary.clone()));
            let url = row.key.as_ref().filter(|_| cfg.jira.is_configured()).map(|k| format!("{base}/browse/{k}"));
            let tasks = row.tasks.iter().map(|t| t.description.as_str()).collect::<Vec<_>>().join(", ");
            let copy_text = match &row.key {
                Some(k) => format!("{k} · {} · {tasks}", hm(row.rounded_minutes)),
                None => format!("{} · {tasks}", hm(row.rounded_minutes)),
            };
            DayRow { row, row_id, title, url, copy_text }
        })
        .collect();
    let total_minutes = rows.iter().map(|r| r.row.raw_minutes).sum();
    let known_tickets = cache.issues.iter().map(|i| (i.key.clone(), i.summary.clone())).collect();
    Ok(DaySummary { date, rows, total_minutes, round_to_minutes: step, known_tickets })
}

/// Ordnet einer Aufgabe lokal ein Ticket zu (`None` entfernt die Zuordnung). Im Vault ändert sich nichts.
#[tauri::command]
pub fn focus_choose_ticket(
    app: tauri::AppHandle,
    lock: tauri::State<'_, FocusLock>,
    task_key: String,
    ticket: Option<String>,
    demo: Option<bool>,
) -> Result<(), String> {
    let (_, source) = resolve_config(&app, demo.unwrap_or(false))?;
    if let Some(t) = &ticket {
        if !tasker_core::jira::is_valid_key(t) {
            return Err("Ungültiger Ticket-Schlüssel.".into());
        }
    }
    let _g = lock.0.lock().map_err(|_| "Fokus-Daten gesperrt.".to_string())?;
    let path = store_path(&app, source == "demo")?;
    let mut store = read_store(&path);
    match ticket {
        Some(t) => store.choices.insert(task_key, t),
        None => store.choices.remove(&task_key),
    };
    write_store(&path, &store)
}

#[tauri::command]
pub fn focus_set_booked(
    app: tauri::AppHandle,
    lock: tauri::State<'_, FocusLock>,
    date: String,
    row_id: String,
    booked: bool,
    demo: Option<bool>,
) -> Result<(), String> {
    let (_, source) = resolve_config(&app, demo.unwrap_or(false))?;
    let date = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(|_| "Ungültiges Datum.".to_string())?;
    let _g = lock.0.lock().map_err(|_| "Fokus-Daten gesperrt.".to_string())?;
    let path = store_path(&app, source == "demo")?;
    let mut store = read_store(&path);
    let k = booked_key(date, &row_id);
    if booked {
        store.booked.insert(k);
    } else {
        store.booked.remove(&k);
    }
    write_store(&path, &store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_roundtrip_and_defaults() {
        let p = std::env::temp_dir().join(format!("tasker-focus-{}.json", std::process::id()));
        assert!(read_store(&p).sessions.is_empty());
        let mut s = FocusStore::default();
        s.choices.insert("a".into(), "ABC-1".into());
        write_store(&p, &s).unwrap();
        assert_eq!(read_store(&p).choices.get("a").map(String::as_str), Some("ABC-1"));
        // ältere Datei ohne neue Felder bleibt lesbar
        std::fs::write(&p, r#"{"sessions":[]}"#).unwrap();
        assert!(read_store(&p).pending_logs.is_empty());
        std::fs::remove_file(&p).unwrap();
    }
}
