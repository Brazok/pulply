use std::collections::HashMap;

use pulply_api::ResourceExt;
use serde_json::Value;

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

    /// Records `pulp_href`/`prn` → `name` (or `username`) for a resource.
    pub fn learn(&mut self, item: &Value) {
        let Some(name) = item.name().or_else(|| item.str_field("username")) else { return };
        for key in [item.href(), item.str_field("prn")].into_iter().flatten() {
            self.insert(key, name);
        }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}
