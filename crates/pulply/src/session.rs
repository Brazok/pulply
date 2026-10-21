//! Connection to one Pulp context: client, status, OpenAPI registry.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pulply_api::{Error, PulpClient, Registry, Status};
use sha2::{Digest, Sha256};

use crate::config::{ContextConfig, Credentials};
use crate::perms::{self, Permissions};

pub struct Session {
    pub client: PulpClient,
    pub status: Status,
    /// Loaded in the background after connect.
    pub registry: Option<Arc<Registry>>,
    pub refresh: Duration,
    pub perms: Permissions,
}

pub struct Connected {
    pub client: PulpClient,
    pub status: Status,
    pub perms: Permissions,
}

/// Builds the client, reads `status/`, validates credentials against `login/` and
/// loads the user's permissions.
pub async fn connect(cfg: &ContextConfig, creds: Option<Credentials>, readonly: bool) -> Result<Connected, Error> {
    let has_creds = creds.is_some();
    let client_cfg = cfg.client_config(creds, readonly).map_err(|e| Error::Config(e.to_string()))?;
    let client = PulpClient::new(client_cfg)?;
    let status = client.status().await?;
    let perms = if has_creds {
        client.get(&client.api_path("login/"), &[]).await?;
        if cfg.rbac_filter.unwrap_or(true) { perms::load(&client).await } else { Permissions::default() }
    } else {
        Permissions::default()
    };
    Ok(Connected { client, status, perms })
}

fn cache_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "pulply").map(|d| d.cache_dir().to_path_buf())
}

fn digest(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0]);
    }
    hex::encode(&h.finalize()[..8])
}

/// Cache key: server identity + every component version, so upgrades invalidate it.
fn cache_file(dir: &Path, client: &PulpClient, status: &Status) -> (PathBuf, String) {
    let server = digest(&[client.base_url(), client.domain().unwrap_or_default()]);
    let mut versions: Vec<String> = status.versions.iter().map(|v| format!("{}={}", v.component, v.version)).collect();
    versions.sort();
    let refs: Vec<&str> = versions.iter().map(String::as_str).collect();
    let file = dir.join(format!("schema-{server}-{}.json", digest(&refs)));
    (file, format!("schema-{server}-"))
}

/// Loads the registry from the disk cache, or downloads and caches the schema.
pub async fn load_registry(client: PulpClient, status: Status, force: bool) -> Result<Registry, Error> {
    let cached = cache_dir().map(|dir| {
        let (file, prefix) = cache_file(&dir, &client, &status);
        (dir, file, prefix)
    });
    let api_prefix = client.api_path("");
    let domain = client.domain().map(str::to_owned);
    if !force
        && let Some((_, file, _)) = &cached
        && let Ok(bytes) = tokio::fs::read(file).await
    {
        let (prefix, dom) = (api_prefix.clone(), domain.clone());
        let parsed = tokio::task::spawn_blocking(move || Registry::from_json(&bytes, &prefix, dom.as_deref()))
            .await
            .map_err(|e| Error::Decode(e.to_string()))?;
        match parsed {
            Ok(reg) => return Ok(reg),
            Err(e) => tracing::warn!("ignoring corrupt schema cache {}: {e}", file.display()),
        }
    }
    let bytes = client.schema_bytes().await?;
    if let Some((dir, file, prefix)) = &cached
        && let Err(e) = store(dir, file, prefix, &bytes).await
    {
        tracing::warn!("cannot cache schema in {}: {e}", dir.display());
    }
    tokio::task::spawn_blocking(move || Registry::from_json(&bytes, &api_prefix, domain.as_deref()))
        .await
        .map_err(|e| Error::Decode(e.to_string()))?
}

/// Atomic write, then removal of older schemas cached for the same server.
async fn store(dir: &Path, file: &Path, prefix: &str, bytes: &[u8]) -> std::io::Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    let tmp = file.with_extension("tmp");
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, file).await?;
    let mut entries = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(prefix) && path != file {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
    Ok(())
}
