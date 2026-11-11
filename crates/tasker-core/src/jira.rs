//! Jira im Kern (SPEC 4): Ticket-Schlüssel im Text erkennen, Suchantworten lesen,
//! Tickets mit dem Vault verknüpfen. Ohne Netzwerkzugriff, der Abruf liegt in der App.

use crate::index::{Index, NoteRef};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::LazyLock;

static RE_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b([A-Z][A-Z0-9]+)-(\d+)\b").unwrap());

/// Filter, welche Schlüssel als echte Tickets gelten.
#[derive(Debug, Clone, Default)]
pub struct KeyFilter {
    /// Projekt-Präfixe aus der Konfiguration, z. B. `ABC`
    pub prefixes: HashSet<String>,
    /// Schlüssel, die in den Jira-Ergebnissen vorkommen
    pub known_keys: HashSet<String>,
}

impl KeyFilter {
    pub fn new(prefixes: impl IntoIterator<Item = String>) -> Self {
        Self { prefixes: prefixes.into_iter().collect(), known_keys: HashSet::new() }
    }

    pub fn accepts(&self, prefix: &str, key: &str) -> bool {
        self.prefixes.contains(prefix) || self.known_keys.contains(key)
    }
}

/// Findet alle akzeptierten Ticket-Schlüssel in `text`, ohne Duplikate, in Fundreihenfolge.
pub fn find_keys(text: &str, filter: &KeyFilter) -> Vec<String> {
    let mut seen = HashSet::new();
    RE_KEY
        .captures_iter(text)
        .filter_map(|c| {
            let key = c[0].to_string();
            (filter.accepts(&c[1], &key) && seen.insert(key.clone())).then_some(key)
        })
        .collect()
}

/// Prüft, ob ein Wert ein syntaktisch gültiger Schlüssel ist (für das Frontmatter-Feld `jira`).
pub fn is_valid_key(s: &str) -> bool {
    RE_KEY.find(s).is_some_and(|m| m.start() == 0 && m.end() == s.len())
}

/// Ein Ticket aus der Jira-Suche, auf die angezeigten Felder reduziert.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssue {
    pub key: String,
    pub summary: String,
    pub status: String,
    /// `new`, `indeterminate` oder `done`
    pub status_category: String,
    pub issue_type: String,
    pub priority: String,
    /// `YYYY-MM-DD` der letzten Änderung
    pub updated: String,
    pub url: String,
}

fn str_at<'a>(v: &'a Value, path: &[&str]) -> &'a str {
    path.iter().try_fold(v, |acc, k| acc.get(*k)).and_then(Value::as_str).unwrap_or("")
}

/// Liest eine Antwort von `/rest/api/3/search/jql`. Gibt die Tickets und das Token für die nächste Seite zurück.
/// `browse_base` ist die Site-URL, aus der die Links gebaut werden.
pub fn parse_search_response(json: &Value, browse_base: &str) -> (Vec<JiraIssue>, Option<String>) {
    let base = browse_base.trim_end_matches('/');
    let issues = json
        .get("issues")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|it| {
                    let key = it.get("key")?.as_str()?.to_string();
                    let f = it.get("fields")?;
                    Some(JiraIssue {
                        url: format!("{base}/browse/{key}"),
                        summary: str_at(f, &["summary"]).to_string(),
                        status: str_at(f, &["status", "name"]).to_string(),
                        status_category: str_at(f, &["status", "statusCategory", "key"]).to_string(),
                        issue_type: str_at(f, &["issuetype", "name"]).to_string(),
                        priority: str_at(f, &["priority", "name"]).to_string(),
                        updated: str_at(f, &["updated"]).chars().take(10).collect(),
                        key,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let last = json.get("isLast").and_then(Value::as_bool).unwrap_or(true);
    let next = json.get("nextPageToken").and_then(Value::as_str).map(String::from).filter(|_| !last);
    (issues, next)
}

/// Ticket mit seinen Bezügen im Vault.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedIssue {
    #[serde(flatten)]
    pub issue: JiraIssue,
    /// Projekte mit diesem Ticket im Feld `jira`
    pub assigned_to: Vec<NoteRef>,
    /// Notizen, die das Ticket im Text erwähnen
    pub mentioned_in: Vec<NoteRef>,
}

/// Eine Statusgruppe der Jira-Ansicht.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusGroup {
    pub status: String,
    pub category: String,
    pub issues: Vec<LinkedIssue>,
}

/// Verknüpft die Tickets mit dem Index und gruppiert sie nach Status.
/// Reihenfolge: „in Arbeit“ (indeterminate) vor „offen“ (new) vor Rest, innerhalb einer Kategorie
/// nach erstem Auftreten. Die Reihenfolge der Tickets aus der JQL-Sortierung bleibt erhalten.
pub fn group_issues(issues: &[JiraIssue], index: &Index) -> Vec<StatusGroup> {
    let rank = |c: &str| match c {
        "indeterminate" => 0,
        "new" => 1,
        _ => 2,
    };
    let mut groups: Vec<StatusGroup> = vec![];
    for i in issues {
        let linked = LinkedIssue {
            assigned_to: index.jira_assigned.get(&i.key).cloned().unwrap_or_default(),
            mentioned_in: index.jira_mentions.get(&i.key).cloned().unwrap_or_default(),
            issue: i.clone(),
        };
        match groups.iter_mut().find(|g| g.status == i.status) {
            Some(g) => g.issues.push(linked),
            None => groups.push(StatusGroup {
                status: i.status.clone(),
                category: i.status_category.clone(),
                issues: vec![linked],
            }),
        }
    }
    groups.sort_by_key(|g| rank(&g.category)); // stabil: erstes Auftreten bleibt
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_false_positives() {
        let f = KeyFilter::new(["ABC".to_string()]);
        let text = "Siehe ABC-123 und ABC-123, Encoding UTF-8, Entscheidung DR-05, ADR-009.";
        assert_eq!(find_keys(text, &f), vec!["ABC-123"]);
    }

    #[test]
    fn accepts_known_keys_from_jira() {
        let mut f = KeyFilter::new([]);
        f.known_keys.insert("XYZ-7".into());
        assert_eq!(find_keys("XYZ-7 und XYZ-8", &f), vec!["XYZ-7"]);
    }

    #[test]
    fn validates_keys() {
        assert!(is_valid_key("ABC-123"));
        assert!(!is_valid_key("abc-123"));
        assert!(!is_valid_key("ABC-123 x"));
    }

    fn sample() -> Value {
        serde_json::json!({
            "issues": [
                {"key": "ABC-124", "fields": {"summary": "Release vorbereiten",
                    "status": {"name": "Offen", "statusCategory": {"key": "new"}},
                    "issuetype": {"name": "Task"}, "priority": {"name": "High"},
                    "updated": "2026-11-02T10:11:12.000+0100"}},
                {"key": "ABC-123", "fields": {"summary": "Vorgaben Hausverwaltung",
                    "status": {"name": "In Arbeit", "statusCategory": {"key": "indeterminate"}},
                    "issuetype": {"name": "Bug"}, "priority": null,
                    "updated": "2026-10-01T08:00:00.000+0200"}},
                {"key": "ABC-200", "fields": {"summary": "Ohne Bezug",
                    "status": {"name": "Offen", "statusCategory": {"key": "new"}},
                    "issuetype": {"name": "Task"}, "priority": {"name": "Low"},
                    "updated": "2026-11-10T08:00:00.000+0100"}}
            ],
            "nextPageToken": "abc", "isLast": false
        })
    }

    #[test]
    fn parses_search_response() {
        let (issues, next) = parse_search_response(&sample(), "https://firma.example/");
        assert_eq!(issues.len(), 3);
        assert_eq!(issues[0].url, "https://firma.example/browse/ABC-124");
        assert_eq!(issues[0].updated, "2026-11-02");
        assert_eq!(issues[1].priority, "", "fehlende Priorität");
        assert_eq!(next.as_deref(), Some("abc"));
        let (_, next) = parse_search_response(&serde_json::json!({"issues": [], "isLast": true, "nextPageToken": "x"}), "");
        assert_eq!(next, None);
    }

    #[test]
    fn groups_and_links_issues() {
        let mut idx = Index::default();
        let r = |n: &str| NoteRef { path: format!("{n}.md"), name: n.into() };
        idx.jira_assigned.insert("ABC-124".into(), vec![r("Release 2.0")]);
        idx.jira_mentions.insert("ABC-123".into(), vec![r("Gartenhaus"), r("Journal")]);
        let (issues, _) = parse_search_response(&sample(), "https://firma.example");
        let g = group_issues(&issues, &idx);
        assert_eq!(g.iter().map(|g| g.status.as_str()).collect::<Vec<_>>(), vec!["In Arbeit", "Offen"]);
        assert_eq!(g[0].issues[0].mentioned_in.len(), 2);
        assert_eq!(g[1].issues[0].issue.key, "ABC-124", "JQL-Reihenfolge bleibt");
        assert_eq!(g[1].issues[0].assigned_to[0].name, "Release 2.0");
        assert!(g[1].issues[1].assigned_to.is_empty() && g[1].issues[1].mentioned_in.is_empty());
    }
}
