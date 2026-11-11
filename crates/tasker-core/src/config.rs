//! Konfiguration (SPEC 11). Alle persönlichen Werte kommen von hier, nichts ist im Code fest verdrahtet.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub vault: VaultConfig,
    pub folders: Folders,
    pub scan: ScanConfig,
    pub sections: Sections,
    pub frontmatter: FrontmatterKeys,
    pub projects: ProjectsConfig,
    pub areas: AreasConfig,
    pub contexts: Vec<String>,
    pub waiting_tag: String,
    pub jira: JiraConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct VaultConfig {
    pub path: PathBuf,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Folders {
    pub inbox: String,
    pub projects: String,
    pub areas: String,
    pub knowledge: String,
    pub archive: String,
    pub journal: String,
    pub templates: String,
}

impl Default for Folders {
    fn default() -> Self {
        Self {
            inbox: "00 Inbox".into(),
            projects: "10 Projekte".into(),
            areas: "20 Bereiche".into(),
            knowledge: "30 Wissen".into(),
            archive: "40 Archiv".into(),
            journal: "Journal".into(),
            templates: "Vorlagen".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScanConfig {
    pub exclude: Vec<String>,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            exclude: vec![
                "Vorlagen/**".into(),
                "40 Archiv/**".into(),
                "30 Wissen/**".into(),
                ".obsidian/**".into(),
                ".claude/**".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Sections {
    pub project_tasks: String,
    pub area_tasks: String,
}

impl Default for Sections {
    fn default() -> Self {
        Self {
            project_tasks: "## Nächste Schritte".into(),
            area_tasks: "## Aufgaben ohne Projekt".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FrontmatterKeys {
    #[serde(rename = "type")]
    pub type_key: String,
    pub project_type: String,
    pub area_type: String,
    pub status: String,
    pub area: String,
    pub review: String,
    pub deadline: String,
    pub jira: String,
}

impl Default for FrontmatterKeys {
    fn default() -> Self {
        Self {
            type_key: "typ".into(),
            project_type: "projekt".into(),
            area_type: "bereich".into(),
            status: "status".into(),
            area: "bereich".into(),
            review: "review".into(),
            deadline: "termin".into(),
            jira: "jira".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProjectsConfig {
    pub review_after_days: i64,
}

impl Default for ProjectsConfig {
    fn default() -> Self {
        Self { review_after_days: 14 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AreasConfig {
    pub work: Vec<String>,
    pub work_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct JiraConfig {
    /// z. B. `https://<firma>.atlassian.net`, leer = Jira aus
    pub base_url: String,
    pub email: String,
    pub jql: String,
    pub project_keys: Vec<String>,
    pub refresh_minutes: u32,
    /// `scoped` (Token mit Bereichen, über api.atlassian.com) oder `classic`
    pub token_type: String,
}

pub const DEFAULT_JQL: &str =
    "assignee = currentUser() AND statusCategory != Done ORDER BY priority DESC, updated DESC";

impl Default for JiraConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            email: String::new(),
            jql: DEFAULT_JQL.into(),
            project_keys: vec![],
            refresh_minutes: 15,
            token_type: "scoped".into(),
        }
    }
}

impl JiraConfig {
    pub fn is_configured(&self) -> bool {
        self.base_url.starts_with("https://") && !self.base_url.contains('<') && !self.email.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Konfiguration nicht lesbar: {0}")]
    Io(#[from] std::io::Error),
    #[error("Konfiguration fehlerhaft: {0}")]
    Json(#[from] serde_json::Error),
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        let mut cfg: Config = serde_json::from_str(&text)?;
        if cfg.waiting_tag.is_empty() {
            cfg.waiting_tag = "#warten".into();
        }
        Ok(cfg)
    }

    /// Standardkonfiguration für einen Vault-Pfad (z. B. den Demo-Vault).
    pub fn for_vault(path: impl Into<PathBuf>) -> Self {
        Self {
            vault: VaultConfig { path: path.into(), name: "demo-vault".into() },
            folders: Folders::default(),
            scan: ScanConfig::default(),
            sections: Sections::default(),
            frontmatter: FrontmatterKeys::default(),
            projects: ProjectsConfig::default(),
            areas: AreasConfig {
                work: vec!["Arbeit".into()],
                work_paths: vec!["10 Projekte/Arbeit/".into(), "20 Bereiche/Arbeit/".into()],
            },
            contexts: ["#@computer", "#@telefon", "#@haus", "#@unterwegs", "#@büro"]
                .map(String::from)
                .to_vec(),
            waiting_tag: "#warten".into(),
            jira: JiraConfig { project_keys: vec!["ABC".into()], ..JiraConfig::default() },
        }
    }
}
