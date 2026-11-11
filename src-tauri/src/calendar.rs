//! Befehle für Google Kalender und abonnierte ICS-Links (SPEC 7). Nur lesen; Termine werden im App-Cache gehalten.

use crate::google::{self, CalendarCache, CalendarInfo, DayEvent, OAuthClient};
use crate::ics;
use crate::{resolve_config, set_config_value, today};
use serde::Serialize;
use std::path::PathBuf;
use tasker_core::config::IcsFeed;
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
    /// mit Google verbunden
    connected: bool,
    calendar_ids: Vec<String>,
    /// abonnierte ICS-Links (nur Name und Id)
    ics: Vec<IcsFeed>,
    /// irgendeine Quelle eingerichtet (Google oder ICS)
    active: bool,
    refresh_minutes: u32,
    fetched_at: Option<String>,
}

#[tauri::command]
pub fn calendar_status(app: tauri::AppHandle) -> Result<CalendarStatus, String> {
    let (cfg, _) = resolve_config(&app, false)?;
    let connected = google::has_refresh_token();
    Ok(CalendarStatus {
        client_set: google::load_client().is_some(),
        connected,
        calendar_ids: cfg.calendar.google.calendar_ids.clone(),
        active: connected || !cfg.calendar.ics.is_empty(),
        ics: cfg.calendar.ics.clone(),
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
    drop_source(&app, |s| s == "google" || s.is_empty())
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
    drop_source(&app, |s| s == "google" || s.is_empty())
}

/// Entfernt die Termine bestimmter Quellen aus dem Cache.
fn drop_source(app: &tauri::AppHandle, f: impl Fn(&str) -> bool) -> Result<(), String> {
    let path = cache_path(app)?;
    let mut cache = google::read_cache(&path);
    cache.events.retain(|e| !f(&e.source));
    google::write_cache(&path, &cache)
}

/// Prüft den ICS-Link (lädt ihn einmal), legt ihn im Schlüsselbund ab und trägt den Namen in die Konfiguration ein.
#[tauri::command]
pub async fn calendar_add_ics(app: tauri::AppHandle, name: String, url: String) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Bitte einen Namen eintragen, z. B. „Firma“.".into());
    }
    let url = ics::normalize_url(&url)?;
    let from = today();
    let text = ics::fetch(&url).await?;
    // Plausibilität: die Datei muss sich lesen lassen (leerer Kalender ist erlaubt)
    let _ = ics::parse_events(&text, from, from);
    let (cfg, _) = resolve_config(&app, false)?;
    let mut feeds = cfg.calendar.ics.clone();
    if feeds.iter().any(|f| f.name.eq_ignore_ascii_case(name)) {
        return Err(format!("Es gibt schon einen Kalender „{name}“."));
    }
    let mut buf = [0u8; 4];
    getrandom::fill(&mut buf).map_err(|_| "Zufallszahlen nicht verfügbar.".to_string())?;
    let id: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    ics::store_url(&id, &url)?;
    feeds.push(IcsFeed { id: id.clone(), name: name.to_string() });
    if let Err(e) = set_config_value(&app, &["calendar", "ics"], serde_json::json!(feeds)) {
        let _ = ics::delete_url(&id);
        return Err(e);
    }
    Ok(())
}

/// Entfernt ein ICS-Abo samt Link im Schlüsselbund und seinen Terminen im Cache.
#[tauri::command]
pub fn calendar_remove_ics(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let (cfg, _) = resolve_config(&app, false)?;
    let feeds: Vec<IcsFeed> = cfg.calendar.ics.iter().filter(|f| f.id != id).cloned().collect();
    set_config_value(&app, &["calendar", "ics"], serde_json::json!(feeds))?;
    ics::delete_url(&id)?;
    let src = format!("ics:{id}");
    drop_source(&app, |s| s == src)
}

/// Lädt die Termine von heute bis `calendar.daysAhead` neu. Gibt die Zahl der Termine zurück.
#[tauri::command]
pub async fn calendar_refresh(app: tauri::AppHandle) -> Result<usize, String> {
    let (cfg, _) = resolve_config(&app, false)?;
    let from = today();
    let to = from + chrono::Duration::days(i64::from(cfg.calendar.days_ahead));
    let path = cache_path(&app)?;
    let old = google::read_cache(&path);
    let mut events: Vec<DayEvent> = vec![];
    let mut errors: Vec<String> = vec![];
    // Bei einem Fehler bleiben die alten Termine dieser Quelle stehen.
    let keep_old = |events: &mut Vec<DayEvent>, src: &str| {
        events.extend(old.events.iter().filter(|e| e.source == src || (src == "google" && e.source.is_empty())).cloned());
    };
    if google::has_refresh_token() {
        match google::fetch_events(&cfg.calendar.google.calendar_ids, from, to).await {
            Ok(v) => events.extend(v.into_iter().map(|e| DayEvent { source: "google".into(), ..e })),
            Err(e) => {
                errors.push(e);
                keep_old(&mut events, "google");
            }
        }
    }
    for feed in &cfg.calendar.ics {
        let src = format!("ics:{}", feed.id);
        let result = match ics::load_url(&feed.id) {
            None => Err("Link fehlt im Schlüsselbund, bitte neu eintragen.".to_string()),
            Some(url) => ics::fetch(&url).await.map(|text| ics::parse_events(&text, from, to)),
        };
        match result {
            Ok(v) => events.extend(v.into_iter().map(|e| DayEvent { source: src.clone(), ..e })),
            Err(e) => {
                errors.push(format!("Kalender „{}“: {e}", feed.name));
                keep_old(&mut events, &src);
            }
        }
    }
    events.sort_by(|a, b| (a.date, &a.event.start).cmp(&(b.date, &b.event.start)));
    let n = events.len();
    let cache = CalendarCache { fetched_at: Some(crate::now_rfc3339()), events };
    google::write_cache(&path, &cache)?;
    if errors.is_empty() {
        Ok(n)
    } else {
        Err(errors.join(" · "))
    }
}
