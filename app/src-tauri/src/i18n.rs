//! Translations for text the desktop shell shows itself: the tray menu and
//! tooltip, system notifications and popup window titles.
//!
//! The interface owns the language (it loads the translation files), so it
//! sends this shell the strings it needs — including the engine's message
//! templates, so error notifications are translated too. Until then (or for
//! anything missing) the English text is used.

use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Default)]
struct Table {
    strings: HashMap<String, String>,
    /// Keys with {placeholders}, most specific first, split into parts.
    templates: Vec<(String, Vec<Part>)>,
}

#[derive(Clone, Debug, PartialEq)]
enum Part {
    Text(String),
    Var(String),
}

static TABLE: RwLock<Option<Table>> = RwLock::new(None);

fn parts(key: &str) -> Vec<Part> {
    let mut out = Vec::new();
    let mut rest = key;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}').map(|c| open + c) else { break };
        let name = &rest[open + 1..close];
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            break;
        }
        if open > 0 {
            out.push(Part::Text(rest[..open].to_string()));
        }
        out.push(Part::Var(name.to_string()));
        rest = &rest[close + 1..];
    }
    if !rest.is_empty() {
        out.push(Part::Text(rest.to_string()));
    }
    out
}

/// Replace the table (called by the interface whenever its language loads).
pub fn set(strings: HashMap<String, String>) {
    let mut templates: Vec<(String, Vec<Part>)> = strings.keys().filter(|k| k.contains('{')).map(|k| (k.clone(), parts(k))).collect();
    let literal = |p: &[Part]| p.iter().map(|x| if let Part::Text(t) = x { t.len() } else { 0 }).sum::<usize>();
    templates.sort_by_key(|t| std::cmp::Reverse(literal(&t.1)));
    *TABLE.write().unwrap_or_else(|p| p.into_inner()) = Some(Table { strings, templates });
}

/// Translate an interface string (English is the key).
pub fn tr(key: &str) -> String {
    TABLE.read().unwrap_or_else(|p| p.into_inner()).as_ref().and_then(|t| t.strings.get(key).cloned()).unwrap_or_else(|| key.to_string())
}

/// Translate a sentence and fill in its {placeholders}.
pub fn trf(key: &str, vars: &[(&str, &str)]) -> String {
    fill(&tr(key), vars)
}

fn fill(s: &str, vars: &[(&str, &str)]) -> String {
    let mut out = s.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// Match `s` against template parts; returns the placeholder values.
fn matches(parts: &[Part], s: &str) -> Option<Vec<(String, String)>> {
    let mut vars = Vec::new();
    let mut pos = 0;
    for (i, p) in parts.iter().enumerate() {
        match p {
            Part::Text(t) => {
                if !s[pos..].starts_with(t.as_str()) {
                    return None;
                }
                pos += t.len();
            }
            Part::Var(name) => {
                let end = match parts.get(i + 1) {
                    None => s.len(),
                    Some(Part::Text(next)) if i + 2 >= parts.len() => {
                        // The last literal must end the message.
                        let e = s.rfind(next.as_str())?;
                        if e + next.len() != s.len() {
                            return None;
                        }
                        e
                    }
                    Some(Part::Text(next)) => pos + 1 + s.get(pos + 1..)?.find(next.as_str())?,
                    Some(Part::Var(_)) => return None,
                };
                if end <= pos {
                    return None;
                }
                vars.push((name.clone(), s[pos..end].to_string()));
                pos = end;
            }
        }
    }
    (pos == s.len()).then_some(vars)
}

fn one(table: &Table, s: &str) -> Option<String> {
    if !s.contains('{') {
        if let Some(t) = table.strings.get(s) {
            return Some(t.clone());
        }
    }
    for (key, parts) in &table.templates {
        let Some(vars) = matches(parts, s) else { continue };
        let vars: Vec<(String, String)> = vars
            .into_iter()
            .map(|(k, v)| {
                // Values that are messages themselves are translated too.
                let v = if matches!(k.as_str(), "reason" | "error" | "detail") { te_with(table, &v) } else { v };
                (k, v)
            })
            .collect();
        let pairs: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        return Some(fill(table.strings.get(key).map(String::as_str).unwrap_or(key), &pairs));
    }
    None
}

fn te_with(table: &Table, msg: &str) -> String {
    if let Some(t) = one(table, msg) {
        return t;
    }
    // A known sentence followed by more text (aria2 appends its own detail):
    // translate the sentence, then whatever follows the same way.
    let mut from = 0;
    while let Some(i) = msg[from..].find(". ").map(|i| from + i) {
        if let Some(head) = one(table, &msg[..=i]) {
            return format!("{head} {}", te_with(table, &msg[i + 2..]));
        }
        from = i + 1;
    }
    msg.to_string()
}

/// Translate a message from the download engine; unknown text is kept.
pub fn te(msg: &str) -> String {
    match TABLE.read().unwrap_or_else(|p| p.into_inner()).as_ref() {
        Some(t) => te_with(t, msg),
        None => msg.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_fixed_templated_and_nested_messages() {
        let map: HashMap<String, String> = [
            ("The connection timed out.", "Délai dépassé."),
            ("Started over: {reason}.", "Reprise depuis le début : {reason}."),
            ("The server reported an internal error (HTTP {code}).", "Erreur interne du serveur (HTTP {code})."),
            ("Download failed: {name}", "Échec : {name}"),
            ("Pause all", "Tout suspendre"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        set(map);
        assert_eq!(tr("Pause all"), "Tout suspendre");
        assert_eq!(tr("Unknown"), "Unknown");
        assert_eq!(te("The server reported an internal error (HTTP 503)."), "Erreur interne du serveur (HTTP 503).");
        assert_eq!(te("Started over: The connection timed out.."), "Reprise depuis le début : Délai dépassé..");
        assert_eq!(te("Download failed: movie.mkv"), "Échec : movie.mkv");
        // Known sentence followed by raw server detail.
        assert_eq!(te("The connection timed out. Peer reset"), "Délai dépassé. Peer reset");
        assert_eq!(te("The connection timed out. peer reset by remote"), "Délai dépassé. peer reset by remote");
        assert_eq!(te("Something else entirely"), "Something else entirely");
    }
}
