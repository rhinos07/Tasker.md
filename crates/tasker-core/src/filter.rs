//! Eigene Filter (SPEC 6.3): eine kleine Abfragesprache über offene Aufgaben.
//!
//! ```text
//! kontext:@telefon & bereich:arbeit & !blockiert
//! (prio:hoch | fällig:<7d) & !warten
//! projekt:"Release 2.0" angebot          ← freier Text sucht in der Beschreibung
//! ```
//!
//! Operatoren: `!` (nicht) vor `&` (und) vor `|` (oder), Klammern. Zwei Begriffe ohne Operator
//! dazwischen sind mit „und“ verknüpft. Groß- und Kleinschreibung spielt keine Rolle.

use crate::index::{Index, Scope, TaskRef};
use crate::task::Priority;
use crate::vault::NoteKind;
use crate::Config;
use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};

/// Ein gespeicherter Filter aus der Konfiguration (`filters`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct SavedFilter {
    pub name: String,
    pub query: String,
}

/// Fehler beim Lesen einer Abfrage, mit Zeichenposition für die Anzeige.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterError {
    pub message: String,
    pub pos: usize,
}

impl std::fmt::Display for FilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (Zeichen {})", self.message, self.pos + 1)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum DateCond {
    /// Feld ohne Wert: Datum vorhanden
    Any,
    None,
    Today,
    Overdue,
    /// heute bis heute + n Tage
    Within(i64),
    On(NaiveDate),
}

#[derive(Debug, Clone, PartialEq)]
enum PrioCond {
    Any,
    High,
    Medium,
    Low,
    None,
}

#[derive(Debug, Clone, PartialEq)]
enum Term {
    Context(String),
    Tag(String),
    Area(String),
    Project(String),
    Path(String),
    Prio(PrioCond),
    Due(DateCond),
    Scheduled(DateCond),
    Blocked,
    InProgress,
    Waiting,
    Jira(Option<String>),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Term(Term),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

/// Eine gelesene Abfrage. Die leere Abfrage passt auf alles.
#[derive(Debug, Clone, PartialEq)]
pub struct Filter(Option<Expr>);

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    LParen,
    RParen,
    And,
    Or,
    Not,
    /// Feld (klein geschrieben) und Wert; ein freies Wort hat kein Feld
    Word { field: Option<String>, value: String },
}

fn err<T>(message: impl Into<String>, pos: usize) -> Result<T, FilterError> {
    Err(FilterError { message: message.into(), pos })
}

/// Liest einen Wert ab `i`: in Anführungszeichen oder bis zum nächsten Trenner.
fn read_value(chars: &[char], mut i: usize) -> Result<(String, usize), FilterError> {
    if chars.get(i) == Some(&'"') {
        let start = i;
        i += 1;
        let mut s = String::new();
        while let Some(&c) = chars.get(i) {
            if c == '"' {
                return Ok((s, i + 1));
            }
            s.push(c);
            i += 1;
        }
        return err("Anführungszeichen nicht geschlossen", start);
    }
    let mut s = String::new();
    while let Some(&c) = chars.get(i) {
        if c.is_whitespace() || matches!(c, '(' | ')' | '&' | '|') {
            break;
        }
        s.push(c);
        i += 1;
    }
    Ok((s, i))
}

/// Felder, die auch ohne Wert stehen dürfen.
const FLAGS: &[&str] =
    &["blockiert", "blocked", "warten", "waiting", "inarbeit", "doing", "jira", "prio", "priority", "fällig", "faellig", "due", "geplant", "scheduled"];

fn tokenize(q: &str) -> Result<Vec<(Tok, usize)>, FilterError> {
    let chars: Vec<char> = q.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let tok = match c {
            c if c.is_whitespace() => {
                i += 1;
                continue;
            }
            '(' => Some(Tok::LParen),
            ')' => Some(Tok::RParen),
            '&' => Some(Tok::And),
            '|' => Some(Tok::Or),
            '!' | '-' => Some(Tok::Not),
            _ => None,
        };
        if let Some(t) = tok {
            out.push((t, i));
            i += 1;
            continue;
        }
        let start = i;
        let (word, next) = read_value(&chars, i)?;
        // feld:wert, der Wert darf in Anführungszeichen stehen
        let (field, value, next) = match word.split_once(':') {
            Some((f, v)) if !f.is_empty() && f.chars().all(char::is_alphabetic) => {
                if v.is_empty() && chars.get(start + f.chars().count() + 1) == Some(&'"') {
                    let (v, n) = read_value(&chars, start + f.chars().count() + 1)?;
                    (Some(f.to_lowercase()), v, n)
                } else {
                    (Some(f.to_lowercase()), v.to_string(), next)
                }
            }
            // alleinstehende Schalter wie `blockiert` oder `prio` sind Felder ohne Wert
            _ if FLAGS.contains(&word.to_lowercase().as_str()) => (Some(word.to_lowercase()), String::new(), next),
            _ => (None, word, next),
        };
        out.push((Tok::Word { field, value }, start));
        i = next;
    }
    Ok(out)
}

fn date_cond(v: &str, pos: usize) -> Result<DateCond, FilterError> {
    let v = v.to_lowercase();
    Ok(match v.as_str() {
        "" => DateCond::Any,
        "keine" | "kein" | "none" => DateCond::None,
        "heute" | "today" => DateCond::Today,
        "überfällig" | "ueberfaellig" | "overdue" => DateCond::Overdue,
        _ => {
            if let Some(n) = v.strip_prefix('<').and_then(|s| s.strip_suffix('d').or_else(|| s.strip_suffix('t'))) {
                match n.parse::<i64>() {
                    Ok(n) if n >= 0 => DateCond::Within(n),
                    _ => return err(format!("Unbekannter Zeitraum „{v}“, z. B. <7d"), pos),
                }
            } else if let Ok(d) = NaiveDate::parse_from_str(&v, "%Y-%m-%d") {
                DateCond::On(d)
            } else {
                return err(format!("Unbekanntes Datum „{v}“: heute, überfällig, <7d, keine oder JJJJ-MM-TT"), pos);
            }
        }
    })
}

fn term(field: Option<&str>, value: &str, pos: usize) -> Result<Term, FilterError> {
    let lower = value.to_lowercase();
    let need = |t: Term| if value.is_empty() { err(format!("„{}:“ braucht einen Wert", field.unwrap_or("")), pos) } else { Ok(t) };
    Ok(match field {
        None => Term::Text(lower),
        Some("kontext" | "context" | "k") => need(Term::Context(lower.trim_start_matches('#').trim_start_matches('@').to_string()))?,
        Some("tag" | "t") => need(Term::Tag(lower.trim_start_matches('#').to_string()))?,
        Some("bereich" | "area") => need(Term::Area(lower))?,
        Some("projekt" | "project" | "p") => need(Term::Project(lower))?,
        Some("pfad" | "path") => need(Term::Path(lower))?,
        Some("prio" | "priority") => Term::Prio(match lower.as_str() {
            "" => PrioCond::Any,
            "hoch" | "high" => PrioCond::High,
            "mittel" | "medium" => PrioCond::Medium,
            "niedrig" | "low" => PrioCond::Low,
            "keine" | "none" => PrioCond::None,
            _ => return err(format!("Unbekannte Priorität „{value}“: hoch, mittel, niedrig, keine"), pos),
        }),
        Some("fällig" | "faellig" | "due") => Term::Due(date_cond(value, pos)?),
        Some("geplant" | "scheduled") => Term::Scheduled(date_cond(value, pos)?),
        Some("blockiert" | "blocked") if value.is_empty() => Term::Blocked,
        Some("warten" | "waiting") if value.is_empty() => Term::Waiting,
        Some("inarbeit" | "doing") if value.is_empty() => Term::InProgress,
        Some("jira") => Term::Jira((!value.is_empty()).then(|| value.to_uppercase())),
        Some(f) => return err(format!("Unbekanntes Feld „{f}“"), pos),
    })
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    i: usize,
    end: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i).map(|(t, _)| t)
    }
    fn pos(&self) -> usize {
        self.toks.get(self.i).map_or(self.end, |(_, p)| *p)
    }
    fn or(&mut self) -> Result<Expr, FilterError> {
        let mut e = self.and()?;
        while self.peek() == Some(&Tok::Or) {
            self.i += 1;
            e = Expr::Or(Box::new(e), Box::new(self.and()?));
        }
        Ok(e)
    }
    fn and(&mut self) -> Result<Expr, FilterError> {
        let mut e = self.not()?;
        loop {
            match self.peek() {
                Some(Tok::And) => self.i += 1,
                // ohne Operator: und
                Some(Tok::Word { .. } | Tok::Not | Tok::LParen) => {}
                _ => return Ok(e),
            }
            e = Expr::And(Box::new(e), Box::new(self.not()?));
        }
    }
    fn not(&mut self) -> Result<Expr, FilterError> {
        if self.peek() == Some(&Tok::Not) {
            self.i += 1;
            return Ok(Expr::Not(Box::new(self.not()?)));
        }
        self.atom()
    }
    fn atom(&mut self) -> Result<Expr, FilterError> {
        let pos = self.pos();
        match self.toks.get(self.i).cloned() {
            Some((Tok::LParen, _)) => {
                self.i += 1;
                let e = self.or()?;
                if self.peek() != Some(&Tok::RParen) {
                    return err("Schließende Klammer fehlt", self.pos());
                }
                self.i += 1;
                Ok(e)
            }
            Some((Tok::Word { field, value }, _)) => {
                self.i += 1;
                Ok(Expr::Term(term(field.as_deref(), &value, pos)?))
            }
            Some((Tok::RParen, _)) => err("Klammer ohne Gegenstück", pos),
            Some(_) => err("Hier fehlt ein Begriff", pos),
            None => err("Abfrage endet unerwartet", pos),
        }
    }
}

impl Filter {
    pub fn parse(q: &str) -> Result<Filter, FilterError> {
        let toks = tokenize(q)?;
        if toks.is_empty() {
            return Ok(Filter(None));
        }
        let mut p = Parser { toks, i: 0, end: q.chars().count() };
        let e = p.or()?;
        if p.i < p.toks.len() {
            let msg = if p.peek() == Some(&Tok::RParen) { "Klammer ohne Gegenstück" } else { "Unerwartetes Zeichen" };
            return err(msg, p.pos());
        }
        Ok(Filter(Some(e)))
    }

    pub fn matches(&self, t: &TaskRef, ctx: &Ctx) -> bool {
        self.0.as_ref().is_none_or(|e| eval(e, t, ctx))
    }
}

/// Was die Auswertung außer der Aufgabe braucht.
pub struct Ctx<'a> {
    pub index: &'a Index,
    pub cfg: &'a Config,
    pub today: NaiveDate,
}

fn date_ok(d: Option<NaiveDate>, c: &DateCond, today: NaiveDate) -> bool {
    match (c, d) {
        (DateCond::None, d) => d.is_none(),
        (_, None) => false,
        (DateCond::Any, Some(_)) => true,
        (DateCond::Today, Some(d)) => d == today,
        (DateCond::Overdue, Some(d)) => d < today,
        (DateCond::Within(n), Some(d)) => d >= today && d <= today + Duration::days(*n),
        (DateCond::On(x), Some(d)) => d == *x,
    }
}

fn eval(e: &Expr, t: &TaskRef, c: &Ctx) -> bool {
    match e {
        Expr::Not(x) => !eval(x, t, c),
        Expr::And(a, b) => eval(a, t, c) && eval(b, t, c),
        Expr::Or(a, b) => eval(a, t, c) || eval(b, t, c),
        Expr::Term(term) => {
            let k = &t.task;
            match term {
                Term::Context(v) => t.contexts().any(|x| x.trim_start_matches("#@").to_lowercase() == *v),
                Term::Tag(v) => t.all_tags().any(|x| x.trim_start_matches('#').to_lowercase() == *v),
                Term::Area(v) => {
                    t.area.as_deref().is_some_and(|a| a.to_lowercase() == *v)
                        || (matches!(v.as_str(), "arbeit" | "work") && t.scope == Scope::Work)
                        || (matches!(v.as_str(), "privat" | "private") && t.scope == Scope::Private)
                }
                Term::Project(v) => t.note_kind == NoteKind::Project && t.note_name.to_lowercase().contains(v.as_str()),
                Term::Path(v) => t.note_path.to_lowercase().contains(v.as_str()),
                Term::Prio(p) => match p {
                    PrioCond::Any => k.priority.is_set(),
                    PrioCond::High => matches!(k.priority, Priority::High | Priority::Highest),
                    PrioCond::Medium => k.priority == Priority::Medium,
                    PrioCond::Low => matches!(k.priority, Priority::Low | Priority::Lowest),
                    PrioCond::None => !k.priority.is_set(),
                },
                Term::Due(d) => date_ok(k.due, d, c.today),
                Term::Scheduled(d) => date_ok(t.scheduled(), d, c.today),
                Term::Blocked => t.blocked,
                Term::InProgress => k.in_progress,
                Term::Waiting => t.has_tag(&c.cfg.waiting_tag),
                Term::Jira(None) => c.index.jira_mentions.values().flatten().any(|n| n.path == t.note_path),
                Term::Jira(Some(key)) => {
                    k.raw.to_uppercase().contains(key.as_str())
                        || c.index.jira_assigned.get(key).is_some_and(|ns| ns.iter().any(|n| n.path == t.note_path))
                        || c.index.jira_mentions.get(key).is_some_and(|ns| ns.iter().any(|n| n.path == t.note_path))
                }
                Term::Text(v) => k.description.to_lowercase().contains(v.as_str()),
            }
        }
    }
}

/// Offene Aufgaben (ohne Journal), die zum Filter und zum Bereich passen.
pub fn apply<'a>(filter: &Filter, index: &'a Index, cfg: &Config, scope: Option<Scope>, today: NaiveDate) -> Vec<&'a TaskRef> {
    let journal = format!("{}/", cfg.folders.journal);
    let ctx = Ctx { index, cfg, today };
    index
        .tasks
        .iter()
        .filter(|t| !t.task.done && !t.note_path.starts_with(&journal))
        .filter(|t| scope.is_none_or(|s| t.scope == s))
        .filter(|t| filter.matches(t, &ctx))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(q: &str) -> FilterError {
        Filter::parse(q).unwrap_err()
    }

    #[test]
    fn parses_precedence_and_implicit_and() {
        let f = Filter::parse("kontext:@computer & !blockiert | prio:hoch").unwrap();
        let Some(Expr::Or(a, _)) = f.0 else { panic!("| bindet am schwächsten") };
        assert!(matches!(*a, Expr::And(_, _)));
        assert_eq!(
            Filter::parse("kontext:@computer !blockiert").unwrap(),
            Filter::parse("kontext:@computer & !blockiert").unwrap(),
            "ohne Operator heißt und"
        );
        assert_eq!(Filter::parse("  ").unwrap(), Filter(None));
        assert!(Filter::parse(r#"projekt:"Release 2.0" & (fällig:<7d | geplant:heute)"#).is_ok());
    }

    #[test]
    fn reports_errors_with_position() {
        assert_eq!(e("farbe:rot").pos, 0);
        assert!(e("farbe:rot").message.contains("farbe"));
        assert_eq!(e("(prio:hoch").message, "Schließende Klammer fehlt");
        assert_eq!(e("prio:hoch &").message, "Abfrage endet unerwartet");
        assert!(e("fällig:morgen").message.contains("Unbekanntes Datum"));
        assert!(e("kontext:").message.contains("braucht einen Wert"));
        assert_eq!(e("prio:hoch )").message, "Klammer ohne Gegenstück");
    }
}
