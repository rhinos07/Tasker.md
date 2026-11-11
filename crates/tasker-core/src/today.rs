//! Heute-Ansicht und Tages-Zeitleiste (SPEC 5.1).
//!
//! Zeitblöcke liegen nur im App-Datenordner, nie im Vault: Die Tasks-Syntax kennt keine Uhrzeit.
//! Ein Block verweist über Pfad und Beschreibung auf seine Aufgabe, nicht über die Zeilennummer,
//! damit er Einfügungen oberhalb der Aufgabe übersteht.

use crate::config::Config;
use crate::index::{Index, Scope, TaskRef};
use chrono::{NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Ein Termin aus einem Kalender. Uhrzeiten als `HH:MM` in lokaler Zeit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub title: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub all_day: bool,
}

/// Ein geplanter Zeitblock für eine Aufgabe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimeBlock {
    pub id: String,
    pub date: NaiveDate,
    /// `HH:MM`
    pub start: String,
    pub minutes: u32,
    pub path: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockView {
    pub block: TimeBlock,
    pub task: TaskRef,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TodayView {
    pub today: Option<NaiveDate>,
    /// ⏳ bis heute, ohne Zeitblock heute
    pub scheduled: Vec<TaskRef>,
    /// 📅 bis heute, nicht schon unter „geplant“
    pub due: Vec<TaskRef>,
    /// Priorität gesetzt, nicht blockiert, nicht schon oben
    pub important: Vec<TaskRef>,
    /// Zeitblöcke von heute
    pub blocks: Vec<BlockView>,
    /// Blöcke früherer Tage, deren Aufgabe noch offen ist und heute keinen Block hat
    pub leftover: Vec<BlockView>,
}

/// Prüft eine Uhrzeit `HH:MM`.
pub fn parse_hm(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s, "%H:%M").ok()
}

fn on_today(t: &TaskRef, today: NaiveDate) -> bool {
    !t.task.done && t.task.scheduled.is_some_and(|d| d <= today)
}

fn find_task<'a>(index: &'a Index, b: &TimeBlock) -> Option<&'a TaskRef> {
    index.tasks.iter().find(|t| !t.task.done && t.note_path == b.path && t.task.description == b.description)
}

/// Baut die Heute-Ansicht. Blöcke, deren Aufgabe erledigt ist oder nicht mehr auf heute liegt, fallen weg.
pub fn today_view(index: &Index, _cfg: &Config, scope: Option<Scope>, today: NaiveDate, blocks: &[TimeBlock]) -> TodayView {
    let in_scope = |t: &&TaskRef| scope.is_none_or(|s| t.scope == s);

    let mut today_blocks: Vec<BlockView> = blocks
        .iter()
        .filter(|b| b.date == today && parse_hm(&b.start).is_some())
        .filter_map(|b| find_task(index, b).filter(|t| on_today(t, today)).map(|t| BlockView { block: b.clone(), task: t.clone() }))
        .collect();
    today_blocks.sort_by(|a, b| a.block.start.cmp(&b.block.start));
    let blocked_keys: HashSet<(&str, &str)> =
        today_blocks.iter().map(|b| (b.block.path.as_str(), b.block.description.as_str())).collect();

    let mut seen = HashSet::new();
    let mut leftover: Vec<BlockView> = vec![];
    let mut older: Vec<&TimeBlock> = blocks.iter().filter(|b| b.date < today).collect();
    older.sort_by_key(|b| std::cmp::Reverse(b.date)); // jüngster zuerst
    for b in older {
        let key = (b.path.as_str(), b.description.as_str());
        if blocked_keys.contains(&key) || !seen.insert(key) {
            continue;
        }
        if let Some(t) = find_task(index, b) {
            leftover.push(BlockView { block: b.clone(), task: t.clone() });
        }
    }

    let open: Vec<&TaskRef> = index.tasks.iter().filter(|t| !t.task.done).filter(in_scope).collect();
    let mut scheduled: Vec<TaskRef> = open
        .iter()
        .filter(|t| on_today(t, today))
        .filter(|t| !blocked_keys.contains(&(t.note_path.as_str(), t.task.description.as_str())))
        .map(|t| (*t).clone())
        .collect();
    scheduled.sort_by_key(|t| (t.task.priority, t.task.scheduled));
    let mut due: Vec<TaskRef> = open
        .iter()
        .filter(|t| t.task.due.is_some_and(|d| d <= today) && !on_today(t, today))
        .map(|t| (*t).clone())
        .collect();
    due.sort_by_key(|t| t.task.due);
    let mut important: Vec<TaskRef> = open
        .iter()
        .filter(|t| t.task.priority.is_set() && !t.blocked)
        .filter(|t| !on_today(t, today) && !t.task.due.is_some_and(|d| d <= today))
        .map(|t| (*t).clone())
        .collect();
    important.sort_by_key(|t| t.task.priority);

    TodayView { today: Some(today), scheduled, due, important, blocks: today_blocks, leftover }
}

/// Entfernt Blöcke, die nicht mehr gebraucht werden: älter als `keep_days` oder Aufgabe erledigt bzw. verschwunden.
pub fn prune_blocks(blocks: &[TimeBlock], index: &Index, today: NaiveDate, keep_days: i64) -> Vec<TimeBlock> {
    blocks
        .iter()
        .filter(|b| (today - b.date).num_days() <= keep_days && b.date <= today + chrono::Duration::days(1))
        .filter(|b| find_task(index, b).is_some())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::KeyFilter;
    use crate::vault::parse_note;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn index(text: &str) -> Index {
        let cfg = Config::for_vault(".");
        let n = parse_note("10 Projekte/P/P.md", text, &cfg, &KeyFilter::default());
        Index::build(&[n], &cfg, d("2026-11-11"))
    }

    fn block(date: &str, start: &str, desc: &str) -> TimeBlock {
        TimeBlock { id: format!("{date}{start}"), date: d(date), start: start.into(), minutes: 30, path: "10 Projekte/P/P.md".into(), description: desc.into() }
    }

    const TEXT: &str = "- [ ] Heute A ⏳ 2026-11-11\n\
                        - [ ] Gestern B ⏳ 2026-11-10\n\
                        - [ ] Fällig C 📅 2026-11-11\n\
                        - [ ] Wichtig D ⏫\n\
                        - [ ] Später E ⏳ 2026-11-20\n\
                        - [x] Fertig F ⏳ 2026-11-11 ✅ 2026-11-11\n";

    #[test]
    fn lists_without_duplicates() {
        let idx = index(TEXT);
        let cfg = Config::for_vault(".");
        let v = today_view(&idx, &cfg, None, d("2026-11-11"), &[]);
        let names = |l: &[TaskRef]| l.iter().map(|t| t.task.description.clone()).collect::<Vec<_>>();
        assert_eq!(names(&v.scheduled), vec!["Gestern B", "Heute A"]);
        assert_eq!(names(&v.due), vec!["Fällig C"]);
        assert_eq!(names(&v.important), vec!["Wichtig D"]);
    }

    #[test]
    fn blocks_follow_their_tasks() {
        // Zeile oberhalb eingefügt: Block findet die Aufgabe trotzdem
        let idx = index(&format!("# Neu\n{TEXT}"));
        let cfg = Config::for_vault(".");
        let blocks = vec![
            block("2026-11-11", "09:00", "Heute A"),
            block("2026-11-11", "10:00", "Fertig F"),  // erledigt → weg
            block("2026-11-11", "11:00", "Später E"),  // nicht mehr auf heute → weg
            block("2026-11-10", "14:00", "Gestern B"), // liegengeblieben
            block("2026-11-10", "15:00", "Heute A"),   // hat heute einen Block → nicht doppelt
        ];
        let v = today_view(&idx, &cfg, None, d("2026-11-11"), &blocks);
        assert_eq!(v.blocks.len(), 1);
        assert_eq!(v.blocks[0].task.task.description, "Heute A");
        assert!(v.scheduled.iter().all(|t| t.task.description != "Heute A"), "mit Block nicht mehr in der Liste");
        assert_eq!(v.leftover.iter().map(|b| b.block.description.as_str()).collect::<Vec<_>>(), vec!["Gestern B"]);

        let kept = prune_blocks(&blocks, &idx, d("2026-11-11"), 7);
        assert_eq!(kept.len(), 4, "nur der Block der erledigten Aufgabe fällt weg");
        assert!(prune_blocks(&blocks, &idx, d("2026-11-30"), 7).is_empty(), "alte Blöcke fallen weg");
    }

    #[test]
    fn parses_times() {
        assert!(parse_hm("07:30").is_some());
        assert!(parse_hm("7.30").is_none());
    }
}
