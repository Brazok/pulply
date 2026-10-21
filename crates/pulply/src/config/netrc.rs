//! Minimal `~/.netrc` reader (machine/default/login/password, `macdef` skipped).

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `None` for the `default` entry.
    pub machine: Option<String>,
    pub login: Option<String>,
    pub password: Option<String>,
}

pub fn default_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("NETRC") {
        return Some(PathBuf::from(p));
    }
    directories::BaseDirs::new().map(|d| d.home_dir().join(".netrc"))
}

pub fn parse(text: &str) -> Vec<Entry> {
    let mut words = Vec::new();
    let mut in_macdef = false;
    for line in text.lines() {
        if in_macdef {
            // A macro body ends at the first empty line.
            in_macdef = !line.trim().is_empty();
            continue;
        }
        for word in line.split_whitespace() {
            if word.starts_with('#') {
                break;
            }
            if word == "macdef" {
                in_macdef = true;
                break;
            }
            words.push(word);
        }
    }

    let mut entries = Vec::new();
    let mut current: Option<Entry> = None;
    let mut it = words.into_iter();
    while let Some(word) = it.next() {
        match word {
            "machine" | "default" => {
                entries.extend(current.take());
                let machine = if word == "machine" { it.next().map(str::to_owned) } else { None };
                current = Some(Entry { machine, login: None, password: None });
            }
            "login" | "password" | "account" => {
                let value = it.next().map(str::to_owned);
                if let Some(e) = current.as_mut() {
                    match word {
                        "login" => e.login = value,
                        "password" => e.password = value,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    entries.extend(current);
    entries
}

/// Finds credentials for `host`, preferring an exact machine match over `default`.
/// When `login` is given, only entries with that login (or no login) match.
pub fn lookup(entries: &[Entry], host: &str, login: Option<&str>) -> Option<Entry> {
    let login_ok = |e: &&Entry| match (login, e.login.as_deref()) {
        (Some(want), Some(have)) => want == have,
        _ => true,
    };
    entries
        .iter()
        .filter(|e| e.machine.as_deref() == Some(host))
        .find(login_ok)
        .or_else(|| entries.iter().filter(|e| e.machine.is_none()).find(login_ok))
        .cloned()
}

pub fn load(path: &Path) -> Vec<Entry> {
    std::fs::read_to_string(path).map(|t| parse(&t)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# comment
machine pulp.example.com login admin password s3cret
machine pulp.example.com
  login reader
  password r3ad
macdef init
  machine evil login x password y

default login anon password anonpw
";

    #[test]
    fn exact_machine_wins_over_default() {
        let e = parse(SAMPLE);
        let hit = lookup(&e, "pulp.example.com", None).unwrap();
        assert_eq!(hit.login.as_deref(), Some("admin"));
        assert_eq!(hit.password.as_deref(), Some("s3cret"));
    }

    #[test]
    fn login_filter_selects_matching_entry() {
        let e = parse(SAMPLE);
        let hit = lookup(&e, "pulp.example.com", Some("reader")).unwrap();
        assert_eq!(hit.password.as_deref(), Some("r3ad"));
    }

    #[test]
    fn macdef_body_is_ignored_and_default_applies() {
        let e = parse(SAMPLE);
        assert!(lookup(&e, "evil", None).is_some_and(|h| h.login.as_deref() == Some("anon")));
        assert_eq!(lookup(&e, "other.host", Some("nobody")), None);
    }
}
