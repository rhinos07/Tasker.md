//! Eigene Filter (SPEC 6.3): ausführen, auflisten, speichern.

use crate::{index_cache, resolve_config, set_config_value, today};
use serde::Serialize;
use tasker_core::filter::{apply, Filter, FilterError, SavedFilter};
use tasker_core::index::TaskRef;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterResult {
    tasks: Vec<TaskRef>,
    /// Fehler in der Abfrage; dann ist `tasks` leer
    error: Option<FilterError>,
}

#[tauri::command]
pub fn filter_run(app: tauri::AppHandle, query: String, scope: Option<String>, demo: Option<bool>) -> Result<FilterResult, String> {
    let filter = match Filter::parse(&query) {
        Ok(f) => f,
        Err(e) => return Ok(FilterResult { tasks: vec![], error: Some(e) }),
    };
    let (cfg, _) = resolve_config(&app, demo.unwrap_or(false))?;
    let day = today();
    let index = index_cache::get(&cfg, day)?;
    let mut tasks: Vec<TaskRef> =
        apply(&filter, &index, &cfg, crate::today::parse_scope(scope.as_deref()), day).into_iter().cloned().collect();
    tasks.sort_by(|a, b| (a.task.priority, &a.note_name, a.task.line).cmp(&(b.task.priority, &b.note_name, b.task.line)));
    Ok(FilterResult { tasks, error: None })
}

#[tauri::command]
pub fn filters_list(app: tauri::AppHandle, demo: Option<bool>) -> Result<Vec<SavedFilter>, String> {
    Ok(resolve_config(&app, demo.unwrap_or(false))?.0.filters)
}

/// Prüft und speichert die komplette Liste in `config.json` (Schlüssel `filters`).
pub fn validate(filters: &[SavedFilter]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for f in filters {
        let name = f.name.trim();
        if name.is_empty() {
            return Err("Jeder Filter braucht einen Namen.".into());
        }
        if !seen.insert(name.to_lowercase()) {
            return Err(format!("Der Name „{name}“ kommt doppelt vor."));
        }
        Filter::parse(&f.query).map_err(|e| format!("Filter „{name}“: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn filters_save(app: tauri::AppHandle, filters: Vec<SavedFilter>, demo: Option<bool>) -> Result<(), String> {
    if demo.unwrap_or(false) || resolve_config(&app, false)?.1 == "demo" {
        return Err("Im Demo-Vault lassen sich keine Filter speichern.".into());
    }
    validate(&filters)?;
    let clean: Vec<SavedFilter> =
        filters.into_iter().map(|f| SavedFilter { name: f.name.trim().into(), query: f.query.trim().into() }).collect();
    set_config_value(&app, &["filters"], serde_json::to_value(clean).map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str, query: &str) -> SavedFilter {
        SavedFilter { name: name.into(), query: query.into() }
    }

    #[test]
    fn validates_names_and_queries() {
        assert!(validate(&[f("A", "prio:hoch"), f("B", "")]).is_ok());
        assert!(validate(&[f(" ", "prio")]).unwrap_err().contains("Namen"));
        assert!(validate(&[f("A", "x"), f("a", "y")]).unwrap_err().contains("doppelt"));
        assert!(validate(&[f("A", "farbe:rot")]).unwrap_err().contains("Filter „A“"));
    }
}
