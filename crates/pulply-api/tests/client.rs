use std::time::Duration;

use pulply_api::{ClientConfig, Error, Method, PulpClient, VerifySsl};
use serde_json::json;
use wiremock::matchers::{basic_auth, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(server: &MockServer, readonly: bool) -> PulpClient {
    PulpClient::new(ClientConfig {
        base_url: server.uri().parse().unwrap(),
        api_root: "/pulp/".into(),
        domain: None,
        username: Some("admin".into()),
        password: Some("password".into()),
        verify_ssl: VerifySsl::Bool(true),
        timeout: Duration::from_secs(5),
        readonly,
    })
    .unwrap()
}

#[tokio::test]
async fn readonly_refuses_mutations_without_sending() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE")).respond_with(ResponseTemplate::new(202)).expect(0).mount(&server).await;
    let c = client(&server, true);
    let err = c.mutate(Method::DELETE, "/pulp/api/v3/repositories/file/file/x/", None).await.unwrap_err();
    assert!(matches!(err, Error::ReadOnly { method: "DELETE", .. }), "{err:?}");
}

#[tokio::test]
async fn conditional_get_serves_cached_body_on_304() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/pulp/api/v3/tasks/"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/pulp/api/v3/tasks/"))
        .and(basic_auth("admin", "password"))
        .respond_with(ResponseTemplate::new(200).insert_header("ETag", "\"v1\"").set_body_json(json!({"count": 1, "results": [{"name": "t"}]})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let c = client(&server, false);
    let first = c.list("/pulp/api/v3/tasks/", &[]).await.unwrap();
    let second = c.list("/pulp/api/v3/tasks/", &[]).await.unwrap();
    assert_eq!(first.count, 1);
    assert_eq!(second.results, first.results);
}

#[tokio::test]
async fn maps_validation_and_permission_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/pulp/api/v3/repositories/file/file/"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"name": ["This field must be unique."]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/pulp/api/v3/users/"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({"detail": "nope"})))
        .mount(&server)
        .await;
    let c = client(&server, false);
    let err = c
        .mutate(Method::POST, "/pulp/api/v3/repositories/file/file/", Some(&json!({"name": "dup"})))
        .await
        .unwrap_err();
    match err {
        Error::Validation(fields) => assert_eq!(fields["name"], ["This field must be unique."]),
        other => panic!("expected validation error, got {other:?}"),
    }
    assert!(matches!(c.get("/pulp/api/v3/users/", &[]).await, Err(Error::Forbidden)));
}

#[test]
fn builds_domain_scoped_paths() {
    let c = PulpClient::new(ClientConfig {
        base_url: "https://pulp.example.com/".parse().unwrap(),
        api_root: "pulp".into(),
        domain: Some("acme".into()),
        username: None,
        password: None,
        verify_ssl: VerifySsl::Bool(true),
        timeout: Duration::from_secs(5),
        readonly: false,
    })
    .unwrap();
    assert_eq!(c.api_path("tasks/"), "/pulp/acme/api/v3/tasks/");
    assert_eq!(c.url("/pulp/acme/api/v3/tasks/"), "https://pulp.example.com/pulp/acme/api/v3/tasks/");
}
