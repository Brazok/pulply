//! `~/.config/pulply/config.toml` plus contexts imported from pulp-cli's `cli.toml`.

pub mod netrc;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use color_eyre::eyre::{Context as _, Result, bail, eyre};
use pulply_api::{ClientConfig, VerifySsl};
use serde::Deserialize;

pub const DEFAULT_REFRESH_SECS: u64 = 5;
pub const MIN_REFRESH_SECS: u64 = 1;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ContextConfig {
    pub base_url: String,
    pub api_root: Option<String>,
    pub domain: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub verify_ssl: Option<VerifySsl>,
    pub timeout: Option<u64>,
    pub readonly: bool,
    pub refresh: Option<u64>,
    /// Hide actions the user lacks permissions for (default true).
    pub rbac_filter: Option<bool>,
    /// Skin for this context (overrides the global `skin`).
    pub skin: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Pulply,
    PulpCli,
}

#[derive(Debug, Clone)]
pub struct Context {
    pub name: String,
    pub origin: Origin,
    pub cfg: ContextConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct FileConfig {
    language: Option<String>,
    start_view: String,
    refresh: u64,
    skin: String,
    upload_chunk_size: String,
    import_pulp_cli: bool,
    default_context: Option<String>,
    contexts: BTreeMap<String, ContextConfig>,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            language: None,
            start_view: "dashboard".into(),
            refresh: DEFAULT_REFRESH_SECS,
            skin: crate::theme::DEFAULT_SKIN.into(),
            upload_chunk_size: "6MiB".into(),
            import_pulp_cli: true,
            default_context: None,
            contexts: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub language: Option<String>,
    pub start_view: String,
    pub refresh: u64,
    /// Bytes per upload chunk.
    pub upload_chunk_size: usize,
    pub default_context: Option<String>,
    pub skin: String,
    /// Directory of the config file, holding the annex files (`skins/`, `hotkeys.toml`…).
    pub dir: Option<PathBuf>,
    /// `hotkeys.toml`, `aliases.toml`, `views.toml`.
    pub custom: crate::custom::Custom,
    /// Sorted by name.
    pub contexts: Vec<Context>,
}

/// `6MiB`, `512KiB`, `1GiB`, `1048576` → bytes (binary units; `KB`/`MB` accepted as aliases).
pub fn parse_size(text: &str) -> Result<usize> {
    let t = text.trim();
    let split = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    let (digits, unit) = t.split_at(split);
    let n: usize = digits.parse().map_err(|_| eyre!("invalid size '{text}'"))?;
    let factor = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1 << 10,
        "m" | "mb" | "mib" => 1 << 20,
        "g" | "gb" | "gib" => 1 << 30,
        other => bail!("unknown size unit '{other}' in '{text}'"),
    };
    match n.checked_mul(factor) {
        Some(0) | None => bail!("invalid size '{text}'"),
        Some(bytes) => Ok(bytes),
    }
}

pub fn config_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "pulply").map(|d| d.config_dir().to_path_buf())
}

pub fn pulp_cli_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.config_dir().join("pulp").join("cli.toml"))
}

impl Config {
    /// Loads the pulply config (missing file = defaults) and merges pulp-cli profiles.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let explicit = path.is_some();
        let path = path.map(Path::to_path_buf).or_else(|| config_dir().map(|d| d.join("config.toml")));
        let file = match &path {
            Some(p) if p.exists() => {
                let text = std::fs::read_to_string(p).wrap_err_with(|| format!("reading {}", p.display()))?;
                toml::from_str::<FileConfig>(&text).wrap_err_with(|| format!("parsing {}", p.display()))?
            }
            Some(p) if explicit => bail!("config file {} not found", p.display()),
            _ => FileConfig::default(),
        };
        let imported = match (file.import_pulp_cli, pulp_cli_path()) {
            (true, Some(p)) if p.exists() => {
                let text = std::fs::read_to_string(&p).wrap_err_with(|| format!("reading {}", p.display()))?;
                parse_pulp_cli(&text).wrap_err_with(|| format!("parsing {}", p.display()))?
            }
            _ => BTreeMap::new(),
        };
        let mut config = Self::merge(file, imported)?;
        config.dir = path.and_then(|p| p.parent().map(Path::to_path_buf));
        config.custom = crate::custom::Custom::load(config.dir.as_deref())?;
        Ok(config)
    }

    fn merge(file: FileConfig, imported: BTreeMap<String, ContextConfig>) -> Result<Self> {
        let mut contexts: BTreeMap<String, Context> = imported
            .into_iter()
            .map(|(name, cfg)| (name.clone(), Context { name, origin: Origin::PulpCli, cfg }))
            .collect();
        for (name, cfg) in file.contexts {
            contexts.insert(name.clone(), Context { name, origin: Origin::Pulply, cfg });
        }
        Ok(Self {
            language: file.language,
            start_view: file.start_view,
            refresh: file.refresh.max(MIN_REFRESH_SECS),
            upload_chunk_size: parse_size(&file.upload_chunk_size).wrap_err("upload_chunk_size")?,
            default_context: file.default_context,
            skin: file.skin,
            dir: None,
            custom: Default::default(),
            contexts: contexts.into_values().collect(),
        })
    }

    pub fn context(&self, name: &str) -> Option<&Context> {
        self.contexts.iter().find(|c| c.name == name)
    }

    /// Requested name > `default_context` > `default` > first by name.
    pub fn select_context(&self, requested: Option<&str>) -> Result<&Context> {
        if let Some(name) = requested {
            return self.context(name).ok_or_else(|| eyre!("unknown context '{name}'"));
        }
        if self.contexts.is_empty() {
            bail!(
                "no Pulp context configured: add [contexts.<name>] to {} or create ~/.config/pulp/cli.toml",
                config_dir().map(|d| d.join("config.toml").display().to_string()).unwrap_or_default()
            );
        }
        let by_default = self.default_context.as_deref().and_then(|n| self.context(n));
        Ok(by_default.or_else(|| self.context("default")).unwrap_or(&self.contexts[0]))
    }
}

/// `[cli]` becomes context `default`, `[cli-<name>]` becomes `<name>`.
/// Sections without `base_url` are skipped.
fn parse_pulp_cli(text: &str) -> Result<BTreeMap<String, ContextConfig>> {
    let table: toml::Table = toml::from_str(text)?;
    let mut out = BTreeMap::new();
    for (key, value) in table {
        let name = match key.as_str() {
            "cli" => "default".to_owned(),
            k => match k.strip_prefix("cli-") {
                Some(rest) if !rest.is_empty() => rest.to_owned(),
                _ => continue,
            },
        };
        let cfg: ContextConfig = value.try_into().wrap_err_with(|| format!("section [{key}]"))?;
        if cfg.base_url.is_empty() {
            tracing::warn!(section = key, "pulp-cli profile without base_url skipped");
            continue;
        }
        out.insert(name, cfg);
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: Option<String>,
}

impl ContextConfig {
    /// Username/password from config, then `~/.netrc`, then `PULPLY_PASSWORD`.
    /// `None` means anonymous; `password: None` means the UI must prompt.
    pub fn credentials(&self, netrc_entries: &[netrc::Entry]) -> Option<Credentials> {
        let host = url::Url::parse(&self.base_url).ok()?.host_str().map(str::to_owned);
        let from_netrc = host.and_then(|h| netrc::lookup(netrc_entries, &h, self.username.as_deref()));
        let username = self.username.clone().or_else(|| from_netrc.as_ref().and_then(|e| e.login.clone()))?;
        let password = self
            .password
            .clone()
            .or_else(|| from_netrc.and_then(|e| e.password))
            .or_else(|| std::env::var("PULPLY_PASSWORD").ok().filter(|p| !p.is_empty()));
        Some(Credentials { username, password })
    }

    pub fn refresh(&self, global: u64) -> Duration {
        Duration::from_secs(self.refresh.unwrap_or(global).max(MIN_REFRESH_SECS))
    }

    pub fn client_config(&self, creds: Option<Credentials>, readonly: bool) -> Result<ClientConfig> {
        Ok(ClientConfig {
            base_url: url::Url::parse(&self.base_url)
                .wrap_err_with(|| format!("invalid base_url '{}'", self.base_url))?,
            api_root: self.api_root.clone().unwrap_or_else(|| "/pulp/".into()),
            domain: self.domain.clone(),
            username: creds.as_ref().map(|c| c.username.clone()),
            password: creds.and_then(|c| c.password),
            verify_ssl: self.verify_ssl.clone().unwrap_or(VerifySsl::Bool(true)),
            timeout: Duration::from_secs(self.timeout.unwrap_or(30)),
            readonly: readonly || self.readonly,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLI_TOML: &str = r#"
[cli]
base_url = "https://pulp.example.com"
username = "admin"
verify_ssl = "/etc/pki/ca.pem"
format = "json"

[cli-lab]
base_url = "http://localhost:8080"
username = "lab"
password = "pw"
timeout = 10

[cli-broken]
username = "nobody"

[other]
base_url = "http://ignored"
"#;

    #[test]
    fn imports_cli_profiles_and_skips_invalid_sections() {
        let ctx = parse_pulp_cli(CLI_TOML).unwrap();
        assert_eq!(ctx.keys().collect::<Vec<_>>(), ["default", "lab"]);
        assert_eq!(ctx["default"].verify_ssl, Some(VerifySsl::CaPath("/etc/pki/ca.pem".into())));
        assert_eq!(ctx["lab"].timeout, Some(10));
    }

    #[test]
    fn pulply_context_overrides_imported_profile() {
        let file: FileConfig = toml::from_str(
            r#"
refresh = 0
[contexts.lab]
base_url = "http://lab:9000"
readonly = true
"#,
        )
        .unwrap();
        let cfg = Config::merge(file, parse_pulp_cli(CLI_TOML).unwrap()).unwrap();
        let lab = cfg.context("lab").unwrap();
        assert_eq!(lab.origin, Origin::Pulply);
        assert_eq!(lab.cfg.base_url, "http://lab:9000");
        assert!(lab.cfg.readonly);
        assert_eq!(cfg.context("default").unwrap().origin, Origin::PulpCli);
        assert_eq!(cfg.refresh, MIN_REFRESH_SECS, "refresh is clamped to the minimum");
    }

    #[test]
    fn upload_chunk_sizes_parse_with_binary_units() {
        assert_eq!(parse_size("6MiB").unwrap(), 6 << 20);
        assert_eq!(parse_size(" 512 KiB ").unwrap(), 512 << 10);
        assert_eq!(parse_size("1048576").unwrap(), 1 << 20);
        assert!(parse_size("0").is_err());
        assert!(parse_size("6 parsecs").is_err());
        assert_eq!(Config::merge(FileConfig::default(), BTreeMap::new()).unwrap().upload_chunk_size, 6 << 20);
    }

    #[test]
    fn context_selection_precedence() {
        let mut cfg = Config::merge(FileConfig::default(), parse_pulp_cli(CLI_TOML).unwrap()).unwrap();
        assert_eq!(cfg.select_context(None).unwrap().name, "default");
        cfg.default_context = Some("lab".into());
        assert_eq!(cfg.select_context(None).unwrap().name, "lab");
        assert_eq!(cfg.select_context(Some("default")).unwrap().name, "default");
        assert!(cfg.select_context(Some("nope")).is_err());
    }

    #[test]
    fn credentials_fall_back_to_netrc() {
        let entries = netrc::parse("machine pulp.example.com login admin password fromnetrc");
        let cfg = ContextConfig { base_url: "https://pulp.example.com:8443".into(), ..Default::default() };
        let creds = cfg.credentials(&entries).unwrap();
        assert_eq!(creds, Credentials { username: "admin".into(), password: Some("fromnetrc".into()) });

        let explicit = ContextConfig { password: Some("cfg".into()), username: Some("admin".into()), ..cfg.clone() };
        assert_eq!(explicit.credentials(&entries).unwrap().password.as_deref(), Some("cfg"));

        let anonymous = ContextConfig { base_url: "https://other".into(), ..Default::default() };
        assert_eq!(anonymous.credentials(&entries), None);
    }
}
