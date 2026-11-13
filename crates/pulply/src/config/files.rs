//! Configuration files editable from `:config` (R14.6): `config.toml`, the annex files next
//! to it, skins and pulp-cli's `cli.toml`. A missing file is created from a template holding
//! the defaults; edits are validated by reloading the whole configuration.

use std::path::{Path, PathBuf};

use super::{Config, DEFAULT_REFRESH_SECS, parse_pulp_cli, pulp_cli_path};
use crate::theme::{self, Theme};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Main,
    Hotkeys,
    Aliases,
    Views,
    Plugins,
    Skin(String),
    PulpCli,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigFile {
    pub kind: Kind,
    pub path: PathBuf,
}

/// State of a file, shown in the `:config` list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Present,
    /// Missing: built-in defaults apply.
    Defaults,
    /// Missing skin shipped with pulply.
    Builtin,
    /// Present but not read (`import_pulp_cli = false`).
    Ignored,
}

impl ConfigFile {
    /// Name used by `:config <key>` (`main`, `hotkeys`, `skin/dark`, `pulp-cli`).
    pub fn key(&self) -> String {
        match &self.kind {
            Kind::Main => "main".into(),
            Kind::Hotkeys => "hotkeys".into(),
            Kind::Aliases => "aliases".into(),
            Kind::Views => "views".into(),
            Kind::Plugins => "plugins".into(),
            Kind::Skin(name) => format!("skin/{name}"),
            Kind::PulpCli => "pulp-cli".into(),
        }
    }

    /// File name as shown to the user (`skins/dark.toml`, `pulp/cli.toml`).
    pub fn label(&self) -> String {
        match &self.kind {
            Kind::Skin(name) => format!("skins/{name}.toml"),
            Kind::PulpCli => "pulp/cli.toml".into(),
            _ => self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        }
    }

    /// Locale key suffix of the description (`config.desc.<key>`).
    pub fn desc_key(&self) -> &'static str {
        match self.kind {
            Kind::Main => "main",
            Kind::Hotkeys => "hotkeys",
            Kind::Aliases => "aliases",
            Kind::Views => "views",
            Kind::Plugins => "plugins",
            Kind::Skin(_) => "skin",
            Kind::PulpCli => "pulp_cli",
        }
    }

    pub fn status(&self, config: &Config) -> Status {
        match (&self.kind, self.path.exists()) {
            (Kind::PulpCli, true) if !config.import_pulp_cli => Status::Ignored,
            (_, true) => Status::Present,
            (Kind::Skin(name), false) if theme::builtin(name).is_some() => Status::Builtin,
            (_, false) => Status::Defaults,
        }
    }

    /// Content offered when the file does not exist yet.
    pub fn template(&self) -> String {
        match &self.kind {
            Kind::Main => main_template(),
            Kind::Hotkeys => HOTKEYS.into(),
            Kind::Aliases => ALIASES.into(),
            Kind::Views => VIEWS.into(),
            Kind::Plugins => PLUGINS.into(),
            Kind::Skin(name) => match theme::builtin(name) {
                Some(text) => text.into(),
                None => format!("# pulply skin '{name}': unset colors come from `inherit`.\ninherit = \"dark\"\n"),
            },
            Kind::PulpCli => PULP_CLI.into(),
        }
    }

    /// Reloads the configuration as pulply would at startup (`config_path` as given to
    /// `--config`), plus the checks the startup does lazily: skins and the pulp-cli file.
    pub fn validate(&self, config_path: Option<&Path>, context: &str) -> Result<Config, String> {
        let config = Config::load(config_path).map_err(|e| format!("{e:#}"))?;
        let dir = config.dir.as_deref();
        match &self.kind {
            Kind::Skin(name) => {
                Theme::load(name, dir)?;
            }
            Kind::PulpCli if self.path.exists() => {
                let text = std::fs::read_to_string(&self.path).map_err(|e| format!("{}: {e}", self.path.display()))?;
                parse_pulp_cli(&text).map_err(|e| format!("{}: {e:#}", self.path.display()))?;
            }
            _ => {}
        }
        let skin = config.context(context).and_then(|c| c.cfg.skin.as_deref()).unwrap_or(&config.skin);
        Theme::load(skin, dir)?;
        Ok(config)
    }
}

/// Every editable file: `config.toml`, annex files, built-in and user skins, pulp-cli.
pub fn list(config: &Config) -> Vec<ConfigFile> {
    let mut out = Vec::new();
    if let Some(path) = &config.path {
        out.push(ConfigFile { kind: Kind::Main, path: path.clone() });
    }
    if let Some(dir) = &config.dir {
        for (kind, name) in [
            (Kind::Hotkeys, "hotkeys.toml"),
            (Kind::Aliases, "aliases.toml"),
            (Kind::Views, "views.toml"),
            (Kind::Plugins, "plugins.toml"),
        ] {
            out.push(ConfigFile { kind, path: dir.join(name) });
        }
        let skins = dir.join("skins");
        let mut names: Vec<String> = theme::builtin_names().map(str::to_owned).collect();
        names.extend(
            std::fs::read_dir(&skins)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "toml"))
                .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned())),
        );
        names.sort();
        names.dedup();
        out.extend(
            names.into_iter().map(|n| ConfigFile { path: skins.join(format!("{n}.toml")), kind: Kind::Skin(n) }),
        );
    }
    if let Some(path) = pulp_cli_path() {
        out.push(ConfigFile { kind: Kind::PulpCli, path });
    }
    out
}

/// `config.toml` holding the default values, ready to be changed.
fn main_template() -> String {
    format!(
        r#"# pulply configuration. Annex files (hotkeys, aliases, views, plugins, skins) live next to it.

# language = "fr"          # fr / en, else the system locale
start_view = "dashboard"   # or any `:` command, e.g. "repo rpm", "task"
refresh = {DEFAULT_REFRESH_SECS}                # seconds
upload_chunk_size = "6MiB" # chunk size for uploads
import_pulp_cli = true     # import the profiles of ~/.config/pulp/cli.toml
skin = "{skin}"              # built-in dark / light, or skins/<name>.toml
# default_context = "prod"

# [contexts.prod]
# base_url = "https://pulp.example.com"
# username = "admin"       # password from ~/.netrc, PULPLY_PASSWORD, or prompted
# readonly = true
# skin = "light"           # per-context skin
# verify_ssl = true        # or false, or a CA bundle path
# api_root = "/pulp/"
# domain = "acme"
# timeout = 30             # seconds, 0 = none
"#,
        skin = theme::DEFAULT_SKIN,
    )
}

const HOTKEYS: &str = r#"# pulply hotkeys: key → `:` command (hotkeys win over view keys).
# "shift-1" = "repo rpm"
# F2 = "task"
"#;

const ALIASES: &str = r#"# pulply aliases: extra `:` words, expanded before the rest of the command.
# rr = "repo rpm"
"#;

const VIEWS: &str = r#"# pulply columns per resource type id (as in `:` completion), JSON paths allowed.
# ["repositories/rpm/rpm"]
# columns = ["name", "latest_version_href", { path = "pulp_labels.env", header = "ENV" }, "pulp_created"]
"#;

const PLUGINS: &str = r#"# pulply plugins: external commands on the selected resource.
# Variables: $HREF $NAME $TYPE $PRN $BASE_URL $API_URL $CONTEXT $DIST_URL $USER
# [plugins.browse]
# key = "ctrl-o"
# description = "open in browser"
# scopes = ["distributions"]      # type ids, categories, plugin names or "all"
# command = "xdg-open"
# args = ["$DIST_URL"]
# background = true               # else the TUI is suspended until Enter is pressed
"#;

const PULP_CLI: &str = r#"# pulp-cli profiles: [cli] becomes context `default`, [cli-<name>] context `<name>`.
[cli]
base_url = "http://localhost:8080"
# username = "admin"
# password = "password"
# verify_ssl = true
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// The user's own pulp-cli file must not leak into the tests.
    const NO_IMPORT: &str = "import_pulp_cli = false\n";

    fn config_in(dir: &Path) -> Config {
        let path = dir.join("config.toml");
        if !path.exists() {
            std::fs::write(&path, NO_IMPORT).unwrap();
        }
        Config::load(Some(&path)).unwrap()
    }

    #[test]
    fn lists_annex_files_and_every_skin() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("skins")).unwrap();
        std::fs::write(dir.path().join("skins/prod.toml"), "inherit = \"light\"\n").unwrap();
        std::fs::write(dir.path().join("skins/dark.toml"), "accent = \"red\"\n").unwrap();
        let config = config_in(dir.path());
        let files = list(&config);
        let keys: Vec<String> = files.iter().map(ConfigFile::key).collect();
        assert_eq!(
            keys[..8],
            ["main", "hotkeys", "aliases", "views", "plugins", "skin/dark", "skin/light", "skin/prod"]
                .map(String::from)
        );
        let status = |key: &str| files.iter().find(|f| f.key() == key).unwrap().status(&config);
        assert_eq!(status("main"), Status::Present);
        assert_eq!(status("hotkeys"), Status::Defaults);
        assert_eq!(status("skin/dark"), Status::Present, "user file overrides the built-in skin");
        assert_eq!(status("skin/light"), Status::Builtin);
    }

    #[test]
    fn templates_are_valid_and_hold_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = config_in(dir.path());
        for file in list(&config).into_iter().filter(|f| !matches!(f.kind, Kind::Main | Kind::PulpCli)) {
            std::fs::create_dir_all(file.path.parent().unwrap()).unwrap();
            std::fs::write(&file.path, file.template()).unwrap();
            let reloaded = file.validate(config.path.as_deref(), "none");
            assert!(reloaded.is_ok(), "{}: {:?}", file.label(), reloaded.err());
        }
        let main: crate::config::FileConfig = toml::from_str(&main_template()).unwrap();
        let defaults = crate::config::FileConfig::default();
        assert_eq!((main.refresh, main.skin, main.start_view), (defaults.refresh, defaults.skin, defaults.start_view));
        assert!(parse_pulp_cli(PULP_CLI).unwrap().contains_key("default"));
    }

    #[test]
    fn invalid_files_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let config = config_in(dir.path());
        let main = ConfigFile { kind: Kind::Main, path: dir.path().join("config.toml") };
        std::fs::write(&main.path, format!("{NO_IMPORT}refresh = \"often\"\n")).unwrap();
        assert!(main.validate(config.path.as_deref(), "none").unwrap_err().contains("config.toml"));

        std::fs::write(&main.path, format!("{NO_IMPORT}skin = \"nope\"\n")).unwrap();
        assert!(main.validate(config.path.as_deref(), "none").unwrap_err().contains("unknown skin"));

        std::fs::write(&main.path, NO_IMPORT).unwrap();
        let skin = ConfigFile { kind: Kind::Skin("x".into()), path: dir.path().join("skins/x.toml") };
        std::fs::create_dir_all(dir.path().join("skins")).unwrap();
        std::fs::write(&skin.path, "acent = \"red\"\n").unwrap();
        assert!(skin.validate(config.path.as_deref(), "none").unwrap_err().contains("acent"));
    }
}
