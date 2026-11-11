//! Datei-Watcher: beobachtet den Vault und meldet relevante Änderungen an die Oberfläche (SPEC 3.1).
//! Mehrere schnell aufeinanderfolgende Änderungen (z. B. Speichern in Obsidian, Cloud-Sync)
//! werden zu einem Ereignis zusammengefasst.

use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tasker_core::vault::ChangeFilter;
use tasker_core::Config;
use tauri::{AppHandle, Emitter};

/// Name des Ereignisses, auf das die Oberfläche hört. Nutzlast: Anzahl geänderter Dateien.
pub const EVENT: &str = "vault-changed";
const DEBOUNCE: Duration = Duration::from_millis(400);

struct Active {
    root: PathBuf,
    _debouncer: Debouncer<RecommendedWatcher>,
}

/// Höchstens ein aktiver Watcher. Ein neuer Vault ersetzt den alten.
#[derive(Default)]
pub struct WatchState(Mutex<Option<Active>>);

pub fn watch(app: &AppHandle, state: &WatchState, cfg: &Config) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|_| "Watcher nicht verfügbar".to_string())?;
    if guard.as_ref().is_some_and(|a| a.root == cfg.vault.path) {
        return Ok(());
    }
    *guard = None; // alten Watcher beenden
    let app = app.clone();
    let debouncer = start(cfg, move |changed| {
        let _ = app.emit(EVENT, changed);
    })?;
    *guard = Some(Active { root: cfg.vault.path.clone(), _debouncer: debouncer });
    Ok(())
}

/// Startet die Beobachtung. `on_change` erhält die Zahl relevanter Dateien pro zusammengefasstem Ereignis.
pub fn start(
    cfg: &Config,
    on_change: impl Fn(usize) + Send + 'static,
) -> Result<Debouncer<RecommendedWatcher>, String> {
    let filter = ChangeFilter::new(cfg).map_err(|e| e.to_string())?;
    let mut debouncer = new_debouncer(DEBOUNCE, move |res: DebounceEventResult| {
        // Fehler des Watchers werden bewusst nicht mit Pfaden geloggt (SPEC 9).
        if let Ok(events) = res {
            let changed = events.iter().filter(|e| filter.is_relevant(&e.path)).count();
            if changed > 0 {
                on_change(changed);
            }
        }
    })
    .map_err(|e| format!("Watcher konnte nicht starten: {e}"))?;
    debouncer
        .watcher()
        .watch(&cfg.vault.path, RecursiveMode::Recursive)
        .map_err(|e| format!("Vault kann nicht beobachtet werden: {e}"))?;
    Ok(debouncer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn reports_relevant_changes_only() {
        let dir = std::env::temp_dir().join(format!("tasker-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("10 Projekte/A")).unwrap();
        std::fs::create_dir_all(dir.join("30 Wissen")).unwrap();
        let cfg = Config::for_vault(&dir);
        let (tx, rx) = mpsc::channel();
        let _w = start(&cfg, move |n| tx.send(n).unwrap()).unwrap();
        std::thread::sleep(Duration::from_millis(200));

        // irrelevant: ausgeschlossener Ordner und keine Markdown-Datei
        std::fs::write(dir.join("30 Wissen/X.md"), "x").unwrap();
        std::fs::write(dir.join("10 Projekte/A/bild.png"), "x").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(1200)).is_err(), "keine Meldung erwartet");

        // relevant: Projektnotiz, mehrfach schnell hintereinander → ein Ereignis
        for i in 0..3 {
            std::fs::write(dir.join("10 Projekte/A/A.md"), format!("- [ ] Tun {i}")).unwrap();
        }
        let n = rx.recv_timeout(Duration::from_secs(3)).expect("Änderung gemeldet");
        assert!(n >= 1);
        drop(_w);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
