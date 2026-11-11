//! Google Kalender, nur lesen (SPEC 7).
//!
//! Anmeldung: OAuth 2.0 für Desktop-Apps mit Loopback-Redirect und PKCE, Scope `calendar.readonly`.
//! Client-ID und -Secret stammen aus dem eigenen Google-Cloud-Projekt des Nutzers.
//! Client-Daten und Refresh-Token liegen nur im OS-Schlüsselbund. Fehlermeldungen enthalten
//! nie Tokens oder Antwortinhalte (SPEC 9).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tasker_core::today::CalendarEvent;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const API: &str = "https://www.googleapis.com/calendar/v3";
const SCOPE: &str = "https://www.googleapis.com/auth/calendar.readonly";
const KEYRING_SERVICE: &str = "md.tasker.app";
const KEY_CLIENT: &str = "google-oauth-client";
const KEY_REFRESH: &str = "google-refresh-token";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);

// --- Schlüsselbund ---

fn entry(user: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, user).map_err(|_| "Schlüsselbund nicht verfügbar.".into())
}

pub(crate) fn get_secret(user: &str) -> Option<String> {
    entry(user).ok()?.get_password().ok().filter(|s| !s.is_empty())
}

pub(crate) fn set_secret(user: &str, value: &str) -> Result<(), String> {
    entry(user)?.set_password(value).map_err(|_| "Konnte nicht im Schlüsselbund speichern.".into())
}

pub(crate) fn delete_secret(user: &str) -> Result<(), String> {
    match entry(user)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("Eintrag im Schlüsselbund konnte nicht gelöscht werden.".into()),
    }
}

#[derive(Serialize, Deserialize)]
pub struct OAuthClient {
    pub id: String,
    pub secret: String,
}

pub fn load_client() -> Option<OAuthClient> {
    get_secret(KEY_CLIENT).and_then(|s| serde_json::from_str(&s).ok())
}

pub fn store_client(c: &OAuthClient) -> Result<(), String> {
    set_secret(KEY_CLIENT, &serde_json::to_string(c).map_err(|_| "Ungültige Client-Daten.".to_string())?)
}

pub fn has_refresh_token() -> bool {
    get_secret(KEY_REFRESH).is_some()
}

// --- Fehler ---

fn net_error(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "Google: Zeitüberschreitung.".into()
    } else if e.is_connect() {
        "Google: Keine Verbindung.".into()
    } else {
        "Google: Anfrage fehlgeschlagen.".into()
    }
}

fn status_message(code: reqwest::StatusCode) -> String {
    match code.as_u16() {
        401 => "Google: Anmeldung abgelaufen, bitte neu verbinden.".into(),
        403 => "Google: Keine Berechtigung. Ist die Calendar API im Cloud-Projekt aktiviert?".into(),
        404 => "Google: Kalender nicht gefunden.".into(),
        429 => "Google: Zu viele Anfragen, bitte später erneut versuchen.".into(),
        c => format!("Google: Unerwartete Antwort (HTTP {c})."),
    }
}

pub(crate) fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("Tasker.md")
        .build()
        .map_err(|_| "HTTP-Client nicht verfügbar.".into())
}

// --- OAuth ---

fn random_b64(bytes: usize) -> Result<String, String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|_| "Zufallszahlen nicht verfügbar.".to_string())?;
    Ok(URL_SAFE_NO_PAD.encode(buf))
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Wartet auf den Redirect des Browsers und gibt den `code` zurück.
fn wait_for_code(listener: TcpListener, state: &str) -> Result<String, String> {
    listener.set_nonblocking(true).map_err(|_| "Lokaler Port nicht nutzbar.".to_string())?;
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    loop {
        if Instant::now() > deadline {
            return Err("Anmeldung abgebrochen (Zeitüberschreitung).".into());
        }
        let stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(_) => return Err("Lokaler Port nicht nutzbar.".into()),
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut line = String::new();
        let _ = BufReader::new(&stream).read_line(&mut line);
        let target = line.split_whitespace().nth(1).unwrap_or("");
        let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).ok();
        let param = |k: &str| url.as_ref().and_then(|u| u.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.into_owned()));
        let (code, err) = (param("code"), param("error"));
        if code.is_none() && err.is_none() {
            // z. B. favicon.ico
            let _ = (&stream).write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            continue;
        }
        let ok = code.is_some() && param("state").as_deref() == Some(state);
        let body = if ok {
            "<h2>Tasker.md ist mit Google Kalender verbunden.</h2><p>Dieses Fenster kann geschlossen werden.</p>"
        } else {
            "<h2>Anmeldung nicht abgeschlossen.</h2><p>Bitte in Tasker.md erneut versuchen.</p>"
        };
        let page = format!("<!doctype html><meta charset=utf-8><title>Tasker.md</title><body style=\"font-family:sans-serif\">{body}</body>");
        let _ = write!(
            &stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
            page.len()
        );
        return match (ok, err) {
            (true, _) => Ok(code.unwrap_or_default()),
            (false, Some(e)) if e == "access_denied" => Err("Zugriff in Google abgelehnt.".into()),
            _ => Err("Anmeldung fehlgeschlagen.".into()),
        };
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
}

/// Komplette Anmeldung: lokalen Port öffnen, Browser starten, Code gegen Tokens tauschen.
/// `open_browser` öffnet die Anmeldeseite (in der App über das Opener-Plugin).
pub async fn login(open_browser: impl FnOnce(&str) -> Result<(), String>) -> Result<(), String> {
    let client = load_client().ok_or("Bitte zuerst Client-ID und Client-Secret eintragen.")?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| "Lokaler Port nicht verfügbar.".to_string())?;
    let port = listener.local_addr().map_err(|_| "Lokaler Port nicht verfügbar.".to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}");
    let verifier = random_b64(48)?;
    let state = random_b64(16)?;
    let mut url = reqwest::Url::parse(AUTH_URL).expect("feste URL");
    url.query_pairs_mut()
        .append_pair("client_id", &client.id)
        .append_pair("redirect_uri", &redirect)
        .append_pair("response_type", "code")
        .append_pair("scope", SCOPE)
        .append_pair("code_challenge", &pkce_challenge(&verifier))
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    open_browser(url.as_str())?;
    let code = tauri::async_runtime::spawn_blocking(move || wait_for_code(listener, &state))
        .await
        .map_err(|_| "Anmeldung abgebrochen.".to_string())??;
    let r = http()?
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client.id.as_str()),
            ("client_secret", client.secret.as_str()),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(net_error)?;
    if !r.status().is_success() {
        return Err("Google: Code konnte nicht eingelöst werden. Client-ID und Secret prüfen.".into());
    }
    let t: TokenResponse = r.json().await.map_err(|_| "Google: Antwort nicht lesbar.".to_string())?;
    let refresh = t.refresh_token.ok_or("Google hat keinen Refresh-Token geliefert. Bitte erneut verbinden.")?;
    set_secret(KEY_REFRESH, &refresh)?;
    ACCESS.set(t.access_token, t.expires_in);
    Ok(())
}

/// Abmelden: Refresh-Token bei Google widerrufen (falls erreichbar) und lokal löschen.
pub async fn logout() -> Result<(), String> {
    if let Some(t) = get_secret(KEY_REFRESH) {
        if let Ok(h) = http() {
            let _ = h.post(REVOKE_URL).form(&[("token", t.as_str())]).send().await;
        }
    }
    ACCESS.clear();
    delete_secret(KEY_REFRESH)
}

/// Entfernt zusätzlich die Client-Daten.
pub async fn forget_all() -> Result<(), String> {
    logout().await?;
    delete_secret(KEY_CLIENT)
}

/// Access-Token nur im Speicher.
struct AccessCache(Mutex<Option<(String, Instant)>>);
static ACCESS: AccessCache = AccessCache(Mutex::new(None));

impl AccessCache {
    fn set(&self, token: String, expires_in: Option<u64>) {
        let valid = Duration::from_secs(expires_in.unwrap_or(3600).saturating_sub(60));
        if let Ok(mut g) = self.0.lock() {
            *g = Some((token, Instant::now() + valid));
        }
    }
    fn get(&self) -> Option<String> {
        let g = self.0.lock().ok()?;
        g.as_ref().filter(|(_, until)| Instant::now() < *until).map(|(t, _)| t.clone())
    }
    fn clear(&self) {
        if let Ok(mut g) = self.0.lock() {
            *g = None;
        }
    }
}

async fn access_token() -> Result<String, String> {
    if let Some(t) = ACCESS.get() {
        return Ok(t);
    }
    let client = load_client().ok_or("Google-Client nicht eingerichtet.")?;
    let refresh = get_secret(KEY_REFRESH).ok_or("Google Kalender ist nicht verbunden.")?;
    let r = http()?
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client.id.as_str()),
            ("client_secret", client.secret.as_str()),
            ("refresh_token", refresh.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(net_error)?;
    if r.status().as_u16() == 400 || r.status().as_u16() == 401 {
        // Refresh-Token widerrufen oder abgelaufen (im Testmodus nach 7 Tagen)
        let _ = delete_secret(KEY_REFRESH);
        return Err("Google: Verbindung abgelaufen, bitte neu verbinden.".into());
    }
    if !r.status().is_success() {
        return Err(status_message(r.status()));
    }
    let t: TokenResponse = r.json().await.map_err(|_| "Google: Antwort nicht lesbar.".to_string())?;
    ACCESS.set(t.access_token.clone(), t.expires_in);
    Ok(t.access_token)
}

async fn get_json(url: reqwest::Url) -> Result<Value, String> {
    let token = access_token().await?;
    let r = http()?.get(url).bearer_auth(token).send().await.map_err(net_error)?;
    if r.status().as_u16() == 401 {
        ACCESS.clear();
    }
    if !r.status().is_success() {
        return Err(status_message(r.status()));
    }
    r.json().await.map_err(|_| "Google: Antwort nicht lesbar.".into())
}

// --- Kalender ---

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarInfo {
    pub id: String,
    pub summary: String,
    pub primary: bool,
}

pub async fn list_calendars() -> Result<Vec<CalendarInfo>, String> {
    let url = reqwest::Url::parse(&format!("{API}/users/me/calendarList?minAccessRole=reader")).expect("feste URL");
    let v = get_json(url).await?;
    Ok(v["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|c| CalendarInfo {
                    id: c["id"].as_str().unwrap_or_default().to_string(),
                    summary: c["summaryOverride"].as_str().or(c["summary"].as_str()).unwrap_or("?").to_string(),
                    primary: c["primary"].as_bool().unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Ein Termin an einem bestimmten Tag (mehrtägige Termine erscheinen an jedem Tag).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayEvent {
    pub date: NaiveDate,
    #[serde(flatten)]
    pub event: CalendarEvent,
    /// Herkunft: `google` oder `ics:<id>`. Damit bleiben bei einem Fehler die alten Termine dieser Quelle erhalten.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

/// Ganztägiger Termin von `start` bis `end` (exklusiv), auf die Tage in `from..=to` verteilt.
pub(crate) fn push_all_day(out: &mut Vec<DayEvent>, title: &str, start: NaiveDate, end: NaiveDate, from: NaiveDate, to: NaiveDate) {
    let mut d = start.max(from);
    while d < end.max(start + ChronoDuration::days(1)) && d <= to {
        out.push(DayEvent { date: d, event: CalendarEvent { title: title.to_string(), start: None, end: None, all_day: true }, source: String::new() });
        d += ChronoDuration::days(1);
    }
}

/// Termin mit Uhrzeit (Ortszeit), auf die Tage in `from..=to` verteilt.
pub(crate) fn push_timed(out: &mut Vec<DayEvent>, title: &str, s: NaiveDateTime, e: NaiveDateTime, from: NaiveDate, to: NaiveDate) {
    let (sd, ed) = (s.date(), e.date());
    // endet genau um Mitternacht → zählt nicht zum Folgetag
    let last = if ed > sd && e.time() == NaiveTime::MIN { ed - ChronoDuration::days(1) } else { ed };
    let mut d = sd.max(from);
    while d <= last && d <= to {
        let start = if d == sd { s.format("%H:%M").to_string() } else { "00:00".into() };
        let end = if d == ed { e.format("%H:%M").to_string() } else { "23:59".into() };
        out.push(DayEvent { date: d, event: CalendarEvent { title: title.to_string(), start: Some(start), end: Some(end), all_day: false }, source: String::new() });
        d += ChronoDuration::days(1);
    }
}

/// Liest eine Seite von `events.list` und rechnet in die Zeitzone `tz` um.
pub fn parse_events<Tz: TimeZone>(v: &Value, tz: &Tz, from: NaiveDate, to: NaiveDate) -> Vec<DayEvent>
where
    Tz::Offset: std::fmt::Display,
{
    let mut out = vec![];
    for item in v["items"].as_array().into_iter().flatten() {
        if item["status"] == "cancelled" {
            continue;
        }
        let declined = item["attendees"].as_array().into_iter().flatten().any(|a| a["self"] == true && a["responseStatus"] == "declined");
        if declined {
            continue;
        }
        let title = item["summary"].as_str().filter(|s| !s.trim().is_empty()).unwrap_or("(ohne Titel)");
        let date = |k: &str| item[k]["date"].as_str().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        let dt = |k: &str| item[k]["dateTime"].as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| d.with_timezone(tz).naive_local());
        if let (Some(s), Some(e)) = (date("start"), date("end")) {
            // ganztägig, Ende exklusiv
            push_all_day(&mut out, title, s, e, from, to);
        } else if let (Some(s), Some(e)) = (dt("start"), dt("end")) {
            push_timed(&mut out, title, s, e, from, to);
        }
    }
    out
}

/// Lädt die Termine aller gewählten Kalender von `from` bis einschließlich `to`.
pub async fn fetch_events(calendar_ids: &[String], from: NaiveDate, to: NaiveDate) -> Result<Vec<DayEvent>, String> {
    let tz = chrono::Local;
    let start = tz.from_local_datetime(&from.and_hms_opt(0, 0, 0).expect("gültig")).earliest().ok_or("Ungültiges Datum.")?;
    let end = tz
        .from_local_datetime(&(to + ChronoDuration::days(1)).and_hms_opt(0, 0, 0).expect("gültig"))
        .earliest()
        .ok_or("Ungültiges Datum.")?;
    let mut all = vec![];
    for id in calendar_ids {
        let mut page: Option<String> = None;
        for _ in 0..5 {
            let mut url = reqwest::Url::parse(API).expect("feste URL");
            url.path_segments_mut().expect("Basis-URL").extend(["calendars", id.as_str(), "events"]);
            url.query_pairs_mut()
                .append_pair("timeMin", &start.to_rfc3339())
                .append_pair("timeMax", &end.to_rfc3339())
                .append_pair("singleEvents", "true")
                .append_pair("orderBy", "startTime")
                .append_pair("maxResults", "250")
                .append_pair("fields", "items(status,summary,start,end,attendees(self,responseStatus)),nextPageToken");
            if let Some(p) = &page {
                url.query_pairs_mut().append_pair("pageToken", p);
            }
            let v = get_json(url).await?;
            all.extend(parse_events(&v, &tz, from, to));
            match v["nextPageToken"].as_str() {
                Some(p) => page = Some(p.to_string()),
                None => break,
            }
        }
    }
    all.sort_by(|a, b| (a.date, &a.event.start).cmp(&(b.date, &b.event.start)));
    Ok(all)
}

// --- Cache ---

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CalendarCache {
    pub fetched_at: Option<String>,
    pub events: Vec<DayEvent>,
}

pub fn read_cache(path: &Path) -> CalendarCache {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn write_cache(path: &Path, cache: &CalendarCache) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|_| "Cache-Ordner nicht anlegbar.".to_string())?;
    }
    let text = serde_json::to_string_pretty(cache).map_err(|_| "Cache nicht schreibbar.".to_string())?;
    std::fs::write(path, text).map_err(|_| "Cache nicht schreibbar.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;
    use serde_json::json;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn parses_and_converts_events() {
        let cet = FixedOffset::east_opt(3600).unwrap();
        let v = json!({ "items": [
            { "summary": "Daily", "start": { "dateTime": "2026-11-11T08:00:00Z" }, "end": { "dateTime": "2026-11-11T08:15:00Z" } },
            { "summary": "Urlaub", "start": { "date": "2026-11-11" }, "end": { "date": "2026-11-13" } },
            { "summary": "Abgesagt", "status": "cancelled", "start": { "dateTime": "2026-11-11T10:00:00Z" }, "end": { "dateTime": "2026-11-11T11:00:00Z" } },
            { "summary": "Abgelehnt", "attendees": [{ "self": true, "responseStatus": "declined" }],
              "start": { "dateTime": "2026-11-11T12:00:00Z" }, "end": { "dateTime": "2026-11-11T13:00:00Z" } },
            { "summary": "Nachtschicht", "start": { "dateTime": "2026-11-11T22:00:00+01:00" }, "end": { "dateTime": "2026-11-12T02:00:00+01:00" } },
            { "summary": "Bis Mitternacht", "start": { "dateTime": "2026-11-11T23:00:00+01:00" }, "end": { "dateTime": "2026-11-12T00:00:00+01:00" } },
            { "start": { "dateTime": "2026-11-20T09:00:00+01:00" }, "end": { "dateTime": "2026-11-20T10:00:00+01:00" } }
        ]});
        let ev = parse_events(&v, &cet, d("2026-11-11"), d("2026-11-12"));
        let s: Vec<String> = ev
            .iter()
            .map(|e| format!("{} {} {}-{}", e.date, e.event.title, e.event.start.as_deref().unwrap_or("ganz"), e.event.end.as_deref().unwrap_or("")))
            .collect();
        assert_eq!(
            s,
            vec![
                "2026-11-11 Daily 09:00-09:15",
                "2026-11-11 Urlaub ganz-",
                "2026-11-12 Urlaub ganz-",
                "2026-11-11 Nachtschicht 22:00-23:59",
                "2026-11-12 Nachtschicht 00:00-02:00",
                "2026-11-11 Bis Mitternacht 23:00-23:59",
            ]
        );
    }

    #[test]
    fn pkce_matches_rfc_example() {
        // RFC 7636, Anhang B
        assert_eq!(pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn loopback_returns_code_and_checks_state() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || wait_for_code(l, "st"));
        let send = |path: &str| {
            let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut resp = String::new();
            let _ = std::io::Read::read_to_string(&mut s, &mut resp);
            resp
        };
        assert!(send("/favicon.ico").starts_with("HTTP/1.1 404"));
        assert!(send("/?state=st&code=abc").contains("verbunden"));
        assert_eq!(h.join().unwrap().unwrap(), "abc");

        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port2 = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || wait_for_code(l, "st"));
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port2)).unwrap();
        write!(s, "GET /?state=falsch&code=abc HTTP/1.1\r\n\r\n").unwrap();
        assert!(h.join().unwrap().is_err(), "falscher state wird abgelehnt");
    }
}
