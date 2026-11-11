//! Gibt eine anonyme Zusammenfassung des Index aus (nur Zahlen, keine Inhalte).
//! Aufruf: cargo run -p tasker-core --example summary -- <pfad/zur/config.json>

use tasker_core::{load_index, Config, Scope};

fn main() {
    let path = std::env::args().nth(1).expect("Pfad zur config.json angeben");
    let cfg = Config::load(path.as_ref()).expect("Konfiguration lesbar");
    let today = chrono::Local::now().date_naive();
    let start = std::time::Instant::now();
    let idx = load_index(&cfg, today).expect("Vault lesbar");
    let ms = start.elapsed().as_millis();
    let open = idx.tasks.iter().filter(|t| !t.task.done).count();
    println!("Index in {ms} ms: {} Aufgaben ({open} offen), {} Projekte, Inbox {}", idx.tasks.len(), idx.projects.len(), idx.inbox_count);
    for (name, scope) in [("Alle", None), ("Arbeit", Some(Scope::Work)), ("Privat", Some(Scope::Private))] {
        let d = idx.dashboard(&cfg, scope, today);
        let n = |m: &std::collections::BTreeMap<String, Vec<_>>| m.values().map(Vec::len).sum::<usize>();
        println!(
            "{name:7} Wichtig {:2} · Überfällig {:2} · Woche {:2} · Nächste {:3} · Warten {:2} · Blockiert {:2}",
            d.important.len(), d.overdue.len(), d.due_this_week.len(), n(&d.next_actions), n(&d.waiting), n(&d.blocked)
        );
    }
    let review = idx.projects.iter().filter(|p| p.review_due).count();
    let no_action = idx.projects.iter().filter(|p| p.status.as_deref() == Some("aktiv") && p.next_actions == 0).count();
    let assigned: usize = idx.projects.iter().map(|p| p.jira_assigned.len()).sum();
    let mentioned: usize = idx.projects.iter().map(|p| p.jira_mentions.len()).sum();
    println!("Projekte: Review fällig {review}, aktiv ohne nächste Aktion {no_action}, Jira zugeordnet {assigned}, erwähnt {mentioned}");
    println!("Hinweise: {}", idx.warnings.len());
}
