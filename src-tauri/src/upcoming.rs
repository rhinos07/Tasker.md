//! Ansicht „Demnächst“ (SPEC 5.5) und Umplanen per Drag & Drop.

use crate::{index_cache, resolve_config, today, writable_config, TaskTarget};
use serde::Serialize;
use tasker_core::edit::{edit_line, EditResult, LineEdit};
use tasker_core::index::TaskRef;
use tasker_core::today::CalendarEvent;
use tasker_core::upcoming::{upcoming_view, UpcomingTask};

/// Heute plus 13 Tage
const DAYS: usize = 14;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    date: chrono::NaiveDate,
    tasks: Vec<UpcomingTask>,
    events: Vec<CalendarEvent>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpcomingData {
    source: &'static str,
    today: chrono::NaiveDate,
    days: Vec<Day>,
    unscheduled: Vec<TaskRef>,
    later: usize,
    calendar_connected: bool,
}

#[tauri::command]
pub fn get_upcoming(
    app: tauri::AppHandle,
    scope: Option<String>,
    demo: Option<bool>,
) -> Result<UpcomingData, String> {
    let (cfg, source) = resolve_config(&app, demo.unwrap_or(false))?;
    let day = today();
    let index = index_cache::get(&cfg, day)?;
    let v = upcoming_view(
        &index,
        &cfg,
        crate::today::parse_scope(scope.as_deref()),
        day,
        DAYS,
    );
    let is_demo = source == "demo";
    let days = v
        .days
        .into_iter()
        .map(|d| {
            let events = match (is_demo, d.date == day) {
                (true, true) => crate::today::demo_events(),
                (true, false) => vec![],
                (false, _) => crate::calendar::events_on(&app, d.date),
            };
            Day {
                date: d.date,
                tasks: d.tasks,
                events,
            }
        })
        .collect();
    Ok(UpcomingData {
        source,
        today: day,
        days,
        unscheduled: v.unscheduled,
        later: v.later,
        calendar_connected: is_demo || crate::google::has_refresh_token(),
    })
}

/// Setzt ⏳ auf `date` (`YYYY-MM-DD`) oder entfernt es (`None`). 📅 bleibt immer unverändert.
#[tauri::command]
pub fn schedule_on(
    app: tauri::AppHandle,
    target: TaskTarget,
    date: Option<String>,
    demo: Option<bool>,
) -> Result<EditResult, String> {
    let date = date
        .map(|s| {
            chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                .map_err(|_| format!("Ungültiges Datum: {s}"))
        })
        .transpose()?;
    let cfg = writable_config(&app, demo.unwrap_or(false))?;
    edit_line(
        &cfg.vault.path,
        &target.path,
        target.line,
        &target.raw,
        &LineEdit::SetScheduled(date),
        today(),
    )
    .map_err(|e| e.to_string())
}
