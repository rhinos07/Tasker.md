//! Einstellungen: liest und ändert `config.json` im App-Datenordner (SPEC 9, 11).
//! Geändert werden nur die Schlüssel, die die Oberfläche schickt; unbekannte Schlüssel und die
//! Reihenfolge bleiben erhalten. Fehlt die Datei, wird sie beim ersten Speichern angelegt.

use crate::config_path;
use serde::Serialize;
use serde_json::Value;
use tasker_core::today::parse_hm;
use tasker_core::Config;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsData {
    path: Option<String>,
    exists: bool,
    config: Config,
}

fn defaults() -> Config {
    Config { waiting_tag: "#warten".into(), ..Config::default() }
}

#[tauri::command]
pub fn get_settings(app: tauri::AppHandle) -> Result<SettingsData, String> {
    let path = config_path(&app);
    let exists = path.as_ref().is_some_and(|p| p.is_file());
    let config = match path.as_ref().filter(|_| exists) {
        Some(p) => Config::load(p).map_err(|e| e.to_string())?,
        None => defaults(),
    };
    Ok(SettingsData { path: path.map(|p| p.display().to_string()), exists, config })
}

/// Tiefes Zusammenführen: Objekte werden gemischt, alles andere ersetzt.
fn merge(dst: &mut Value, patch: &Value) {
    match (dst, patch) {
        (Value::Object(d), Value::Object(p)) => {
            for (k, v) in p {
                match d.get_mut(k) {
                    Some(existing) if existing.is_object() && v.is_object() => merge(existing, v),
                    _ => {
                        d.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (d, p) => *d = p.clone(),
    }
}

/// Prüft die zusammengeführte Konfiguration. Meldungen ohne private Werte.
fn validate(cfg: &Config) -> Result<(), String> {
    if cfg.vault.path.as_os_str().is_empty() {
        return Err("Bitte den Vault-Ordner wählen.".into());
    }
    if !cfg.vault.path.is_dir() {
        return Err("Der Vault-Ordner existiert nicht.".into());
    }
    let (s, e) = (parse_hm(&cfg.timeline.start), parse_hm(&cfg.timeline.end));
    match (s, e) {
        (Some(s), Some(e)) if s < e => {}
        _ => return Err("Zeitleiste: Beginn und Ende als HH:MM, Beginn vor Ende.".into()),
    }
    if !(5..=480).contains(&cfg.timeline.default_block_minutes) {
        return Err("Zeitleiste: Standard-Blocklänge zwischen 5 und 480 Minuten.".into());
    }
    let p = &cfg.focus.pomodoro;
    if !(1..=180).contains(&p.work) || p.pause > 60 {
        return Err("Pomodoro: Arbeit 1–180 min, Pause 0–60 min.".into());
    }
    if !cfg.focus.log_section.trim_start().starts_with('#') {
        return Err("Log-Überschrift muss mit # beginnen, z. B. „## Log“.".into());
    }
    for k in ["{date}", "{minutes}", "{task}"] {
        if !cfg.focus.log_format.contains(k) {
            return Err(format!("Log-Format braucht den Platzhalter {k}."));
        }
    }
    let url = cfg.jira.base_url.trim();
    if !url.is_empty() && !url.starts_with("https://") {
        return Err("Jira-Adresse muss mit https:// beginnen.".into());
    }
    if cfg.jira.refresh_minutes == 0 || cfg.calendar.refresh_minutes == 0 {
        return Err("Aktualisierungsintervalle müssen mindestens 1 Minute sein.".into());
    }
    if !(1..=60).contains(&cfg.calendar.days_ahead) {
        return Err("Kalender: 1 bis 60 Tage im Voraus.".into());
    }
    if cfg.projects.review_after_days < 1 {
        return Err("Review-Intervall muss mindestens 1 Tag sein.".into());
    }
    Ok(())
}

/// Wendet die Änderungen auf den vorhandenen Text (oder die Standardwerte) an und gibt den neuen Text zurück.
fn apply_patch(existing: Option<&str>, patch: &Value) -> Result<String, String> {
    let mut v: Value = match existing {
        Some(t) => serde_json::from_str(t).map_err(|_| "Die vorhandene Konfiguration ist kein gültiges JSON.".to_string())?,
        None => serde_json::to_value(defaults()).map_err(|_| "Standardwerte nicht verfügbar.".to_string())?,
    };
    if !v.is_object() || !patch.is_object() {
        return Err("Konfiguration hat eine unerwartete Struktur.".into());
    }
    merge(&mut v, patch);
    let cfg: Config = serde_json::from_value(v.clone()).map_err(|e| format!("Ungültiger Wert: {e}"))?;
    validate(&cfg)?;
    Ok(serde_json::to_string_pretty(&v).map_err(|_| "Konfiguration nicht schreibbar.".to_string())? + "\n")
}

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, patch: Value) -> Result<(), String> {
    let path = config_path(&app).ok_or("App-Datenordner unbekannt.")?;
    let existing = path.is_file().then(|| std::fs::read_to_string(&path)).transpose().map_err(|_| "Konfiguration nicht lesbar.".to_string())?;
    let out = apply_patch(existing.as_deref(), &patch)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|_| "App-Datenordner nicht anlegbar.".to_string())?;
    }
    tasker_core::edit::write_atomic(&path, &out).map_err(|_| "Konfiguration nicht schreibbar.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn vault() -> String {
        std::env::temp_dir().display().to_string()
    }

    #[test]
    fn keeps_unknown_keys_and_order() {
        let existing = json!({
            "vault": { "path": vault(), "name": "V" },
            "eigenes": { "x": 1 },
            "focus": { "pomodoro": { "work": 25, "break": 5 }, "logSection": "## Log" }
        })
        .to_string();
        let out = apply_patch(Some(&existing), &json!({ "focus": { "pomodoro": { "work": 50 } } })).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["focus"]["pomodoro"], json!({ "work": 50, "break": 5 }));
        assert_eq!(v["eigenes"], json!({ "x": 1 }));
        assert_eq!(v["focus"]["logSection"], "## Log");
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["vault", "eigenes", "focus"], "Reihenfolge bleibt");
    }

    #[test]
    fn creates_from_defaults() {
        let out = apply_patch(None, &json!({ "vault": { "path": vault(), "name": "V" } })).unwrap();
        let cfg: Config = serde_json::from_str(&out).unwrap();
        assert_eq!(cfg.vault.name, "V");
        assert_eq!(cfg.focus.pomodoro.work, 25);
        assert_eq!(cfg.waiting_tag, "#warten");
    }

    #[test]
    fn rejects_invalid_values() {
        let base = json!({ "vault": { "path": vault() } }).to_string();
        let bad = |p: Value| apply_patch(Some(&base), &p).unwrap_err();
        assert!(bad(json!({ "vault": { "path": "" } })).contains("Vault"));
        assert!(bad(json!({ "timeline": { "start": "20:00", "end": "07:00" } })).contains("Zeitleiste"));
        assert!(bad(json!({ "jira": { "baseUrl": "http://x" } })).contains("https"));
        assert!(bad(json!({ "focus": { "logFormat": "{date}" } })).contains("Platzhalter"));
        assert!(bad(json!({ "focus": { "pomodoro": { "work": "viel" } } })).contains("Ungültiger Wert"));
    }
}
