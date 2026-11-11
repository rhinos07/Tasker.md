//! Befehle für die Heute-Ansicht und die Zeitblöcke (SPEC 5.1).
//! Zeitblöcke liegen in `timeblocks.json` im App-Datenordner, nie im Vault.

use crate::{resolve_config, today};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tasker_core::config::TimelineConfig;
use tasker_core::today::{parse_hm, prune_blocks, today_view, CalendarEvent, TimeBlock, TodayView};
use tasker_core::{load_index, Index, Scope};
use tauri::Manager;

/// Schützt Lesen-Ändern-Schreiben der Block-Datei vor parallelen Befehlen.
#[derive(Default)]
pub struct BlockLock(Mutex<()>);

const KEEP_DAYS: i64 = 7;

fn blocks_path(app: &tauri::AppHandle, demo: bool) -> Result<PathBuf, String> {
    let name = if demo { "timeblocks-demo.json" } else { "timeblocks.json" };
    app.path().app_data_dir().map(|d| d.join(name)).map_err(|_| "App-Datenordner unbekannt.".into())
}

fn read_blocks(path: &Path) -> Vec<TimeBlock> {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn write_blocks(path: &Path, blocks: &[TimeBlock]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|_| "App-Datenordner nicht anlegbar.".to_string())?;
    }
    let text = serde_json::to_string_pretty(blocks).map_err(|_| "Zeitblöcke nicht schreibbar.".to_string())?;
    tasker_core::edit::write_atomic(path, &text).map_err(|_| "Zeitblöcke nicht schreibbar.".into())
}

fn parse_scope(scope: Option<&str>) -> Option<Scope> {
    match scope {
        Some("work") => Some(Scope::Work),
        Some("private") => Some(Scope::Private),
        _ => None,
    }
}

/// Erfundene Termine für den Demo-Vault.
fn demo_events() -> Vec<CalendarEvent> {
    let ev = |title: &str, start: &str, end: &str| CalendarEvent {
        title: title.into(),
        start: Some(start.into()),
        end: Some(end.into()),
        all_day: false,
    };
    vec![
        CalendarEvent { title: "Müllabfuhr".into(), start: None, end: None, all_day: true },
        ev("Daily", "09:00", "09:15"),
        ev("Release-Abstimmung", "11:00", "12:00"),
        ev("Mittagessen", "12:30", "13:15"),
        ev("Zahnarzt", "16:30", "17:15"),
    ]
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayData {
    source: &'static str,
    timeline: TimelineConfig,
    /// Termine des Tages; `calendarConnected = false` heißt: noch keine Kalenderanbindung
    events: Vec<CalendarEvent>,
    calendar_connected: bool,
    #[serde(flatten)]
    view: TodayView,
}

fn load(app: &tauri::AppHandle, demo: bool) -> Result<(tasker_core::Config, &'static str, Index), String> {
    let (cfg, source) = resolve_config(app, demo)?;
    let index = load_index(&cfg, today()).map_err(|e| e.to_string())?;
    Ok((cfg, source, index))
}

#[tauri::command]
pub fn get_today(app: tauri::AppHandle, scope: Option<String>, demo: Option<bool>) -> Result<TodayData, String> {
    let (cfg, source, index) = load(&app, demo.unwrap_or(false))?;
    let blocks = read_blocks(&blocks_path(&app, source == "demo")?);
    let view = today_view(&index, &cfg, parse_scope(scope.as_deref()), today(), &blocks);
    let (events, calendar_connected) = if source == "demo" { (demo_events(), true) } else { (vec![], false) };
    Ok(TodayData { source, timeline: cfg.timeline.clone(), events, calendar_connected, view })
}

/// Legt einen Zeitblock an oder ändert ihn (gleiche `id`). Leere `id` = neu.
#[tauri::command]
pub fn save_block(
    app: tauri::AppHandle,
    lock: tauri::State<'_, BlockLock>,
    mut block: TimeBlock,
    demo: Option<bool>,
) -> Result<TimeBlock, String> {
    if parse_hm(&block.start).is_none() || !(5..=12 * 60).contains(&block.minutes) {
        return Err("Ungültige Uhrzeit oder Dauer.".into());
    }
    let (_, source, index) = load(&app, demo.unwrap_or(false))?;
    let _g = lock.0.lock().map_err(|_| "Zeitblöcke gesperrt.".to_string())?;
    let path = blocks_path(&app, source == "demo")?;
    let mut blocks = prune_blocks(&read_blocks(&path), &index, today(), KEEP_DAYS);
    if block.id.is_empty() {
        block.id = format!("b{}", chrono::Local::now().timestamp_nanos_opt().unwrap_or_default());
    }
    // pro Aufgabe und Tag höchstens ein Block
    blocks.retain(|b| b.id != block.id && !(b.date == block.date && b.path == block.path && b.description == block.description));
    blocks.push(block.clone());
    write_blocks(&path, &blocks)?;
    Ok(block)
}

#[tauri::command]
pub fn remove_block(app: tauri::AppHandle, lock: tauri::State<'_, BlockLock>, id: String, demo: Option<bool>) -> Result<(), String> {
    let (_, source) = resolve_config(&app, demo.unwrap_or(false))?;
    let _g = lock.0.lock().map_err(|_| "Zeitblöcke gesperrt.".to_string())?;
    let path = blocks_path(&app, source == "demo")?;
    let mut blocks = read_blocks(&path);
    blocks.retain(|b| b.id != id);
    write_blocks(&path, &blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_file_roundtrip() {
        let p = std::env::temp_dir().join(format!("tasker-blocks-{}.json", std::process::id()));
        let b = TimeBlock {
            id: "b1".into(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 11, 11).unwrap(),
            start: "09:30".into(),
            minutes: 30,
            path: "10 Projekte/P/P.md".into(),
            description: "Tun".into(),
        };
        write_blocks(&p, std::slice::from_ref(&b)).unwrap();
        assert_eq!(read_blocks(&p), vec![b]);
        std::fs::remove_file(&p).unwrap();
        assert!(read_blocks(&p).is_empty());
    }
}
