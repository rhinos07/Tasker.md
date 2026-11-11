//! Tauri-Backend: stellt den Vault-Index als Befehle für die Oberfläche bereit.
//! Die Konfiguration liegt im App-Datenordner (SPEC 9), nie im Repo.

use serde::Serialize;
use std::path::PathBuf;
use tasker_core::{load_index, Config, Dashboard, Index, Scope};
use tauri::Manager;

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
    let today = chrono::Local::now().date_naive();
    let index = load_index(&cfg, today).map_err(|e| e.to_string())?;
    let scope = match scope.as_deref() {
        Some("work") => Some(Scope::Work),
        Some("private") => Some(Scope::Private),
        _ => None,
    };
    let dashboard = index.dashboard(&cfg, scope, today);
    Ok(Overview { vault_name: cfg.vault.name.clone(), source, index, dashboard })
}

#[tauri::command]
fn config_location(app: tauri::AppHandle) -> Option<String> {
    config_path(&app).map(|p| p.display().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_overview, config_location])
        .run(tauri::generate_context!())
        .expect("Tasker.md konnte nicht gestartet werden");
}
