//! `.env` at the repo root, read the way `set -a; source ./.env` read it: a
//! value written there wins over the ambient environment, `${VAR}` expands
//! against what is already known, and single quotes suppress that expansion.
//!
//! Nothing is pushed into the process environment — a child that needs one of
//! these values is handed it explicitly on its `Command`.

use std::collections::BTreeMap;
use std::path::Path;

use crate::shell::Res;

pub struct Env {
    vars: BTreeMap<String, String>,
}

impl Env {
    /// Missing `.env` is not an error: local play needs none of these keys.
    pub fn load(root: &Path) -> Res<Self> {
        let path = root.join(".env");
        let mut vars = BTreeMap::new();
        if path.is_file() {
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("reading {}: {error}", path.display()))?;
            for line in text.lines() {
                let Some((key, value)) = parse_line(line) else {
                    continue;
                };
                let value = value.resolve(&vars);
                vars.insert(key.to_string(), value);
            }
        }
        Ok(Self { vars })
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.vars
            .get(key)
            .cloned()
            .or_else(|| std::env::var(key).ok())
            .filter(|value| !value.is_empty())
    }

    pub fn require(&self, key: &str) -> Res<String> {
        self.get(key)
            .ok_or_else(|| format!("set {key} in .env (see .env.example)"))
    }
}

enum Raw<'a> {
    Literal(&'a str),
    Expanded(&'a str),
}

impl Raw<'_> {
    fn resolve(&self, known: &BTreeMap<String, String>) -> String {
        match self {
            Self::Literal(value) => (*value).to_string(),
            Self::Expanded(value) => expand(value, known),
        }
    }
}

fn parse_line(line: &str) -> Option<(&str, Raw<'_>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    let value = value.trim();
    if let Some(inner) = quoted(value, '\'') {
        return Some((key, Raw::Literal(inner)));
    }
    if let Some(inner) = quoted(value, '"') {
        return Some((key, Raw::Expanded(inner)));
    }
    Some((key, Raw::Expanded(value)))
}

fn quoted(value: &str, quote: char) -> Option<&str> {
    let inner = value.strip_prefix(quote)?.strip_suffix(quote)?;
    Some(inner)
}

/// `${NAME}` and `$NAME`. An unset name expands to nothing, as in the shell.
fn expand(value: &str, known: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 1..];
        let (name, after) = if let Some(braced) = tail.strip_prefix('{') {
            match braced.split_once('}') {
                Some((name, after)) => (name, after),
                None => {
                    out.push_str(&rest[at..]);
                    return out;
                }
            }
        } else {
            let end = tail
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(tail.len());
            (&tail[..end], &tail[end..])
        };
        if name.is_empty() {
            out.push('$');
        } else {
            let value = known
                .get(name)
                .cloned()
                .or_else(|| std::env::var(name).ok())
                .unwrap_or_else(String::new);
            out.push_str(&value);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}
