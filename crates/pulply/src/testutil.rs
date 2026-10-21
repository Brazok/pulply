//! Test fixtures: the real OpenAPI schema captured from `pulp/pulp` and an offline session.

use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use pulply_api::{ClientConfig, PulpClient, Registry, Status, VerifySsl};

use crate::config::Config;
use crate::session::Session;

pub fn registry() -> Registry {
    let gz = include_bytes!("../../pulply-api/tests/fixtures/api.json.gz");
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(&gz[..]).read_to_end(&mut bytes).unwrap();
    Registry::from_json(&bytes, "/pulp/api/v3/", None).unwrap()
}

/// Session that never connects (no request is sent by view logic under test).
pub fn session(readonly: bool) -> Session {
    let client = PulpClient::new(ClientConfig {
        base_url: "http://pulp.invalid".parse().unwrap(),
        api_root: "/pulp/".into(),
        domain: None,
        username: Some("admin".into()),
        password: Some("x".into()),
        verify_ssl: VerifySsl::Bool(true),
        timeout: Duration::from_secs(1),
        readonly,
    })
    .unwrap();
    Session { client, status: Status::default(), registry: Some(Arc::new(registry())), refresh: Duration::from_secs(5) }
}

pub fn config() -> Config {
    Config {
        language: None,
        start_view: "dashboard".into(),
        refresh: 5,
        upload_chunk_size: 6 << 20,
        default_context: None,
        contexts: Vec::new(),
    }
}
