//! Tauri-Backend: stellt den Vault-Index als Befehle für die Oberfläche bereit.
//! Die Konfiguration liegt im App-Datenordner (SPEC 9), nie im Repo.

mod watcher;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tasker_core::edit::{edit_line, EditResult, LineEdit};
use tasker_core::{load_index, Config, Dashboard, Index, Scope};
use tauri::{Manager, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Overview {
    vault_name: String,
    source: &'static str,
    index: Index,
    dashboard: Dashboard,
}

fn config_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("config.json"))
}

/// Demo-Vault aus dem Repo, nur in Debug-Builds.
fn demo_config() -> Option<Config> {
    if cfg!(debug_assertions) {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-vault");
        p.is_dir().then(|| Config::for_vault(p))
    } else {
        None
    }
}

fn resolve_config(app: &tauri::AppHandle, demo: bool) -> Result<(Config, &'static str), String> {
    if !demo {
        if let Some(p) = config_path(app).filter(|p| p.is_file()) {
            return Config::load(&p).map(|c| (c, "config")).map_err(|e| e.to_string());
        }
    }
    demo_config()
        .map(|c| (c, "demo"))
        .ok_or_else(|| "Keine Konfiguration gefunden. Bitte die Einrichtung durchführen.".into())
}

#[tauri::command]
fn get_overview(app: tauri::AppHandle, scope: Option<String>, demo: Option<bool>) -> Result<Overview, String> {
    let (cfg, source) = resolve_config(&app, demo.unwrap_or(false))?;
    let today = today();
    let index = load_index(&cfg, today).map_err(|e| e.to_string())?;
    let scope = match scope.as_deref() {
        Some("work") => Some(Scope::Work),
        Some("private") => Some(Scope::Private),
        _ => None,
    };
    let dashboard = index.dashboard(&cfg, scope, today);
    Ok(Overview { vault_name: cfg.vault.name.clone(), source, index, dashboard })
}

/// Eine Aufgabenzeile, wie die Oberfläche sie aus dem Index kennt.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskTarget {
    path: String,
    line: usize,
    raw: String,
}

/// Konfiguration für Schreibzugriffe. Der Demo-Vault im Repo ist schreibgeschützt,
/// damit die Tests stabil bleiben.
fn writable_config(app: &tauri::AppHandle, demo: bool) -> Result<Config, String> {
    match resolve_config(app, demo)? {
        (_, "demo") => Err("Der Demo-Vault ist schreibgeschützt.".into()),
        (cfg, _) => Ok(cfg),
    }
}

fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

#[tauri::command]
fn complete_task(app: tauri::AppHandle, target: TaskTarget, demo: Option<bool>) -> Result<EditResult, String> {
    let cfg = writable_config(&app, demo.unwrap_or(false))?;
    edit_line(&cfg.vault.path, &target.path, target.line, &target.raw, &LineEdit::Complete, today())
        .map_err(|e| e.to_string())
}

/// `today = true` setzt ⏳ auf heute, `false` entfernt ⏳.
#[tauri::command]
fn schedule_task(
    app: tauri::AppHandle,
    target: TaskTarget,
    today_flag: bool,
    demo: Option<bool>,
) -> Result<EditResult, String> {
    let cfg = writable_config(&app, demo.unwrap_or(false))?;
    let edit = LineEdit::SetScheduled(today_flag.then(today));
    edit_line(&cfg.vault.path, &target.path, target.line, &target.raw, &edit, today()).map_err(|e| e.to_string())
}

/// Macht eine Änderung rückgängig, aber nur, wenn die Zeile noch genau so aussieht wie nach der Änderung.
#[tauri::command]
fn undo_edit(app: tauri::AppHandle, edit: EditResult, demo: Option<bool>) -> Result<EditResult, String> {
    let cfg = writable_config(&app, demo.unwrap_or(false))?;
    edit_line(&cfg.vault.path, &edit.path, edit.line, &edit.new, &LineEdit::Replace(edit.old.clone()), today())
        .map_err(|e| e.to_string())
}

/// Startet (oder wechselt) den Datei-Watcher für den aktuell angezeigten Vault.
#[tauri::command]
fn watch_vault(app: tauri::AppHandle, state: State<'_, watcher::WatchState>, demo: Option<bool>) -> Result<(), String> {
    let (cfg, _) = resolve_config(&app, demo.unwrap_or(false))?;
    watcher::watch(&app, &state, &cfg)
}

#[tauri::command]
fn config_location(app: tauri::AppHandle) -> Option<String> {
    config_path(&app).map(|p| p.display().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(watcher::WatchState::default())
        .invoke_handler(tauri::generate_handler![
            get_overview,
            complete_task,
            schedule_task,
            undo_edit,
            watch_vault,
            config_location
        ])
        .run(tauri::generate_context!())
        .expect("Tasker.md konnte nicht gestartet werden");
}
