//! Single-page application embedded at build time (`web/dist`, built by `npm run build`).
//! Unknown paths fall back to `index.html` so client-side routes can be reloaded.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};

use super::WebState;

#[derive(rust_embed::Embed)]
#[folder = "web/dist"]
#[allow_missing = true]
struct Assets;

pub fn available() -> bool {
    Assets::get("index.html").is_some()
}

const MISSING: &str = "<!doctype html><meta charset=utf-8><title>pulply</title>\
<p>This pulply build has no web interface. Build it with <code>npm ci && npm run build</code> \
in <code>crates/pulply/web</code>, then rebuild pulply.</p>";

/// `index.html`, with the access token injected on loopback addresses.
fn index(state: &WebState) -> Response {
    let Some(file) = Assets::get("index.html") else { return Html(MISSING).into_response() };
    let mut html = String::from_utf8_lossy(&file.data).into_owned();
    if state.local {
        let meta = format!(r#"<meta name="pulply-token" content="{}">"#, state.token);
        html = html.replacen("</head>", &format!("{meta}</head>"), 1);
    }
    ([(header::CACHE_CONTROL, "no-store")], Html(html)).into_response()
}

pub async fn serve(State(state): State<Arc<WebState>>, req: Request) -> Response {
    // Hashed bundles (`assets/`) are immutable; the other files are revalidated by `ETag`.
    let path = req.uri().path().trim_start_matches('/');
    if path.starts_with("api/") {
        return super::error(StatusCode::NOT_FOUND, "not_found", "unknown API route");
    }
    if path.is_empty() || path == "index.html" {
        return index(&state);
    }
    match Assets::get(path) {
        Some(file) => {
            let immutable = path.starts_with("assets/");
            let cache = if immutable { "public, max-age=31536000, immutable" } else { "no-cache" };
            let tag = format!("\"{}\"", hex::encode(&file.metadata.sha256_hash()[..16]));
            let headers = [(header::CACHE_CONTROL, cache.to_owned()), (header::ETAG, tag.clone())];
            if super::not_modified(req.headers(), &tag) {
                return (StatusCode::NOT_MODIFIED, headers).into_response();
            }
            ([(header::CONTENT_TYPE, file.metadata.mimetype().to_owned())], headers, file.data).into_response()
        }
        // Client-side route (`/c/lab/browse/repositories/rpm/rpm`), not a missing file.
        None if !path.rsplit('/').next().unwrap_or_default().contains('.') => index(&state),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
