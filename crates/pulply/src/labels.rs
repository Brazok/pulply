//! Display labels for resource types (sidebar entries, view titles).

use pulply_api::ResourceType;
use rust_i18n::t;

/// `access_policies` → `Access policies`.
pub fn titleize(id: &str) -> String {
    let words = id.replace(['_', '-', '/'], " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Translation of a runtime-built key (`t!` borrows its key, so it cannot take a temporary).
pub fn tr(key: &str) -> String {
    t!(key).into_owned()
}

/// [`tr`] with a `%{count}` argument.
pub fn tr_count(key: &str, count: u64) -> String {
    t!(key, count = count).into_owned()
}

/// Translated label of a category or core resource id, falling back to [`titleize`].
pub fn category(id: &str) -> String {
    let key = format!("resource.{id}");
    let label = tr(&key);
    if label.ends_with(&key) { titleize(id) } else { label }
}

/// Label inside a group: `Repositories`, `Content: packages`, `Content guards: rbac`.
pub fn type_label(t: &ResourceType) -> String {
    if t.category == "other" {
        return titleize(&t.name);
    }
    if t.is_core() && !t.id.contains('/') {
        return category(&t.id);
    }
    let cat = category(&t.category);
    if t.name == t.plugin && t.category != "content" { cat } else { format!("{cat}: {}", t.name) }
}

/// View title: plugin-qualified for plugin types (`rpm · Repositories`).
pub fn type_title(t: &ResourceType) -> String {
    if t.is_core() { type_label(t) } else { format!("{} · {}", t.plugin, type_label(t)) }
}
