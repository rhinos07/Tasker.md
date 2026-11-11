//! Mit dem Betriebssystem starten (Windows: Registry-Eintrag „Run“ des Nutzers, macOS: LaunchAgent,
//! Linux: Autostart-Eintrag).
//!
//! Der Zustand liegt nur im Betriebssystem, nicht in der Konfiguration. So kann er nicht
//! auseinanderlaufen, wenn jemand den Eintrag dort abschaltet (z. B. im Task-Manager).
//! Der Autostart übergibt `--minimized`: Die App startet versteckt im Infobereich.
//! Unter Windows nur für den aktuellen Nutzer, auch wenn die App mit Adminrechten läuft.

use auto_launch::{AutoLaunch, AutoLaunchBuilder, WindowsEnableMode};
#[cfg(target_os = "linux")]
use tauri::Manager;

/// Argument für den versteckten Start.
pub const MINIMIZED: &str = "--minimized";

/// Wurde die App versteckt gestartet?
pub fn start_minimized(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().any(|a| a == MINIMIZED)
}

fn launcher(app: &tauri::AppHandle) -> Result<AutoLaunch, String> {
    let exe = std::env::current_exe().map_err(|_| "Programmpfad unbekannt.".to_string())?;
    #[allow(unused_mut)]
    let mut path = exe.display().to_string();
    // macOS: LaunchAgent (Standard) startet die ausführbare Datei im Bündel direkt
    // Linux: bei AppImage die Image-Datei, nicht den entpackten Pfad
    #[cfg(target_os = "linux")]
    if let Some(img) = app.env().appimage.and_then(|p| p.to_str().map(String::from)) {
        path = img;
    }
    AutoLaunchBuilder::new()
        .set_app_name(&app.package_info().name)
        .set_app_path(&path)
        .set_args(&[MINIMIZED])
        .set_windows_enable_mode(WindowsEnableMode::CurrentUser)
        .build()
        .map_err(|e| format!("Autostart nicht verfügbar: {e}"))
}

/// Windows merkt sich zusätzlich „im Task-Manager aktiviert“; beim Ausschalten mit entfernen,
/// damit nichts zurückbleibt.
#[cfg(windows)]
fn forget_task_manager_flag(name: &str) {
    const KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
    if let Ok(k) = windows_registry::CURRENT_USER.options().write().open(KEY) {
        let _ = k.remove_value(name);
    }
}

#[tauri::command]
pub fn autostart_status(app: tauri::AppHandle) -> Result<bool, String> {
    launcher(&app)?.is_enabled().map_err(|e| format!("Autostart nicht lesbar: {e}"))
}

#[tauri::command]
pub fn autostart_set(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    let al = launcher(&app)?;
    if enabled {
        al.enable().map_err(|e| format!("Autostart nicht einschaltbar: {e}"))?;
    } else {
        al.disable().map_err(|e| format!("Autostart nicht ausschaltbar: {e}"))?;
        #[cfg(windows)]
        forget_task_manager_flag(al.get_app_name());
    }
    al.is_enabled().map_err(|e| format!("Autostart nicht lesbar: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_minimized_flag() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(start_minimized(a(&["tasker-md.exe", "--minimized"])));
        assert!(!start_minimized(a(&["tasker-md.exe"])));
        assert!(!start_minimized(a(&["tasker-md.exe", "--minimized=no"])));
    }
}
