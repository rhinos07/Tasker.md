//! Jira-Client (nur lesen, SPEC 4): E-Mail plus API-Token, Token im OS-Schlüsselbund,
//! Ergebnisse im lokalen Cache. Fehlermeldungen enthalten nie Antwortinhalte oder Tokens (SPEC 9).

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tasker_core::config::JiraConfig;
use tasker_core::jira::{parse_search_response, JiraIssue};

/// API-Gateway für Tokens mit Bereichen (scoped tokens).
pub const ATLASSIAN_GATEWAY: &str = "https://api.atlassian.com";
const KEYRING_SERVICE: &str = "md.tasker.app";
const KEYRING_USER: &str = "jira-api-token";
const FIELDS: &str = "summary,status,issuetype,priority,updated";
const PAGE_SIZE: &str = "100";
const MAX_PAGES: usize = 10;

pub struct Client {
    http: reqwest::Client,
    api_base: String,
    browse_base: String,
    email: String,
    token: String,
}

fn status_message(code: reqwest::StatusCode) -> String {
    match code.as_u16() {
        401 => "Jira: Anmeldung fehlgeschlagen. E-Mail oder Token prüfen, eventuell ist der Token abgelaufen.".into(),
        403 => "Jira: Keine Berechtigung. Hat der Token die nötigen Bereiche (z. B. read:jira-work)?".into(),
        404 => "Jira: Adresse nicht gefunden. Site-URL und Token-Typ prüfen.".into(),
        429 => "Jira: Zu viele Anfragen, bitte später erneut versuchen.".into(),
        c => format!("Jira: Unerwartete Antwort (HTTP {c})."),
    }
}

fn net_error(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "Jira: Zeitüberschreitung.".into()
    } else if e.is_connect() {
        "Jira: Keine Verbindung. Netzwerk oder VPN prüfen.".into()
    } else {
        "Jira: Anfrage fehlgeschlagen.".into()
    }
}

impl Client {
    /// Baut den Client aus der Konfiguration (prüft, dass Jira eingerichtet ist).
    pub async fn connect(cfg: &JiraConfig, token: String) -> Result<Self, String> {
        if !cfg.is_configured() {
            return Err("Jira ist nicht eingerichtet (Site-URL und E-Mail fehlen).".into());
        }
        Self::connect_to(&cfg.base_url, &cfg.email, token, cfg.token_type == "classic", ATLASSIAN_GATEWAY).await
    }

    /// Bei Tokens mit Bereichen wird einmalig die Cloud-ID über `<site>/_edge/tenant_info` ermittelt
    /// und das API-Gateway verwendet, bei klassischen Tokens direkt die Site.
    async fn connect_to(site: &str, email: &str, token: String, classic: bool, gateway: &str) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("Tasker.md")
            .build()
            .map_err(|_| "Jira: HTTP-Client nicht verfügbar.".to_string())?;
        let site = site.trim_end_matches('/').to_string();
        let api_base = if classic {
            site.clone()
        } else {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Tenant {
                cloud_id: String,
            }
            let r = http.get(format!("{site}/_edge/tenant_info")).send().await.map_err(net_error)?;
            if !r.status().is_success() {
                return Err(status_message(r.status()));
            }
            let t: Tenant = r.json().await.map_err(|_| "Jira: Cloud-ID nicht lesbar.".to_string())?;
            format!("{}/ex/jira/{}", gateway.trim_end_matches('/'), t.cloud_id)
        };
        Ok(Self { http, api_base, browse_base: site, email: email.to_string(), token })
    }

    async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<serde_json::Value, String> {
        let r = self
            .http
            .get(format!("{}{path}", self.api_base))
            .basic_auth(&self.email, Some(&self.token))
            .header("Accept", "application/json")
            .query(query)
            .send()
            .await
            .map_err(net_error)?;
        if !r.status().is_success() {
            return Err(status_message(r.status()));
        }
        r.json().await.map_err(|_| "Jira: Antwort nicht lesbar.".to_string())
    }

    /// Anzeigename des angemeldeten Nutzers (Verbindungstest).
    pub async fn myself(&self) -> Result<String, String> {
        let v = self.get("/rest/api/3/myself", &[]).await?;
        Ok(v.get("displayName").and_then(|n| n.as_str()).unwrap_or("?").to_string())
    }

    /// Führt die JQL-Suche aus, mit Blättern über `nextPageToken`.
    pub async fn search(&self, jql: &str) -> Result<Vec<JiraIssue>, String> {
        let mut all = vec![];
        let mut token: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut q = vec![("jql", jql), ("fields", FIELDS), ("maxResults", PAGE_SIZE)];
            if let Some(t) = token.as_deref() {
                q.push(("nextPageToken", t));
            }
            let v = self.get("/rest/api/3/search/jql", &q).await?;
            let (issues, next) = parse_search_response(&v, &self.browse_base);
            all.extend(issues);
            match next {
                Some(n) => token = Some(n),
                None => break,
            }
        }
        Ok(all)
    }
}

// --- Token im Schlüsselbund ---

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|_| "Schlüsselbund nicht verfügbar.".into())
}

pub fn load_token() -> Option<String> {
    entry().ok()?.get_password().ok().filter(|t| !t.is_empty())
}

pub fn store_token(token: &str) -> Result<(), String> {
    entry()?.set_password(token).map_err(|_| "Token konnte nicht im Schlüsselbund gespeichert werden.".into())
}

pub fn delete_token() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("Token konnte nicht gelöscht werden.".into()),
    }
}

// --- Cache ---

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JiraCache {
    /// Zeitpunkt des Abrufs (RFC 3339, lokale Zeit)
    pub fetched_at: Option<String>,
    pub user: Option<String>,
    pub issues: Vec<JiraIssue>,
}

pub fn read_cache(path: &Path) -> JiraCache {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn write_cache(path: &Path, cache: &JiraCache) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|_| "Cache-Ordner nicht anlegbar.".to_string())?;
    }
    let text = serde_json::to_string_pretty(cache).map_err(|_| "Cache nicht schreibbar.".to_string())?;
    std::fs::write(path, text).map_err(|_| "Cache nicht schreibbar.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// Minimaler HTTP-Server für Tests: antwortet je nach Pfad und merkt sich die Anfragen.
    fn mock_server(pages: usize) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(vec![]));
        let seen2 = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                seen2.lock().unwrap().push(request.clone());
                let first = request.lines().next().unwrap_or("").to_string();
                let auth_ok = request.contains("authorization: Basic ") || request.contains("Authorization: Basic ");
                let (code, body) = if first.contains("/_edge/tenant_info") {
                    (200, r#"{"cloudId":"cid-1"}"#.to_string())
                } else if !auth_ok {
                    (401, "{}".into())
                } else if first.contains("/rest/api/3/myself") {
                    (200, r#"{"displayName":"Test Nutzer"}"#.into())
                } else if first.contains("/rest/api/3/search/jql") {
                    let page2 = first.contains("nextPageToken=p2");
                    let key = if page2 { "ABC-2" } else { "ABC-1" };
                    let last = page2 || pages == 1;
                    (200, format!(
                        r#"{{"issues":[{{"key":"{key}","fields":{{"summary":"S","status":{{"name":"Offen","statusCategory":{{"key":"new"}}}},"issuetype":{{"name":"Task"}},"priority":{{"name":"High"}},"updated":"2026-11-01T00:00:00.000+0000"}}}}],"isLast":{last},"nextPageToken":"p2"}}"#
                    ))
                } else {
                    (404, "{}".into())
                };
                let mut s = stream;
                let _ = write!(s, "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        (base, seen)
    }

    #[test]
    fn scoped_token_uses_gateway_and_pages() {
        let (base, seen) = mock_server(2);
        tauri::async_runtime::block_on(async {
            let client = Client::connect_to(&base, "nutzer@example.com", "tok".into(), false, &base).await.unwrap();
            assert_eq!(client.api_base, format!("{base}/ex/jira/cid-1"));
            assert_eq!(client.myself().await.unwrap(), "Test Nutzer");
            let issues = client.search("assignee = currentUser()").await.unwrap();
            assert_eq!(issues.iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), vec!["ABC-1", "ABC-2"]);
            assert_eq!(issues[0].url, format!("{base}/browse/ABC-1"));
        });
        let reqs = seen.lock().unwrap();
        let search: Vec<_> = reqs.iter().filter(|r| r.contains("/search/jql")).collect();
        assert_eq!(search.len(), 2);
        assert!(search[0].contains("/ex/jira/cid-1/rest/api/3/search/jql?jql=assignee"));
        assert!(search[0].contains("fields=summary%2Cstatus"));
        assert!(search[1].contains("nextPageToken=p2"));
        assert!(!reqs.iter().any(|r| r.contains("tok")), "Token nie im Klartext in der URL");
    }

    #[test]
    fn classic_token_uses_site_and_maps_errors() {
        let (base, _) = mock_server(1);
        tauri::async_runtime::block_on(async {
            let client = Client::connect_to(&base, "nutzer@example.com", "tok".into(), true, "http://unbenutzt").await.unwrap();
            assert_eq!(client.api_base, base);
            let err = client.get("/gibt/es/nicht", &[]).await.unwrap_err();
            assert!(err.contains("nicht gefunden"), "{err}");
        });
        assert!(status_message(reqwest::StatusCode::UNAUTHORIZED).contains("Anmeldung fehlgeschlagen"));
    }

    #[test]
    fn rejects_unconfigured() {
        let c = JiraConfig { base_url: "https://<firma>.atlassian.net".into(), ..JiraConfig::default() };
        assert!(tauri::async_runtime::block_on(Client::connect(&c, "t".into())).is_err());
    }

    #[test]
    fn cache_roundtrip() {
        let p = std::env::temp_dir().join(format!("tasker-jira-cache-{}.json", std::process::id()));
        let c = JiraCache { fetched_at: Some("2026-11-11T10:00:00+01:00".into()), user: None, issues: vec![] };
        write_cache(&p, &c).unwrap();
        assert_eq!(read_cache(&p).fetched_at, c.fetched_at);
        std::fs::remove_file(&p).unwrap();
        assert!(read_cache(&p).fetched_at.is_none(), "fehlender Cache ist leer");
    }
}
