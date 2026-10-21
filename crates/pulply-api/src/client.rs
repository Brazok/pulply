use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::{Method, StatusCode, header};
use serde::Deserialize;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::models::Page;
use crate::status::Status;

/// `verify_ssl` as accepted by pulp-cli: a boolean or a CA bundle path.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum VerifySsl {
    Bool(bool),
    CaPath(PathBuf),
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: url::Url,
    /// Pulp's `API_ROOT`, default `/pulp/`.
    pub api_root: String,
    pub domain: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub verify_ssl: VerifySsl,
    pub timeout: Duration,
    pub readonly: bool,
}

const ETAG_CACHE_MAX: usize = 512;

#[derive(Clone)]
pub struct PulpClient {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    /// `base_url` without trailing slash; hrefs (absolute paths) are appended to it.
    origin: String,
    api_root: String,
    api_prefix: String,
    domain: Option<String>,
    auth: Option<(String, Option<String>)>,
    readonly: bool,
    /// URL -> (ETag, body) for conditional GETs.
    etags: Mutex<HashMap<String, (String, Value)>>,
}

impl std::fmt::Debug for PulpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PulpClient")
            .field("origin", &self.inner.origin)
            .field("api_prefix", &self.inner.api_prefix)
            .field("readonly", &self.inner.readonly)
            .finish_non_exhaustive()
    }
}

fn normalize_root(root: &str) -> String {
    let trimmed = root.trim_matches('/');
    if trimmed.is_empty() { "/".into() } else { format!("/{trimmed}/") }
}

fn api_prefix(api_root: &str, domain: Option<&str>) -> String {
    match domain {
        Some(d) => format!("{api_root}{d}/api/v3/"),
        None => format!("{api_root}api/v3/"),
    }
}

impl PulpClient {
    pub fn new(cfg: ClientConfig) -> Result<Self> {
        let mut builder = reqwest::Client::builder()
            .timeout(cfg.timeout)
            .connect_timeout(cfg.timeout.min(Duration::from_secs(10)))
            .user_agent(concat!("pulply/", env!("CARGO_PKG_VERSION")));
        match &cfg.verify_ssl {
            VerifySsl::Bool(true) => {}
            VerifySsl::Bool(false) => builder = builder.danger_accept_invalid_certs(true),
            VerifySsl::CaPath(path) => {
                let pem = std::fs::read(path)
                    .map_err(|e| Error::Config(format!("reading CA bundle {}: {e}", path.display())))?;
                for cert in reqwest::Certificate::from_pem_bundle(&pem)
                    .map_err(|e| Error::Config(format!("parsing CA bundle {}: {e}", path.display())))?
                {
                    builder = builder.add_root_certificate(cert);
                }
            }
        }
        let http = builder.build().map_err(|e| Error::Config(e.to_string()))?;
        let api_root = normalize_root(&cfg.api_root);
        let api_prefix = api_prefix(&api_root, cfg.domain.as_deref());
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                origin: cfg.base_url.as_str().trim_end_matches('/').to_owned(),
                api_root,
                api_prefix,
                domain: cfg.domain,
                auth: cfg.username.map(|u| (u, cfg.password)),
                readonly: cfg.readonly,
                etags: Mutex::new(HashMap::new()),
            }),
        })
    }

    /// Client scoped as pulp-cli does: the configured domain (`default` when unset) when
    /// the server has domains enabled, no domain otherwise.
    pub fn with_domains(&self, enabled: bool) -> Self {
        let inner = &self.inner;
        let domain = enabled.then(|| inner.domain.clone().unwrap_or_else(|| "default".into()));
        Self {
            inner: Arc::new(Inner {
                http: inner.http.clone(),
                origin: inner.origin.clone(),
                api_root: inner.api_root.clone(),
                api_prefix: api_prefix(&inner.api_root, domain.as_deref()),
                domain,
                auth: inner.auth.clone(),
                readonly: inner.readonly,
                etags: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn readonly(&self) -> bool {
        self.inner.readonly
    }

    pub fn username(&self) -> Option<&str> {
        self.inner.auth.as_ref().map(|(u, _)| u.as_str())
    }

    pub fn domain(&self) -> Option<&str> {
        self.inner.domain.as_deref()
    }

    pub fn base_url(&self) -> &str {
        &self.inner.origin
    }

    /// Absolute path for an endpoint relative to `api/v3/`, e.g. `tasks/`.
    pub fn api_path(&self, rel: &str) -> String {
        format!("{}{}", self.inner.api_prefix, rel.trim_start_matches('/'))
    }

    pub fn url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_owned()
        } else {
            format!("{}{path}", self.inner.origin)
        }
    }

    pub(crate) fn request(&self, method: Method, url: &str) -> reqwest::RequestBuilder {
        let req = self.inner.http.request(method, url);
        match &self.inner.auth {
            Some((user, pass)) => req.basic_auth(user, pass.as_deref()),
            None => req,
        }
    }

    /// GET returning JSON. Uses `If-None-Match` when the server previously sent an ETag
    /// and serves the cached body on `304 Not Modified`.
    pub async fn get(&self, path: &str, query: &[(String, String)]) -> Result<Value> {
        let mut url =
            reqwest::Url::parse(&self.url(path)).map_err(|e| Error::Config(format!("invalid URL {path}: {e}")))?;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        let key = url.to_string();
        let cached_etag = self.etag_lookup(&key).map(|(etag, _)| etag);
        let mut req = self.request(Method::GET, url.as_str());
        if let Some(etag) = &cached_etag {
            req = req.header(header::IF_NONE_MATCH, etag);
        }
        let resp = req.send().await?;
        let status = resp.status();
        if status == StatusCode::NOT_MODIFIED
            && let Some((_, body)) = self.etag_lookup(&key)
        {
            return Ok(body);
        }
        let etag = resp.headers().get(header::ETAG).and_then(|v| v.to_str().ok()).map(str::to_owned);
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(Error::from_response(status.as_u16(), path, &text));
        }
        let body: Value = serde_json::from_str(&text).map_err(|e| Error::Decode(format!("{path}: {e}")))?;
        if let Some(etag) = etag {
            let mut cache = self.inner.etags.lock().expect("etag cache poisoned");
            if cache.len() >= ETAG_CACHE_MAX {
                cache.clear();
            }
            cache.insert(key, (etag, body.clone()));
        }
        Ok(body)
    }

    fn etag_lookup(&self, key: &str) -> Option<(String, Value)> {
        self.inner.etags.lock().expect("etag cache poisoned").get(key).cloned()
    }

    pub async fn list(&self, path: &str, query: &[(String, String)]) -> Result<Page> {
        let body = self.get(path, query).await?;
        serde_json::from_value(body).map_err(|e| Error::Decode(format!("{path}: {e}")))
    }

    /// Total `count` of a list endpoint (fetches a single item).
    pub async fn count(&self, path: &str, query: &[(String, String)]) -> Result<u64> {
        let mut q = query.to_vec();
        q.push(("limit".into(), "1".into()));
        q.push(("fields".into(), "pulp_href".into()));
        Ok(self.list(path, &q).await?.count)
    }

    /// `{api_root}api/v3/status/`, not domain-scoped.
    pub async fn status(&self) -> Result<Status> {
        let path = format!("{}api/v3/status/", self.inner.api_root);
        let body = self.get(&path, &[]).await?;
        serde_json::from_value(body).map_err(|e| Error::Decode(format!("status: {e}")))
    }

    /// Raw OpenAPI document (`{api_root}api/v3/docs/api.json`), not domain-scoped.
    pub async fn schema_bytes(&self) -> Result<Vec<u8>> {
        let path = format!("{}api/v3/docs/api.json", self.inner.api_root);
        let resp = self.request(Method::GET, &self.url(&path)).send().await?;
        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            return Err(Error::from_response(status.as_u16(), &path, &String::from_utf8_lossy(&bytes)));
        }
        Ok(bytes.to_vec())
    }

    /// POST/PUT/PATCH/DELETE. Refused locally in read-only mode, before anything is sent.
    pub async fn mutate(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value> {
        let name = match method {
            Method::POST => "POST",
            Method::PUT => "PUT",
            Method::PATCH => "PATCH",
            Method::DELETE => "DELETE",
            _ => return Err(Error::Config(format!("{method} is not a mutating method"))),
        };
        if self.inner.readonly {
            return Err(Error::ReadOnly { method: name, path: path.to_owned() });
        }
        let mut req = self.request(method, &self.url(path));
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(Error::from_response(status.as_u16(), path, &text));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).map_err(|e| Error::Decode(format!("{path}: {e}")))
    }
}
