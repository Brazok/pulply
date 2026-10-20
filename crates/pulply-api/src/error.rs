use std::collections::BTreeMap;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("read-only mode: {method} {path} refused")]
    ReadOnly { method: &'static str, path: String },
    #[error("authentication failed (HTTP {status})")]
    Unauthorized { status: u16 },
    #[error("permission denied")]
    Forbidden,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation failed: {}", format_fields(.0))]
    Validation(BTreeMap<String, Vec<String>>),
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("network error: {0}")]
    Network(String),
    #[error("invalid response: {0}")]
    Decode(String),
    #[error("invalid configuration: {0}")]
    Config(String),
}

fn format_fields(fields: &BTreeMap<String, Vec<String>>) -> String {
    fields.iter().map(|(k, v)| format!("{k}: {}", v.join(", "))).collect::<Vec<_>>().join("; ")
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        // Walk the source chain: reqwest's top-level message alone ("error sending request")
        // hides the actual cause (DNS, refused, TLS...).
        let mut msg = e.to_string();
        let mut src = std::error::Error::source(&e);
        while let Some(s) = src {
            msg.push_str(": ");
            msg.push_str(&s.to_string());
            src = s.source();
        }
        Error::Network(msg)
    }
}

impl Error {
    /// Maps a non-success response body to a typed error.
    pub(crate) fn from_response(status: u16, path: &str, body: &str) -> Self {
        match status {
            401 => Error::Unauthorized { status },
            403 => Error::Forbidden,
            404 => Error::NotFound(path.to_owned()),
            400 => match serde_json::from_str::<serde_json::Value>(body) {
                Ok(serde_json::Value::Object(map)) => Error::Validation(
                    map.into_iter()
                        .map(|(k, v)| {
                            let msgs = match v {
                                serde_json::Value::Array(a) => a.into_iter().map(value_text).collect(),
                                other => vec![value_text(other)],
                            };
                            (k, msgs)
                        })
                        .collect(),
                ),
                _ => Error::Http { status, body: truncate(body) },
            },
            _ => Error::Http { status, body: truncate(body) },
        }
    }
}

fn value_text(v: serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    }
}

fn truncate(body: &str) -> String {
    const MAX: usize = 300;
    match body.char_indices().nth(MAX) {
        Some((i, _)) => format!("{}…", &body[..i]),
        None => body.to_owned(),
    }
}
