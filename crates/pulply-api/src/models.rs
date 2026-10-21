use serde::Deserialize;
use serde_json::Value;

/// One page of a `limit`/`offset` paginated list.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Page {
    pub count: u64,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub results: Vec<Value>,
}

/// Accessors on untyped Pulp resources.
pub trait ResourceExt {
    fn str_field(&self, key: &str) -> Option<&str>;
    fn href(&self) -> Option<&str> {
        self.str_field("pulp_href")
    }
    fn name(&self) -> Option<&str> {
        self.str_field("name")
    }
}

impl ResourceExt for Value {
    fn str_field(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }
}

/// Task href of an asynchronous operation response (`{"task": "/pulp/api/v3/tasks/…/"}`).
pub fn task_href(response: &Value) -> Option<&str> {
    response.get("task").and_then(Value::as_str)
}

/// Is this string a Pulp href (absolute API path)?
pub fn is_href(s: &str) -> bool {
    s.starts_with('/') && s.contains("/api/v3/") && s.ends_with('/')
}

/// Version number from a repository version href (`…/versions/12/` → `12`).
pub fn version_number(href: &str) -> Option<u64> {
    let mut parts = href.trim_end_matches('/').rsplit('/');
    let num = parts.next()?.parse().ok()?;
    (parts.next()? == "versions").then_some(num)
}

/// Repository href owning a version href (`…/<uuid>/versions/3/` → `…/<uuid>/`).
pub fn version_repository(href: &str) -> Option<&str> {
    let idx = href.trim_end_matches('/').rfind("/versions/")?;
    Some(&href[..=idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_href_helpers() {
        let href = "/pulp/api/v3/repositories/file/file/0190-ab/versions/12/";
        assert_eq!(version_number(href), Some(12));
        assert_eq!(version_repository(href), Some("/pulp/api/v3/repositories/file/file/0190-ab/"));
        assert_eq!(version_number("/pulp/api/v3/tasks/0190-ab/"), None);
        assert!(is_href(href));
        assert!(!is_href("rhel9"));
    }
}
