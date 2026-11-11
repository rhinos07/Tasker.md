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
    let filter = jira::KeyFilter::new(cfg.jira.project_keys.iter().cloned());
    let notes = vault::scan_vault(cfg, &filter)?;
    Ok(Index::build(&notes, cfg, today))
}
