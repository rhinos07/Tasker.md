//! Erinnerungen über ⏰ und Tagesbriefing (SPEC 6.7): reine Funktionen.
//! Planer und Benachrichtigungen liegen in der App (`src-tauri/src/reminders.rs`).

use crate::index::{Index, TaskRef};
use crate::task::Reminder;
use crate::today::{parse_hm, CalendarEvent, TodayView};
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use serde::Serialize;

/// Zeitpunkt einer Erinnerung. Ohne eigenes Datum: Tag aus ⏳ (auch aus der Tagesnotiz), sonst 📅, sonst 🛫.
/// Ohne jedes Datum gibt es keine Erinnerung.
pub fn reminder_at(t: &TaskRef, r: &Reminder) -> Option<NaiveDateTime> {
    let day = r.date.or_else(|| t.scheduled()).or(t.task.due).or(t.task.start)?;
    Some(day.and_time(parse_hm(&r.time)?))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DueReminder {
    /// eindeutig je Aufgabe und Zeitpunkt, zum Merken bereits gezeigter Erinnerungen
    pub key: String,
    pub at: NaiveDateTime,
    pub task: TaskRef,
}

/// Erinnerungen offener Aufgaben mit `from < at <= to`, nach Zeit sortiert.
pub fn due_reminders(index: &Index, from: NaiveDateTime, to: NaiveDateTime) -> Vec<DueReminder> {
    let mut v: Vec<DueReminder> = index
        .tasks
        .iter()
        .filter(|t| !t.task.done)
        .flat_map(|t| t.task.reminders.iter().filter_map(move |r| Some((t, reminder_at(t, r)?))))
        .filter(|(_, at)| *at > from && *at <= to)
        .map(|(t, at)| DueReminder {
            key: format!("{}|{}|{}", t.note_path, t.task.description, at.format("%Y-%m-%d %H:%M")),
            at,
            task: t.clone(),
        })
        .collect();
    v.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.key.cmp(&b.key)));
    v.dedup_by(|a, b| a.key == b.key);
    v
}

/// Beginn des Prüffensters: seit der letzten Prüfung, aber höchstens `late_minutes` zurück.
/// Liegt die letzte Prüfung in der Zukunft (Uhr verstellt), nur die letzte Minute.
pub fn window_start(last_check: Option<NaiveDateTime>, now: NaiveDateTime, late_minutes: u32) -> NaiveDateTime {
    let earliest = now - Duration::minutes(late_minutes as i64);
    match last_check {
        Some(l) if l > now => now - Duration::minutes(1),
        Some(l) => l.max(earliest),
        None => earliest,
    }
}

/// Briefing-Zeitpunkte mit `from < t <= to` (auch über Mitternacht).
pub fn briefing_times(times: &[String], from: NaiveDateTime, to: NaiveDateTime) -> Vec<NaiveDateTime> {
    let mut out = vec![];
    let mut day: NaiveDate = from.date();
    while day <= to.date() {
        for t in times.iter().filter_map(|t| parse_hm(t)) {
            let at = day.and_time(t);
            if at > from && at <= to {
                out.push(at);
            }
        }
        day += Duration::days(1);
    }
    out.sort();
    out.dedup();
    out
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Text der Tageszusammenfassung, z. B. „3 geplant, 1 überfällig · nächster Termin 09:00 Daily“.
pub fn briefing_text(view: &TodayView, events: &[CalendarEvent], now: NaiveTime) -> String {
    let planned = view.scheduled.len() + view.blocks.len();
    let overdue = view.due.iter().filter(|t| t.task.due.is_some_and(|d| Some(d) < view.today)).count();
    let due_today = view.due.len() - overdue;
    let mut parts = vec![];
    if planned > 0 {
        parts.push(format!("{planned} geplant"));
    }
    if due_today > 0 {
        parts.push(format!("{due_today} heute fällig"));
    }
    if overdue > 0 {
        parts.push(format!("{overdue} überfällig"));
    }
    if !view.in_progress.is_empty() {
        parts.push(format!("{} in Arbeit", view.in_progress.len()));
    }
    let mut text = if parts.is_empty() { "Heute ist nichts geplant.".to_string() } else { parts.join(", ") };
    let mut timed: Vec<(NaiveTime, &str)> = events
        .iter()
        .filter(|e| !e.all_day)
        .filter_map(|e| Some((parse_hm(e.start.as_deref()?)?, e.title.as_str())))
        .filter(|(t, _)| *t >= now)
        .collect();
    timed.sort();
    if let Some((t, title)) = timed.first() {
        let more = timed.len() - 1;
        text.push_str(&format!(" · nächster Termin {} {title}", t.format("%H:%M")));
        if more > 0 {
            text.push_str(&format!(" (+{})", plural(more, "weiterer", "weitere")));
        }
    }
    let first_block = view.blocks.iter().map(|b| b.block.start.as_str()).filter(|s| parse_hm(s).is_some_and(|t| t >= now)).min();
    if let Some(s) = first_block {
        text.push_str(&format!(" · erster Zeitblock {s}"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::jira::KeyFilter;
    use crate::today::today_view;
    use crate::vault::parse_note;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }
    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn index(text: &str) -> Index {
        let cfg = Config::for_vault(".");
        let n = parse_note("10 Projekte/P/P.md", text, &cfg, &KeyFilter::default());
        Index::build(&[n], &cfg, d("2026-11-11"))
    }

    const TEXT: &str = "---\ntyp: projekt\n---\n## Nächste Schritte\n\
- [ ] Arzt anrufen ⏰ 09:00 ⏳ 2026-11-11\n\
- [ ] Bericht ⏰ 2026-11-11 14:30 📅 2026-11-20\n\
- [ ] Ohne Datum ⏰ 10:00\n\
- [x] Erledigt ⏰ 09:00 ⏳ 2026-11-11 ✅ 2026-11-10\n\
- [ ] Morgen ⏰ 08:00 📅 2026-11-12\n";

    #[test]
    fn reminder_times_and_window() {
        let idx = index(TEXT);
        let all = due_reminders(&idx, dt("2026-11-11 00:00"), dt("2026-11-12 23:59"));
        let got: Vec<(String, &str)> = all.iter().map(|r| (r.at.format("%d. %H:%M").to_string(), r.task.task.description.as_str())).collect();
        assert_eq!(
            got,
            vec![
                ("11. 09:00".to_string(), "Arzt anrufen"),
                ("11. 14:30".to_string(), "Bericht"),
                ("12. 08:00".to_string(), "Morgen"),
            ],
            "ohne Datum keine Erinnerung, erledigte nie"
        );
        // Fenster: from ausgeschlossen, to eingeschlossen
        assert!(due_reminders(&idx, dt("2026-11-11 09:00"), dt("2026-11-11 09:30")).is_empty());
        assert_eq!(due_reminders(&idx, dt("2026-11-11 08:59"), dt("2026-11-11 09:00")).len(), 1);
    }

    #[test]
    fn window_start_limits_catch_up() {
        let now = dt("2026-11-11 12:00");
        assert_eq!(window_start(None, now, 60), dt("2026-11-11 11:00"));
        assert_eq!(window_start(Some(dt("2026-11-11 11:59")), now, 60), dt("2026-11-11 11:59"));
        assert_eq!(window_start(Some(dt("2026-11-10 08:00")), now, 60), dt("2026-11-11 11:00"), "nicht endlos nachholen");
        assert_eq!(window_start(Some(dt("2026-11-11 13:00")), now, 60), dt("2026-11-11 11:59"), "Uhr zurückgestellt");
    }

    #[test]
    fn briefing_times_over_midnight() {
        let t = vec!["08:00".to_string(), "17:30".to_string(), "kaputt".to_string()];
        assert_eq!(briefing_times(&t, dt("2026-11-11 07:59"), dt("2026-11-11 08:00")), vec![dt("2026-11-11 08:00")]);
        assert!(briefing_times(&t, dt("2026-11-11 08:00"), dt("2026-11-11 08:30")).is_empty());
        assert_eq!(briefing_times(&t, dt("2026-11-10 17:00"), dt("2026-11-11 09:00")).len(), 2);
    }

    #[test]
    fn briefing_summary() {
        let idx = index(
            "- [ ] A ⏳ 2026-11-11
- [ ] B ⏳ 2026-11-10
- [ ] C 📅 2026-11-11
- [ ] D 📅 2026-11-09
",
        );
        let view = today_view(&idx, &Config::for_vault("."), None, d("2026-11-11"), &[]);
        let ev = |title: &str, start: &str| CalendarEvent { title: title.into(), start: Some(start.into()), end: None, all_day: false };
        let events = vec![ev("Daily", "09:00"), ev("Früh", "07:00"), ev("Abstimmung", "11:00")];
        let now = NaiveTime::from_hms_opt(8, 0, 0).unwrap();
        assert_eq!(
            briefing_text(&view, &events, now),
            "2 geplant, 1 heute fällig, 1 überfällig · nächster Termin 09:00 Daily (+1 weiterer)"
        );
        assert_eq!(briefing_text(&TodayView::default(), &[], now), "Heute ist nichts geplant.");
    }
}
