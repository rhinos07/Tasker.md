//! „In Obsidian öffnen“ (SPEC 6.1): Link auf eine Aufgabe erzeugen.
//!
//! Mit dem Plugin „Advanced URI“ springt der Link genau auf die Zeile. Ob es installiert und aktiv
//! ist, prüft die App nur lesend; in `.obsidian/` wird nie etwas geändert.

use crate::Config;
use serde::Serialize;
use std::path::Path;

const PLUGIN_ID: &str = "obsidian-advanced-uri";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ObsidianLink {
    pub url: String,
    /// `true` = Sprung auf die Zeile (Advanced URI), sonst nur die Notiz
    pub exact: bool,
}

/// Ist Advanced URI im Vault installiert und eingeschaltet? Liest nur.
pub fn advanced_uri_available(root: &Path) -> bool {
    let dir = root.join(".obsidian");
    let enabled = std::fs::read_to_string(dir.join("community-plugins.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .is_some_and(|ids| ids.iter().any(|i| i == PLUGIN_ID));
    enabled && dir.join("plugins").join(PLUGIN_ID).is_dir()
}

/// Prozent-Kodierung wie `encodeURIComponent`.
fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Link auf die Aufgabe in Zeile `line` (0-basiert) der Notiz `rel`.
pub fn link(cfg: &Config, rel: &str, line: usize) -> ObsidianLink {
    let vault = enc(&cfg.vault.name);
    let use_adv = cfg.obsidian.use_advanced_uri != "never" && advanced_uri_available(&cfg.vault.path);
    if use_adv {
        ObsidianLink { url: format!("obsidian://adv-uri?vault={vault}&filepath={}&line={}", enc(rel), line + 1), exact: true }
    } else {
        ObsidianLink { url: format!("obsidian://open?vault={vault}&file={}", enc(rel)), exact: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_like_encode_uri_component() {
        assert_eq!(enc("10 Projekte/Gartenhaus/Gartenhaus.md"), "10%20Projekte%2FGartenhaus%2FGartenhaus.md");
        assert_eq!(enc("Größe & Co"), "Gr%C3%B6%C3%9Fe%20%26%20Co");
    }
}
