//! Schnelleingabe (SPEC 6.2): natürliche Sprache (Deutsch/Englisch) → Tasks-Syntax.
//!
//! Beispiel: `Müller anrufen morgen @telefon !1 >Gartenhaus`
//! → `- [ ] Müller anrufen #@telefon ⏫ ⏳ <morgen>` mit Ziel „Gartenhaus“.
//! `>Ziel` steht am Ende und darf Leerzeichen enthalten.

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
        desc.push(w.clone());
        i += 1;
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
    fn titles() {
        assert_eq!(note_title("Was ist mit A/B? #x"), "Was ist mit A B x");
        assert_eq!(note_title("  "), "Neue Aufgabe");
    }
}
