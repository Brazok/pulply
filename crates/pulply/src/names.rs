use std::collections::HashMap;

use pulply_api::ResourceExt;
use serde_json::Value;

/// Fields tried, in order, for a resource's display name.
pub const LABEL_FIELDS: [&str; 4] = ["name", "username", "relative_path", "digest"];

/// href or PRN → display name, fed by every list/detail payload received.
#[derive(Debug, Default)]
pub struct NameCache(HashMap<String, String>);

impl NameCache {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn insert(&mut self, key: impl Into<String>, name: impl Into<String>) {
        self.0.insert(key.into(), name.into());
    }

    /// Display name of a resource: `name`, `username`, or `relative_path`/`digest` for unnamed content.
    pub fn label(item: &Value) -> Option<&str> {
        LABEL_FIELDS.into_iter().find_map(|k| item.str_field(k))
    }

    /// Records `pulp_href`/`prn` → [`NameCache::label`] for a resource.
    pub fn learn(&mut self, item: &Value) {
        let Some(name) = Self::label(item) else { return };
        for key in [item.href(), item.str_field("prn")].into_iter().flatten() {
            self.insert(key, name);
        }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}
