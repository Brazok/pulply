use std::time::Duration;

use pulply_api::{ClientConfig, Error, Method, PulpClient, VerifySsl};
use serde_json::json;
use wiremock::matchers::{basic_auth, body_string_contains, header, header_regex, method, path};
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
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"v1\"")
                .set_body_json(json!({"count": 1, "results": [{"name": "t"}]})),
        )
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

#[test]
fn applies_domain_only_when_server_enables_domains() {
    let with = |domain: Option<&str>| {
        PulpClient::new(ClientConfig {
            base_url: "https://pulp.example.com/".parse().unwrap(),
            api_root: "/pulp/".into(),
            domain: domain.map(Into::into),
            username: None,
            password: None,
            verify_ssl: VerifySsl::Bool(true),
            timeout: Duration::from_secs(5),
            readonly: false,
        })
        .unwrap()
    };
    let disabled = with(Some("default")).with_domains(false);
    assert_eq!(disabled.api_path("tasks/"), "/pulp/api/v3/tasks/");
    assert_eq!(disabled.domain(), None);
    assert_eq!(with(Some("acme")).with_domains(true).api_path("tasks/"), "/pulp/acme/api/v3/tasks/");
    let fallback = with(None).with_domains(true);
    assert_eq!(fallback.api_path("tasks/"), "/pulp/default/api/v3/tasks/");
    assert_eq!(fallback.domain(), Some("default"));
}

#[tokio::test]
async fn reads_status_outside_the_domain() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/pulp/api/v3/status/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"domain_enabled": false})))
        .expect(1)
        .mount(&server)
        .await;
    let c = PulpClient::new(ClientConfig {
        base_url: server.uri().parse().unwrap(),
        api_root: "/pulp/".into(),
        domain: Some("default".into()),
        username: None,
        password: None,
        verify_ssl: VerifySsl::Bool(true),
        timeout: Duration::from_secs(5),
        readonly: false,
    })
    .unwrap();
    assert!(!c.status().await.unwrap().domain_enabled);
}

#[tokio::test]
async fn uploads_file_in_ranged_chunks() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/pulp/api/v3/uploads/"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"pulp_href": "/pulp/api/v3/uploads/u1/"})))
        .expect(1)
        .mount(&server)
        .await;
    for range in ["bytes 0-3/10", "bytes 4-7/10", "bytes 8-9/10"] {
        Mock::given(method("PUT"))
            .and(path("/pulp/api/v3/uploads/u1/"))
            .and(header("content-range", range))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(1)
            .mount(&server)
            .await;
    }
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pkg.rpm");
    std::fs::write(&file, b"0123456789").unwrap();
    let mut seen = Vec::new();
    let up = client(&server, false).upload_file(&file, 4, |sent, total| seen.push((sent, total))).await.unwrap();
    assert_eq!(up.href, "/pulp/api/v3/uploads/u1/");
    assert_eq!(up.size, 10);
    // sha256("0123456789")
    assert_eq!(up.sha256, "84d89877f0d4041efb6bf91a16f0248f2fd573e6af05c19f96bedb9f882f7882");
    assert_eq!(seen, [(0, 10), (4, 10), (8, 10), (10, 10)]);
}

#[tokio::test]
async fn readonly_refuses_uploads_and_form_posts() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any()).respond_with(ResponseTemplate::new(200)).expect(0).mount(&server).await;
    let c = client(&server, true);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f");
    std::fs::write(&file, b"x").unwrap();
    assert!(matches!(c.upload_file(&file, 4, |_, _| {}).await, Err(Error::ReadOnly { .. })));
    let form = [("upload".to_owned(), "/u/".to_owned())];
    assert!(matches!(c.post_form("/pulp/api/v3/content/file/files/", &form).await, Err(Error::ReadOnly { .. })));
    let multipart = c.mutate_multipart(Method::POST, "/pulp/api/v3/rpm/comps/", None, &[]).await;
    assert!(matches!(multipart, Err(Error::ReadOnly { .. })));
}

#[tokio::test]
async fn multipart_mutation_sends_fields_and_files() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/pulp/api/v3/rpm/comps/"))
        .and(header_regex("content-type", "^multipart/form-data"))
        .and(body_string_contains("name=\"repository\"\r\n\r\n/pulp/api/v3/repositories/rpm/rpm/1/"))
        .and(body_string_contains("filename=\"comps.xml\""))
        .and(body_string_contains("<comps/>"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({"task": "/pulp/api/v3/tasks/1/"})))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("comps.xml");
    std::fs::write(&file, b"<comps/>").unwrap();
    let body = json!({"repository": "/pulp/api/v3/repositories/rpm/rpm/1/"});
    let files = [("file".to_owned(), file)];
    let out =
        client(&server, false).mutate_multipart(Method::POST, "/pulp/api/v3/rpm/comps/", Some(&body), &files).await;
    assert_eq!(out.unwrap()["task"], "/pulp/api/v3/tasks/1/");
}
