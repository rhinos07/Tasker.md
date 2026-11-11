//! Tauri-Backend: stellt den Vault-Index als Befehle für die Oberfläche bereit.
//! Die Konfiguration liegt im App-Datenordner (SPEC 9), nie im Repo.

mod autostart;
mod calendar;
mod filters;
mod focus;
mod google;
mod hotkey;
mod index_cache;
mod jira;
mod quick;
mod settings;
mod today;
mod tray;
mod upcoming;
mod watcher;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tasker_core::edit::{edit_line, EditResult, LineEdit};
use tasker_core::index::NoteRef;
use tasker_core::jira::{group_issues, JiraIssue, StatusGroup};
use tasker_core::{load_index_with_keys, Config, Dashboard, Index, Scope};
use tauri::{Manager, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Overview {
    vault_name: String,
    source: &'static str,
    /// false = weder Arbeitsbereiche noch Arbeitspfade eingetragen, alles zählt als privat
    work_configured: bool,
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
    let index = index_cache::get(&cfg, today)?;
    let scope = match scope.as_deref() {
        Some("work") => Some(Scope::Work),
        Some("private") => Some(Scope::Private),
        _ => None,
    };
    let dashboard = index.dashboard(&cfg, scope, today);
    let work_configured = !cfg.areas.work.is_empty() || !cfg.areas.work_paths.is_empty();
    Ok(Overview { vault_name: cfg.vault.name.clone(), source, work_configured, index: (*index).clone(), dashboard })
}

/// Eine Aufgabenzeile, wie die Oberfläche sie aus dem Index kennt.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskTarget {
    path: String,
    line: usize,
    raw: String,
}

/// Konfiguration für Schreibzugriffe. Der Demo-Vault im Repo ist schreibgeschützt,
/// damit die Tests stabil bleiben.
/// Konfiguration für einen Schreibvorgang. Verwirft den Index-Zwischenspeicher, denn gleich ändert sich der Vault.
fn writable_config(app: &tauri::AppHandle, demo: bool) -> Result<Config, String> {
    index_cache::invalidate();
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

/// Priorität setzen: `high` (⏫), `medium` (🔼) oder `none`.
#[tauri::command]
fn set_priority(app: tauri::AppHandle, target: TaskTarget, priority: String, demo: Option<bool>) -> Result<EditResult, String> {
    use tasker_core::task::Priority;
    let p = match priority.as_str() {
        "highest" => Priority::Highest,
        "high" => Priority::High,
        "medium" => Priority::Medium,
        "low" => Priority::Low,
        "lowest" => Priority::Lowest,
        "none" => Priority::None,
        other => return Err(format!("Unbekannte Priorität: {other}")),
    };
    let cfg = writable_config(&app, demo.unwrap_or(false))?;
    edit_line(&cfg.vault.path, &target.path, target.line, &target.raw, &LineEdit::SetPriority(p), today())
        .map_err(|e| e.to_string())
}

/// Status setzen: `open` (`[ ]`) oder `inProgress` (`[/]`).
#[tauri::command]
fn set_status(app: tauri::AppHandle, target: TaskTarget, status: String, demo: Option<bool>) -> Result<EditResult, String> {
    let st = match status.as_str() {
        "open" => ' ',
        "inProgress" => '/',
        other => return Err(format!("Unbekannter Status: {other}")),
    };
    let cfg = writable_config(&app, demo.unwrap_or(false))?;
    edit_line(&cfg.vault.path, &target.path, target.line, &target.raw, &LineEdit::SetStatus(st), today())
        .map_err(|e| e.to_string())
}

/// Öffnet die Notiz in Obsidian, mit Zeilensprung, wenn Advanced URI aktiv ist. Liest `.obsidian/` nur.
#[tauri::command]
fn open_in_obsidian(
    app: tauri::AppHandle,
    path: String,
    line: usize,
    demo: Option<bool>,
) -> Result<tasker_core::obsidian::ObsidianLink, String> {
    use tauri_plugin_opener::OpenerExt;
    let (cfg, _) = resolve_config(&app, demo.unwrap_or(false))?;
    let link = tasker_core::obsidian::link(&cfg, &path, line);
    app.opener()
        .open_url(&link.url, None::<&str>)
        .map_err(|_| "Obsidian ließ sich nicht öffnen. Ist es installiert?".to_string())?;
    Ok(link)
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

/// Setzt einen einzelnen Wert in `config.json` und lässt alles andere unverändert (Reihenfolge bleibt).
fn set_config_value(app: &tauri::AppHandle, path: &[&str], value: serde_json::Value) -> Result<(), String> {
    let file = config_path(app).filter(|p| p.is_file()).ok_or("Keine Konfigurationsdatei gefunden.")?;
    let text = std::fs::read_to_string(&file).map_err(|_| "Konfiguration nicht lesbar.".to_string())?;
    let mut v: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| "Konfiguration ist kein gültiges JSON.".to_string())?;
    let (last, parents) = path.split_last().ok_or("Leerer Schlüssel.")?;
    let mut cur = &mut v;
    for k in parents {
        let obj = cur.as_object_mut().ok_or("Konfiguration hat eine unerwartete Struktur.")?;
        cur = obj.entry(*k).or_insert_with(|| serde_json::json!({}));
    }
    cur.as_object_mut().ok_or("Konfiguration hat eine unerwartete Struktur.")?.insert((*last).into(), value);
    let out = serde_json::to_string_pretty(&v).map_err(|_| "Konfiguration nicht schreibbar.".to_string())? + "
";
    tasker_core::edit::write_atomic(&file, &out).map_err(|_| "Konfiguration nicht schreibbar.".into())
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
    set_config_value(&app, &["jira", "email"], email.into())?;
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

/// Gibt es den Demo-Vault aus dem Repo? Nur in Debug-Builds.
#[tauri::command]
fn demo_available() -> bool {
    demo_config().is_some()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // zuerst: ein zweiter Start (z. B. über das Startmenü, während die App im Tray läuft)
        // holt nur das vorhandene Fenster nach vorn
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(hotkey::plugin())
        .manage(hotkey::HotkeyState::default())
        .setup(|app| {
            hotkey::apply(app.handle());
            // Ohne Tray läuft die App trotzdem, nur ohne Timer im Infobereich
            let tray = tray::setup(app.handle());
            if let Err(e) = &tray {
                eprintln!("Tray nicht verfügbar: {e}");
            }
            // Das Hauptfenster startet unsichtbar (tauri.conf.json); versteckt bleibt es nur
            // beim Autostart und nur, wenn es einen Tray gibt, über den man es zurückholt
            if !(tray.is_ok() && autostart::start_minimized(std::env::args())) {
                tray::show_main(app.handle());
            }
            Ok(())
        })
        .manage(watcher::WatchState::default())
        .manage(today::BlockLock::default())
        .manage(focus::FocusLock::default())
        .invoke_handler(tauri::generate_handler![
            get_overview,
            set_priority,
            open_in_obsidian,
            hotkey::hotkey_status,
            hotkey::hotkey_apply,
            hotkey::quick_window,
            filters::filter_run,
            filters::filters_list,
            filters::filters_save,
            upcoming::get_upcoming,
            upcoming::schedule_on,
            complete_task,
            schedule_task,
            set_status,
            undo_edit,
            watch_vault,
            config_location,
            demo_available,
            jira_status,
            jira_save_credentials,
            jira_delete_token,
            jira_test,
            jira_issues,
            jira_refresh,
            today::get_today,
            today::save_block,
            today::remove_block,
            calendar::calendar_status,
            calendar::calendar_save_client,
            calendar::calendar_connect,
            calendar::calendar_disconnect,
            calendar::calendar_list,
            calendar::calendar_select,
            calendar::calendar_refresh,
            focus::focus_settings,
            tray::focus_tray,
            focus::focus_finish,
            focus::focus_undo_log,
            focus::focus_day,
            focus::focus_choose_ticket,
            focus::focus_set_booked,
            settings::get_settings,
            settings::save_settings,
            quick::quick_preview,
            quick::quick_add,
            quick::quick_undo,
            autostart::autostart_status,
            autostart::autostart_set
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if tray::hide_on_close(window) {
                    api.prevent_close();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("Tasker.md konnte nicht gestartet werden")
        .run(|app, event| {
            // macOS: Klick aufs Dock-Symbol holt das versteckte Fenster zurück
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                tray::show_main(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
