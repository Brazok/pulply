//! JSON endpoints: contexts, connection, session details and registry.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{WebSession, WebState, api_error, error};
use crate::config::Origin;
use crate::perms::Access;

/// Public: version and how the interface obtains its token.
pub async fn info(State(state): State<Arc<WebState>>) -> Json<Value> {
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "auth": if state.local { "page" } else { "token" },
    }))
}

pub async fn contexts(State(state): State<Arc<WebState>>) -> Json<Value> {
    let sessions = state.sessions.read().await;
    let list: Vec<Value> = state
        .config
        .contexts
        .iter()
        .map(|c| {
            let creds = c.cfg.credentials(&state.netrc);
            json!({
                "name": c.name,
                "base_url": c.cfg.base_url,
                "origin": match c.origin { Origin::Pulply => "pulply", Origin::PulpCli => "pulp-cli" },
                "username": creds.as_ref().map(|c| c.username.clone()),
                "needs_password": creds.as_ref().is_some_and(|c| c.password.is_none()),
                "readonly": state.readonly || c.cfg.readonly,
                "domain": c.cfg.domain,
                "connected": sessions.contains_key(&c.name),
            })
        })
        .collect();
    Json(json!({
        "contexts": list,
        "default": state.default_context,
        "refresh": state.config.refresh,
        "upload_chunk_size": state.config.upload_chunk_size,
    }))
}

#[derive(Deserialize, Default)]
pub struct ConnectBody {
    #[serde(default)]
    password: Option<String>,
}

/// Connects (or reconnects) a context. Without a stored password, answers 401
/// `password_required` so the interface can prompt.
pub async fn connect(
    State(state): State<Arc<WebState>>,
    Path(ctx): Path<String>,
    body: Option<Json<ConnectBody>>,
) -> Response {
    let Some(context) = state.config.context(&ctx).cloned() else {
        return error(StatusCode::NOT_FOUND, "unknown_context", format!("unknown context {ctx}"));
    };
    let given = body.and_then(|Json(b)| b.password).filter(|p| !p.is_empty());
    let creds = context.cfg.credentials(&state.netrc).map(|mut c| {
        if given.is_some() {
            c.password = given.clone();
        }
        c
    });
    if creds.as_ref().is_some_and(|c| c.password.is_none()) {
        return error(StatusCode::UNAUTHORIZED, "password_required", "password required");
    }
    match crate::session::connect(&context.cfg, creds, state.readonly).await {
        Ok(connected) => {
            let session = Arc::new(WebSession::new(connected));
            let body = session_json(&ctx, &context.cfg.base_url, &session);
            state.sessions.write().await.insert(ctx, session);
            Json(body).into_response()
        }
        Err(e @ pulply_api::Error::Unauthorized { .. }) => {
            error(StatusCode::UNAUTHORIZED, "bad_credentials", e.to_string())
        }
        Err(e) => api_error(&e),
    }
}

fn session_json(ctx: &str, base_url: &str, s: &WebSession) -> Value {
    let (access, permissions) = match &s.perms.access {
        Access::All => ("all", Vec::new()),
        Access::ObjectsOnly => ("objects", Vec::new()),
        Access::Limited(set) => {
            let mut v: Vec<String> = set.iter().cloned().collect();
            v.sort();
            ("limited", v)
        }
    };
    json!({
        "context": ctx,
        "base_url": base_url,
        "api_prefix": s.client.api_path(""),
        "status_path": s.client.status_path(),
        "domain": s.client.domain(),
        "ignored_domain": s.ignored_domain,
        "readonly": s.client.readonly(),
        "username": s.perms.username.clone().or_else(|| s.client.username().map(str::to_owned)),
        "access": access,
        "permissions": permissions,
        "versions": s.status.versions.iter().map(|v| json!({"component": v.component, "version": v.version})).collect::<Vec<_>>(),
        "domain_enabled": s.status.domain_enabled,
    })
}

pub async fn session(State(state): State<Arc<WebState>>, Path(ctx): Path<String>) -> Response {
    let (Some(s), Some(c)) = (state.session(&ctx).await, state.config.context(&ctx)) else {
        return error(StatusCode::CONFLICT, "not_connected", format!("context {ctx} is not connected"));
    };
    Json(session_json(&ctx, &c.cfg.base_url, &s)).into_response()
}

pub async fn disconnect(State(state): State<Arc<WebState>>, Path(ctx): Path<String>) -> StatusCode {
    state.sessions.write().await.remove(&ctx);
    StatusCode::NO_CONTENT
}

async fn registry_response(state: &WebState, ctx: &str, force: bool) -> Response {
    let Some(s) = state.session(ctx).await else {
        return error(StatusCode::CONFLICT, "not_connected", format!("context {ctx} is not connected"));
    };
    match s.registry_json(force).await {
        Ok(json) => ([(header::CONTENT_TYPE, "application/json")], json.as_ref().clone()).into_response(),
        Err(e) => api_error(&e),
    }
}

pub async fn registry(State(state): State<Arc<WebState>>, Path(ctx): Path<String>) -> Response {
    registry_response(&state, &ctx, false).await
}

pub async fn refresh_registry(State(state): State<Arc<WebState>>, Path(ctx): Path<String>) -> Response {
    registry_response(&state, &ctx, true).await
}
