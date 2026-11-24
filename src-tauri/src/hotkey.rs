//! Globales Tastenkürzel (SPEC 6.6): öffnet die Schnellerfassung als kleines Fenster,
//! auch wenn die App im Hintergrund ist.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

pub const QUICK_LABEL: &str = "quick";

/// Startparameter: öffnet nur die Schnellerfassung. Für Systeme, auf denen das globale Kürzel
/// nicht greift (Wayland, z. B. Hyprland/Omarchy): dort bindet man `tasker-md --quick` an eine Taste.
/// Läuft die App schon, reicht der zweite Start den Parameter an die laufende Instanz weiter.
pub const QUICK_ARG: &str = "--quick";

/// Wurde die App mit `--quick` gestartet?
pub fn wants_quick<S: AsRef<str>>(args: impl IntoIterator<Item = S>) -> bool {
    args.into_iter().any(|a| a.as_ref() == QUICK_ARG)
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    /// eingetragenes Kürzel, leer = aus
    shortcut: String,
    active: bool,
    error: Option<String>,
}

#[derive(Default)]
pub struct HotkeyState(Mutex<HotkeyStatus>);

/// Plugin mit Handler: Jedes registrierte Kürzel öffnet die Schnellerfassung.
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                show_quick(app);
            }
        })
        .build()
}

/// Zeigt das Fenster der Schnellerfassung (legt es bei Bedarf an).
///
/// Läuft in einem eigenen Task: Ein Fenster aus dem Haupt-Thread heraus anzulegen (Kürzel-Handler,
/// synchroner Befehl) blockiert unter Windows die Ereignisschleife.
pub fn show_quick(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { open_quick(&app) });
}

fn open_quick(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(QUICK_LABEL) {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, QUICK_LABEL, WebviewUrl::App("index.html#quick".into()))
        .title("Neue Aufgabe – Tasker.md")
        .inner_size(680.0, 290.0)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .center()
        .focused(true)
        .build();
}

/// Läuft die Sitzung unter Wayland (z. B. Hyprland)?
fn is_wayland() -> bool {
    cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
}

/// Registriert das Kürzel aus der Konfiguration neu (nach Start und nach dem Speichern der Einstellungen).
pub fn apply(app: &AppHandle) -> HotkeyStatus {
    let shortcut = crate::resolve_config(app, false).map(|(c, _)| c.hotkeys.quick_capture).unwrap_or_default();
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let status = if shortcut.trim().is_empty() || is_wayland() {
        // Unter Wayland kommen globale Kürzel nur bei X11-Fenstern an, also praktisch nie.
        // Stattdessen bindet man dort `tasker-md --quick` im Compositor an eine Taste.
        HotkeyStatus { shortcut, active: false, error: None }
    } else {
        match gs.register(shortcut.trim()) {
            Ok(()) => HotkeyStatus { shortcut, active: true, error: None },
            Err(e) => HotkeyStatus {
                error: Some(format!("Kürzel nicht verfügbar ({e}). Vielleicht belegt es schon eine andere App.")),
                shortcut,
                active: false,
            },
        }
    };
    if let Some(s) = app.try_state::<HotkeyState>() {
        *s.0.lock().unwrap_or_else(|p| p.into_inner()) = status.clone();
    }
    status
}

#[tauri::command]
pub fn hotkey_status(state: tauri::State<'_, HotkeyState>) -> HotkeyStatus {
    state.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

#[tauri::command]
pub async fn hotkey_apply(app: AppHandle) -> HotkeyStatus {
    apply(&app)
}

/// Öffnet die Schnellerfassung auch über die Oberfläche (zum Ausprobieren).
#[tauri::command]
pub async fn quick_window(app: AppHandle) {
    show_quick(&app);
}
