//! Abonnierte Kalender über ICS-Links (SPEC 7), z. B. Outlook „Kalender veröffentlichen“. Nur lesen.
//!
//! Der Link enthält einen geheimen Schlüssel. Er liegt deshalb nur im Schlüsselbund
//! (`ics-url-<id>`), in der Konfiguration stehen nur Id und Name. Fehlermeldungen nennen ihn nie.
//! Serientermine (RRULE, EXDATE, geänderte Einzeltermine über RECURRENCE-ID) werden aufgelöst.

use crate::google::{push_all_day, push_timed, DayEvent};
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use rrule::{RRule, Tz, Unvalidated};
use std::collections::{HashMap, HashSet};

/// Höchstgröße einer ICS-Datei
const MAX_BYTES: usize = 20 * 1024 * 1024;
/// Höchstzahl Vorkommen eines Serientermins im geladenen Zeitraum
const MAX_OCCURRENCES: u16 = 500;

// --- Schlüsselbund ---

fn key(id: &str) -> String {
    format!("ics-url-{id}")
}

pub fn load_url(id: &str) -> Option<String> {
    crate::google::get_secret(&key(id))
}

pub fn store_url(id: &str, url: &str) -> Result<(), String> {
    crate::google::set_secret(&key(id), url)
}

pub fn delete_url(id: &str) -> Result<(), String> {
    crate::google::delete_secret(&key(id))
}

/// `webcal://` wird zu `https://`; nur http(s) ist erlaubt.
pub fn normalize_url(s: &str) -> Result<String, String> {
    let s = s.trim();
    let s = match s.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("webcal") || scheme.eq_ignore_ascii_case("webcals") => format!("https://{rest}"),
        _ => s.to_string(),
    };
    let url = reqwest::Url::parse(&s).map_err(|_| "Das ist kein gültiger Link.".to_string())?;
    if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
        return Err("Der Link muss mit https:// oder webcal:// beginnen.".into());
    }
    Ok(url.into())
}

/// Lädt die ICS-Datei. Fehlermeldungen enthalten den Link nicht.
pub async fn fetch(url: &str) -> Result<String, String> {
    let client = crate::google::http()?;
    let resp = client.get(url).send().await.map_err(|e| {
        if e.is_timeout() {
            "Zeitüberschreitung.".to_string()
        } else if e.is_connect() {
            "Keine Verbindung.".to_string()
        } else {
            "Abruf fehlgeschlagen.".to_string()
        }
    })?;
    match resp.status().as_u16() {
        200..=299 => {}
        401 | 403 => return Err("Zugriff verweigert. Ist der Kalender noch veröffentlicht?".into()),
        404 => return Err("Nicht gefunden. Wurde die Veröffentlichung beendet oder der Link erneuert?".into()),
        c => return Err(format!("Unerwartete Antwort (HTTP {c}).")),
    }
    let bytes = resp.bytes().await.map_err(|_| "Abruf fehlgeschlagen.".to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("Die Kalenderdatei ist zu groß.".into());
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if !text.contains("BEGIN:VCALENDAR") {
        return Err("Der Link liefert keinen Kalender (ICS). Bitte den ICS-Link nehmen, nicht den HTML-Link.".into());
    }
    Ok(text)
}

// --- Parser ---

#[derive(Debug, Clone)]
struct Prop {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Prop {
    fn param(&self, k: &str) -> Option<&str> {
        self.params.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.as_str())
    }
}

/// Zeilen entfalten (RFC 5545 3.1): eine Zeile, die mit Leerzeichen oder Tab beginnt, setzt die vorige fort.
fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        match line.strip_prefix([' ', '\t']) {
            Some(rest) if !out.is_empty() => out.last_mut().expect("nicht leer").push_str(rest),
            _ => out.push(line.to_string()),
        }
    }
    out
}

/// `NAME;P1=a;P2="b:c":WERT`
fn parse_line(line: &str) -> Option<Prop> {
    let mut in_quotes = false;
    let mut split = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ':' if !in_quotes => {
                split = Some(i);
                break;
            }
            _ => {}
        }
    }
    let i = split?;
    let (head, value) = (&line[..i], &line[i + 1..]);
    let mut parts = vec![];
    let (mut cur, mut q) = (String::new(), false);
    for c in head.chars() {
        match c {
            '"' => q = !q,
            ';' if !q => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    parts.push(cur);
    let name = parts.first()?.trim().to_ascii_uppercase();
    let params = parts[1..].iter().filter_map(|p| p.split_once('=')).map(|(k, v)| (k.trim().to_ascii_uppercase(), v.to_string())).collect();
    Some(Prop { name, params, value: value.to_string() })
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n' | 'N') => out.push(' '),
                Some(x) => out.push(x),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Windows-Zeitzonennamen (Outlook/Exchange) auf IANA abbilden. Unbekanntes → Ortszeit.
fn windows_zone(name: &str) -> Option<&'static str> {
    Some(match name {
        "W. Europe Standard Time" => "Europe/Berlin",
        "Central Europe Standard Time" => "Europe/Budapest",
        "Central European Standard Time" => "Europe/Warsaw",
        "Romance Standard Time" => "Europe/Paris",
        "GMT Standard Time" => "Europe/London",
        "Greenwich Standard Time" => "Atlantic/Reykjavik",
        "E. Europe Standard Time" => "Europe/Chisinau",
        "FLE Standard Time" => "Europe/Kyiv",
        "GTB Standard Time" => "Europe/Bucharest",
        "Russian Standard Time" => "Europe/Moscow",
        "Turkey Standard Time" => "Europe/Istanbul",
        "Israel Standard Time" => "Asia/Jerusalem",
        "Arabian Standard Time" => "Asia/Dubai",
        "India Standard Time" => "Asia/Kolkata",
        "China Standard Time" => "Asia/Shanghai",
        "Singapore Standard Time" => "Asia/Singapore",
        "Tokyo Standard Time" => "Asia/Tokyo",
        "Korea Standard Time" => "Asia/Seoul",
        "AUS Eastern Standard Time" => "Australia/Sydney",
        "New Zealand Standard Time" => "Pacific/Auckland",
        "Eastern Standard Time" => "America/New_York",
        "Central Standard Time" => "America/Chicago",
        "Mountain Standard Time" => "America/Denver",
        "Pacific Standard Time" => "America/Los_Angeles",
        "Atlantic Standard Time" => "America/Halifax",
        "E. South America Standard Time" => "America/Sao_Paulo",
        "UTC" | "Coordinated Universal Time" => "UTC",
        _ => return None,
    })
}

fn zone(tzid: Option<&str>) -> Tz {
    let Some(name) = tzid.map(|s| s.trim().trim_matches('"')) else { return Tz::LOCAL };
    let iana = windows_zone(name).unwrap_or(name);
    // manche Server schreiben „/Europe/Berlin“ oder ein Präfix davor
    let iana = iana.rsplit_once("/mozilla.org/").map_or(iana, |(_, z)| z).trim_start_matches('/');
    iana.parse::<chrono_tz::Tz>().map(Tz::Tz).unwrap_or(Tz::LOCAL)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum When {
    Date(NaiveDate),
    At(DateTime<Tz>),
}

impl When {
    /// Vergleichswert für EXDATE/RECURRENCE-ID
    fn stamp(&self) -> i64 {
        match self {
            When::Date(d) => d.and_hms_opt(0, 0, 0).map_or(0, |n| n.and_utc().timestamp()),
            When::At(t) => t.timestamp(),
        }
    }
}

fn local_in(tz: &Tz, n: NaiveDateTime) -> Option<DateTime<Tz>> {
    // in der Zeitumstellung: die frühere Zeit, in der Lücke eine Stunde später
    tz.from_local_datetime(&n).earliest().or_else(|| tz.from_local_datetime(&(n + Duration::hours(1))).earliest())
}

/// Ein Datums- oder Zeitwert, `tzid` aus den Parametern.
fn parse_value(value: &str, tzid: Option<&str>, is_date: bool) -> Option<When> {
    let v = value.trim();
    if is_date || v.len() == 8 {
        return NaiveDate::parse_from_str(v, "%Y%m%d").ok().map(When::Date);
    }
    if let Some(utc) = v.strip_suffix('Z') {
        let n = NaiveDateTime::parse_from_str(utc, "%Y%m%dT%H%M%S").ok()?;
        return Some(When::At(Utc.from_utc_datetime(&n).with_timezone(&Tz::UTC)));
    }
    let n = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()?;
    local_in(&zone(tzid), n).map(When::At)
}

fn parse_prop_time(p: &Prop) -> Option<When> {
    let is_date = p.param("VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE"));
    parse_value(&p.value, p.param("TZID"), is_date)
}

/// Alle Werte einer EXDATE-Zeile (kommagetrennt).
fn parse_prop_times(p: &Prop) -> Vec<When> {
    let is_date = p.param("VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE"));
    p.value.split(',').filter_map(|v| parse_value(v, p.param("TZID"), is_date)).collect()
}

/// `P1D`, `PT1H30M`, `-PT15M`, `P1W`
fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let s = s.strip_prefix('P')?;
    let (mut total, mut num, mut in_time) = (Duration::zero(), String::new(), false);
    for c in s.chars() {
        match c {
            'T' => in_time = true,
            '0'..='9' => num.push(c),
            _ => {
                let n: i64 = std::mem::take(&mut num).parse().ok()?;
                total += match (c, in_time) {
                    ('W', _) => Duration::weeks(n),
                    ('D', _) => Duration::days(n),
                    ('H', true) => Duration::hours(n),
                    ('M', true) => Duration::minutes(n),
                    ('S', true) => Duration::seconds(n),
                    _ => return None,
                };
            }
        }
    }
    Some(if neg { -total } else { total })
}

#[derive(Debug, Default)]
struct RawEvent {
    uid: String,
    summary: String,
    start: Option<When>,
    end: Option<When>,
    duration: Option<Duration>,
    rrule: Option<String>,
    exdates: Vec<When>,
    recurrence_id: Option<When>,
    cancelled: bool,
}

fn parse_raw(text: &str) -> Vec<RawEvent> {
    let mut out = vec![];
    let mut cur: Option<RawEvent> = None;
    // Unterkomponenten (VALARM) im Termin überspringen
    let mut nested = 0usize;
    for line in unfold(text) {
        let Some(p) = parse_line(&line) else { continue };
        match (p.name.as_str(), cur.is_some()) {
            ("BEGIN", false) if p.value.trim().eq_ignore_ascii_case("VEVENT") => cur = Some(RawEvent::default()),
            ("BEGIN", true) => nested += 1,
            ("END", true) if nested > 0 => nested -= 1,
            ("END", true) if p.value.trim().eq_ignore_ascii_case("VEVENT") => out.extend(cur.take()),
            (_, true) if nested == 0 => {
                let e = cur.as_mut().expect("im Termin");
                match p.name.as_str() {
                    "UID" => e.uid = p.value.trim().to_string(),
                    "SUMMARY" => e.summary = unescape(&p.value).trim().to_string(),
                    "DTSTART" => e.start = parse_prop_time(&p),
                    "DTEND" => e.end = parse_prop_time(&p),
                    "DURATION" => e.duration = parse_duration(&p.value),
                    "RRULE" => e.rrule = Some(p.value.trim().to_string()),
                    "EXDATE" => e.exdates.extend(parse_prop_times(&p)),
                    "RECURRENCE-ID" => e.recurrence_id = parse_prop_time(&p),
                    "STATUS" => e.cancelled = p.value.trim().eq_ignore_ascii_case("CANCELLED"),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

/// Startpunkt für die Serienberechnung; ganztägige Termine als Mitternacht Ortszeit.
fn rrule_start(w: When) -> Option<DateTime<Tz>> {
    match w {
        When::Date(d) => local_in(&Tz::LOCAL, d.and_hms_opt(0, 0, 0)?),
        When::At(t) => Some(t),
    }
}

/// UNTIL ohne `Z` in UTC umrechnen, damit `rrule` es zu jedem Startpunkt akzeptiert.
fn normalize_rrule(rule: &str, tz: &Tz) -> String {
    rule.split(';')
        .map(|part| match part.split_once('=') {
            Some((k, v)) if k.eq_ignore_ascii_case("UNTIL") && !v.ends_with('Z') => {
                let naive = if v.len() == 8 {
                    NaiveDate::parse_from_str(v, "%Y%m%d").ok().and_then(|d| d.and_hms_opt(23, 59, 59))
                } else {
                    NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()
                };
                match naive.and_then(|n| local_in(tz, n)) {
                    Some(t) => format!("UNTIL={}", t.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ")),
                    None => part.to_string(),
                }
            }
            _ => part.to_string(),
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Vorkommen eines Termins, die in `[win_start - dauer, win_end)` beginnen.
fn occurrences(e: &RawEvent, start: When, dur: Duration, win_start: DateTime<Tz>, win_end: DateTime<Tz>, skip: &HashSet<i64>) -> Vec<When> {
    let Some(rule) = &e.rrule else { return vec![start] };
    let Some(dt_start) = rrule_start(start) else { return vec![] };
    let set = normalize_rrule(rule, &dt_start.timezone()).parse::<RRule<Unvalidated>>().ok().and_then(|r| r.build(dt_start).ok());
    // unbekannte Regel: wenigstens den ersten Termin zeigen
    let Some(set) = set else { return vec![start] };
    let after = win_start - dur - Duration::seconds(1);
    let excluded: HashSet<i64> = e.exdates.iter().map(When::stamp).chain(skip.iter().copied()).collect();
    set.after(after)
        .before(win_end)
        .all(MAX_OCCURRENCES)
        .dates
        .into_iter()
        .map(|t| match start {
            When::Date(_) => When::Date(t.date_naive()),
            When::At(_) => When::At(t),
        })
        .filter(|w| !excluded.contains(&w.stamp()))
        .collect()
}

/// Termine einer ICS-Datei für die Tage `from..=to`, in Ortszeit.
pub fn parse_events(text: &str, from: NaiveDate, to: NaiveDate) -> Vec<DayEvent> {
    let raw = parse_raw(text);
    let (Some(win_start), Some(win_end)) = (
        from.and_hms_opt(0, 0, 0).and_then(|n| local_in(&Tz::LOCAL, n)),
        (to + Duration::days(1)).and_hms_opt(0, 0, 0).and_then(|n| local_in(&Tz::LOCAL, n)),
    ) else {
        return vec![];
    };
    // geänderte oder abgesagte Einzeltermine einer Serie: Original überspringen
    let mut overridden: HashMap<&str, HashSet<i64>> = HashMap::new();
    for e in raw.iter().filter(|e| !e.uid.is_empty()) {
        if let Some(r) = e.recurrence_id {
            overridden.entry(e.uid.as_str()).or_default().insert(r.stamp());
        }
    }
    let none = HashSet::new();
    let mut out = vec![];
    for e in &raw {
        if e.cancelled {
            continue;
        }
        let Some(start) = e.start else { continue };
        let title = if e.summary.is_empty() { "(ohne Titel)" } else { e.summary.as_str() };
        let dur = match (start, e.end) {
            (When::Date(s), Some(When::Date(x))) => x - s,
            (When::At(s), Some(When::At(x))) => x - s,
            _ => e.duration.unwrap_or(match start {
                When::Date(_) => Duration::days(1),
                When::At(_) => Duration::zero(),
            }),
        }
        .max(Duration::zero());
        let skip = if e.recurrence_id.is_none() { overridden.get(e.uid.as_str()).unwrap_or(&none) } else { &none };
        for occ in occurrences(e, start, dur, win_start, win_end, skip) {
            match occ {
                When::Date(d) => push_all_day(&mut out, title, d, d + dur.max(Duration::days(1)), from, to),
                When::At(t) => {
                    let s = t.with_timezone(&Local).naive_local();
                    let e = (t + dur).with_timezone(&Local).naive_local();
                    push_timed(&mut out, title, s, e, from, to);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// „Datum Titel Start-Ende“; Zeiten in Ortszeit, deshalb nur mit TZID = Ortszeit-unabhängigen Fällen prüfen
    fn show(ev: &[DayEvent]) -> Vec<String> {
        let mut v: Vec<String> = ev
            .iter()
            .map(|e| format!("{} {} {}-{}", e.date, e.event.title, e.event.start.as_deref().unwrap_or("ganz"), e.event.end.as_deref().unwrap_or("")))
            .collect();
        v.sort();
        v
    }

    fn cal(events: &str) -> String {
        format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{events}END:VCALENDAR\r\n")
    }

    #[test]
    fn single_all_day_and_folded_lines() {
        // floating times (ohne TZID) gelten als Ortszeit und bleiben unverändert
        let text = cal(concat!(
            "BEGIN:VEVENT\r\nUID:1\r\nSUMMARY:Planungs\r\n runde\\, Team\r\nDTSTART:20261111T090000\r\nDTEND:20261111T100000\r\n",
            "BEGIN:VALARM\r\nSUMMARY:Erinnerung\r\nEND:VALARM\r\nEND:VEVENT\r\n",
            "BEGIN:VEVENT\r\nUID:2\r\nSUMMARY:Messe\r\nDTSTART;VALUE=DATE:20261111\r\nDTEND;VALUE=DATE:20261113\r\nEND:VEVENT\r\n",
            "BEGIN:VEVENT\r\nUID:3\r\nSUMMARY:Abgesagt\r\nSTATUS:CANCELLED\r\nDTSTART:20261111T110000\r\nDTEND:20261111T120000\r\nEND:VEVENT\r\n",
            "BEGIN:VEVENT\r\nUID:4\r\nSUMMARY:Später\r\nDTSTART:20261201T090000\r\nDURATION:PT1H\r\nEND:VEVENT\r\n",
        ));
        assert_eq!(
            show(&parse_events(&text, d("2026-11-11"), d("2026-11-12"))),
            vec!["2026-11-11 Messe ganz-", "2026-11-11 Planungsrunde, Team 09:00-10:00", "2026-11-12 Messe ganz-"]
        );
    }

    #[test]
    fn weekly_series_with_exdate_and_override() {
        let text = cal(concat!(
            "BEGIN:VEVENT\r\nUID:s\r\nSUMMARY:Jour fixe\r\nDTSTART:20261102T100000\r\nDTEND:20261102T103000\r\n",
            "RRULE:FREQ=WEEKLY;BYDAY=MO,TH;UNTIL=20261130T235959\r\nEXDATE:20261105T100000\r\nEND:VEVENT\r\n",
            // verschobener Einzeltermin der Serie
            "BEGIN:VEVENT\r\nUID:s\r\nRECURRENCE-ID:20261109T100000\r\nSUMMARY:Jour fixe (verschoben)\r\nDTSTART:20261109T140000\r\nDTEND:20261109T143000\r\nEND:VEVENT\r\n",
        ));
        assert_eq!(
            show(&parse_events(&text, d("2026-11-05"), d("2026-11-13"))),
            vec!["2026-11-09 Jour fixe (verschoben) 14:00-14:30", "2026-11-12 Jour fixe 10:00-10:30"]
        );
    }

    #[test]
    fn series_started_long_ago_and_monthly_rules() {
        let text = cal(concat!(
            "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Daily\r\nDTSTART:20200106T091500\r\nDTEND:20200106T093000\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR\r\nEND:VEVENT\r\n",
            "BEGIN:VEVENT\r\nUID:b\r\nSUMMARY:Monatsrunde\r\nDTSTART:20260113T150000\r\nDTEND:20260113T160000\r\nRRULE:FREQ=MONTHLY;BYDAY=2TU;COUNT=24\r\nEND:VEVENT\r\n",
            "BEGIN:VEVENT\r\nUID:c\r\nSUMMARY:Geburtstag\r\nDTSTART;VALUE=DATE:20101111\r\nDTEND;VALUE=DATE:20101112\r\nRRULE:FREQ=YEARLY\r\nEND:VEVENT\r\n",
        ));
        assert_eq!(
            show(&parse_events(&text, d("2026-11-09"), d("2026-11-11"))),
            vec![
                "2026-11-09 Daily 09:15-09:30",
                "2026-11-10 Daily 09:15-09:30",
                "2026-11-10 Monatsrunde 15:00-16:00",
                "2026-11-11 Daily 09:15-09:30",
                "2026-11-11 Geburtstag ganz-",
            ]
        );
    }

    #[test]
    fn time_zones_are_converted() {
        // UTC und Windows-Zonenname gegen dieselbe Ortszeit: beide Termine sind gleichzeitig
        let text = cal(concat!(
            "BEGIN:VEVENT\r\nUID:u\r\nSUMMARY:A\r\nDTSTART:20261111T080000Z\r\nDTEND:20261111T090000Z\r\nEND:VEVENT\r\n",
            "BEGIN:VEVENT\r\nUID:w\r\nSUMMARY:B\r\nDTSTART;TZID=W. Europe Standard Time:20261111T090000\r\nDTEND;TZID=W. Europe Standard Time:20261111T100000\r\nEND:VEVENT\r\n",
            "BEGIN:VEVENT\r\nUID:i\r\nSUMMARY:C\r\nDTSTART;TZID=\"Europe/Berlin\":20261111T090000\r\nDTEND;TZID=Europe/Berlin:20261111T100000\r\n",
            "RRULE:FREQ=DAILY;UNTIL=20261112\r\nEND:VEVENT\r\n",
        ));
        let ev = parse_events(&text, d("2026-11-10"), d("2026-11-14"));
        let starts: Vec<_> = ev.iter().filter(|e| e.date == d("2026-11-11")).map(|e| e.event.start.clone()).collect();
        assert_eq!(starts.len(), 3);
        assert!(starts.iter().all(|s| s == &starts[0]), "{starts:?}");
        assert_eq!(ev.iter().filter(|e| e.event.title == "C").count(), 2, "UNTIL als Datum schließt den Tag ein");
    }

    #[test]
    fn helpers() {
        assert_eq!(parse_duration("PT1H30M"), Some(Duration::minutes(90)));
        assert_eq!(parse_duration("P1W"), Some(Duration::weeks(1)));
        assert_eq!(parse_duration("-PT15M"), Some(Duration::minutes(-15)));
        assert_eq!(normalize_url("webcal://outlook.example.com/cal/x/calendar.ics").unwrap(), "https://outlook.example.com/cal/x/calendar.ics");
        assert!(normalize_url("file:///c:/x.ics").is_err());
        assert!(normalize_url("kein link").is_err());
        assert_eq!(zone(Some("Unbekannte Zone")), Tz::LOCAL);
    }
}
