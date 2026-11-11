//! Ansicht „Demnächst“ (SPEC 5.5): heute plus die folgenden Tage als Spalten, dazu „Ohne Datum“.
//! Eine Aufgabe steht am Tag ihres ⏳, ohne ⏳ am Tag ihres 📅. Was davor liegt, steht unter heute
//! und ist als überfällig markiert.

use crate::index::{Index, Scope, TaskRef};
use crate::Config;
use chrono::{Duration, NaiveDate};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpcomingTask {
    #[serde(flatten)]
    pub task: TaskRef,
    /// Datum liegt vor heute
    pub overdue: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpcomingDay {
    pub date: NaiveDate,
    pub tasks: Vec<UpcomingTask>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpcomingView {
    pub days: Vec<UpcomingDay>,
    /// offen, nicht blockiert, nicht wartend, ohne ⏳ und 📅
    pub unscheduled: Vec<TaskRef>,
    /// Aufgaben mit Datum nach dem letzten Tag
    pub later: usize,
}

/// Tag, an dem eine Aufgabe in „Demnächst“ steht: ⏳, sonst 📅.
pub fn column_date(t: &TaskRef) -> Option<NaiveDate> {
    t.task.scheduled.or(t.task.due)
}

pub fn upcoming_view(
    index: &Index,
    cfg: &Config,
    scope: Option<Scope>,
    today: NaiveDate,
    days: usize,
) -> UpcomingView {
    let journal = format!("{}/", cfg.folders.journal);
    let open: Vec<&TaskRef> = index
        .tasks
        .iter()
        .filter(|t| !t.task.done)
        .filter(|t| !t.note_path.starts_with(&journal))
        .filter(|t| scope.is_none_or(|s| t.scope == s))
        .collect();
    let last = today + Duration::days(days.saturating_sub(1) as i64);
    let mut v = UpcomingView {
        days: (0..days)
            .map(|i| UpcomingDay {
                date: today + Duration::days(i as i64),
                tasks: vec![],
            })
            .collect(),
        ..Default::default()
    };
    for t in &open {
        match column_date(t) {
            Some(d) if d > last => v.later += 1,
            Some(d) => {
                let i = (d.max(today) - today).num_days() as usize;
                v.days[i].tasks.push(UpcomingTask {
                    task: (*t).clone(),
                    overdue: d < today,
                });
            }
            None if !t.blocked && !t.task.has_tag(&cfg.waiting_tag) => {
                v.unscheduled.push((*t).clone())
            }
            None => {}
        }
    }
    for d in &mut v.days {
        d.tasks
            .sort_by_key(|u| (!u.overdue, u.task.task.priority, u.task.note_name.clone()));
    }
    v.unscheduled
        .sort_by_key(|t| (t.task.priority, t.note_name.clone()));
    v
}
