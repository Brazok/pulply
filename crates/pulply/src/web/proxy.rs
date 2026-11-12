//! Relay of browser requests to the connected Pulp server, with the server-side
//! credentials. Bodies are streamed both ways (chunked uploads, large lists).

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderName, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures::TryStreamExt;
use pulply_api::reqwest;

use super::{WebState, api_error, error};

/// Request headers relayed to Pulp (credentials and cookies are never relayed).
const REQUEST_HEADERS: [HeaderName; 4] =
    [header::CONTENT_TYPE, header::CONTENT_RANGE, header::ACCEPT, header::IF_NONE_MATCH];
/// Response headers relayed to the browser.
const RESPONSE_HEADERS: [HeaderName; 4] =
    [header::CONTENT_TYPE, header::CONTENT_DISPOSITION, header::ETAG, header::LAST_MODIFIED];

/// `/api/contexts/{ctx}/pulp/pulp/api/v3/tasks/?limit=5` → `/pulp/api/v3/tasks/?limit=5`,
/// taken from the raw URI so percent-encoding is preserved.
pub fn upstream_path(uri: &axum::http::Uri) -> Option<String> {
    let rest = uri.path().strip_prefix("/api/contexts/")?;
    let (_, after) = rest.split_once('/')?;
    let path = after.strip_prefix("pulp")?;
    if !path.starts_with('/') || path.starts_with("//") {
        return None;
    }
    Some(match uri.query() {
        Some(q) => format!("{path}?{q}"),
        None => path.to_owned(),
    })
}

pub async fn proxy(
    State(state): State<Arc<WebState>>,
    Path((ctx, _)): Path<(String, String)>,
    req: Request,
) -> Response {
    let Some(session) = state.session(&ctx).await else {
        return error(StatusCode::CONFLICT, "not_connected", format!("context {ctx} is not connected"));
    };
    let Some(path) = upstream_path(req.uri()) else {
        return error(StatusCode::BAD_REQUEST, "path", "invalid upstream path");
    };
    let Ok(method) = reqwest::Method::from_bytes(req.method().as_str().as_bytes()) else {
        return error(StatusCode::METHOD_NOT_ALLOWED, "method", "unsupported method");
    };
    let mut upstream = match session.client.raw(method.clone(), &path) {
        Ok(r) => r,
        Err(e) => return api_error(&e),
    };
    for name in &REQUEST_HEADERS {
        if let Some(v) = req.headers().get(name).and_then(|v| v.to_str().ok()) {
            upstream = upstream.header(name.as_str(), v);
        }
    }
    if let Some(len) = req.headers().get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok())
        && len != "0"
    {
        upstream = upstream.header("content-length", len);
    }
    if !matches!(method, reqwest::Method::GET | reqwest::Method::HEAD) {
        let stream = req.into_body().into_data_stream().map_err(std::io::Error::other);
        upstream = upstream.body(reqwest::Body::wrap_stream(stream));
    }
    tracing::debug!(%method, path, "proxy");
    let resp = match upstream.send().await {
        Ok(r) => r,
        Err(e) => return api_error(&pulply_api::Error::from(e)),
    };
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut out = Response::builder().status(status);
    for name in &RESPONSE_HEADERS {
        if let Some(v) = resp.headers().get(name.as_str()).and_then(|v| v.to_str().ok()) {
            out = out.header(name, v);
        }
    }
    out = out.header(header::CACHE_CONTROL, "no-store");
    let body = Body::from_stream(resp.bytes_stream().map_err(std::io::Error::other));
    out.body(body).unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}
