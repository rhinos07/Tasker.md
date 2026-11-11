//! Akzeptanztests gegen `examples/demo-vault` (SPEC 12), Stichtag 2026-11-11.

use chrono::NaiveDate;
use std::path::PathBuf;
use tasker_core::jira::KeyFilter;
use tasker_core::task::complete_line;
use tasker_core::vault::{parse_note, scan_vault};
use tasker_core::{Config, Index, Scope};

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 11).unwrap()
}

fn cfg() -> Config {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/demo-vault");
    Config::for_vault(p)
}

fn filter(c: &Config) -> KeyFilter {
    KeyFilter::new(c.jira.project_keys.iter().cloned())
}

fn descs(v: &[tasker_core::index::TaskRef]) -> Vec<&str> {
    v.iter().map(|t| t.task.description.as_str()).collect()
}

#[test]
fn excluded_folders_and_empty_checkboxes_are_ignored() {
    let c = cfg();
    let notes = scan_vault(&c, &filter(&c)).unwrap();
    let paths: Vec<&str> = notes.iter().map(|n| n.path.as_str()).collect();
    assert!(paths.iter().all(|p| !p.starts_with("40 Archiv") && !p.starts_with("30 Wissen") && !p.starts_with("Vorlagen")));
    let journal = notes.iter().find(|n| n.path == "Journal/2026-11-11.md").unwrap();
    assert_eq!(journal.tasks.len(), 1, "leere Checkbox wird ignoriert");
}

#[test]
fn dependency_chain_is_blocked_until_done() {
    // Kriterium 1
    let c = cfg();
    let idx = tasker_core::load_index(&c, today()).unwrap();
    let d = idx.dashboard(&c, None, today());
    let blocked: Vec<&str> = d.blocked.get("Gartenhaus").map(|v| descs(v)).unwrap_or_default();
    assert_eq!(blocked, vec!["Dachpappe bestellen", "Solarleuchte montieren"]);
    assert!(descs(&d.next_actions["#@haus"]).contains(&"Dachfläche ausmessen"));

    // gh-a abhaken (im Speicher) → gh-b wird frei, die Solarleuchte (⛔ gh-b) bleibt blockiert
    let path = c.vault.path.join("10 Projekte/Gartenhaus/Gartenhaus.md");
    let text = std::fs::read_to_string(path).unwrap();
    let changed: String = text
        .split('\n')
        .map(|l| if l.contains("🆔 gh-a") { complete_line(l, today()).unwrap() } else { l.to_string() })
        .collect::<Vec<_>>()
        .join("\n");
    let note = parse_note("10 Projekte/Gartenhaus/Gartenhaus.md", &changed, &c, &filter(&c));
    let idx = Index::build(&[note], &c, today());
    let d = idx.dashboard(&c, None, today());
    assert!(descs(&d.next_actions["#@computer"]).contains(&"Dachpappe bestellen"));
    assert_eq!(descs(&d.blocked["Gartenhaus"]), vec!["Solarleuchte montieren"]);
}

#[test]
fn jira_mentions_and_assignments() {
    // Kriterium 2
    let c = cfg();
    let idx = tasker_core::load_index(&c, today()).unwrap();
    let garden = idx.projects.iter().find(|p| p.name == "Gartenhaus").unwrap();
    assert_eq!(garden.jira_mentions, vec!["ABC-123"], "UTF-8 und DR-05 sind keine Tickets");
    assert!(garden.jira_assigned.is_empty());
    let release = idx.projects.iter().find(|p| p.name == "Release 2.0").unwrap();
    assert_eq!(release.jira_assigned, vec!["ABC-124"]);
}

#[test]
fn dashboard_sections() {
    let c = cfg();
    let idx = tasker_core::load_index(&c, today()).unwrap();
    let all = idx.dashboard(&c, None, today());

    assert_eq!(descs(&all.important)[0..2], ["Angebot Zimmerei Müller einholen", "Wasser im Garten abstellen"]);
    assert_eq!(descs(&all.overdue), vec!["Testbericht abgeben"]);
    assert_eq!(descs(&all.due_this_week), vec!["Bauantrag prüfen"]);
    assert_eq!(all.waiting.values().flatten().count(), 2);
    assert!(all.next_actions.values().flatten().all(|t| !t.note_path.starts_with("Journal/")));

    let work = idx.dashboard(&c, Some(Scope::Work), today());
    assert!(work.important.iter().all(|t| t.scope == Scope::Work));
    assert_eq!(descs(&work.important), vec!["Release-Notes schreiben"]);
    assert!(descs(&work.next_actions["#@computer"]).contains(&"Reisekosten abrechnen"));

    let private = idx.dashboard(&c, Some(Scope::Private), today());
    assert!(private.overdue.is_empty());
}

#[test]
fn projects_review_and_next_actions() {
    let c = cfg();
    let idx = tasker_core::load_index(&c, today()).unwrap();
    let p = |n: &str| idx.projects.iter().find(|p| p.name == n).unwrap();
    assert!(p("Release 2.0").review_due, "Review vor 22 Tagen");
    assert!(!p("Gartenhaus").review_due);
    assert_eq!(p("Gartenhaus").days_to_deadline, Some(34));
    assert_eq!(p("Schulung planen").next_actions, 0, "Projekt ohne nächste Aktion");
    assert_eq!(p("Release 2.0").scope, Scope::Work);
    assert_eq!(p("Gartenhaus").scope, Scope::Private);
    assert_eq!(idx.inbox_count, 1);
    assert!(idx.warnings.is_empty(), "{:?}", idx.warnings);
}

#[test]
fn example_config_parses() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config.example.json");
    let c = Config::load(&p).unwrap();
    assert_eq!(c.frontmatter.jira, "jira");
    assert_eq!(c.waiting_tag, "#warten");
}
