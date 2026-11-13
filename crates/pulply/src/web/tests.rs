use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;
use wiremock::matchers::{basic_auth, body_string_contains, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::config::{Context, ContextConfig, Origin};

const TOKEN: &str = "s3cret";

async fn pulp() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/pulp/api/v3/status/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"domain_enabled": false, "versions": []})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/pulp/api/v3/login/"))
        .and(basic_auth("admin", "password"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"username": "admin"})))
        .mount(&server)
        .await;
    server
}

fn state(server: &MockServer, readonly: bool, listen: &str) -> Arc<WebState> {
    let mut config = crate::testutil::config();
    let cfg = |password: Option<&str>| ContextConfig {
        base_url: server.uri(),
        username: Some("admin".into()),
        password: password.map(Into::into),
        ..Default::default()
    };
    config.contexts = vec![
        Context { name: "lab".into(), origin: Origin::Pulply, cfg: cfg(Some("password")) },
        Context { name: "prompt".into(), origin: Origin::PulpCli, cfg: cfg(None) },
    ];
    let opts = ServeOptions {
        listen: listen.parse().unwrap(),
        readonly,
        context: Some("lab".into()),
        token: Some(TOKEN.into()),
        open: false,
    };
    let mut state = build_state(config, &opts).unwrap();
    // Isolate from the developer's ~/.netrc and PULPLY_PASSWORD.
    Arc::get_mut(&mut state).unwrap().netrc = Vec::new();
    state
}

fn req(method: &str, uri: &str) -> axum::http::request::Builder {
    Request::builder().method(method).uri(uri).header(header::HOST, "127.0.0.1:8484").header(TOKEN_HEADER, TOKEN)
}

async fn send(state: &Arc<WebState>, request: Request<Body>) -> (StatusCode, Value) {
    let resp = router(state.clone()).oneshot(request).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn connect(state: &Arc<WebState>, ctx: &str, body: Value) -> (StatusCode, Value) {
    let r = req("POST", &format!("/api/contexts/{ctx}/connect"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(state, r).await
}

#[tokio::test]
async fn api_requires_the_token_except_info() {
    let server = pulp().await;
    let st = state(&server, false, "127.0.0.1:8484");
    let anonymous = |uri: &str| Request::get(uri).header(header::HOST, "localhost:8484").body(Body::empty()).unwrap();
    let (status, body) = send(&st, anonymous("/api/info")).await;
    assert_eq!((status, body["auth"].as_str()), (StatusCode::OK, Some("page")));
    assert_eq!(send(&st, anonymous("/api/contexts")).await.0, StatusCode::UNAUTHORIZED);
    let wrong = req("GET", "/api/contexts").header(TOKEN_HEADER, "nope").body(Body::empty()).unwrap();
    assert_eq!(send(&st, wrong).await.0, StatusCode::UNAUTHORIZED);
    let (status, body) = send(&st, req("GET", "/api/contexts").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["default"], "lab");
    let prompt = body["contexts"].as_array().unwrap().iter().find(|c| c["name"] == "prompt").unwrap();
    assert_eq!((prompt["needs_password"].as_bool(), prompt["origin"].as_str()), (Some(true), Some("pulp-cli")));
}

#[tokio::test]
async fn loopback_rejects_foreign_hosts() {
    let server = pulp().await;
    let st = state(&server, false, "127.0.0.1:8484");
    let rebound = Request::get("/api/info").header(header::HOST, "evil.example:8484").body(Body::empty()).unwrap();
    assert_eq!(send(&st, rebound).await.0, StatusCode::MISDIRECTED_REQUEST);
    // Not on loopback: the token protects the API, any Host is accepted.
    let public = state(&server, false, "0.0.0.0:8484");
    let r = Request::get("/api/info").header(header::HOST, "pulply.example").body(Body::empty()).unwrap();
    let (status, body) = send(&public, r).await;
    assert_eq!((status, body["auth"].as_str()), (StatusCode::OK, Some("token")));
}

#[tokio::test]
async fn connect_prompts_for_missing_passwords() {
    let server = pulp().await;
    let st = state(&server, false, "127.0.0.1:8484");
    let (status, body) = connect(&st, "prompt", json!({})).await;
    assert_eq!((status, body["error"].as_str()), (StatusCode::UNAUTHORIZED, Some("password_required")));
    let (status, body) = connect(&st, "prompt", json!({"password": "password"})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!((body["username"].as_str(), body["access"].as_str()), (Some("admin"), Some("all")));
    assert_eq!(body["api_prefix"], "/pulp/api/v3/");
    assert_eq!(body["status_path"], "/pulp/api/v3/status/");
    assert_eq!(connect(&st, "nope", json!({})).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn proxy_relays_requests_with_server_credentials() {
    let server = pulp().await;
    Mock::given(method("GET"))
        .and(path("/pulp/api/v3/tasks/"))
        .and(query_param("state", "failed"))
        .and(basic_auth("admin", "password"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"count": 2, "results": []})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/pulp/api/v3/repositories/file/file/"))
        .and(body_string_contains("\"name\":\"iso\""))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"name": ["This field must be unique."]})))
        .mount(&server)
        .await;
    let st = state(&server, false, "127.0.0.1:8484");
    let get = || req("GET", "/api/contexts/lab/pulp/pulp/api/v3/tasks/?state=failed").body(Body::empty()).unwrap();
    assert_eq!(send(&st, get()).await.0, StatusCode::CONFLICT, "not connected yet");
    assert_eq!(connect(&st, "lab", json!({})).await.0, StatusCode::OK);
    let (status, body) = send(&st, get()).await;
    assert_eq!((status, body["count"].as_u64()), (StatusCode::OK, Some(2)));
    // Upstream errors are relayed as-is (validation messages for forms).
    let post = req("POST", "/api/contexts/lab/pulp/pulp/api/v3/repositories/file/file/")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"iso"}"#))
        .unwrap();
    let (status, body) = send(&st, post).await;
    assert_eq!((status, body["name"][0].as_str()), (StatusCode::BAD_REQUEST, Some("This field must be unique.")));
}

#[tokio::test]
async fn readonly_proxy_refuses_mutations_without_sending() {
    let server = pulp().await;
    // Opening the session cookie (`POST login/`) does not modify Pulp: it is the only POST allowed.
    Mock::given(method("POST"))
        .and(path("/pulp/api/v3/login/"))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST")).respond_with(ResponseTemplate::new(202)).expect(0).mount(&server).await;
    let st = state(&server, true, "127.0.0.1:8484");
    let (_, body) = connect(&st, "lab", json!({})).await;
    assert_eq!(body["readonly"], true);
    let post = req("POST", "/api/contexts/lab/pulp/pulp/api/v3/repair/").body(Body::from("{}")).unwrap();
    let (status, body) = send(&st, post).await;
    assert_eq!((status, body["error"].as_str()), (StatusCode::FORBIDDEN, Some("readonly")));
}

#[test]
fn upstream_path_keeps_query_and_rejects_other_origins() {
    let p = |u: &str| proxy::upstream_path(&u.parse().unwrap());
    assert_eq!(
        p("/api/contexts/lab/pulp/pulp/api/v3/content/?q=a%20b").as_deref(),
        Some("/pulp/api/v3/content/?q=a%20b")
    );
    assert_eq!(p("/api/contexts/lab/pulp//evil.example/x"), None);
    assert_eq!(p("/api/contexts/lab/other/x"), None);
}

#[tokio::test]
async fn unknown_api_routes_are_not_the_spa() {
    let server = pulp().await;
    let st = state(&server, false, "127.0.0.1:8484");
    let (status, _) = send(&st, req("GET", "/api/nope").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let page = Request::get("/ctx/lab/tasks").header(header::HOST, "localhost:8484").body(Body::empty()).unwrap();
    let resp = router(st.clone()).oneshot(page).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let html = String::from_utf8(to_bytes(resp.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
    if assets::available() {
        assert!(html.contains(&format!(r#"<meta name="pulply-token" content="{TOKEN}">"#)));
    } else {
        assert!(html.contains("no web interface"));
    }
}

#[tokio::test]
async fn responses_carry_security_headers() {
    let server = pulp().await;
    let st = state(&server, false, "127.0.0.1:8484");
    for uri in ["/", "/api/info", "/api/nope"] {
        let resp = router(st.clone()).oneshot(req("GET", uri).body(Body::empty()).unwrap()).await.unwrap();
        let h = resp.headers();
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff", "{uri}");
        assert_eq!(h[header::X_FRAME_OPTIONS], "DENY", "{uri}");
        assert!(h[header::CONTENT_SECURITY_POLICY].to_str().unwrap().contains("frame-ancestors 'none'"), "{uri}");
    }
}

#[tokio::test]
async fn unchanged_storage_state_is_not_sent_again() {
    let server = pulp().await;
    let st = state(&server, false, "127.0.0.1:8484");
    assert_eq!(connect(&st, "lab", json!({})).await.0, StatusCode::OK);
    let first =
        router(st.clone()).oneshot(req("GET", "/api/contexts/lab/storage").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let tag = first.headers()[header::ETAG].clone();
    let again =
        req("GET", "/api/contexts/lab/storage").header(header::IF_NONE_MATCH, tag.clone()).body(Body::empty()).unwrap();
    let resp = router(st.clone()).oneshot(again).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(resp.headers()[header::ETAG], tag);
    assert!(to_bytes(resp.into_body(), usize::MAX).await.unwrap().is_empty());
}

#[test]
fn if_none_match_uses_weak_comparison() {
    let tag = super::etag(b"{}");
    assert!(tag.starts_with("W/\""));
    let with = |v: &str| {
        let mut h = HeaderMap::new();
        h.insert(header::IF_NONE_MATCH, v.parse().unwrap());
        super::not_modified(&h, &tag)
    };
    assert!(with(&tag));
    assert!(with(&format!("\"other\", {}", tag.trim_start_matches("W/"))));
    assert!(with("*"));
    assert!(!with("\"other\""));
    assert!(!super::not_modified(&HeaderMap::new(), &tag));
}

#[test]
fn wildcard_addresses_list_the_network_interfaces() {
    let ifaces = || {
        [
            ("lo", "127.0.0.1"),
            ("eth0", "192.168.1.20"),
            ("docker0", "172.17.0.1"),
            ("eth0", "fe80::1"),
            ("eth0", "2001:db8::20"),
            ("wt0", "100.72.0.5"),
        ]
        .map(|(n, ip)| (n.to_owned(), ip.parse().unwrap()))
    };
    let hosts = |listen: &str| super::display_hosts(listen.parse().unwrap(), ifaces());
    assert_eq!(hosts("0.0.0.0:8484"), ["localhost", "192.168.1.20", "100.72.0.5"]);
    assert_eq!(hosts("[::]:8484"), ["localhost", "192.168.1.20", "[2001:db8::20]", "100.72.0.5"]);
    assert_eq!(hosts("10.0.0.5:8484"), ["10.0.0.5"]);
    assert_eq!(hosts("[::1]:8484"), ["[::1]"]);
}
