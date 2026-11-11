//! Wiederkehrende Aufgaben (🔁): nächste Instanz berechnen, wie es das Obsidian-Tasks-Plugin macht (SPEC 6.1).
//!
//! Unterstützt:
//! - `every [N] day(s) | week(s) | month(s) | year(s)`, `every other week` usw.
//! - `every weekday` (Montag bis Freitag)
//! - `every monday[, thursday]`, `every [N] week(s) on monday[, thursday]`
//! - `every [N] month(s) on the 15th | on the last [day]`
//! - Zusatz `when done`: gerechnet ab dem Erledigt-Datum statt ab dem Termin
//!
//! Bezugsdatum ist 📅, sonst ⏳, sonst 🛫. Die übrigen Daten verschieben sich um denselben Abstand.
//! Hat die Aufgabe gar kein Datum, wird ab heute gerechnet und die neue Instanz bekommt ⏳.

use crate::task::Task;
use chrono::{Datelike, Duration, Months, NaiveDate, Weekday};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq)]
enum Rule {
    Days(u32),
    Weeks(u32),
    Months(u32),
    Years(u32),
    /// Montag bis Freitag
    Weekdays,
    /// alle `weeks` Wochen an diesen Wochentagen
    OnWeekdays { weeks: u32, days: Vec<Weekday> },
    /// alle `months` Monate am Tag `day` (`None` = letzter Tag)
    MonthDay { months: u32, day: Option<u32> },
}

#[derive(Debug, Clone, PartialEq)]
struct Recurrence {
    rule: Rule,
    when_done: bool,
}

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("Wiederholung „{0}“ wird noch nicht unterstützt, bitte in Obsidian abhaken")]
pub struct RecurError(pub String);

fn weekday(w: &str) -> Option<Weekday> {
    Some(match w.trim_end_matches('s') {
        "monday" | "mon" => Weekday::Mon,
        "tuesday" | "tue" => Weekday::Tue,
        "wednesday" | "wed" => Weekday::Wed,
        "thursday" | "thu" => Weekday::Thu,
        "friday" | "fri" => Weekday::Fri,
        "saturday" | "sat" => Weekday::Sat,
        "sunday" | "sun" => Weekday::Sun,
        _ => return None,
    })
}

/// Liste wie `monday, thursday` oder `monday and thursday`.
fn weekday_list(s: &str) -> Option<Vec<Weekday>> {
    let days: Option<Vec<Weekday>> = s
        .split([',', ' '])
        .map(str::trim)
        .filter(|w| !w.is_empty() && *w != "and")
        .map(weekday)
        .collect();
    days.filter(|d| !d.is_empty())
}

fn parse(rule: &str) -> Option<Recurrence> {
    static RE_INTERVAL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^every (?:(\d+|other) )?(day|week|month|year)s?(?: on (.+))?$").unwrap()
    });
    static RE_MONTHDAY: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^the (\d{1,2})(?:st|nd|rd|th)?$|^the last(?: day)?$").unwrap());

    let mut s = rule.trim().to_lowercase();
    let when_done = s.ends_with(" when done");
    if when_done {
        s.truncate(s.len() - " when done".len());
    }
    let s = s.trim();
    let r = |rule| Some(Recurrence { rule, when_done });

    if s == "every weekday" {
        return r(Rule::Weekdays);
    }
    if let Some(c) = RE_INTERVAL.captures(s) {
        let n = match c.get(1).map(|m| m.as_str()) {
            None => 1,
            Some("other") => 2,
            Some(d) => d.parse().ok().filter(|n| *n > 0)?,
        };
        let on = c.get(3).map(|m| m.as_str().trim());
        return match (&c[2], on) {
            ("day", None) => r(Rule::Days(n)),
            ("week", None) => r(Rule::Weeks(n)),
            ("month", None) => r(Rule::Months(n)),
            ("year", None) => r(Rule::Years(n)),
            ("week", Some(list)) => r(Rule::OnWeekdays { weeks: n, days: weekday_list(list)? }),
            ("month", Some(on)) => {
                let m = RE_MONTHDAY.captures(on)?;
                let day = match m.get(1) {
                    Some(d) => Some(d.as_str().parse().ok().filter(|d| (1..=31).contains(d))?),
                    None => None,
                };
                r(Rule::MonthDay { months: n, day })
            }
            _ => None,
        };
    }
    // every monday, thursday
    let list = s.strip_prefix("every ")?;
    r(Rule::OnWeekdays { weeks: 1, days: weekday_list(list)? })
}

fn add_months(d: NaiveDate, n: u32) -> NaiveDate {
    // chrono kappt am Monatsende: 31.01. + 1 Monat = 28./29.02.
    d.checked_add_months(Months::new(n)).unwrap_or(d)
}

fn last_of_month(year: i32, month: u32) -> NaiveDate {
    let first_next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    first_next.expect("gültiges Datum") - Duration::days(1)
}

/// Tag `day` im Monat von `d`, gekappt auf den Monatsletzten; `None` = Monatsletzter.
fn day_in_month(d: NaiveDate, day: Option<u32>) -> NaiveDate {
    let last = last_of_month(d.year(), d.month());
    match day {
        Some(day) => d.with_day(day.min(last.day())).unwrap_or(last),
        None => last,
    }
}

fn next_date(rule: &Rule, base: NaiveDate) -> NaiveDate {
    match rule {
        Rule::Days(n) => base + Duration::days(i64::from(*n)),
        Rule::Weeks(n) => base + Duration::days(7 * i64::from(*n)),
        Rule::Months(n) => add_months(base, *n),
        Rule::Years(n) => add_months(base, 12 * n),
        Rule::Weekdays => {
            let mut d = base + Duration::days(1);
            while matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
                d += Duration::days(1);
            }
            d
        }
        Rule::OnWeekdays { weeks, days } => {
            // erst ein späterer Tag in derselben Woche (Montag bis Sonntag) …
            let to_sunday = 6 - i64::from(base.weekday().num_days_from_monday());
            for i in 1..=to_sunday {
                let d = base + Duration::days(i);
                if days.contains(&d.weekday()) {
                    return d;
                }
            }
            // … sonst der erste passende Tag in der Woche, die `weeks` Wochen weiter beginnt
            let monday = base + Duration::days(to_sunday + 1 + 7 * (i64::from(*weeks) - 1));
            (0..7)
                .map(|i| monday + Duration::days(i))
                .find(|d| days.contains(&d.weekday()))
                .unwrap_or(monday)
        }
        Rule::MonthDay { months, day } => {
            let same = day_in_month(base, *day);
            if same > base {
                same
            } else {
                day_in_month(add_months(base.with_day(1).unwrap_or(base), *months), *day)
            }
        }
    }
}

/// Termine der nächsten Instanz.
#[derive(Debug, Clone, PartialEq)]
pub struct NextDates {
    pub due: Option<NaiveDate>,
    pub scheduled: Option<NaiveDate>,
    pub start: Option<NaiveDate>,
}

/// Berechnet die Termine der nächsten Instanz. `done_on` ist der Tag des Abhakens.
pub fn next_occurrence(task: &Task, done_on: NaiveDate) -> Result<NextDates, RecurError> {
    let text = task.recurrence.clone().unwrap_or_default();
    let rec = parse(&text).ok_or_else(|| RecurError(text.clone()))?;
    let reference = task.due.or(task.scheduled).or(task.start);
    let base = if rec.when_done { done_on } else { reference.unwrap_or(done_on) };
    let next = next_date(&rec.rule, base);
    Ok(match reference {
        Some(r) => {
            let delta = next - r;
            NextDates {
                due: task.due.map(|d| d + delta),
                scheduled: task.scheduled.map(|d| d + delta),
                start: task.start.map(|d| d + delta),
            }
        }
        None => NextDates { due: None, scheduled: Some(next), start: None },
    })
}

/// Die Zeile der nächsten Instanz: offen, neue Termine, ohne ✅/❌/🆔, ➕ auf heute.
/// `raw` ist die noch offene Originalzeile.
pub fn next_line(raw: &str, task: &Task, done_on: NaiveDate) -> Result<String, RecurError> {
    static RE_STATUS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*[-*+] \[).\]").unwrap());
    static RE_DATE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(📅|⏳|🛫|➕)(\u{FE0F}?\s*)\d{4}-\d{2}-\d{2}").unwrap());
    static RE_DROP: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s*(?:(?:✅|❌)\u{FE0F}?\s*\d{4}-\d{2}-\d{2}|🆔\s*[A-Za-z0-9_-]+)").unwrap());

    let next = next_occurrence(task, done_on)?;
    let fmt = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    let line = RE_STATUS.replace(raw, "$1 ]").into_owned();
    let line = RE_DROP.replace_all(&line, "").into_owned();
    let line = RE_DATE
        .replace_all(&line, |c: &regex::Captures| {
            let d = match &c[1] {
                "📅" => next.due,
                "⏳" => next.scheduled,
                "🛫" => next.start,
                _ => Some(done_on), // ➕
            };
            match d {
                Some(d) => format!("{}{}{}", &c[1], &c[2], fmt(d)),
                None => c[0].to_string(),
            }
        })
        .into_owned();
    let mut line = line.trim_end().to_string();
    if task.due.is_none() && task.scheduled.is_none() && task.start.is_none() {
        if let Some(s) = next.scheduled {
            line.push_str(&format!(" ⏳ {}", fmt(s)));
        }
    }
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::parse_task_line;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    /// Mittwoch
    const TODAY: &str = "2026-11-11";

    fn next(raw: &str) -> String {
        let t = parse_task_line(0, raw).unwrap();
        next_line(raw, &t, d(TODAY)).unwrap()
    }

    #[test]
    fn parses_rules() {
        let p = |s: &str| parse(s).map(|r| (r.rule, r.when_done));
        assert_eq!(p("every day"), Some((Rule::Days(1), false)));
        assert_eq!(p("every 3 days when done"), Some((Rule::Days(3), true)));
        assert_eq!(p("every other week"), Some((Rule::Weeks(2), false)));
        assert_eq!(p("Every Month"), Some((Rule::Months(1), false)));
        assert_eq!(p("every 2 years"), Some((Rule::Years(2), false)));
        assert_eq!(p("every weekday"), Some((Rule::Weekdays, false)));
        assert_eq!(
            p("every monday, thursday"),
            Some((Rule::OnWeekdays { weeks: 1, days: vec![Weekday::Mon, Weekday::Thu] }, false))
        );
        assert_eq!(
            p("every 2 weeks on friday"),
            Some((Rule::OnWeekdays { weeks: 2, days: vec![Weekday::Fri] }, false))
        );
        assert_eq!(p("every month on the 15th"), Some((Rule::MonthDay { months: 1, day: Some(15) }, false)));
        assert_eq!(p("every month on the last"), Some((Rule::MonthDay { months: 1, day: None }, false)));
        for bad in ["every", "jeden tag", "every 0 days", "every month on the 40th", "every blursday"] {
            assert_eq!(p(bad), None, "{bad}");
        }
    }

    #[test]
    fn shifts_all_dates_by_the_same_delta() {
        assert_eq!(
            next("- [ ] Gießen 🔁 every week ⏳ 2026-11-09 📅 2026-11-10"),
            "- [ ] Gießen 🔁 every week ⏳ 2026-11-16 📅 2026-11-17"
        );
        // ✅, 🆔 fallen weg, ⛔ und Tags bleiben, ➕ wird heute
        assert_eq!(
            next("  - [ ] Bericht #@computer 🔁 every month ➕ 2026-10-01 🆔 ab1 ⛔ xy 📅 2026-10-31"),
            "  - [ ] Bericht #@computer 🔁 every month ➕ 2026-11-11 ⛔ xy 📅 2026-11-30"
        );
    }

    #[test]
    fn when_done_counts_from_today() {
        assert_eq!(next("- [ ] Filter 🔁 every 3 days when done ⏳ 2026-11-01"), "- [ ] Filter 🔁 every 3 days when done ⏳ 2026-11-14");
    }

    #[test]
    fn without_dates_schedules_from_today() {
        assert_eq!(next("- [ ] Rasen 🔁 every week"), "- [ ] Rasen 🔁 every week ⏳ 2026-11-18");
    }

    #[test]
    fn weekday_rules() {
        // Mittwoch → Donnerstag, Freitag → Montag
        assert_eq!(next("- [ ] A 🔁 every weekday ⏳ 2026-11-11"), "- [ ] A 🔁 every weekday ⏳ 2026-11-12");
        assert_eq!(next("- [ ] A 🔁 every weekday ⏳ 2026-11-13"), "- [ ] A 🔁 every weekday ⏳ 2026-11-16");
        // Montag, Donnerstag: ab Mittwoch → Donnerstag, ab Donnerstag → Montag
        assert_eq!(next("- [ ] B 🔁 every monday, thursday ⏳ 2026-11-11"), "- [ ] B 🔁 every monday, thursday ⏳ 2026-11-12");
        assert_eq!(next("- [ ] B 🔁 every monday, thursday ⏳ 2026-11-12"), "- [ ] B 🔁 every monday, thursday ⏳ 2026-11-16");
        // alle 2 Wochen freitags, ab Freitag 13.11. → 27.11.
        assert_eq!(next("- [ ] C 🔁 every 2 weeks on friday 📅 2026-11-13"), "- [ ] C 🔁 every 2 weeks on friday 📅 2026-11-27");
    }

    #[test]
    fn month_end_is_clamped() {
        assert_eq!(next("- [ ] M 🔁 every month 📅 2027-01-31"), "- [ ] M 🔁 every month 📅 2027-02-28");
        assert_eq!(next("- [ ] M 🔁 every month on the last 📅 2027-01-31"), "- [ ] M 🔁 every month on the last 📅 2027-02-28");
        assert_eq!(next("- [ ] M 🔁 every month on the 15th 📅 2026-11-10"), "- [ ] M 🔁 every month on the 15th 📅 2026-11-15");
        assert_eq!(next("- [ ] M 🔁 every month on the 15th 📅 2026-11-15"), "- [ ] M 🔁 every month on the 15th 📅 2026-12-15");
        assert_eq!(next("- [ ] J 🔁 every year 📅 2028-02-29"), "- [ ] J 🔁 every year 📅 2029-02-28");
    }

    #[test]
    fn unknown_rule_is_an_error() {
        let raw = "- [ ] X 🔁 every blursday 📅 2026-11-11";
        let t = parse_task_line(0, raw).unwrap();
        assert_eq!(next_line(raw, &t, d(TODAY)).unwrap_err(), RecurError("every blursday".into()));
    }
}
