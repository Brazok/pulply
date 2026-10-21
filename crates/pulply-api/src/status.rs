use std::fmt;

use serde::Deserialize;

/// Oldest pulpcore version pulply targets.
pub const MIN_SUPPORTED: Version = Version(3, 50, 0);

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Status {
    #[serde(default)]
    pub versions: Vec<ComponentVersion>,
    #[serde(default)]
    pub online_workers: Vec<AppStatus>,
    #[serde(default)]
    pub online_api_apps: Vec<AppStatus>,
    #[serde(default)]
    pub online_content_apps: Vec<AppStatus>,
    #[serde(default)]
    pub database_connection: Option<Connection>,
    #[serde(default)]
    pub redis_connection: Option<Connection>,
    #[serde(default)]
    pub storage: Option<Storage>,
    #[serde(default)]
    pub content_settings: Option<ContentSettings>,
    #[serde(default)]
    pub domain_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComponentVersion {
    pub component: String,
    pub version: String,
    #[serde(default)]
    pub package: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppStatus {
    pub name: String,
    #[serde(default)]
    pub last_heartbeat: Option<String>,
    #[serde(default)]
    pub current_task: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Connection {
    pub connected: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Storage {
    pub total: Option<u64>,
    pub used: Option<u64>,
    pub free: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContentSettings {
    pub content_origin: Option<String>,
    pub content_path_prefix: Option<String>,
}

impl Status {
    pub fn component(&self, name: &str) -> Option<&ComponentVersion> {
        self.versions.iter().find(|v| v.component == name)
    }

    pub fn core_version(&self) -> Option<Version> {
        self.component("core").and_then(|c| Version::parse(&c.version))
    }

    /// False when the server is older than [`MIN_SUPPORTED`] or reports no core version.
    pub fn supported(&self) -> bool {
        self.core_version().is_some_and(|v| v >= MIN_SUPPORTED)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32, pub u32, pub u32);

impl Version {
    /// Parses `3.85.0`, `3.85.0.dev`, `3.85`.
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split('.').map(|p| p.parse::<u32>());
        let major = parts.next()?.ok()?;
        let minor = parts.next()?.ok()?;
        let patch = parts.next().and_then(Result::ok).unwrap_or(0);
        Some(Self(major, minor, patch))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_status_payload() {
        let status: Status = serde_json::from_str(include_str!("../tests/fixtures/status.json")).unwrap();
        assert_eq!(status.core_version(), Some(Version(3, 122, 2)));
        assert!(status.supported());
        assert!(status.component("rpm").is_some());
        assert!(status.database_connection.unwrap().connected);
    }

    #[test]
    fn version_parsing_and_support_boundary() {
        assert_eq!(Version::parse("3.49.12"), Some(Version(3, 49, 12)));
        assert_eq!(Version::parse("3.50.0.dev"), Some(Version(3, 50, 0)));
        assert_eq!(Version::parse("3.50"), Some(Version(3, 50, 0)));
        assert_eq!(Version::parse("x"), None);
        assert!(Version(3, 49, 99) < MIN_SUPPORTED);
        assert!(Version(3, 50, 0) >= MIN_SUPPORTED);
    }
}
