//! Zwischenspeicher für den Vault-Index.
//!
//! Ohne ihn liest jede Anfrage (Dashboard, Heute, Schnelleingabe) den ganzen Vault neu ein, beim
//! Umschalten des Bereichsfilters zweimal parallel. Gültig ist ein Eintrag nur für dieselbe
//! Konfiguration, denselben Tag und dieselbe Generation. Die Generation steigt bei jeder gemeldeten
//! Dateiänderung (Watcher) und vor jedem Schreibvorgang der App. Als Sicherheitsnetz, falls der Watcher
//! eine Änderung nicht meldet, verfällt ein Eintrag nach [`MAX_AGE`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tasker_core::{load_index, Config, Index};

struct Entry {
    key: String,
    generation: u64,
    created: Instant,
    index: Arc<Index>,
}

const MAX_AGE: Duration = Duration::from_secs(30);

static GENERATION: AtomicU64 = AtomicU64::new(0);
/// Hält die Sperre während des Scans: Gleichzeitige Anfragen warten auf einen gemeinsamen Lauf.
static SLOT: Mutex<Option<Entry>> = Mutex::new(None);

/// Verwirft den Zwischenspeicher, z. B. nach einer Änderung im Vault.
pub fn invalidate() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Index für `cfg` und `today`, aus dem Zwischenspeicher oder frisch eingelesen.
pub fn get(cfg: &Config, today: chrono::NaiveDate) -> Result<Arc<Index>, String> {
    let key = format!("{today}|{}", serde_json::to_string(cfg).map_err(|e| e.to_string())?);
    let mut slot = SLOT.lock().unwrap_or_else(|p| p.into_inner());
    let generation = GENERATION.load(Ordering::SeqCst);
    if let Some(e) = slot.as_ref().filter(|e| e.key == key && e.generation == generation && e.created.elapsed() < MAX_AGE) {
        return Ok(e.index.clone());
    }
    let index = Arc::new(load_index(cfg, today).map_err(|e| e.to_string())?);
    // Generation vom Beginn des Scans: Kam währenddessen eine Änderung, gilt der Eintrag nicht.
    *slot = Some(Entry { key, generation, created: Instant::now(), index: index.clone() });
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_until_invalidated() {
        let cfg = crate::demo_config().expect("Demo-Vault im Debug-Build");
        let day = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        let a = get(&cfg, day).unwrap();
        assert!(Arc::ptr_eq(&a, &get(&cfg, day).unwrap()), "zweiter Aufruf aus dem Zwischenspeicher");
        invalidate();
        assert!(!Arc::ptr_eq(&a, &get(&cfg, day).unwrap()), "nach Änderung neu eingelesen");
        let other = day.succ_opt().unwrap();
        let b = get(&cfg, day).unwrap();
        assert!(!Arc::ptr_eq(&b, &get(&cfg, other).unwrap()), "anderer Tag, anderer Index");
    }
}
