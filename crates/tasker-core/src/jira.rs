//! Erkennung von Jira-Ticket-Schlüsseln im Text (SPEC 4, Vault-Markierung).

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

static RE_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b([A-Z][A-Z0-9]+)-(\d+)\b").unwrap());

/// Filter, welche Schlüssel als echte Tickets gelten.
#[derive(Debug, Clone, Default)]
pub struct KeyFilter {
    /// Projekt-Präfixe aus der Konfiguration, z. B. `ABC`
    pub prefixes: HashSet<String>,
    /// Schlüssel, die in den Jira-Ergebnissen vorkommen
    pub known_keys: HashSet<String>,
}

impl KeyFilter {
    pub fn new(prefixes: impl IntoIterator<Item = String>) -> Self {
        Self { prefixes: prefixes.into_iter().collect(), known_keys: HashSet::new() }
    }

    pub fn accepts(&self, prefix: &str, key: &str) -> bool {
        self.prefixes.contains(prefix) || self.known_keys.contains(key)
    }
}

/// Findet alle akzeptierten Ticket-Schlüssel in `text`, ohne Duplikate, in Fundreihenfolge.
pub fn find_keys(text: &str, filter: &KeyFilter) -> Vec<String> {
    let mut seen = HashSet::new();
    RE_KEY
        .captures_iter(text)
        .filter_map(|c| {
            let key = c[0].to_string();
            (filter.accepts(&c[1], &key) && seen.insert(key.clone())).then_some(key)
        })
        .collect()
}

/// Prüft, ob ein Wert ein syntaktisch gültiger Schlüssel ist (für das Frontmatter-Feld `jira`).
pub fn is_valid_key(s: &str) -> bool {
    RE_KEY.find(s).is_some_and(|m| m.start() == 0 && m.end() == s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_false_positives() {
        let f = KeyFilter::new(["ABC".to_string()]);
        let text = "Siehe ABC-123 und ABC-123, Encoding UTF-8, Entscheidung DR-05, ADR-009.";
        assert_eq!(find_keys(text, &f), vec!["ABC-123"]);
    }

    #[test]
    fn accepts_known_keys_from_jira() {
        let mut f = KeyFilter::new([]);
        f.known_keys.insert("XYZ-7".into());
        assert_eq!(find_keys("XYZ-7 und XYZ-8", &f), vec!["XYZ-7"]);
    }

    #[test]
    fn validates_keys() {
        assert!(is_valid_key("ABC-123"));
        assert!(!is_valid_key("abc-123"));
        assert!(!is_valid_key("ABC-123 x"));
    }
}
