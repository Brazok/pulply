//! User customization files next to `config.toml` (R14.2–R14.5): `hotkeys.toml`,
//! `aliases.toml`, `views.toml` and `plugins.toml`.

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

/// `plugins.toml` entry (R14.5): an external command run on the selected resource.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginFile {
    key: String,
    description: Option<String>,
    /// Type ids (`repositories/rpm/rpm`), categories (`distributions`), plugins (`rpm`) or `all`.
    #[serde(default)]
    scopes: Vec<String>,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    /// Run without suspending the TUI (output discarded).
    #[serde(default)]
    background: bool,
}

#[derive(Debug, Clone)]
pub struct Plugin {
    pub name: String,
    pub key: KeySpec,
    /// Key as written in the config, for hints.
    pub key_label: String,
    pub description: String,
    pub scopes: Vec<String>,
    pub command: String,
    pub args: Vec<String>,
    pub background: bool,
}

/// Variables offered to plugin arguments, also exported as `PULPLY_<NAME>`.
pub const PLUGIN_VARS: &[&str] = &["HREF", "NAME", "TYPE", "PRN", "BASE_URL", "API_URL", "CONTEXT", "DIST_URL", "USER"];

impl Plugin {
    /// Applies to a resource of type `id`, in `category`, from `plugin`.
    pub fn in_scope(&self, id: &str, category: &str, plugin: &str) -> bool {
        self.scopes.iter().any(|s| s == "all" || s == id || s == category || s == plugin)
    }
}

/// Replaces `$VAR` / `${VAR}` for known variables; unknown ones stay as written.
pub fn substitute(arg: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while let Some(i) = rest.find('$') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let (name, len) = match after.strip_prefix('{').and_then(|a| a.find('}').map(|end| &a[..end])) {
            Some(name) => (name, name.len() + 2),
            None => {
                let end = after.find(|c: char| !(c.is_ascii_uppercase() || c == '_')).unwrap_or(after.len());
                (&after[..end], end)
            }
        };
        match vars.get(name) {
            Some(value) => out.push_str(value),
            None => out.push_str(&rest[i..i + 1 + len]),
        }
        rest = &after[len..];
    }
    out.push_str(rest);
    out
}

#[derive(Debug, Clone, Default)]
pub struct Custom {
    pub hotkeys: Vec<(KeySpec, String)>,
    /// alias → command line.
    pub aliases: BTreeMap<String, String>,
    /// Resource type id (`repositories/rpm/rpm`) → columns.
    pub views: HashMap<String, Vec<ColumnSpec>>,
    pub plugins: Vec<Plugin>,
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
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct PluginsFile {
            #[serde(default)]
            plugins: BTreeMap<String, PluginFile>,
        }
        let plugins: PluginsFile = read(dir, "plugins.toml")?.unwrap_or(PluginsFile { plugins: BTreeMap::new() });
        for (name, p) in plugins.plugins {
            let key = KeySpec::parse(&p.key)
                .wrap_err_with(|| format!("{}: plugin '{name}'", dir.join("plugins.toml").display()))?;
            custom.plugins.push(Plugin {
                description: p.description.unwrap_or_else(|| name.clone()),
                name,
                key,
                key_label: p.key,
                scopes: p.scopes,
                command: p.command,
                args: p.args,
                background: p.background,
            });
        }
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
    fn plugin_arguments_substitute_known_variables() {
        let vars = HashMap::from([("NAME", "iso".to_owned()), ("DIST_URL", "http://h/pulp/content/iso/".to_owned())]);
        assert_eq!(substitute("curl -s $DIST_URL", &vars), "curl -s http://h/pulp/content/iso/");
        assert_eq!(substitute("${NAME}.tar $NAME-x", &vars), "iso.tar iso-x");
        assert_eq!(substitute("$HOME $1 $ ${NOPE", &vars), "$HOME $1 $ ${NOPE", "unknown variables stay as written");
    }

    #[test]
    fn plugins_load_with_scopes() {
        let dir = tempfile::tempdir().unwrap();
        let text = "[plugins.browse]\nkey = \"ctrl-o\"\nscopes = [\"distributions\"]\ncommand = \"xdg-open\"\nargs = [\"$DIST_URL\"]\nbackground = true\n";
        std::fs::write(dir.path().join("plugins.toml"), text).unwrap();
        let custom = Custom::load(Some(dir.path())).unwrap();
        let p = &custom.plugins[0];
        assert_eq!((p.name.as_str(), p.description.as_str(), p.background), ("browse", "browse", true));
        assert!(p.in_scope("distributions/file/file", "distributions", "file"));
        assert!(!p.in_scope("repositories/file/file", "repositories", "file"));

        std::fs::write(dir.path().join("plugins.toml"), "[plugins.x]\nkey = \"ctrl-o\"\n").unwrap();
        assert!(Custom::load(Some(dir.path())).is_err(), "command is required");
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
