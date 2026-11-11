//! Tauri-Backend: stellt den Vault-Index als Befehle für die Oberfläche bereit.
//! Die Konfiguration liegt im App-Datenordner (SPEC 9), nie im Repo.

mod jira;
mod today;
mod watcher;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tasker_core::edit::{edit_line, EditResult, LineEdit};
use tasker_core::index::NoteRef;
use tasker_core::jira::{group_issues, JiraIssue, StatusGroup};
use tasker_core::{load_index, load_index_with_keys, Config, Dashboard, Index, Scope};
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

// --- Jira (SPEC 4) ---

fn cache_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path().app_cache_dir().map(|d| d.join("jira-cache.json")).map_err(|_| "Cache-Ordner unbekannt.".into())
}

fn now_rfc3339() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JiraStatus {
    /// Site-URL und E-Mail sind eingetragen
    configured: bool,
    site_set: bool,
    has_token: bool,
    email: String,
    token_type: String,
    refresh_minutes: u32,
    fetched_at: Option<String>,
    user: Option<String>,
    demo: bool,
}

#[tauri::command]
fn jira_status(app: tauri::AppHandle, demo: Option<bool>) -> Result<JiraStatus, String> {
    let (cfg, source) = resolve_config(&app, demo.unwrap_or(false))?;
    if source == "demo" {
        return Ok(JiraStatus {
            configured: true,
            site_set: true,
            has_token: true,
            email: String::new(),
            token_type: "demo".into(),
            refresh_minutes: 0,
            fetched_at: Some(now_rfc3339()),
            user: Some("Demo".into()),
            demo: true,
        });
    }
    let cache = jira::read_cache(&cache_path(&app)?);
    Ok(JiraStatus {
        configured: cfg.jira.is_configured(),
        site_set: cfg.jira.base_url.starts_with("https://") && !cfg.jira.base_url.contains('<'),
        has_token: jira::load_token().is_some(),
        email: cfg.jira.email.clone(),
        token_type: cfg.jira.token_type.clone(),
        refresh_minutes: cfg.jira.refresh_minutes,
        fetched_at: cache.fetched_at,
        user: cache.user,
        demo: false,
    })
}

/// Speichert die E-Mail in der Konfiguration und den Token (falls angegeben) im Schlüsselbund.
/// Der Token wird nie in eine Datei geschrieben und nie zurückgegeben.
#[tauri::command]
fn jira_save_credentials(app: tauri::AppHandle, email: String, token: Option<String>) -> Result<(), String> {
    let email = email.trim();
    if !email.contains('@') {
        return Err("Bitte eine gültige E-Mail-Adresse eingeben.".into());
    }
    let path = config_path(&app).filter(|p| p.is_file()).ok_or("Keine Konfigurationsdatei gefunden.")?;
    let text = std::fs::read_to_string(&path).map_err(|_| "Konfiguration nicht lesbar.".to_string())?;
    let mut v: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| "Konfiguration ist kein gültiges JSON.".to_string())?;
    let root = v.as_object_mut().ok_or("Konfiguration ist kein JSON-Objekt.")?;
    let jira = root.entry("jira").or_insert_with(|| serde_json::json!({}));
    jira.as_object_mut().ok_or("Feld jira ist kein Objekt.")?.insert("email".into(), email.into());
    let out = serde_json::to_string_pretty(&v).map_err(|_| "Konfiguration nicht schreibbar.".to_string())? + "\n";
    tasker_core::edit::write_atomic(&path, &out).map_err(|_| "Konfiguration nicht schreibbar.".to_string())?;
    if let Some(t) = token.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) {
        jira::store_token(&t)?;
    }
    Ok(())
}

/// Entfernt den Token aus dem Schlüsselbund und leert den Cache.
#[tauri::command]
fn jira_delete_token(app: tauri::AppHandle) -> Result<(), String> {
    jira::delete_token()?;
    let _ = std::fs::remove_file(cache_path(&app)?);
    Ok(())
}

async fn jira_client(cfg: &Config) -> Result<jira::Client, String> {
    let token = jira::load_token().ok_or("Kein Jira-Token hinterlegt.")?;
    jira::Client::connect(&cfg.jira, token).await
}

/// Verbindungstest: liefert den Anzeigenamen des Jira-Nutzers.
#[tauri::command]
async fn jira_test(app: tauri::AppHandle) -> Result<String, String> {
    let (cfg, _) = resolve_config(&app, false)?;
    jira_client(&cfg).await?.myself().await
}

/// Ticket, das im Vault vorkommt, aber nicht im Ergebnis der Abfrage ist.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VaultOnlyKey {
    key: String,
    url: String,
    assigned_to: Vec<NoteRef>,
    mentioned_in: Vec<NoteRef>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JiraView {
    fetched_at: Option<String>,
    user: Option<String>,
    total: usize,
    groups: Vec<StatusGroup>,
    vault_only: Vec<VaultOnlyKey>,
}

fn build_view(cfg: &Config, cache: jira::JiraCache, browse_base: &str) -> Result<JiraView, String> {
    let index = load_index_with_keys(cfg, today(), cache.issues.iter().map(|i| i.key.clone()))
        .map_err(|e| e.to_string())?;
    let groups = group_issues(&cache.issues, &index);
    let base = browse_base.trim_end_matches('/');
    let keys: std::collections::BTreeSet<&String> = index
        .jira_assigned
        .keys()
        .chain(index.jira_mentions.keys())
        .filter(|k| !cache.issues.iter().any(|i| &i.key == *k))
        .collect();
    let mut vault_only: Vec<VaultOnlyKey> = keys
        .into_iter()
        .map(|k| VaultOnlyKey {
            key: k.clone(),
            url: format!("{base}/browse/{k}"),
            assigned_to: index.jira_assigned.get(k).cloned().unwrap_or_default(),
            mentioned_in: index.jira_mentions.get(k).cloned().unwrap_or_default(),
        })
        .collect();
    vault_only.sort_by_key(|v| v.assigned_to.is_empty()); // zugeordnete zuerst, sonst alphabetisch
    Ok(JiraView { fetched_at: cache.fetched_at, user: cache.user, total: cache.issues.len(), groups, vault_only })
}

const DEMO_SITE: &str = "https://jira.example.com";

/// Erfundene Tickets passend zum Demo-Vault.
fn demo_issues() -> Vec<JiraIssue> {
    let mk = |key: &str, summary: &str, status: &str, cat: &str, typ: &str, prio: &str, updated: &str| JiraIssue {
        key: key.into(),
        summary: summary.into(),
        status: status.into(),
        status_category: cat.into(),
        issue_type: typ.into(),
        priority: prio.into(),
        updated: updated.into(),
        url: format!("{DEMO_SITE}/browse/{key}"),
    };
    vec![
        mk("ABC-124", "Release 2.0 freigeben", "In Arbeit", "indeterminate", "Story", "High", "2026-11-10"),
        mk("ABC-140", "Fehler beim Export", "Review", "indeterminate", "Bug", "Highest", "2026-11-11"),
        mk("ABC-123", "Beleuchtung bestellen", "Offen", "new", "Task", "Medium", "2026-11-05"),
        mk("ABC-131", "Schulungsunterlagen prüfen", "Offen", "new", "Task", "Low", "2026-10-28"),
    ]
}

/// Tickets aus dem Cache, verknüpft mit dem aktuellen Vault (ohne Netzwerk).
#[tauri::command]
fn jira_issues(app: tauri::AppHandle, demo: Option<bool>) -> Result<JiraView, String> {
    let (cfg, source) = resolve_config(&app, demo.unwrap_or(false))?;
    if source == "demo" {
        let cache = jira::JiraCache { fetched_at: Some(now_rfc3339()), user: Some("Demo".into()), issues: demo_issues() };
        return build_view(&cfg, cache, DEMO_SITE);
    }
    build_view(&cfg, jira::read_cache(&cache_path(&app)?), &cfg.jira.base_url)
}

/// Holt die Tickets neu von Jira und speichert sie im Cache.
#[tauri::command]
async fn jira_refresh(app: tauri::AppHandle, demo: Option<bool>) -> Result<JiraView, String> {
    let (cfg, source) = resolve_config(&app, demo.unwrap_or(false))?;
    if source == "demo" {
        return jira_issues(app, demo);
    }
    let client = jira_client(&cfg).await?;
    let user = client.myself().await?;
    let issues = client.search(&cfg.jira.jql).await?;
    let cache = jira::JiraCache { fetched_at: Some(now_rfc3339()), user: Some(user), issues };
    jira::write_cache(&cache_path(&app)?, &cache)?;
    build_view(&cfg, cache, &cfg.jira.base_url)
}

#[tauri::command]
fn config_location(app: tauri::AppHandle) -> Option<String> {
    config_path(&app).map(|p| p.display().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(watcher::WatchState::default())
        .manage(today::BlockLock::default())
        .invoke_handler(tauri::generate_handler![
            get_overview,
            complete_task,
            schedule_task,
            undo_edit,
            watch_vault,
            config_location,
            jira_status,
            jira_save_credentials,
            jira_delete_token,
            jira_test,
            jira_issues,
            jira_refresh,
            today::get_today,
            today::save_block,
            today::remove_block
        ])
        .run(tauri::generate_context!())
        .expect("Tasker.md konnte nicht gestartet werden");
}
