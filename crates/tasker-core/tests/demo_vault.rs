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
    assert_eq!(journal.tasks.len(), 2, "leere Checkbox wird ignoriert");
}

/// Geerbte Tags aus dem Frontmatter und Datum aus der Tagesnotiz: nur Anzeige, Filter und Ansichten.
#[test]
fn inherited_tags_and_daily_note_date() {
    let c = cfg();
    let idx = tasker_core::load_index(&c, today()).unwrap();
    let find = |d: &str| idx.tasks.iter().find(|t| t.task.description == d).unwrap();

    let roof = find("Dachfläche ausmessen");
    assert_eq!(roof.inherited_tags, vec!["#projekt", "#garten"]);
    assert_eq!(roof.task.tags, vec!["#@haus"], "die Zeile selbst bleibt unverändert");
    let f = tasker_core::filter::Filter::parse("tag:garten & kontext:@haus").unwrap();
    let hits = tasker_core::filter::apply(&f, &idx, &c, None, today());
    assert!(hits.iter().all(|t| t.note_name == "Gartenhaus") && !hits.is_empty());

    // ohne eigenes Datum: Datum der Tagesnotiz; mit eigenem ⏳: keins abgeleitet
    let quick = find("Schnell notiert im Journal");
    assert_eq!(quick.inferred_scheduled, Some(today()));
    assert_eq!(quick.task.scheduled, None);
    assert_eq!(find("Termin beim Optiker vereinbaren").inferred_scheduled, None);
    let view = tasker_core::today::today_view(&idx, &c, None, today(), &[]);
    assert!(descs(&view.scheduled).contains(&"Schnell notiert im Journal"));
    let d = idx.dashboard(&c, None, today());
    assert!(d.next_actions.values().flatten().all(|t| !t.note_path.starts_with("Journal/")), "Dashboard ohne Tagesnotizen");
    let up = tasker_core::upcoming::upcoming_view(&idx, &c, None, today(), 14);
    let day = |i: usize| up.days[i].tasks.iter().map(|u| u.task.task.description.as_str()).collect::<Vec<_>>();
    assert!(day(0).contains(&"Schnell notiert im Journal"), "abgeleitetes Datum: heute");
    assert!(day(1).contains(&"Termin beim Optiker vereinbaren"), "eigenes ⏳ aus der Tagesnotiz");
    assert!(up.unscheduled.iter().all(|t| !t.note_path.starts_with("Journal/")));

    // abschaltbar
    let mut off = cfg();
    off.inherit.frontmatter_tags = false;
    off.inherit.daily_note_date = false;
    let idx = tasker_core::load_index(&off, today()).unwrap();
    assert!(idx.tasks.iter().all(|t| t.inherited_tags.is_empty() && t.inferred_scheduled.is_none()));
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

    // [/] steht unter „In Arbeit“ statt bei den nächsten Aktionen, [-] nirgends
    assert_eq!(descs(&all.in_progress), vec!["Werkzeugliste zusammenstellen"]);
    let everywhere: Vec<&str> = all.next_actions.values().flatten().map(|t| t.task.description.as_str()).collect();
    assert!(!everywhere.contains(&"Werkzeugliste zusammenstellen"));
    assert!(!everywhere.contains(&"Fertiggartenhaus aus dem Katalog bestellen"));
    let today_view = tasker_core::today::today_view(&idx, &c, None, today(), &[]);
    assert_eq!(descs(&today_view.in_progress), vec!["Werkzeugliste zusammenstellen"]);
    let f = tasker_core::filter::Filter::parse("inarbeit").unwrap();
    let hits = tasker_core::filter::apply(&f, &idx, &c, None, today());
    assert_eq!(hits.iter().map(|t| t.task.description.as_str()).collect::<Vec<_>>(), vec!["Werkzeugliste zusammenstellen"]);
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

/// Kopiert den Demo-Vault in ein temporäres Verzeichnis, damit Schreibtests das Repo nicht verändern.
fn temp_vault(name: &str) -> Config {
    let src = cfg().vault.path;
    let dst = std::env::temp_dir().join(format!("tasker-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dst);
    for e in walkdir::WalkDir::new(&src).into_iter().filter_map(Result::ok) {
        let rel = e.path().strip_prefix(&src).unwrap();
        let to = dst.join(rel);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&to).unwrap();
        } else {
            std::fs::copy(e.path(), &to).unwrap();
        }
    }
    Config::for_vault(dst)
}

#[test]
fn complete_undo_schedule_roundtrip_on_disk() {
    use tasker_core::edit::{edit_line, EditError, LineEdit};
    let c = temp_vault("roundtrip");
    let rel = "10 Projekte/Gartenhaus/Gartenhaus.md";
    let file = c.vault.path.join(rel);
    let original = std::fs::read(&file).unwrap();

    let idx = tasker_core::load_index(&c, today()).unwrap();
    let t = idx.tasks.iter().find(|t| t.task.description == "Dachfläche ausmessen").unwrap();

    // Abhaken → gh-b ist danach nicht mehr blockiert
    let r = edit_line(&c.vault.path, rel, t.task.line, &t.task.raw, &LineEdit::Complete, today()).unwrap();
    assert!(r.new.ends_with("✅ 2026-11-11"));
    let idx2 = tasker_core::load_index(&c, today()).unwrap();
    let d = idx2.dashboard(&c, None, today());
    assert!(descs(&d.next_actions["#@computer"]).contains(&"Dachpappe bestellen"));

    // Rückgängig → Datei ist byte-gleich zum Original
    edit_line(&c.vault.path, rel, r.line, &r.new, &LineEdit::Replace(r.old.clone()), today()).unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), original);

    // ⏳ heute setzen → erscheint mit Datum, Rest der Datei unverändert
    let s = edit_line(&c.vault.path, rel, t.task.line, &t.task.raw, &LineEdit::SetScheduled(Some(today())), today()).unwrap();
    assert_eq!(s.new, "- [ ] Dachfläche ausmessen #@haus 🆔 gh-a ⏳ 2026-11-11");
    let after = std::fs::read_to_string(&file).unwrap();
    let before = String::from_utf8(original.clone()).unwrap();
    let diff: Vec<_> = before.lines().zip(after.lines()).filter(|(a, b)| a != b).collect();
    assert_eq!(diff.len(), 1, "genau eine Zeile geändert");

    // Konflikt: alte Fassung der Zeile wird nicht mehr akzeptiert
    let e = edit_line(&c.vault.path, rel, t.task.line, &t.task.raw, &LineEdit::Complete, today()).unwrap_err();
    assert_eq!(e, EditError::Conflict);

    std::fs::remove_dir_all(&c.vault.path).unwrap();
}

/// SPEC 6.1: Abhaken einer 🔁-Aufgabe legt die nächste Instanz darüber an; Rückgängig stellt die Datei wieder her.
#[test]
fn recurring_complete_and_undo_on_disk() {
    use tasker_core::edit::{edit_line, LineEdit};
    let c = temp_vault("recur");
    let rel = "20 Bereiche/Haus/Haus.md";
    let file = c.vault.path.join(rel);
    let original = std::fs::read(&file).unwrap();

    let idx = tasker_core::load_index(&c, today()).unwrap();
    let t = idx.tasks.iter().find(|t| t.task.description == "Rauchmelder testen").unwrap();
    let r = edit_line(&c.vault.path, rel, t.task.line, &t.task.raw, &LineEdit::Complete, today()).unwrap();
    // ohne Datum: ab heute gerechnet, neue Instanz bekommt ⏳
    assert_eq!(
        r.new,
        "- [ ] Rauchmelder testen #@haus 🔁 every year ⏳ 2027-11-11\n- [x] Rauchmelder testen #@haus 🔁 every year ✅ 2026-11-11"
    );

    let idx2 = tasker_core::load_index(&c, today()).unwrap();
    let mine: Vec<_> = idx2.tasks.iter().filter(|t| t.task.description == "Rauchmelder testen").collect();
    assert_eq!(mine.len(), 2);
    assert!(!mine[0].task.done && mine[0].task.scheduled == Some(NaiveDate::from_ymd_opt(2027, 11, 11).unwrap()));
    assert!(mine[1].task.done);

    edit_line(&c.vault.path, rel, r.line, &r.new, &LineEdit::Replace(r.old.clone()), today()).unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), original);
    std::fs::remove_dir_all(&c.vault.path).unwrap();
}

/// SPEC 12, Kriterium 5: zwei Pomodoros → eine Log-Zeile, die beim zweiten aktualisiert wird; 3 min schreiben nichts.
#[test]
fn focus_log_acceptance() {
    use tasker_core::focus::{task_day_total, Session};
    use tasker_core::focuslog::write_log;
    let c = temp_vault("focuslog");
    let rel = "10 Projekte/Gartenhaus/Gartenhaus.md";
    let before = std::fs::read_to_string(c.vault.path.join(rel)).unwrap();
    let session = |id: &str, minutes: u32| Session {
        id: id.into(),
        date: today(),
        start: "09:00".into(),
        minutes,
        path: rel.into(),
        description: "Fundament ausmessen".into(),
        task_id: None,
        line_keys: vec![],
        project_keys: vec![],
        project_mentions: vec![],
    };
    let f = &c.focus;
    let mut sessions = vec![session("a", 3)];
    let key = sessions[0].task_key();
    assert_eq!(task_day_total(&sessions, today(), &key, f.min_minutes), (0, 0), "3 min zählen nicht");

    for (i, id) in ["b", "c"].iter().enumerate() {
        sessions.push(session(id, 25));
        let (m, n) = task_day_total(&sessions, today(), &key, f.min_minutes);
        write_log(&c.vault.path, rel, &f.log_section, &f.log_format, today(), "Fundament ausmessen", m, n).unwrap();
        let text = std::fs::read_to_string(c.vault.path.join(rel)).unwrap();
        let lines: Vec<&str> = text.lines().filter(|l| l.contains('⏱')).collect();
        let expected = if i == 0 { "25 min (1 Sitzung)" } else { "50 min (2 Sitzungen)" };
        assert_eq!(lines, vec![format!("- 2026-11-11 ⏱ {expected} · Fundament ausmessen").as_str()]);
    }
    let after = std::fs::read_to_string(c.vault.path.join(rel)).unwrap();
    assert_eq!(after.lines().count(), before.lines().count() + 1, "genau eine Zeile dazu");
    std::fs::remove_dir_all(&c.vault.path).unwrap();
}

/// SPEC 5.5 und Kriterium 8: Spalten nach ⏳/📅; Ziehen auf übermorgen ändert nur das ⏳-Datum.
#[test]
fn upcoming_columns_and_drag_to_day_after_tomorrow() {
    use tasker_core::edit::{edit_line, LineEdit};
    use tasker_core::upcoming::upcoming_view;
    let c = cfg();
    let idx = tasker_core::load_index(&c, today()).unwrap();
    let v = upcoming_view(&idx, &c, None, today(), 14);
    assert_eq!(v.days.len(), 14);
    let col = |i: usize| v.days[i].tasks.iter().map(|u| u.task.task.description.as_str()).collect::<Vec<_>>();
    assert!(col(0).contains(&"Wasser im Garten abstellen"), "⏳ heute");
    assert!(col(0).contains(&"Testbericht abgeben"), "📅 überfällig steht unter heute");
    assert!(v.days[0].tasks.iter().any(|u| u.overdue && u.task.task.description == "Testbericht abgeben"));
    assert!(col(1).contains(&"Mit Teamleitung über Weiterbildung sprechen"));
    assert!(col(2).contains(&"Bauantrag prüfen"), "📅 ohne ⏳");
    assert!(v.unscheduled.iter().all(|t| t.task.scheduled.is_none() && t.task.due.is_none() && !t.blocked));
    assert!(v.unscheduled.iter().any(|t| t.task.description == "Dachfläche ausmessen"));
    let work = upcoming_view(&idx, &c, Some(Scope::Work), today(), 14);
    assert!(work.days.iter().flat_map(|d| &d.tasks).all(|u| u.task.scope == Scope::Work));

    // Ziehen auf übermorgen
    let c = temp_vault("upcoming");
    let rel = "10 Projekte/Gartenhaus/Gartenhaus.md";
    let file = c.vault.path.join(rel);
    let before = std::fs::read_to_string(&file).unwrap();
    let idx = tasker_core::load_index(&c, today()).unwrap();
    let t = idx.tasks.iter().find(|t| t.task.description == "Bauantrag prüfen").unwrap();
    let target = today() + chrono::Duration::days(2);
    let r = edit_line(&c.vault.path, rel, t.task.line, &t.task.raw, &LineEdit::SetScheduled(Some(target)), today()).unwrap();
    assert_eq!(r.new, "- [ ] Bauantrag prüfen #@computer 📅 2026-11-13 ⏳ 2026-11-13");
    let after = std::fs::read_to_string(&file).unwrap();
    let diff: Vec<_> = before.lines().zip(after.lines()).filter(|(a, b)| a != b).collect();
    assert_eq!(diff.len(), 1, "genau eine Zeile geändert");
    assert_eq!(before.lines().count(), after.lines().count());
    std::fs::remove_dir_all(&c.vault.path).unwrap();
}

/// SPEC 12, Kriterium 7: derselbe Satz wie „Nächste Aktionen“ in der Gruppe @computer; dazu die Felder.
#[test]
fn filter_matches_next_actions_and_fields() {
    use tasker_core::filter::{apply, Filter};
    let c = cfg();
    let idx = tasker_core::load_index(&c, today()).unwrap();
    let run = |q: &str| -> Vec<String> {
        let f = Filter::parse(q).unwrap();
        let mut v: Vec<String> = apply(&f, &idx, &c, None, today()).iter().map(|t| t.task.description.clone()).collect();
        v.sort();
        v
    };
    let d = idx.dashboard(&c, None, today());
    let mut expected: Vec<String> = d.next_actions["#@computer"].iter().map(|t| t.task.description.clone()).collect();
    expected.sort();
    assert!(!expected.is_empty());
    assert_eq!(run("kontext:@computer & !blockiert & !warten & prio:keine & fällig:keine"), expected);

    assert_eq!(run("geplant:heute"), vec!["Wasser im Garten abstellen"]);
    assert_eq!(run("fällig:überfällig"), vec!["Testbericht abgeben"]);
    assert!(run("fällig:<7d").contains(&"Bauantrag prüfen".to_string()));
    assert!(run("projekt:garten").contains(&"Bauantrag prüfen".to_string()));
    assert!(run("bereich:arbeit").contains(&"Testbericht abgeben".to_string()));
    assert!(!run("bereich:privat").contains(&"Testbericht abgeben".to_string()));
    assert!(!run("jira:ABC-123").is_empty(), "Ticket in der Notiz erwähnt");
    assert_eq!(run("blockiert").len(), d.blocked.values().map(Vec::len).sum::<usize>());
    assert_eq!(run("dachfläche"), vec!["Dachfläche ausmessen"], "freier Text");
    let all = run("");
    assert_eq!(run("prio | !prio"), all, "leer und Tautologie passen auf alles");
}

/// SPEC 12, Kriterium 11: Link mit Zeile, wenn Advanced URI aktiv ist, sonst obsidian://open; .obsidian bleibt unverändert.
#[test]
fn obsidian_link_with_and_without_advanced_uri() {
    use tasker_core::obsidian::link;
    let mut c = temp_vault("obsidian");
    let rel = "10 Projekte/Gartenhaus/Gartenhaus.md";
    let plain = link(&c, rel, 17);
    assert!(!plain.exact);
    assert_eq!(plain.url, "obsidian://open?vault=demo-vault&file=10%20Projekte%2FGartenhaus%2FGartenhaus.md");

    let ob = c.vault.path.join(".obsidian");
    std::fs::create_dir_all(ob.join("plugins/obsidian-advanced-uri")).unwrap();
    std::fs::write(ob.join("community-plugins.json"), r#"["dataview","obsidian-advanced-uri"]"#).unwrap();
    let snapshot = |p: &std::path::Path| -> Vec<(String, Vec<u8>)> {
        let mut v: Vec<_> = walkdir::WalkDir::new(p)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .map(|e| (e.path().display().to_string(), std::fs::read(e.path()).unwrap()))
            .collect();
        v.sort();
        v
    };
    let before = snapshot(&ob);
    let exact = link(&c, rel, 17);
    assert!(exact.exact);
    assert_eq!(exact.url, "obsidian://adv-uri?vault=demo-vault&filepath=10%20Projekte%2FGartenhaus%2FGartenhaus.md&line=18");
    assert_eq!(snapshot(&ob), before, ".obsidian bleibt byte-gleich");

    c.obsidian.use_advanced_uri = "never".into();
    assert!(!link(&c, rel, 17).exact, "abschaltbar");
    std::fs::remove_dir_all(&c.vault.path).unwrap();
}

/// Erinnerungen (SPEC 6.7): ⏰ aus dem Demo-Vault, Filter `erinnerung`, Briefing-Text.
#[test]
fn reminders_on_demo_vault() {
    use tasker_core::reminders::{briefing_text, due_reminders};
    let c = cfg();
    let idx = tasker_core::load_index(&c, today()).unwrap();
    let at = |h: u32, m: u32| today().and_hms_opt(h, m, 0).unwrap();
    let due = due_reminders(&idx, at(16, 59), at(17, 0));
    assert_eq!(due.iter().map(|r| r.task.task.description.as_str()).collect::<Vec<_>>(), vec!["Wasser im Garten abstellen"]);
    assert!(due_reminders(&idx, at(17, 0), at(23, 59)).is_empty());

    let f = tasker_core::filter::Filter::parse("erinnerung").unwrap();
    let hits = tasker_core::filter::apply(&f, &idx, &c, None, today());
    assert_eq!(hits.iter().map(|t| t.task.description.as_str()).collect::<Vec<_>>(), vec!["Wasser im Garten abstellen"]);

    let view = tasker_core::today::today_view(&idx, &c, None, today(), &[]);
    let text = briefing_text(&view, &[], chrono::NaiveTime::from_hms_opt(8, 0, 0).unwrap());
    assert!(text.contains("geplant") && text.contains("überfällig"), "{text}");
}
