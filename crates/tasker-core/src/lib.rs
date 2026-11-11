//! Kern von Tasker.md: liest einen Obsidian-Vault und berechnet die GTD-Ansichten.
//! Ohne UI- und Netzwerk-Abhängigkeiten, damit alles gegen den Demo-Vault testbar ist.

pub mod config;
pub mod edit;
pub mod index;
pub mod jira;
pub mod task;
pub mod vault;

pub use config::Config;
pub use index::{Dashboard, Index, Scope};

use chrono::NaiveDate;

/// Scannt den Vault und baut den Index.
pub fn load_index(cfg: &Config, today: NaiveDate) -> Result<Index, vault::ScanError> {
    load_index_with_keys(cfg, today, std::iter::empty())
}

/// Wie [`load_index`], erkennt zusätzlich alle Schlüssel aus den Jira-Ergebnissen als Tickets,
/// auch wenn ihr Präfix nicht in `jira.projectKeys` steht.
pub fn load_index_with_keys(
    cfg: &Config,
    today: NaiveDate,
    known_keys: impl IntoIterator<Item = String>,
) -> Result<Index, vault::ScanError> {
    let mut filter = jira::KeyFilter::new(cfg.jira.project_keys.iter().cloned());
    filter.known_keys.extend(known_keys);
    let notes = vault::scan_vault(cfg, &filter)?;
    Ok(Index::build(&notes, cfg, today))
}
