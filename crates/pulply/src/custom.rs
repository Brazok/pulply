//! User customization files next to `config.toml` (R14.2–R14.4): `hotkeys.toml`,
//! `aliases.toml` and `views.toml`.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use color_eyre::eyre::{Result, WrapErr, bail, eyre};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;

/// A key combination from the config (`shift-1`, `ctrl-r`, `alt-x`, `F2`, `R`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySpec {
    code: KeyCode,
    /// Ctrl/Alt only: Shift is folded into the character.
    mods: KeyModifiers,
}

/// US-layout characters typed with Shift on the digit row.
const SHIFTED_DIGITS: &[(char, char)] = &[
    ('1', '!'),
    ('2', '@'),
    ('3', '#'),
    ('4', '$'),
    ('5', '%'),
    ('6', '^'),
    ('7', '&'),
    ('8', '*'),
    ('9', '('),
    ('0', ')'),
];

impl KeySpec {
    pub fn parse(spec: &str) -> Result<Self> {
        let lower = spec.trim().to_ascii_lowercase();
        // `ctrl--` binds Ctrl and `-`: split the last char off before the modifiers.
        let (mods_part, key) = match lower.rsplit_once('-') {
            Some((m, "")) => (m.strip_suffix('-').unwrap_or(m), "-"),
            Some((m, k)) => (m, k),
            None => ("", lower.as_str()),
        };
        let mut mods = KeyModifiers::NONE;
        let mut shift = false;
        for m in mods_part.split('-').filter(|m| !m.is_empty()) {
            match m {
                "ctrl" | "control" | "c" => mods |= KeyModifiers::CONTROL,
                "alt" | "meta" | "m" => mods |= KeyModifiers::ALT,
                "shift" | "s" => shift = true,
                other => bail!("unknown modifier '{other}' in key '{spec}'"),
            }
        }
        // Single characters keep their case (`R` = Shift-r).
        let original_key = &spec.trim()[spec.trim().len() - key.len()..];
        let code = match key {
            "enter" | "return" => KeyCode::Enter,
            "tab" => KeyCode::Tab,
            "space" => KeyCode::Char(' '),
            "backspace" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "insert" | "ins" => KeyCode::Insert,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" => KeyCode::PageUp,
            "pagedown" => KeyCode::PageDown,
            k if k.len() > 1 && k.starts_with('f') && k[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)) => {
                KeyCode::F(k[1..].parse().unwrap_or(1))
            }
            _ => {
                let mut chars = original_key.chars();
                let (Some(c), None) = (chars.next(), chars.next()) else { bail!("unknown key '{spec}'") };
                let c = if shift {
                    SHIFTED_DIGITS.iter().find(|(d, _)| *d == c).map(|(_, s)| *s).unwrap_or(c.to_ascii_uppercase())
                } else {
                    c
                };
                KeyCode::Char(c)
            }
        };
        if shift && !matches!(code, KeyCode::Char(_)) {
            mods |= KeyModifiers::SHIFT;
        }
        Ok(Self { code, mods })
    }

    pub fn matches(&self, key: &KeyEvent) -> bool {
        let mut mods = key.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        if matches!(key.code, KeyCode::Char(_)) {
            mods.remove(KeyModifiers::SHIFT);
        }
        // Ctrl-letters arrive lower-case whatever the Shift state.
        let code = match (key.code, mods.contains(KeyModifiers::CONTROL)) {
            (KeyCode::Char(c), true) => KeyCode::Char(c.to_ascii_lowercase()),
            (code, _) => code,
        };
        let own = match (self.code, self.mods.contains(KeyModifiers::CONTROL)) {
            (KeyCode::Char(c), true) => KeyCode::Char(c.to_ascii_lowercase()),
            (code, _) => code,
        };
        code == own && mods == self.mods
    }
}

/// A `views.toml` column: a JSON path (`name`, `pulp_labels.env`, `artifacts.0`), with an
/// optional header.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ColumnSpec {
    Path(String),
    Full { path: String, header: Option<String> },
}

impl ColumnSpec {
    pub fn path(&self) -> &str {
        match self {
            Self::Path(p) | Self::Full { path: p, .. } => p,
        }
    }

    pub fn header(&self) -> String {
        match self {
            Self::Full { header: Some(h), .. } => h.clone(),
            _ => self.path().to_uppercase().replace('_', " "),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewFile {
    columns: Vec<ColumnSpec>,
}

#[derive(Debug, Clone, Default)]
pub struct Custom {
    pub hotkeys: Vec<(KeySpec, String)>,
    /// alias → command line.
    pub aliases: BTreeMap<String, String>,
    /// Resource type id (`repositories/rpm/rpm`) → columns.
    pub views: HashMap<String, Vec<ColumnSpec>>,
}

fn read<T: for<'de> Deserialize<'de>>(dir: &Path, name: &str) -> Result<Option<T>> {
    let path = dir.join(name);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).wrap_err_with(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).map(Some).wrap_err_with(|| format!("parsing {}", path.display()))
}

impl Custom {
    /// Loads the files present in `dir` (missing files = no customization).
    pub fn load(dir: Option<&Path>) -> Result<Self> {
        let Some(dir) = dir else { return Ok(Self::default()) };
        let mut custom = Self::default();
        for (key, cmd) in read::<BTreeMap<String, String>>(dir, "hotkeys.toml")?.unwrap_or_default() {
            let spec = KeySpec::parse(&key).wrap_err_with(|| format!("{}", dir.join("hotkeys.toml").display()))?;
            custom.hotkeys.push((spec, cmd));
        }
        custom.aliases = read(dir, "aliases.toml")?.unwrap_or_default();
        if let Some((alias, _)) = custom.aliases.iter().find(|(a, _)| a.contains(char::is_whitespace)) {
            return Err(eyre!("{}: alias '{alias}' contains a space", dir.join("aliases.toml").display()));
        }
        let views: BTreeMap<String, ViewFile> = read(dir, "views.toml")?.unwrap_or_default();
        custom.views = views.into_iter().map(|(id, v)| (id, v.columns)).collect();
        Ok(custom)
    }

    /// Command bound to `key`, if any.
    pub fn hotkey(&self, key: &KeyEvent) -> Option<&str> {
        self.hotkeys.iter().find(|(spec, _)| spec.matches(key)).map(|(_, cmd)| cmd.as_str())
    }

    /// `rr 2` with `rr = "repo rpm"` → `repo rpm 2`; other input unchanged.
    pub fn expand(&self, input: &str) -> String {
        let input = input.trim();
        let (head, rest) = input.split_once(char::is_whitespace).unwrap_or((input, ""));
        match self.aliases.get(head) {
            Some(cmd) if rest.is_empty() => cmd.clone(),
            Some(cmd) => format!("{cmd} {}", rest.trim()),
            None => input.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn key_specs_match_terminal_events() {
        let shift1 = KeySpec::parse("Shift-1").unwrap();
        assert!(shift1.matches(&ev(KeyCode::Char('!'), KeyModifiers::SHIFT)));
        assert!(shift1.matches(&ev(KeyCode::Char('!'), KeyModifiers::NONE)));
        assert!(!shift1.matches(&ev(KeyCode::Char('1'), KeyModifiers::NONE)));

        let ctrl_r = KeySpec::parse("ctrl-r").unwrap();
        assert!(ctrl_r.matches(&ev(KeyCode::Char('r'), KeyModifiers::CONTROL)));
        assert!(!ctrl_r.matches(&ev(KeyCode::Char('r'), KeyModifiers::NONE)));

        let upper = KeySpec::parse("R").unwrap();
        assert!(upper.matches(&ev(KeyCode::Char('R'), KeyModifiers::SHIFT)));
        assert!(!upper.matches(&ev(KeyCode::Char('r'), KeyModifiers::NONE)));
        assert_eq!(KeySpec::parse("shift-r").unwrap(), upper);

        assert!(KeySpec::parse("F2").unwrap().matches(&ev(KeyCode::F(2), KeyModifiers::NONE)));
        assert!(KeySpec::parse("ctrl--").unwrap().matches(&ev(KeyCode::Char('-'), KeyModifiers::CONTROL)));
        assert!(KeySpec::parse("hyper-x").is_err());
        assert!(KeySpec::parse("xy").is_err());
    }

    #[test]
    fn aliases_expand_the_first_word_only() {
        let custom = Custom { aliases: BTreeMap::from([("rr".into(), "repo rpm".into())]), ..Custom::default() };
        assert_eq!(custom.expand("rr"), "repo rpm");
        assert_eq!(custom.expand(" rr  extra "), "repo rpm extra");
        assert_eq!(custom.expand("repo rr"), "repo rr");
    }

    #[test]
    fn files_load_from_the_config_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hotkeys.toml"), "\"shift-1\" = \"repo rpm\"\n").unwrap();
        std::fs::write(dir.path().join("aliases.toml"), "rr = \"repo rpm\"\n").unwrap();
        std::fs::write(
            dir.path().join("views.toml"),
            "[\"repositories/rpm/rpm\"]\ncolumns = [\"name\", { path = \"pulp_labels.env\", header = \"ENV\" }]\n",
        )
        .unwrap();
        let custom = Custom::load(Some(dir.path())).unwrap();
        assert_eq!(custom.hotkey(&ev(KeyCode::Char('!'), KeyModifiers::SHIFT)), Some("repo rpm"));
        let cols = &custom.views["repositories/rpm/rpm"];
        assert_eq!(cols.iter().map(ColumnSpec::header).collect::<Vec<_>>(), ["NAME", "ENV"]);
        assert_eq!(cols[1].path(), "pulp_labels.env");

        std::fs::write(dir.path().join("views.toml"), "[x]\ncolums = []\n").unwrap();
        assert!(format!("{:#}", Custom::load(Some(dir.path())).unwrap_err()).contains("views.toml"));
    }
}
