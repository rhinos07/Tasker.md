//! Befehle für Google Kalender (SPEC 7). Nur lesen; Termine werden im App-Cache gehalten.

use crate::google::{self, CalendarCache, CalendarInfo, OAuthClient};
use crate::{resolve_config, set_config_value, today};
use serde::Serialize;
use std::path::PathBuf;
use tasker_core::today::CalendarEvent;
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

pub fn cache_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path().app_cache_dir().map(|d| d.join("calendar-cache.json")).map_err(|_| "Cache-Ordner unbekannt.".into())
}

/// Termine eines Tages aus dem Cache.
pub fn events_on(app: &tauri::AppHandle, date: chrono::NaiveDate) -> Vec<CalendarEvent> {
    let Ok(p) = cache_path(app) else { return vec![] };
    google::read_cache(&p).events.into_iter().filter(|e| e.date == date).map(|e| e.event).collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarStatus {
    client_set: bool,
    connected: bool,
    calendar_ids: Vec<String>,
    refresh_minutes: u32,
    fetched_at: Option<String>,
}

#[tauri::command]
pub fn calendar_status(app: tauri::AppHandle) -> Result<CalendarStatus, String> {
    let (cfg, _) = resolve_config(&app, false)?;
    Ok(CalendarStatus {
        client_set: google::load_client().is_some(),
        connected: google::has_refresh_token(),
        calendar_ids: cfg.calendar.google.calendar_ids.clone(),
        refresh_minutes: cfg.calendar.refresh_minutes,
        fetched_at: google::read_cache(&cache_path(&app)?).fetched_at,
    })
}

/// Speichert Client-ID und -Secret des eigenen Google-Cloud-Projekts im Schlüsselbund.
#[tauri::command]
pub fn calendar_save_client(client_id: String, client_secret: String) -> Result<(), String> {
    let (id, secret) = (client_id.trim().to_string(), client_secret.trim().to_string());
    if !id.ends_with(".apps.googleusercontent.com") {
        return Err("Die Client-ID endet normalerweise auf .apps.googleusercontent.com.".into());
    }
    if secret.is_empty() {
        return Err("Bitte das Client-Secret eintragen.".into());
    }
    google::store_client(&OAuthClient { id, secret })
}

/// Startet die Anmeldung im Browser und wartet auf die Rückmeldung (höchstens 3 Minuten).
#[tauri::command]
pub async fn calendar_connect(app: tauri::AppHandle) -> Result<(), String> {
    let opener = app.clone();
    google::login(move |url| {
        opener.opener().open_url(url, None::<&str>).map_err(|_| "Browser konnte nicht geöffnet werden.".to_string())
    })
    .await
}

/// Trennt die Verbindung. `forget_client = true` entfernt auch Client-ID und -Secret.
#[tauri::command]
pub async fn calendar_disconnect(app: tauri::AppHandle, forget_client: Option<bool>) -> Result<(), String> {
    if forget_client.unwrap_or(false) {
        google::forget_all().await?;
    } else {
        google::logout().await?;
    }
    let _ = std::fs::remove_file(cache_path(&app)?);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarChoice {
    #[serde(flatten)]
    info: CalendarInfo,
    selected: bool,
}

#[tauri::command]
pub async fn calendar_list(app: tauri::AppHandle) -> Result<Vec<CalendarChoice>, String> {
    let (cfg, _) = resolve_config(&app, false)?;
    let ids = &cfg.calendar.google.calendar_ids;
    Ok(google::list_calendars()
        .await?
        .into_iter()
        .map(|info| {
            let selected = ids.contains(&info.id) || (info.primary && ids.iter().any(|i| i == "primary"));
            CalendarChoice { info, selected }
        })
        .collect())
}

/// Speichert die Auswahl der angezeigten Kalender in der Konfiguration.
#[tauri::command]
pub fn calendar_select(app: tauri::AppHandle, ids: Vec<String>) -> Result<(), String> {
    set_config_value(&app, &["calendar", "google", "calendarIds"], serde_json::json!(ids))?;
    let _ = std::fs::remove_file(cache_path(&app)?);
    Ok(())
}

/// Lädt die Termine von heute bis `calendar.daysAhead` neu. Gibt die Zahl der Termine zurück.
#[tauri::command]
pub async fn calendar_refresh(app: tauri::AppHandle) -> Result<usize, String> {
    let (cfg, _) = resolve_config(&app, false)?;
    let from = today();
    let to = from + chrono::Duration::days(i64::from(cfg.calendar.days_ahead));
    let events = google::fetch_events(&cfg.calendar.google.calendar_ids, from, to).await?;
    let n = events.len();
    let cache = CalendarCache { fetched_at: Some(crate::now_rfc3339()), events };
    google::write_cache(&cache_path(&app)?, &cache)?;
    Ok(n)
}
