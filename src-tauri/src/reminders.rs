//! Planer für Erinnerungen (⏰) und Tagesbriefing (SPEC 6.7).
//!
//! Läuft alle 30 s im Hintergrund, solange die App läuft (auch versteckt im Infobereich).
//! Bereits gezeigte Erinnerungen stehen in `reminders.json` im App-Datenordner, damit nach einem
//! Neustart nichts doppelt kommt. Verpasstes wird höchstens `lateMinutes` später nachgeholt.
//! Nur für den eigenen Vault, nie für den Demo-Vault.

use crate::resolve_config;
use chrono::{Duration, Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tasker_core::config::RemindersConfig;
use tasker_core::reminders::{briefing_times, due_reminders, window_start};
use tasker_core::Index;
use tauri::Manager;

const TICK: std::time::Duration = std::time::Duration::from_secs(30);
/// Gemerkte Erinnerungen so lange aufheben
const KEEP_FIRED_DAYS: i64 = 3;
/// Mehr Einzelmeldungen auf einmal werden zusammengefasst
const MAX_NOTICES: usize = 4;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct State {
    last_check: Option<NaiveDateTime>,
    fired: Vec<Fired>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fired {
    key: String,
    at: NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Notice {
    pub title: String,
    pub body: String,
}

fn state_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map(|d| d.join("reminders.json")).map_err(|_| "App-Datenordner unbekannt.".into())
}

fn read_state(path: &Path) -> State {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn write_state(path: &Path, st: &State) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|_| "App-Datenordner nicht anlegbar.".to_string())?;
    }
    let text = serde_json::to_string_pretty(st).map_err(|_| "Erinnerungen nicht schreibbar.".to_string())?;
    tasker_core::edit::write_atomic(path, &text).map_err(|_| "Erinnerungen nicht schreibbar.".into())
}

/// Entscheidet, was jetzt zu zeigen ist, und merkt es sich in `st`. Reine Funktion bis auf `briefing`.
pub(crate) fn plan(
    st: &mut State,
    now: NaiveDateTime,
    cfg: &RemindersConfig,
    index: &Index,
    briefing: impl FnOnce() -> String,
) -> Vec<Notice> {
    let from = window_start(st.last_check, now, cfg.late_minutes);
    let seen = |k: &str| st.fired.iter().any(|f| f.key == k);
    let mut fired: Vec<Fired> = vec![];
    let mut notices: Vec<Notice> = vec![];

    // mehrere Briefing-Zeiten im Fenster (z. B. nach Ruhezustand): nur eine Meldung
    let mut briefing_due = false;
    for t in briefing_times(&cfg.briefing, from, now) {
        let key = format!("briefing|{}", t.format("%Y-%m-%d %H:%M"));
        briefing_due |= !seen(&key);
        fired.push(Fired { key, at: t });
    }
    if briefing_due {
        notices.push(Notice { title: "Tasker.md · Heute".into(), body: briefing() });
    }

    let due: Vec<_> = due_reminders(index, from, now).into_iter().filter(|r| !seen(&r.key)).collect();
    for (i, r) in due.iter().enumerate() {
        if i == MAX_NOTICES && due.len() > MAX_NOTICES + 1 {
            notices.push(Notice {
                title: format!("⏰ {} weitere Erinnerungen", due.len() - MAX_NOTICES),
                body: due[MAX_NOTICES..].iter().map(|r| r.task.task.description.as_str()).collect::<Vec<_>>().join(" · "),
            });
            break;
        }
        let mut body = r.task.note_name.clone();
        if r.at < now - Duration::minutes(2) {
            let when = if r.at.date() == now.date() { r.at.format("%H:%M").to_string() } else { r.at.format("%d.%m. %H:%M").to_string() };
            body.push_str(&format!(" · war um {when}"));
        }
        notices.push(Notice { title: format!("⏰ {}", r.task.task.description), body });
    }
    fired.extend(due.into_iter().map(|r| Fired { key: r.key, at: r.at }));

    st.fired.retain(|f| f.at >= now - Duration::days(KEEP_FIRED_DAYS));
    for f in fired {
        if !st.fired.iter().any(|x| x.key == f.key) {
            st.fired.push(f);
        }
    }
    st.last_check = Some(now);
    notices
}

fn tick(app: &tauri::AppHandle) -> Result<(), String> {
    let (cfg, source) = resolve_config(app, false)?;
    if source != "config" || !cfg.reminders.enabled {
        return Ok(());
    }
    let now = Local::now().naive_local();
    let today = now.date();
    let index = crate::index_cache::get(&cfg, today)?;
    let path = state_path(app)?;
    let mut st = read_state(&path);
    let notices = plan(&mut st, now, &cfg.reminders, &index, || {
        let blocks = crate::today::current_blocks(app, false);
        let view = tasker_core::today::today_view(&index, &cfg, None, today, &blocks);
        let events = crate::calendar::events_on(app, today);
        tasker_core::reminders::briefing_text(&view, &events, now.time())
    });
    // erst merken, dann zeigen: lieber eine verpasst als doppelt
    write_state(&path, &st)?;
    use tauri_plugin_notification::NotificationExt;
    for n in notices {
        let _ = app.notification().builder().title(&n.title).body(&n.body).show();
    }
    Ok(())
}

/// Startet den Planer im Hintergrund.
pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        // kurz warten, bis Fenster und Index bereit sind
        std::thread::sleep(std::time::Duration::from_secs(5));
        loop {
            if let Err(e) = tick(&app) {
                eprintln!("Erinnerungen: {e}");
            }
            std::thread::sleep(TICK);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasker_core::jira::KeyFilter;
    use tasker_core::vault::parse_note;
    use tasker_core::Config;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn index(text: &str) -> Index {
        let cfg = Config::for_vault(".");
        let n = parse_note("10 Projekte/P/P.md", text, &cfg, &KeyFilter::default());
        Index::build(&[n], &cfg, dt("2026-11-11 00:00").date())
    }

    #[test]
    fn fires_once_and_catches_up_late_ones() {
        let idx = index("- [ ] Arzt anrufen ⏰ 09:00 ⏳ 2026-11-11\n- [ ] Bericht ⏰ 10:30 ⏳ 2026-11-11\n");
        let cfg = RemindersConfig { enabled: true, briefing: vec!["08:00".into()], late_minutes: 60 };
        let mut st = State::default();

        // erster Start 09:20: Briefing 08:00 ist zu alt, Erinnerung 09:00 wird nachgeholt
        let n = plan(&mut st, dt("2026-11-11 09:20"), &cfg, &idx, || "B".into());
        assert_eq!(n, vec![Notice { title: "⏰ Arzt anrufen".into(), body: "P · war um 09:00".into() }]);
        // gleich danach nichts mehr, auch nach „Neustart“ mit demselben Zustand
        assert!(plan(&mut st, dt("2026-11-11 09:20"), &cfg, &idx, || "B".into()).is_empty());
        let text = serde_json::to_string(&st).unwrap();
        let mut again: State = serde_json::from_str(&text).unwrap();
        again.last_check = None;
        assert!(plan(&mut again, dt("2026-11-11 09:30"), &cfg, &idx, || "B".into()).is_empty(), "kein Doppel nach Neustart");
        // pünktlich: ohne „war um“
        let n = plan(&mut st, dt("2026-11-11 10:30"), &cfg, &idx, || "B".into());
        assert_eq!(n, vec![Notice { title: "⏰ Bericht".into(), body: "P".into() }]);
    }

    #[test]
    fn briefing_once_per_time() {
        let idx = index("- [ ] A\n");
        let cfg = RemindersConfig { enabled: true, briefing: vec!["08:00".into()], late_minutes: 60 };
        let mut st = State { last_check: Some(dt("2026-11-11 07:59")), fired: vec![] };
        let n = plan(&mut st, dt("2026-11-11 08:00"), &cfg, &idx, || "2 geplant".into());
        assert_eq!(n, vec![Notice { title: "Tasker.md · Heute".into(), body: "2 geplant".into() }]);
        st.last_check = Some(dt("2026-11-11 07:00"));
        assert!(plan(&mut st, dt("2026-11-11 08:10"), &cfg, &idx, || unreachable!()).is_empty());
    }

    #[test]
    fn many_at_once_are_summarized() {
        let text: String = (1..=7).map(|i| format!("- [ ] T{i} ⏰ 09:00 ⏳ 2026-11-11\n")).collect();
        let idx = index(&text);
        let cfg = RemindersConfig { enabled: true, briefing: vec![], late_minutes: 60 };
        let n = plan(&mut State::default(), dt("2026-11-11 09:00"), &cfg, &idx, || unreachable!());
        assert_eq!(n.len(), MAX_NOTICES + 1);
        assert_eq!(n[MAX_NOTICES].title, "⏰ 3 weitere Erinnerungen");
    }

    #[test]
    fn old_entries_are_pruned() {
        let idx = index("- [ ] A\n");
        let cfg = RemindersConfig::default();
        let mut st = State { last_check: None, fired: vec![Fired { key: "alt".into(), at: dt("2026-11-01 09:00") }] };
        plan(&mut st, dt("2026-11-11 12:00"), &cfg, &idx, String::new);
        assert!(st.fired.is_empty());
    }
}
