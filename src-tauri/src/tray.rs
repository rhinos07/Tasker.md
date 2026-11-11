//! Symbol im Infobereich (Tray): zeigt den laufenden Fokus-Timer und steuert ihn.
//!
//! Der Timer selbst läuft in der Oberfläche. Sie meldet ihren Stand über `focus_tray`;
//! Klicks im Tray-Menü gehen als Ereignis `tray-focus` (`toggle` | `stop`) zurück.

use serde::Deserialize;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

const IDLE: &str = "Kein Fokus-Timer aktiv";

pub struct TrayState {
    tray: TrayIcon<Wry>,
    status: MenuItem<Wry>,
    toggle: MenuItem<Wry>,
    stop: MenuItem<Wry>,
    /// zuletzt gesetzter Tooltip, um unnötige Aktualisierungen zu sparen
    last: Mutex<String>,
}

/// Stand des Timers aus der Oberfläche.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayFocus {
    /// angezeigte Zeit, z. B. `18:23` (Restzeit beim Pomodoro, sonst gelaufene Zeit)
    time: String,
    /// Aufgabe oder „Pause“
    label: String,
    running: bool,
    /// `work` | `break`
    phase: String,
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>().trim_end())
    }
}

/// Hauptfenster zeigen und nach vorn holen.
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", IDLE, false, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "focus-toggle", "Pause", false, None::<&str>)?;
    let stop = MenuItem::with_id(app, "focus-stop", "Stopp und verbuchen", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Tasker.md öffnen", true, None::<&str>)?;
    let quick = MenuItem::with_id(app, "quick", "Neue Aufgabe …", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Beenden", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &toggle,
            &stop,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &quick,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Tasker.md")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "focus-toggle" => {
                let _ = app.emit_to("main", "tray-focus", "toggle");
            }
            "focus-stop" => {
                let _ = app.emit_to("main", "tray-focus", "stop");
            }
            "open" => show_main(app),
            "quick" => crate::hotkey::show_quick(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder.build(app)?;

    app.manage(TrayState { tray, status, toggle, stop, last: Mutex::new(String::new()) });
    Ok(())
}

/// Tooltip und Menü an den Stand des Timers anpassen. `None` = kein Timer.
///
/// Async, damit die Menü-Aufrufe nicht im Haupt-Thread warten (vgl. `hotkey::show_quick`).
#[tauri::command]
pub async fn focus_tray(app: AppHandle, focus: Option<TrayFocus>) -> Result<(), String> {
    let Some(st) = app.try_state::<TrayState>() else {
        return Ok(()); // kein Tray (z. B. Plattform ohne Infobereich)
    };
    let tooltip = match &focus {
        None => "Tasker.md".to_string(),
        Some(f) => format!("{}{} · {} – Tasker.md", if f.running { "⏱ " } else { "⏸ " }, f.time, short(&f.label, 60)),
    };
    {
        let mut last = st.last.lock().unwrap_or_else(|p| p.into_inner());
        if *last == tooltip {
            return Ok(());
        }
        *last = tooltip.clone();
    }
    let err = |e: tauri::Error| e.to_string();
    st.tray.set_tooltip(Some(&tooltip)).map_err(err)?;
    match &focus {
        None => {
            // macOS: Zeit neben dem Symbol in der Menüleiste; Windows ignoriert den Titel
            st.tray.set_title(None::<&str>).map_err(err)?;
            st.status.set_text(IDLE).map_err(err)?;
            st.toggle.set_text("Pause").map_err(err)?;
            st.toggle.set_enabled(false).map_err(err)?;
            st.stop.set_text("Stopp und verbuchen").map_err(err)?;
            st.stop.set_enabled(false).map_err(err)?;
        }
        Some(f) => {
            st.tray.set_title(Some(&f.time)).map_err(err)?;
            st.status.set_text(format!("{} · {}", f.time, short(&f.label, 40))).map_err(err)?;
            st.toggle.set_text(if f.running { "Pause" } else { "Weiter" }).map_err(err)?;
            st.toggle.set_enabled(true).map_err(err)?;
            st.stop.set_text(if f.phase == "break" { "Pause beenden" } else { "Stopp und verbuchen" }).map_err(err)?;
            st.stop.set_enabled(true).map_err(err)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::short;

    #[test]
    fn shortens_long_labels() {
        assert_eq!(short("kurz", 10), "kurz");
        assert_eq!(short("Präsentation fertigstellen", 13), "Präsentation…");
    }
}
