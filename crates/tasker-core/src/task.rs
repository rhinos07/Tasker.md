//! Parser für Aufgabenzeilen in der Syntax des Obsidian-Tasks-Plugins (SPEC 3.2)
//! und zeilengenaue Änderungen (SPEC 6.1, 6.4).

use chrono::NaiveDate;
use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;

/// Priorität, Reihenfolge von hoch nach niedrig.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Highest,
    High,
    Medium,
    #[default]
    None,
    Low,
    Lowest,
}

impl Priority {
    pub fn is_set(self) -> bool {
        self != Priority::None
    }
    fn from_emoji(e: &str) -> Option<Self> {
        match e.trim_end_matches('\u{FE0F}') {
            "🔺" => Some(Self::Highest),
            "⏫" => Some(Self::High),
            "🔼" => Some(Self::Medium),
            "🔽" => Some(Self::Low),
            "⏬" => Some(Self::Lowest),
            _ => None,
        }
    }
}

/// Eine geparste Aufgabe. `line` ist 0-basiert.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub line: usize,
    pub indent: usize,
    pub status: char,
    pub done: bool,
    /// Text ohne Metadaten und ohne Tags
    pub description: String,
    pub due: Option<NaiveDate>,
    pub scheduled: Option<NaiveDate>,
    pub start: Option<NaiveDate>,
    pub done_date: Option<NaiveDate>,
    pub created: Option<NaiveDate>,
    pub priority: Priority,
    pub recurrence: Option<String>,
    pub id: Option<String>,
    pub depends_on: Vec<String>,
    /// alle Tags inkl. `#`, z. B. `#@computer`, `#warten`
    pub tags: Vec<String>,
    /// die Originalzeile
    pub raw: String,
}

impl Task {
    pub fn contexts(&self) -> impl Iterator<Item = &str> {
        self.tags.iter().map(String::as_str).filter(|t| t.starts_with("#@"))
    }
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

static RE_TASK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)[-*+] \[(.)\]\s?(.*)$").unwrap());
static RE_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(📅|⏳|🛫|✅|➕|❌)\u{FE0F}?\s*(\d{4}-\d{2}-\d{2})").unwrap()
});
static RE_PRIO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(🔺|⏫|🔼|🔽|⏬)\u{FE0F}?").unwrap());
static RE_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"🆔\s*([A-Za-z0-9_-]+)").unwrap());
static RE_DEPENDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"⛔\u{FE0F}?\s*([A-Za-z0-9_-]+(?:\s*,\s*[A-Za-z0-9_-]+)*)").unwrap()
});
static RE_RECUR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"🔁\u{FE0F}?\s*([^📅⏳🛫✅➕❌🔺⏫🔼🔽⏬🆔⛔#]+)").unwrap()
});
static RE_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)(#[^\s#,;:!?()\[\]]+)").unwrap());
static RE_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s{2,}").unwrap());

fn date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

/// Parst eine Zeile. Liefert `None` für Nicht-Aufgaben und leere Checkboxen.
pub fn parse_task_line(line_no: usize, raw: &str) -> Option<Task> {
    let caps = RE_TASK.captures(raw)?;
    let indent = caps[1].chars().count();
    let status = caps[2].chars().next()?;
    let body = caps[3].to_string();
    if body.trim().is_empty() {
        return None;
    }

    let mut t = Task {
        line: line_no,
        indent,
        status,
        done: matches!(status, 'x' | 'X'),
        description: String::new(),
        due: None,
        scheduled: None,
        start: None,
        done_date: None,
        created: None,
        priority: Priority::None,
        recurrence: None,
        id: None,
        depends_on: vec![],
        tags: vec![],
        raw: raw.to_string(),
    };

    for c in RE_DATE.captures_iter(&body) {
        let d = date(&c[2]);
        match &c[1] {
            "📅" => t.due = d,
            "⏳" => t.scheduled = d,
            "🛫" => t.start = d,
            "✅" => t.done_date = d,
            "➕" => t.created = d,
            _ => {}
        }
    }
    if let Some(p) = RE_PRIO.find(&body).and_then(|m| Priority::from_emoji(m.as_str())) {
        t.priority = p;
    }
    if let Some(c) = RE_ID.captures(&body) {
        t.id = Some(c[1].to_string());
    }
    for c in RE_DEPENDS.captures_iter(&body) {
        t.depends_on
            .extend(c[1].split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
    }
    if let Some(c) = RE_RECUR.captures(&body) {
        let r = c[1].trim();
        if !r.is_empty() {
            t.recurrence = Some(r.to_string());
        }
    }
    t.tags = RE_TAG.captures_iter(&body).map(|c| c[1].to_string()).collect();

    let mut desc = body.clone();
    for re in [&*RE_DATE, &*RE_DEPENDS, &*RE_ID, &*RE_RECUR, &*RE_PRIO] {
        desc = re.replace_all(&desc, " ").into_owned();
    }
    desc = RE_TAG.replace_all(&desc, " ").into_owned();
    t.description = RE_SPACES.replace_all(desc.trim(), " ").into_owned();
    Some(t)
}

/// Markiert eine offene Aufgabenzeile als erledigt: `[ ]` → `[x]` plus ` ✅ YYYY-MM-DD`.
/// Alle anderen Zeichen der Zeile bleiben unverändert. Gibt `None` zurück,
/// wenn die Zeile keine offene Aufgabe ist.
/// Wiederholungen (🔁) werden noch nicht fortgeschrieben (TODO, SPEC 6.1).
pub fn complete_line(raw: &str, today: NaiveDate) -> Option<String> {
    let caps = RE_TASK.captures(raw)?;
    if &caps[2] != " " {
        return None;
    }
    let status_pos = caps.get(2)?.start();
    let mut out = String::with_capacity(raw.len() + 14);
    out.push_str(&raw[..status_pos]);
    out.push('x');
    out.push_str(raw[status_pos + 1..].trim_end());
    out.push_str(&format!(" ✅ {}", today.format("%Y-%m-%d")));
    Some(out)
}

/// Setzt oder entfernt das geplante Datum (⏳) einer Aufgabenzeile.
pub fn set_scheduled_line(raw: &str, date: Option<NaiveDate>) -> Option<String> {
    RE_TASK.captures(raw)?;
    static RE_SCHED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s*⏳\u{FE0F}?\s*\d{4}-\d{2}-\d{2}").unwrap());
    let without = RE_SCHED.replace_all(raw, "").into_owned();
    let without = without.trim_end().to_string();
    Some(match date {
        Some(d) => format!("{without} ⏳ {}", d.format("%Y-%m-%d")),
        None => without,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        date(s).unwrap()
    }

    #[test]
    fn parses_full_line() {
        let t = parse_task_line(
            3,
            "- [ ] Angebot anfragen #@telefon 🔼 🆔 gh-a ⛔ x-1, x-2 📅 2026-11-20 ⏳ 2026-11-13",
        )
        .unwrap();
        assert_eq!(t.description, "Angebot anfragen");
        assert_eq!(t.tags, vec!["#@telefon"]);
        assert_eq!(t.priority, Priority::Medium);
        assert_eq!(t.id.as_deref(), Some("gh-a"));
        assert_eq!(t.depends_on, vec!["x-1", "x-2"]);
        assert_eq!(t.due, Some(d("2026-11-20")));
        assert_eq!(t.scheduled, Some(d("2026-11-13")));
        assert!(!t.done);
        assert_eq!(t.line, 3);
    }

    #[test]
    fn parses_done_and_recurrence() {
        let t = parse_task_line(0, "  - [x] Rasen mähen 🔁 every week ✅ 2026-11-06").unwrap();
        assert!(t.done);
        assert_eq!(t.indent, 2);
        assert_eq!(t.recurrence.as_deref(), Some("every week"));
        assert_eq!(t.done_date, Some(d("2026-11-06")));
        assert_eq!(t.description, "Rasen mähen");
    }

    #[test]
    fn ignores_non_tasks_and_empty_checkboxes() {
        assert!(parse_task_line(0, "- normaler Punkt").is_none());
        assert!(parse_task_line(0, "- [ ] ").is_none());
        assert!(parse_task_line(0, "- [ ]").is_none());
        assert!(parse_task_line(0, "# Überschrift").is_none());
    }

    #[test]
    fn waiting_tag_and_contexts() {
        let t = parse_task_line(0, "- [ ] Rückmeldung von Max #warten #@büro").unwrap();
        assert!(t.has_tag("#warten"));
        assert_eq!(t.contexts().collect::<Vec<_>>(), vec!["#@büro"]);
    }

    #[test]
    fn complete_keeps_rest_of_line() {
        let raw = "\t- [ ] Test #@computer ⏫";
        let out = complete_line(raw, d("2026-11-11")).unwrap();
        assert_eq!(out, "\t- [x] Test #@computer ⏫ ✅ 2026-11-11");
        assert!(complete_line("- [x] fertig", d("2026-11-11")).is_none());
    }

    #[test]
    fn set_and_clear_scheduled() {
        let raw = "- [ ] Test #@computer ⏳ 2026-11-01";
        assert_eq!(
            set_scheduled_line(raw, Some(d("2026-11-12"))).unwrap(),
            "- [ ] Test #@computer ⏳ 2026-11-12"
        );
        assert_eq!(set_scheduled_line(raw, None).unwrap(), "- [ ] Test #@computer");
    }
}
