//! Fokus-Sitzungen und die Tageszusammenfassung für Jira (SPEC 6.5). Reine Logik, ohne Dateizugriff.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Eine abgeschlossene Fokus-Sitzung. Die Ticket-Bezüge werden beim Beenden festgehalten,
/// damit die Zusammenfassung stabil bleibt, auch wenn die Aufgabe später geändert wird.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub date: NaiveDate,
    /// `HH:MM`
    pub start: String,
    pub minutes: u32,
    pub path: String,
    pub description: String,
    /// 🆔 der Aufgabe, falls vorhanden
    #[serde(default)]
    pub task_id: Option<String>,
    /// Ticket-Schlüssel in der Aufgabenzeile
    #[serde(default)]
    pub line_keys: Vec<String>,
    /// Feld `jira` des Projekts
    #[serde(default)]
    pub project_keys: Vec<String>,
    /// im Projekt erwähnte Tickets
    #[serde(default)]
    pub project_mentions: Vec<String>,
}

impl Session {
    /// Zuordnung zur Aufgabe: Datei plus 🆔, sonst Datei plus Text.
    pub fn task_key(&self) -> String {
        task_key(&self.path, self.task_id.as_deref(), &self.description)
    }
}

pub fn task_key(path: &str, id: Option<&str>, description: &str) -> String {
    match id {
        Some(id) => format!("{path}#🆔{id}"),
        None => format!("{path}#{description}"),
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TicketSource {
    /// Schlüssel steht in der Aufgabenzeile
    Line,
    /// Projekt hat genau ein Ticket im Feld `jira`
    Project,
    /// vom Nutzer lokal zugeordnet
    Chosen,
    /// Projekt erwähnt genau ein Ticket (unsicher)
    Guess,
    /// Projekt hat mehrere Tickets, Auswahl nötig
    Multi,
    /// kein Bezug
    None,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskTime {
    pub task_key: String,
    pub description: String,
    pub path: String,
    pub minutes: u32,
    pub sessions: u32,
}

/// Eine Zeile der Tageszusammenfassung: ein Ticket, oder (ohne Ticket) eine einzelne Aufgabe.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketRow {
    pub key: Option<String>,
    pub source: TicketSource,
    /// Auswahl bei `multi` (Tickets des Projekts)
    pub options: Vec<String>,
    pub raw_minutes: u32,
    pub rounded_minutes: u32,
    pub sessions: u32,
    pub first_start: String,
    pub tasks: Vec<TaskTime>,
    pub booked: bool,
}

fn resolve(s: &Session, choices: &BTreeMap<String, String>) -> (Option<String>, TicketSource, Vec<String>) {
    if let Some(k) = s.line_keys.first() {
        return (Some(k.clone()), TicketSource::Line, vec![]);
    }
    if let Some(k) = choices.get(&s.task_key()) {
        return (Some(k.clone()), TicketSource::Chosen, vec![]);
    }
    match s.project_keys.len() {
        1 => return (Some(s.project_keys[0].clone()), TicketSource::Project, vec![]),
        n if n > 1 => return (None, TicketSource::Multi, s.project_keys.clone()),
        _ => {}
    }
    if s.project_mentions.len() == 1 {
        return (Some(s.project_mentions[0].clone()), TicketSource::Guess, vec![]);
    }
    (None, TicketSource::None, vec![])
}

/// Rundet auf das nächstgelegene Vielfache, mindestens ein Vielfaches. `step = 0` = keine Rundung.
pub fn round_minutes(raw: u32, step: u32) -> u32 {
    if step == 0 || raw == 0 {
        return raw;
    }
    (((raw + step / 2) / step) * step).max(step)
}

pub fn booked_key(date: NaiveDate, row_key: &str) -> String {
    format!("{date}|{row_key}")
}

/// Tageszusammenfassung: Tickets zuerst (sichere vor vermuteten), dann Aufgaben mit Auswahl, dann ohne Ticket.
pub fn day_summary(
    sessions: &[Session],
    date: NaiveDate,
    choices: &BTreeMap<String, String>,
    booked: &BTreeSet<String>,
    round_step: u32,
) -> Vec<TicketRow> {
    let mut rows: Vec<TicketRow> = vec![];
    let mut day: Vec<&Session> = sessions.iter().filter(|s| s.date == date).collect();
    day.sort_by(|a, b| a.start.cmp(&b.start));
    for s in day {
        let (key, source, options) = resolve(s, choices);
        // Tickets werden zusammengefasst, Aufgaben ohne eindeutiges Ticket bleiben einzeln (Auswahl pro Aufgabe)
        let group = key.clone().map_or_else(|| format!("task:{}", s.task_key()), |k| format!("key:{k}"));
        let idx = rows.iter().position(|r| r.key.clone().map_or_else(|| format!("task:{}", r.tasks[0].task_key), |k| format!("key:{k}")) == group);
        let row = match idx {
            Some(i) => &mut rows[i],
            None => {
                rows.push(TicketRow {
                    key: key.clone(),
                    source,
                    options,
                    raw_minutes: 0,
                    rounded_minutes: 0,
                    sessions: 0,
                    first_start: s.start.clone(),
                    tasks: vec![],
                    booked: false,
                });
                rows.last_mut().expect("eben eingefügt")
            }
        };
        // sicherste Quelle gewinnt (z. B. Zeile vor Vermutung)
        if (source as u8) < (row.source as u8) {
            row.source = source;
        }
        row.raw_minutes += s.minutes;
        row.sessions += 1;
        let tk = s.task_key();
        match row.tasks.iter_mut().find(|t| t.task_key == tk) {
            Some(t) => {
                t.minutes += s.minutes;
                t.sessions += 1;
            }
            None => row.tasks.push(TaskTime { task_key: tk, description: s.description.clone(), path: s.path.clone(), minutes: s.minutes, sessions: 1 }),
        }
    }
    for r in &mut rows {
        r.rounded_minutes = round_minutes(r.raw_minutes, round_step);
        let id = r.key.clone().unwrap_or_else(|| r.tasks[0].task_key.clone());
        r.booked = booked.contains(&booked_key(date, &id));
    }
    rows.sort_by_key(|r| (r.key.is_none(), r.source as u8, r.first_start.clone()));
    rows
}

/// Minuten und Zahl der Sitzungen einer Aufgabe an einem Tag, nur Sitzungen ab `min_minutes` (für das Vault-Log).
pub fn task_day_total(sessions: &[Session], date: NaiveDate, task_key: &str, min_minutes: u32) -> (u32, u32) {
    sessions
        .iter()
        .filter(|s| s.date == date && s.task_key() == task_key && s.minutes >= min_minutes)
        .fold((0, 0), |(m, n), s| (m + s.minutes, n + 1))
}

/// `0h 45m`
pub fn hm(minutes: u32) -> String {
    format!("{}h {:02}m", minutes / 60, minutes % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn s(start: &str, min: u32, desc: &str, line: &[&str], proj: &[&str], ment: &[&str]) -> Session {
        Session {
            id: format!("{start}{desc}"),
            date: d("2026-11-11"),
            start: start.into(),
            minutes: min,
            path: "10 Projekte/P/P.md".into(),
            description: desc.into(),
            task_id: None,
            line_keys: line.iter().map(|x| x.to_string()).collect(),
            project_keys: proj.iter().map(|x| x.to_string()).collect(),
            project_mentions: ment.iter().map(|x| x.to_string()).collect(),
        }
    }

    #[test]
    fn rounding() {
        assert_eq!(round_minutes(50, 15), 45);
        assert_eq!(round_minutes(53, 15), 60);
        assert_eq!(round_minutes(3, 15), 15, "mindestens ein Vielfaches");
        assert_eq!(round_minutes(50, 0), 50);
        assert_eq!(hm(45), "0h 45m");
    }

    #[test]
    fn acceptance_two_pomodoros_on_line_key() {
        // SPEC 12, Kriterium 9: 25 + 25 min mit ABC-123 in der Zeile → ABC-123 · 0h 45m
        let ss = vec![s("09:00", 25, "Test ABC-123", &["ABC-123"], &[], &[]), s("10:00", 25, "Test ABC-123", &["ABC-123"], &[], &[])];
        let rows = day_summary(&ss, d("2026-11-11"), &BTreeMap::new(), &BTreeSet::new(), 15);
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].key.as_deref(), rows[0].raw_minutes, rows[0].rounded_minutes), (Some("ABC-123"), 50, 45));
        assert_eq!(rows[0].source, TicketSource::Line);
        assert_eq!(rows[0].sessions, 2);
        assert_eq!(rows[0].tasks.len(), 1);
    }

    #[test]
    fn resolution_order() {
        let ss = vec![
            s("08:00", 30, "Zeile", &["ABC-1"], &["ABC-9"], &[]),
            s("09:00", 30, "Projekt", &[], &["ABC-2"], &["ABC-5"]),
            s("10:00", 30, "Mehrere", &[], &["ABC-3", "ABC-4"], &[]),
            s("11:00", 30, "Vermutet", &[], &[], &["ABC-5"]),
            s("12:00", 30, "Ohne", &[], &[], &["ABC-6", "ABC-7"]),
            s("13:00", 30, "Gewählt", &[], &["ABC-3", "ABC-4"], &[]),
        ];
        let mut choices = BTreeMap::new();
        choices.insert(ss[5].task_key(), "ABC-4".to_string());
        let rows = day_summary(&ss, d("2026-11-11"), &choices, &BTreeSet::new(), 15);
        let v: Vec<(Option<&str>, TicketSource)> = rows.iter().map(|r| (r.key.as_deref(), r.source)).collect();
        assert_eq!(
            v,
            vec![
                (Some("ABC-1"), TicketSource::Line),
                (Some("ABC-2"), TicketSource::Project),
                (Some("ABC-4"), TicketSource::Chosen),
                (Some("ABC-5"), TicketSource::Guess),
                (None, TicketSource::Multi),
                (None, TicketSource::None),
            ]
        );
        assert_eq!(rows[4].options, vec!["ABC-3", "ABC-4"]);
    }

    #[test]
    fn task_totals_ignore_short_sessions() {
        let ss = vec![s("09:00", 25, "A", &[], &[], &[]), s("10:00", 3, "A", &[], &[], &[]), s("11:00", 25, "A", &[], &[], &[])];
        assert_eq!(task_day_total(&ss, d("2026-11-11"), &ss[0].task_key(), 5), (50, 2));
    }
}
