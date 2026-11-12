//! `pulply serve`: the web interface. The server keeps the Pulp credentials, relays the
//! browser's requests to Pulp (`/api/contexts/{ctx}/pulp/...`), exposes the OpenAPI
//! registry built by `pulply-api`, and serves the single-page application embedded at
//! build time from `web/dist`.
//!
//! Every `/api` request carries `X-Pulply-Token`. On a loopback address the token is
//! written into the served page, and the `Host` header is checked so that another site
//! cannot reach the API through DNS rebinding; on any other address the token is only
//! printed at startup and must be entered in the browser.

mod api;
mod assets;
mod proxy;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::{Json, Router};
use color_eyre::eyre::{Result, WrapErr};
use pulply_api::{PulpClient, Registry, Status};
use serde_json::json;
use tokio::sync::{Mutex, RwLock};
use tower_http::compression::CompressionLayer;

use crate::config::{Config, netrc};
use crate::perms::Permissions;

pub const TOKEN_HEADER: &str = "x-pulply-token";

pub struct ServeOptions {
    pub listen: SocketAddr,
    pub readonly: bool,
    /// Context selected first by the interface.
    pub context: Option<String>,
    /// Fixed access token (`--token` / `PULPLY_WEB_TOKEN`), else random.
    pub token: Option<String>,
    /// Open the interface in the default browser once listening.
    pub open: bool,
}

/// One connected Pulp context.
pub struct WebSession {
    pub client: PulpClient,
    pub status: Status,
    pub perms: Permissions,
    pub ignored_domain: Option<String>,
    /// JSON export of the registry, loaded on first use.
    registry: Mutex<Option<Arc<Vec<u8>>>>,
}

pub struct WebState {
    pub config: Config,
    pub readonly: bool,
    pub default_context: Option<String>,
    pub token: String,
    /// Loopback: the token is embedded in the page and `Host` is checked.
    pub local: bool,
    pub port: u16,
    pub netrc: Vec<netrc::Entry>,
    pub sessions: RwLock<HashMap<String, Arc<WebSession>>>,
}

impl WebSession {
    fn new(connected: crate::session::Connected) -> Self {
        Self {
            client: connected.client,
            status: connected.status,
            perms: connected.perms,
            ignored_domain: connected.ignored_domain,
            registry: Mutex::new(None),
        }
    }

    /// Registry JSON, loaded (from the schema cache or the server) on first call or when `force`.
    pub async fn registry_json(&self, force: bool) -> Result<Arc<Vec<u8>>, pulply_api::Error> {
        let mut slot = self.registry.lock().await;
        if !force && let Some(json) = slot.as_ref() {
            return Ok(json.clone());
        }
        let reg: Registry = crate::session::load_registry(self.client.clone(), self.status.clone(), force).await?;
        let json = tokio::task::spawn_blocking(move || serde_json::to_vec(&reg.export()))
            .await
            .map_err(|e| pulply_api::Error::Decode(e.to_string()))?
            .map_err(|e| pulply_api::Error::Decode(e.to_string()))?;
        let json = Arc::new(json);
        *slot = Some(json.clone());
        Ok(json)
    }
}

impl WebState {
    pub async fn session(&self, ctx: &str) -> Option<Arc<WebSession>> {
        self.sessions.read().await.get(ctx).cloned()
    }

    /// `Host` values accepted on a loopback address.
    fn host_allowed(&self, host: &str) -> bool {
        let port = self.port;
        ["localhost", "127.0.0.1", "[::1]"].iter().any(|h| host == *h || host == format!("{h}:{port}"))
    }
}

/// JSON error `{error, detail}`.
pub fn error(status: StatusCode, code: &str, detail: impl Into<String>) -> Response {
    (status, Json(json!({"error": code, "detail": detail.into()}))).into_response()
}

/// Status and code for an error of the Pulp client.
pub fn api_error(e: &pulply_api::Error) -> Response {
    use pulply_api::Error as E;
    let (status, code) = match e {
        E::Unauthorized { .. } => (StatusCode::UNAUTHORIZED, "unauthorized"),
        E::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
        E::ReadOnly { .. } => (StatusCode::FORBIDDEN, "readonly"),
        E::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
        E::Validation(_) => (StatusCode::BAD_REQUEST, "validation"),
        E::Config(_) => (StatusCode::BAD_REQUEST, "config"),
        E::Network(_) => (StatusCode::BAD_GATEWAY, "network"),
        E::Http { .. } | E::Decode(_) => (StatusCode::BAD_GATEWAY, "upstream"),
    };
    error(status, code, e.to_string())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn host_ok(state: &WebState, headers: &HeaderMap) -> bool {
    !state.local || headers.get(header::HOST).and_then(|h| h.to_str().ok()).is_some_and(|h| state.host_allowed(h))
}

/// DNS-rebinding guard for every route.
async fn check_host(State(state): State<Arc<WebState>>, req: Request, next: Next) -> Response {
    if !host_ok(&state, req.headers()) {
        return error(StatusCode::MISDIRECTED_REQUEST, "host", "unexpected Host header");
    }
    next.run(req).await
}

/// Token check for `/api` routes (except `/api/info`).
async fn require_token(State(state): State<Arc<WebState>>, req: Request, next: Next) -> Response {
    // Exactly one value: an extra header must not be ignored.
    let mut values = req.headers().get_all(TOKEN_HEADER).iter();
    let given = match (values.next(), values.next()) {
        (Some(v), None) => v.as_bytes(),
        _ => b"",
    };
    if !constant_time_eq(given, state.token.as_bytes()) {
        return error(StatusCode::UNAUTHORIZED, "token", "missing or invalid access token");
    }
    next.run(req).await
}

pub fn router(state: Arc<WebState>) -> Router {
    let protected = Router::new()
        .route("/api/contexts", get(api::contexts))
        .route("/api/contexts/{ctx}/connect", post(api::connect))
        .route("/api/contexts/{ctx}/session", get(api::session).delete(api::disconnect))
        .route("/api/contexts/{ctx}/registry", get(api::registry))
        .route("/api/contexts/{ctx}/registry/refresh", post(api::refresh_registry))
        .route("/api/contexts/{ctx}/pulp/{*rest}", any(proxy::proxy))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_token));
    Router::new()
        .route("/api/info", get(api::info))
        .merge(protected)
        .fallback(assets::serve)
        .layer(middleware::from_fn_with_state(state.clone(), check_host))
        .layer(CompressionLayer::new())
        .with_state(state)
}

fn random_token() -> Result<String> {
    let mut buf = [0u8; 24];
    getrandom::fill(&mut buf).map_err(|e| color_eyre::eyre::eyre!("random token: {e}"))?;
    Ok(hex::encode(buf))
}

fn open_browser(url: &str) {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(cmd).arg(url).spawn() {
        tracing::warn!("cannot open a browser with {cmd}: {e}");
    }
}

pub fn build_state(config: Config, opts: &ServeOptions) -> Result<Arc<WebState>> {
    if let Some(name) = &opts.context {
        config.context(name).ok_or_else(|| color_eyre::eyre::eyre!("unknown context '{name}'"))?;
    }
    let token = match &opts.token {
        Some(t) if !t.is_empty() => t.clone(),
        _ => random_token()?,
    };
    let default_context = opts.context.clone().or_else(|| config.select_context(None).ok().map(|c| c.name.clone()));
    Ok(Arc::new(WebState {
        netrc: netrc::default_path().map(|p| netrc::load(&p)).unwrap_or_default(),
        config,
        readonly: opts.readonly,
        default_context,
        token,
        local: opts.listen.ip().is_loopback(),
        port: opts.listen.port(),
        sessions: RwLock::new(HashMap::new()),
    }))
}

pub async fn serve(config: Config, opts: ServeOptions) -> Result<()> {
    let state = build_state(config, &opts)?;
    let listener = tokio::net::TcpListener::bind(opts.listen)
        .await
        .wrap_err_with(|| format!("cannot listen on {}", opts.listen))?;
    let addr = listener.local_addr()?;
    let host = if addr.ip().is_unspecified() { "localhost".to_owned() } else { addr.ip().to_string() };
    let host = if addr.is_ipv6() && !addr.ip().is_unspecified() { format!("[{host}]") } else { host };
    let url = if state.local {
        format!("http://{host}:{}/", addr.port())
    } else {
        format!("http://{host}:{}/#token={}", addr.port(), state.token)
    };
    if !assets::available() {
        eprintln!("warning: this build has no web interface assets (build crates/pulply/web first)");
    }
    println!("pulply web interface: {url}");
    if !state.local {
        println!("listening on {addr}: anyone reaching this address with the token gets your Pulp access");
    }
    tracing::info!(%addr, "web interface listening");
    if opts.open {
        open_browser(&url);
    }
    axum::serve(listener, router(state)).with_graceful_shutdown(shutdown()).await?;
    Ok(())
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests;
