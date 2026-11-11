//! Schnelleingabe (SPEC 6.2): natürliche Sprache (Deutsch/Englisch) → Tasks-Syntax.
//!
//! Beispiel: `Müller anrufen morgen @telefon !1 >Gartenhaus`
//! → `- [ ] Müller anrufen #@telefon ⏫ ⏳ <morgen>` mit Ziel „Gartenhaus“.
//! `>Ziel` steht am Ende und darf Leerzeichen enthalten.
//! Uhrzeit und Dauer (`14:00 90m`, `14-15:30`) ergeben einen lokalen Zeitblock und stehen nicht in der Zeile.

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QuickTask {
    pub description: String,
    pub tags: Vec<String>,
    /// `⏫` / `🔼` / `🔽`
    pub priority: Option<String>,
    pub recurrence: Option<String>,
    pub scheduled: Option<NaiveDate>,
    pub due: Option<NaiveDate>,
    /// Text nach `>`, noch nicht aufgelöst
    pub target: Option<String>,
    /// Uhrzeit (und Dauer) für einen Zeitblock am geplanten Tag
    pub block: Option<QuickBlock>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuickBlock {
    /// `HH:MM`
    pub start: String,
    /// `None` = Standarddauer aus der Konfiguration
    pub minutes: Option<u32>,
}

/// `14:00`, `9:30`, mit `allow_bare` auch `14`. Gibt Minuten seit Mitternacht zurück.
fn clock(w: &str, allow_bare: bool) -> Option<u32> {
    let (h, m) = match w.split_once(':') {
        Some((h, m)) if m.len() == 2 => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None if allow_bare => (w.parse::<u32>().ok()?, 0),
        _ => return None,
    };
    (h < 24 && m < 60 && !w.starts_with('+') && !w.starts_with('-')).then_some(h * 60 + m)
}

fn hm(min: u32) -> String {
    format!("{:02}:{:02}", min / 60, min % 60)
}

fn is_uhr(w: Option<&String>) -> bool {
    w.is_some_and(|w| matches!(w.to_lowercase().as_str(), "uhr" | "h"))
}

/// Erkennt eine Uhrzeit ab `words[i]`: `14:00`, `14 Uhr`, `um 14`, `at 9:30`, `14-15:30`.
/// Gibt Start, ggf. Dauer aus einer Spanne und die Zahl der verbrauchten Wörter zurück.
fn parse_time(words: &[String], i: usize) -> Option<(u32, Option<u32>, usize)> {
    let w = words.get(i)?.to_lowercase();
    if w == "um" || w == "at" {
        let (s, d, n) = parse_time_word(words, i + 1, true)?;
        return Some((s, d, n + 1));
    }
    parse_time_word(words, i, false)
}

fn parse_time_word(words: &[String], i: usize, after_um: bool) -> Option<(u32, Option<u32>, usize)> {
    let w = words.get(i)?;
    let uhr = is_uhr(words.get(i + 1));
    let extra = usize::from(uhr);
    // Spanne: mindestens eine Seite mit Doppelpunkt, oder „Uhr“/„um“ dabei
    if let Some((a, b)) = w.split_once('-') {
        let bare = uhr || after_um || a.contains(':') || b.contains(':');
        let (s, e) = (clock(a, bare)?, clock(b, bare)?);
        return (e > s).then_some((s, Some(e - s), 1 + extra));
    }
    let s = clock(w, uhr || after_um)?;
    Some((s, None, 1 + extra))
}

/// `90m`, `90min`, `1h`, `1,5h`, `1.5h`, `2h30`, `2h30m` → Minuten.
fn parse_duration(w: &str) -> Option<u32> {
    let w = w.to_lowercase();
    let minutes = if let Some(n) = w.strip_suffix("min").or_else(|| w.strip_suffix('m')) {
        if let Some((h, m)) = n.split_once('h') {
            h.parse::<u32>().ok()? * 60 + m.parse::<u32>().ok()?
        } else {
            n.parse::<u32>().ok()?
        }
    } else if let Some((h, m)) = w.split_once('h') {
        let hours: f64 = h.replace(',', ".").parse().ok()?;
        let extra = if m.is_empty() { 0 } else { m.parse::<u32>().ok()? };
        if !hours.is_finite() || hours < 0.0 || (extra > 0 && hours.fract() != 0.0) {
            return None;
        }
        (hours * 60.0).round() as u32 + extra
    } else {
        return None;
    };
    (minutes > 0).then_some(minutes)
}

impl QuickTask {
    /// Zeile in der Reihenfolge des Tasks-Plugins: Text, Tags, Priorität, 🔁, ⏳, 📅.
    pub fn line(&self) -> String {
        let mut parts = vec!["- [ ]".to_string(), self.description.clone()];
        parts.extend(self.tags.iter().cloned());
        if let Some(p) = &self.priority {
            parts.push(p.clone());
        }
        if let Some(r) = &self.recurrence {
            parts.push(format!("🔁 {r}"));
        }
        if let Some(d) = self.scheduled {
            parts.push(format!("⏳ {d}"));
        }
        if let Some(d) = self.due {
            parts.push(format!("📅 {d}"));
        }
        parts.retain(|p| !p.is_empty());
        parts.join(" ")
    }
}

fn weekday(w: &str) -> Option<Weekday> {
    Some(match w {
        "montag" | "mo" | "monday" | "mon" => Weekday::Mon,
        "dienstag" | "di" | "tuesday" | "tue" => Weekday::Tue,
        "mittwoch" | "mi" | "wednesday" | "wed" => Weekday::Wed,
        "donnerstag" | "do" | "thursday" | "thu" => Weekday::Thu,
        "freitag" | "fr" | "friday" | "fri" => Weekday::Fri,
        "samstag" | "sa" | "saturday" | "sat" => Weekday::Sat,
        "sonntag" | "so" | "sunday" | "sun" => Weekday::Sun,
        _ => return None,
    })
}

fn weekday_en(w: Weekday) -> &'static str {
    match w {
        Weekday::Mon => "monday",
        Weekday::Tue => "tuesday",
        Weekday::Wed => "wednesday",
        Weekday::Thu => "thursday",
        Weekday::Fri => "friday",
        Weekday::Sat => "saturday",
        Weekday::Sun => "sunday",
    }
}

/// Nächster solcher Wochentag nach heute (heute selbst zählt nicht).
fn next_weekday(today: NaiveDate, w: Weekday) -> NaiveDate {
    let diff = (7 + w.num_days_from_monday() as i64 - today.weekday().num_days_from_monday() as i64) % 7;
    today + Duration::days(if diff == 0 { 7 } else { diff })
}

/// Erkennt ein Datum ab `words[i]`. Gibt Datum und Zahl der verbrauchten Wörter zurück.
fn parse_date(words: &[String], i: usize, today: NaiveDate) -> Option<(NaiveDate, usize)> {
    let w = words.get(i)?.to_lowercase();
    let simple = match w.as_str() {
        "heute" | "today" => Some(today),
        "morgen" | "tomorrow" => Some(today + Duration::days(1)),
        "übermorgen" => Some(today + Duration::days(2)),
        _ => None,
    };
    if let Some(d) = simple {
        return Some((d, 1));
    }
    // Kurzformen (mo, di, …) nur nach „am“/„on“, sonst zu viele Fehltreffer
    if w.len() > 3 {
        if let Some(wd) = weekday(&w) {
            return Some((next_weekday(today, wd), 1));
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(&w, "%Y-%m-%d") {
        return Some((d, 1));
    }
    // 24.12. oder 24.12.2026
    if let Some(d) = parse_german_date(&w, today) {
        return Some((d, 1));
    }
    // „in 3 tagen“ / „in 2 wochen“ / „in 3 days“
    if w == "in" {
        let n: i64 = words.get(i + 1)?.parse().ok()?;
        let unit = words.get(i + 2)?.to_lowercase();
        let days = match unit.as_str() {
            "tag" | "tagen" | "tage" | "day" | "days" => n,
            "woche" | "wochen" | "week" | "weeks" => n * 7,
            _ => return None,
        };
        return Some((today + Duration::days(days), 3));
    }
    if w == "am" || w == "on" || w == "next" || w == "nächsten" || w == "kommenden" {
        let next = words.get(i + 1)?.to_lowercase();
        if let Some(wd) = weekday(&next) {
            return Some((next_weekday(today, wd), 2));
        }
        if w == "am" {
            let (d, n) = parse_date(words, i + 1, today)?;
            return Some((d, n + 1));
        }
    }
    None
}

fn parse_german_date(w: &str, today: NaiveDate) -> Option<NaiveDate> {
    let parts: Vec<&str> = w.trim_end_matches('.').split('.').collect();
    let (d, m) = (parts.first()?.parse::<u32>().ok()?, parts.get(1)?.parse::<u32>().ok()?);
    let y = match parts.get(2) {
        Some(y) if !y.is_empty() => y.parse::<i32>().ok()?,
        _ => {
            // ohne Jahr: nächstes Vorkommen ab heute
            let this = NaiveDate::from_ymd_opt(today.year(), m, d)?;
            return Some(if this < today { NaiveDate::from_ymd_opt(today.year() + 1, m, d)? } else { this });
        }
    };
    NaiveDate::from_ymd_opt(y, m, d)
}

/// Erkennt eine Wiederholung ab `words[i]`.
fn parse_recurrence(words: &[String], i: usize) -> Option<(String, Option<Weekday>, usize)> {
    let w = words.get(i)?.to_lowercase();
    let single = match w.as_str() {
        "täglich" | "daily" => Some("every day"),
        "wöchentlich" | "weekly" => Some("every week"),
        "monatlich" | "monthly" => Some("every month"),
        "jährlich" | "yearly" => Some("every year"),
        _ => None,
    };
    if let Some(r) = single {
        return Some((r.into(), None, 1));
    }
    if matches!(w.as_str(), "jeden" | "jede" | "jedes" | "every") {
        let next = words.get(i + 1)?.to_lowercase();
        if let Some(wd) = weekday(&next) {
            return Some((format!("every {}", weekday_en(wd)), Some(wd), 2));
        }
        let unit = match next.as_str() {
            "tag" | "day" => "every day",
            "woche" | "week" => "every week",
            "monat" | "month" => "every month",
            "jahr" | "year" => "every year",
            "werktag" | "weekday" => "every weekday",
            _ => return None,
        };
        return Some((unit.into(), None, 2));
    }
    None
}

/// Übersetzt die Eingabe. `waiting_tag` ist z. B. `#warten`.
pub fn parse_quick(input: &str, today: NaiveDate, waiting_tag: &str) -> QuickTask {
    let mut q = QuickTask::default();
    let (body, target) = match input.split_once('>') {
        Some((b, t)) => (b, Some(t.trim().to_string()).filter(|t| !t.is_empty())),
        None => (input, None),
    };
    q.target = target;
    let words: Vec<String> = body.split_whitespace().map(String::from).collect();
    let mut desc: Vec<String> = vec![];
    // (Position in desc, Wort, Minuten)
    let mut durations: Vec<(usize, String, u32)> = vec![];
    let mut i = 0;
    while i < words.len() {
        let w = &words[i];
        let lw = w.to_lowercase();
        if let Some(ctx) = w.strip_prefix('@').filter(|c| !c.is_empty()) {
            q.tags.push(format!("#@{ctx}"));
            i += 1;
            continue;
        }
        if w.starts_with('#') && w.len() > 1 {
            q.tags.push(w.clone());
            i += 1;
            continue;
        }
        match lw.as_str() {
            "!1" => {
                q.priority = Some("⏫".into());
                i += 1;
                continue;
            }
            "!2" => {
                q.priority = Some("🔼".into());
                i += 1;
                continue;
            }
            "!3" => {
                q.priority = Some("🔽".into());
                i += 1;
                continue;
            }
            "warten" | "waiting" => {
                if !q.tags.iter().any(|t| t == waiting_tag) {
                    q.tags.push(waiting_tag.to_string());
                }
                i += 1;
                continue;
            }
            "fällig" | "due" | "bis" => {
                if let Some((d, n)) = parse_date(&words, i + 1, today) {
                    q.due = Some(d);
                    i += 1 + n;
                    continue;
                }
            }
            _ => {}
        }
        if let Some((r, wd, n)) = parse_recurrence(&words, i) {
            q.recurrence = Some(r);
            if let (Some(wd), None) = (wd, q.scheduled) {
                q.scheduled = Some(next_weekday(today - Duration::days(1), wd)); // heute zählt, wenn passend
            }
            i += n;
            continue;
        }
        if let Some((d, n)) = parse_date(&words, i, today) {
            q.scheduled = Some(d);
            i += n;
            continue;
        }
        if q.block.is_none() {
            if let Some((start, minutes, n)) = parse_time(&words, i) {
                q.block = Some(QuickBlock { start: hm(start), minutes });
                i += n;
                continue;
            }
        }
        if let Some(m) = parse_duration(w) {
            durations.push((desc.len(), w.clone(), m));
            i += 1;
            continue;
        }
        desc.push(w.clone());
        i += 1;
    }
    // Dauer gilt nur zusammen mit einer Uhrzeit, sonst bleibt sie Text
    match q.block.as_mut() {
        Some(b) if b.minutes.is_none() && durations.len() == 1 => b.minutes = Some(durations[0].2),
        _ => {
            for (at, w, _) in durations.into_iter().rev() {
                desc.insert(at, w);
            }
        }
    }
    if q.block.is_some() && q.scheduled.is_none() {
        q.scheduled = Some(today);
    }
    q.description = desc.join(" ");
    q
}

/// Dateiname für eine neue Inbox-Notiz aus dem Aufgabentext.
pub fn note_title(description: &str) -> String {
    let cleaned: String = description
        .chars()
        .map(|c| if "\\/:*?\"<>|#^[]".contains(c) || c.is_control() { ' ' } else { c })
        .collect();
    let t = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let t: String = t.chars().take(80).collect();
    let t = t.trim_end_matches('.').trim().to_string();
    if t.is_empty() { "Neue Aufgabe".into() } else { t }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-11-11 ist ein Mittwoch
    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 11).unwrap()
    }
    fn p(s: &str) -> QuickTask {
        parse_quick(s, today(), "#warten")
    }

    #[test]
    fn spec_example() {
        let q = p("Müller anrufen morgen @telefon !1 >Gartenhaus");
        assert_eq!(q.line(), "- [ ] Müller anrufen #@telefon ⏫ ⏳ 2026-11-12");
        assert_eq!(q.target.as_deref(), Some("Gartenhaus"));
        // Akzeptanzkriterium 6
        assert_eq!(p("Test morgen @telefon !1 >Gartenhaus").line(), "- [ ] Test #@telefon ⏫ ⏳ 2026-11-12");
    }

    #[test]
    fn dates() {
        assert_eq!(p("A heute").scheduled, Some(today()));
        assert_eq!(p("A übermorgen").scheduled.unwrap().to_string(), "2026-11-13");
        assert_eq!(p("A freitag").scheduled.unwrap().to_string(), "2026-11-13");
        assert_eq!(p("A mittwoch").scheduled.unwrap().to_string(), "2026-11-18", "heute zählt nicht");
        assert_eq!(p("A in 3 tagen").scheduled.unwrap().to_string(), "2026-11-14");
        assert_eq!(p("A in 2 weeks").scheduled.unwrap().to_string(), "2026-11-25");
        assert_eq!(p("A 2026-12-01").scheduled.unwrap().to_string(), "2026-12-01");
        assert_eq!(p("A 24.12.").scheduled.unwrap().to_string(), "2026-12-24");
        assert_eq!(p("A 3.1.").scheduled.unwrap().to_string(), "2027-01-03", "Vergangenes → nächstes Jahr");
        assert_eq!(p("A am mo").scheduled.unwrap().to_string(), "2026-11-16");
        let q = p("Bericht fällig freitag");
        assert_eq!((q.due.map(|d| d.to_string()), q.scheduled), (Some("2026-11-13".into()), None));
        assert_eq!(p("Report due tomorrow").line(), "- [ ] Report 📅 2026-11-12");
    }

    #[test]
    fn short_weekday_needs_am() {
        // „so“ und „do“ sind normale Wörter
        assert_eq!(p("Mach das so").description, "Mach das so");
        assert_eq!(p("Mach das so").scheduled, None);
    }

    #[test]
    fn recurrence_tags_priority() {
        let q = p("Rauchmelder testen jeden montag");
        assert_eq!(q.line(), "- [ ] Rauchmelder testen 🔁 every monday ⏳ 2026-11-16");
        assert_eq!(p("Pflanzen gießen täglich").recurrence.as_deref(), Some("every day"));
        assert_eq!(p("Auf Paket warten @haus !2").line(), "- [ ] Auf Paket #warten #@haus 🔼");
        assert_eq!(p("Lesen #buch").tags, vec!["#buch"]);
        assert_eq!(p("A >Release 2.0").target.as_deref(), Some("Release 2.0"));
        assert_eq!(p("A > ").target, None);
    }

    #[test]
    fn time_and_duration() {
        let b = |s: &str| p(s).block.map(|b| (b.start, b.minutes));
        assert_eq!(b("Konzept schreiben 14:00 90m"), Some(("14:00".into(), Some(90))));
        assert_eq!(p("Konzept schreiben 14:00 90m").line(), "- [ ] Konzept schreiben ⏳ 2026-11-11", "Zeit nicht in der Zeile");
        assert_eq!(b("A 9:30"), Some(("09:30".into(), None)));
        assert_eq!(b("A 14 Uhr 1h"), Some(("14:00".into(), Some(60))));
        assert_eq!(b("A um 14"), Some(("14:00".into(), None)));
        assert_eq!(b("A at 8:15 1,5h"), Some(("08:15".into(), Some(90))));
        assert_eq!(b("A 14-15:30"), Some(("14:00".into(), Some(90))));
        assert_eq!(b("A 14-16 Uhr"), Some(("14:00".into(), Some(120))));
        assert_eq!(b("A 14:00 2h30"), Some(("14:00".into(), Some(150))));
        assert_eq!(b("A 14:00 45min"), Some(("14:00".into(), Some(45))));
        let q = p("Arzt morgen 10:00 30m @telefon");
        assert_eq!((q.scheduled.unwrap().to_string(), q.description.as_str()), ("2026-11-12".into(), "Arzt"));
        // ohne Uhrzeit bleibt eine Dauer Text; Zahlen ohne Doppelpunkt sind keine Uhrzeit
        assert_eq!(p("Lauf 5m").description, "Lauf 5m");
        assert_eq!(p("Lauf 5m").block, None);
        assert_eq!(p("Seite 14 lesen").block, None);
        assert_eq!(p("Kapitel 3-4 lesen").block, None);
        assert_eq!(p("Version 25:00").block, None);
        assert_eq!(p("Zwei Uhr 14:00 15:00").description, "Zwei Uhr 15:00", "nur eine Uhrzeit");
    }

    #[test]
    fn titles() {
        assert_eq!(note_title("Was ist mit A/B? #x"), "Was ist mit A B x");
        assert_eq!(note_title("  "), "Neue Aufgabe");
    }
}
