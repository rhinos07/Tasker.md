//! Index über alle Notizen und die festen GTD-Ansichten (SPEC 5.2, 5.3).

use crate::config::Config;
use crate::task::Task;
use crate::vault::{Note, NoteKind};
use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Work,
    Private,
}

/// Aufgabe mit Kontext für die Oberfläche.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRef {
    pub note_path: String,
    pub note_name: String,
    pub note_kind: NoteKind,
    /// Projekt oder Bereich, zu dem die Aufgabe gehört
    pub area: Option<String>,
    pub scope: Scope,
    pub blocked: bool,
    pub task: Task,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub path: String,
    pub name: String,
    pub status: Option<String>,
    pub area: Option<String>,
    pub scope: Scope,
    pub review: Option<NaiveDate>,
    pub deadline: Option<NaiveDate>,
    pub days_since_review: Option<i64>,
    pub days_to_deadline: Option<i64>,
    pub review_due: bool,
    pub open_tasks: usize,
    pub next_actions: usize,
    pub jira_assigned: Vec<String>,
    pub jira_mentions: Vec<String>,
    /// Fokus-Minuten laut Log: seit Montag dieser Woche und insgesamt
    pub focus_week: u32,
    pub focus_total: u32,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub important: Vec<TaskRef>,
    pub overdue: Vec<TaskRef>,
    pub due_this_week: Vec<TaskRef>,
    pub next_actions: BTreeMap<String, Vec<TaskRef>>,
    pub waiting: BTreeMap<String, Vec<TaskRef>>,
    pub blocked: BTreeMap<String, Vec<TaskRef>>,
}

/// Verweis auf eine Notiz, z. B. als Fundstelle eines Jira-Tickets.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NoteRef {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    pub today: Option<NaiveDate>,
    pub tasks: Vec<TaskRef>,
    pub projects: Vec<ProjectInfo>,
    pub inbox_count: usize,
    pub warnings: Vec<String>,
    /// Ticket-Schlüssel → Notizen, die ihn im Text erwähnen
    pub jira_mentions: BTreeMap<String, Vec<NoteRef>>,
    /// Ticket-Schlüssel → Projekte, denen er im Feld `jira` zugeordnet ist
    pub jira_assigned: BTreeMap<String, Vec<NoteRef>>,
}

fn date_of(note: &Note, key: &str) -> Option<NaiveDate> {
    note.fm_str(key).and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
}

fn scope_of(cfg: &Config, path: &str, area: Option<&str>) -> Scope {
    let by_area = area.is_some_and(|a| cfg.areas.work.iter().any(|w| w == a));
    let by_path = cfg.areas.work_paths.iter().any(|p| path.starts_with(p.as_str()));
    if by_area || by_path {
        Scope::Work
    } else {
        Scope::Private
    }
}

impl Index {
    pub fn build(notes: &[Note], cfg: &Config, today: NaiveDate) -> Self {
        let fm = &cfg.frontmatter;
        let mut warnings: Vec<String> = notes
            .iter()
            .flat_map(|n| n.warnings.iter().map(move |w| format!("{}: {w}", n.path)))
            .collect();

        // offene IDs für Abhängigkeiten
        let all_ids: HashSet<&str> = notes
            .iter()
            .flat_map(|n| n.tasks.iter())
            .filter_map(|t| t.id.as_deref())
            .collect();
        let open_ids: HashSet<&str> = notes
            .iter()
            .flat_map(|n| n.tasks.iter())
            .filter(|t| !t.done)
            .filter_map(|t| t.id.as_deref())
            .collect();

        let mut tasks = vec![];
        for n in notes {
            let area = match n.kind {
                NoteKind::Project => n.fm_str(&fm.area),
                NoteKind::Area => Some(n.name.clone()),
                _ => None,
            };
            let scope = scope_of(cfg, &n.path, area.as_deref());
            for t in &n.tasks {
                for dep in &t.depends_on {
                    if !all_ids.contains(dep.as_str()) {
                        warnings.push(format!("{}: unbekannte Abhängigkeit {dep}", n.path));
                    }
                }
                let blocked = t.depends_on.iter().any(|d| open_ids.contains(d.as_str()));
                tasks.push(TaskRef {
                    note_path: n.path.clone(),
                    note_name: n.name.clone(),
                    note_kind: n.kind,
                    area: area.clone(),
                    scope,
                    blocked,
                    task: t.clone(),
                });
            }
        }

        let projects = notes
            .iter()
            .filter(|n| n.kind == NoteKind::Project)
            .map(|n| {
                let review = date_of(n, &fm.review);
                let deadline = date_of(n, &fm.deadline);
                let area = n.fm_str(&fm.area);
                let mine = tasks.iter().filter(|t| t.note_path == n.path && !t.task.done);
                let open_tasks = mine.clone().count();
                let next_actions = mine.filter(|t| !t.blocked).count();
                let days_since_review = review.map(|r| (today - r).num_days());
                let status = n.fm_str(&fm.status);
                let monday = today - Duration::days(i64::from(today.weekday().num_days_from_monday()));
                ProjectInfo {
                    path: n.path.clone(),
                    name: n.name.clone(),
                    scope: scope_of(cfg, &n.path, area.as_deref()),
                    review_due: status.as_deref() == Some("aktiv")
                        && days_since_review.is_none_or(|d| d > cfg.projects.review_after_days),
                    status,
                    area,
                    review,
                    deadline,
                    days_since_review,
                    days_to_deadline: deadline.map(|d| (d - today).num_days()),
                    open_tasks,
                    next_actions,
                    jira_assigned: n.jira_assigned.clone(),
                    jira_mentions: n.jira_mentions.clone(),
                    focus_week: n.focus_log.iter().filter(|(d, _)| *d >= monday && *d <= today).map(|(_, m)| m).sum(),
                    focus_total: n.focus_log.iter().map(|(_, m)| m).sum(),
                }
            })
            .collect();

        let inbox_count = notes.iter().filter(|n| n.kind == NoteKind::Inbox).count();
        let mut jira_mentions: BTreeMap<String, Vec<NoteRef>> = BTreeMap::new();
        let mut jira_assigned: BTreeMap<String, Vec<NoteRef>> = BTreeMap::new();
        for n in notes {
            let r = NoteRef { path: n.path.clone(), name: n.name.clone() };
            for k in &n.jira_mentions {
                jira_mentions.entry(k.clone()).or_default().push(r.clone());
            }
            for k in &n.jira_assigned {
                jira_assigned.entry(k.clone()).or_default().push(r.clone());
            }
        }
        Index { today: Some(today), tasks, projects, inbox_count, warnings, jira_mentions, jira_assigned }
    }

    /// Die Abschnitte aus SPEC 5.2. `scope = None` heißt „Alle“.
    /// Tagesnotizen fließen nicht ins Dashboard ein (SPEC 3.1).
    pub fn dashboard(&self, cfg: &Config, scope: Option<Scope>, today: NaiveDate) -> Dashboard {
        let journal = format!("{}/", cfg.folders.journal);
        let open: Vec<&TaskRef> = self
            .tasks
            .iter()
            .filter(|t| !t.task.done)
            .filter(|t| !t.note_path.starts_with(&journal))
            .filter(|t| scope.is_none_or(|s| t.scope == s))
            .collect();
        let waiting = |t: &TaskRef| t.task.has_tag(&cfg.waiting_tag);
        let week_end = today + Duration::days(7);

        let mut d = Dashboard::default();
        let mut important: Vec<TaskRef> =
            open.iter().filter(|t| t.task.priority.is_set() && !t.blocked).map(|t| (*t).clone()).collect();
        important.sort_by_key(|t| t.task.priority);
        d.important = important;
        d.overdue = open.iter().filter(|t| t.task.due.is_some_and(|x| x < today)).map(|t| (*t).clone()).collect();
        let mut week: Vec<TaskRef> = open
            .iter()
            .filter(|t| t.task.due.is_some_and(|x| x >= today && x <= week_end))
            .map(|t| (*t).clone())
            .collect();
        week.sort_by_key(|t| t.task.due);
        d.due_this_week = week;

        for t in &open {
            if t.blocked {
                d.blocked.entry(t.note_name.clone()).or_default().push((*t).clone());
            } else if waiting(t) {
                d.waiting.entry(t.note_name.clone()).or_default().push((*t).clone());
            } else if t.task.due.is_none() && !t.task.priority.is_set() {
                let ctx = t.task.contexts().next().unwrap_or("(ohne Kontext)").to_string();
                d.next_actions.entry(ctx).or_default().push((*t).clone());
            }
        }
        d
    }
}
