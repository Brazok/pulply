//! Table columns derived from resource schemas.

use pulply_api::ResourceType;
use pulply_api::models::version_number;
use pulply_api::registry::Schema;
use ratatui::style::Style;
use serde_json::Value;

use super::Ctx;
use crate::format;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColKey {
    /// Scalar field rendered as text.
    Field(String),
    /// Href field rendered as a name.
    Href(String),
    /// `latest_version_href` → version number.
    VersionOf(String),
    /// First non-null of several href fields (distribution → repository/publication).
    AnyHref(Vec<String>),
    /// Timestamp rendered as an age.
    Age(String),
    /// Byte count.
    Bytes(String),
    /// `content_summary` of a repository version: `+added -removed =present`.
    Summary,
    TaskState,
    TaskName,
    TaskResources,
    TaskDuration,
}

#[derive(Debug, Clone)]
pub struct Column {
    pub header: String,
    pub key: ColKey,
}

impl Column {
    pub fn new(header: impl Into<String>, key: ColKey) -> Self {
        Self { header: header.into(), key }
    }

    /// API fields needed to render this column (for `?fields=`).
    pub fn api_fields(&self) -> Vec<&str> {
        match &self.key {
            ColKey::Field(f) | ColKey::Href(f) | ColKey::VersionOf(f) | ColKey::Age(f) | ColKey::Bytes(f) => vec![f],
            ColKey::AnyHref(fs) => fs.iter().map(String::as_str).collect(),
            ColKey::Summary => vec!["content_summary"],
            ColKey::TaskState => vec!["state"],
            ColKey::TaskName => vec!["name"],
            ColKey::TaskResources => vec!["reserved_resources_record"],
            ColKey::TaskDuration => vec!["started_at", "finished_at"],
        }
    }

    /// Field usable with `?ordering=`.
    pub fn sort_field(&self) -> Option<&str> {
        match &self.key {
            ColKey::Field(f) | ColKey::Age(f) | ColKey::Bytes(f) => Some(f),
            ColKey::TaskState => Some("state"),
            ColKey::TaskName => Some("name"),
            ColKey::TaskDuration => Some("started_at"),
            _ => None,
        }
    }

    /// Sort key for client-side sorting: numbers compare numerically, ages by timestamp.
    pub fn sort_value(&self, item: &Value, ctx: &Ctx) -> SortValue {
        let raw = |f: &str| item.get(f).cloned().unwrap_or(Value::Null);
        match &self.key {
            ColKey::Age(f) => SortValue::Text(item.get(f).and_then(Value::as_str).unwrap_or_default().to_owned()),
            ColKey::Field(f) | ColKey::Bytes(f) => match raw(f) {
                Value::Number(n) => SortValue::Num(n.as_f64().unwrap_or_default()),
                _ => SortValue::Text(self.text(item, ctx).0.to_lowercase()),
            },
            ColKey::VersionOf(f) => {
                SortValue::Num(item.get(f).and_then(Value::as_str).and_then(version_number).unwrap_or_default() as f64)
            }
            _ => SortValue::Text(self.text(item, ctx).0.to_lowercase()),
        }
    }

    pub fn text(&self, item: &Value, ctx: &Ctx) -> (String, Style) {
        let theme = ctx.theme;
        let str_of = |f: &str| item.get(f).and_then(Value::as_str);
        match &self.key {
            ColKey::Field(f) => (scalar(item.get(f)), Style::new()),
            ColKey::Href(f) => match str_of(f) {
                Some(h) => (format::href(h, ctx.names), Style::new()),
                None => (String::new(), Style::new()),
            },
            ColKey::AnyHref(fs) => fs
                .iter()
                .find_map(|f| str_of(f))
                .map(|h| (format::href(h, ctx.names), Style::new()))
                .unwrap_or_default(),
            ColKey::VersionOf(f) => {
                (str_of(f).and_then(version_number).map(|n| n.to_string()).unwrap_or_default(), Style::new())
            }
            ColKey::Age(f) => (str_of(f).map(|t| format::age(t, ctx.now)).unwrap_or_default(), theme.muted()),
            ColKey::Bytes(f) => {
                (item.get(f).and_then(Value::as_u64).map(format::bytes).unwrap_or_default(), Style::new())
            }
            ColKey::Summary => (summary(item.get("content_summary")), Style::new()),
            ColKey::TaskState => {
                let s = str_of("state").unwrap_or_default();
                (s.to_owned(), theme.task_state(s))
            }
            ColKey::TaskName => (str_of("name").map(format::task_name).unwrap_or_default(), Style::new()),
            ColKey::TaskResources => {
                let text = item
                    .get("reserved_resources_record")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            // Exclusive resources first: they are what the task acts on.
                            .filter(|r| !r.starts_with("shared:"))
                            .chain(a.iter().filter_map(Value::as_str).filter(|r| r.starts_with("shared:")))
                            .filter(|r| !format::is_domain_record(r))
                            .map(|r| format::resource_record(r, ctx.names))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                (text, Style::new())
            }
            ColKey::TaskDuration => {
                (format::elapsed(str_of("started_at"), str_of("finished_at"), ctx.now), Style::new())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum SortValue {
    Num(f64),
    Text(String),
}

pub fn scalar(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.replace('\n', " "),
        Some(Value::Bool(true)) => "✓".into(),
        Some(Value::Bool(false)) => "✗".into(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Array(a)) => format!("[{}]", a.len()),
        Some(Value::Object(o)) => format!("{{{}}}", o.len()),
    }
}

/// `+2 -1 =40` from a version's content summary.
fn summary(v: Option<&Value>) -> String {
    let total = |key: &str| -> u64 {
        v.and_then(|s| s.get(key))
            .and_then(Value::as_object)
            .map(|types| types.values().filter_map(|t| t.get("count").and_then(Value::as_u64)).sum())
            .unwrap_or_default()
    };
    format!("+{} -{} ={}", total("added"), total("removed"), total("present"))
}

/// Fields never worth a column.
const SKIP: &[&str] = &[
    "pulp_href",
    "prn",
    "pulp_created",
    "pulp_last_updated",
    "pulp_labels",
    "description",
    "md5",
    "sha1",
    "sha224",
    "sha384",
    "sha512",
    "artifact",
    "artifacts",
    "vuln_report",
    "hidden_fields",
    "headers",
    "ca_cert",
    "client_cert",
    "client_key",
    "password",
    "username",
    "proxy_url",
    "proxy_username",
    "proxy_password",
    "versions_href",
    "content_guard",
    "pulp_domain",
    "pkgId",
    "checksum_type",
    "location_base",
    "location_href",
    "summary",
    "url",
];

const MAX_COLUMNS: usize = 6;

/// Translated header for a preset column (`col.<key>` in the locale files).
fn col(key: &str) -> String {
    crate::labels::tr(&format!("col.{key}"))
}

/// Header for a schema-derived column: the API field name, upper-cased.
fn field_header(name: &str) -> String {
    name.to_uppercase().replace('_', " ")
}

/// Columns for a resource type: category presets, then schema scalars, then AGE.
pub fn for_type(t: &ResourceType, schema: Option<&Schema>) -> Vec<Column> {
    use ColKey::*;
    let has = |f: &str| schema.is_none_or(|s| s.properties.contains_key(f));
    let field = |key: &str, f: &str| Column::new(col(key), Field(f.into()));
    let preset: Vec<Column> = match (t.category.as_str(), t.name.as_str()) {
        ("tasks", _) => return tasks(),
        (_, "versions") => vec![
            field("version", "number"),
            Column::new(col("content"), Summary),
            Column::new(col("base"), Href("base_version".into())),
        ],
        ("repositories", _) => vec![
            field("name", "name"),
            Column::new(col("version"), VersionOf("latest_version_href".into())),
            Column::new(col("remote"), Href("remote".into())),
            field("retain", "retain_repo_versions"),
        ],
        ("remotes", _) => vec![field("name", "name"), field("url", "url"), field("policy", "policy")],
        ("distributions", _) => vec![
            field("name", "name"),
            field("base_path", "base_path"),
            Column::new(
                col("serves"),
                AnyHref(vec!["repository".into(), "publication".into(), "repository_version".into()]),
            ),
            field("base_url", "base_url"),
        ],
        ("publications", _) => vec![
            Column::new(col("version"), Href("repository_version".into())),
            Column::new(col("repository"), Href("repository".into())),
        ],
        ("workers", _) => vec![
            field("name", "name"),
            Column::new(col("heartbeat"), Age("last_heartbeat".into())),
            Column::new(col("task"), Href("current_task".into())),
        ],
        ("artifacts", _) => {
            vec![field("sha256", "sha256"), Column::new(col("size"), Bytes("size".into())), field("file", "file")]
        }
        ("uploads", _) => {
            vec![Column::new(col("size"), Bytes("size".into())), Column::new(col("completed"), Age("completed".into()))]
        }
        _ => Vec::new(),
    };
    let mut cols: Vec<Column> = preset.into_iter().filter(|c| c.api_fields().iter().any(|f| has(f))).collect();
    if let Some(schema) = schema
        && cols.len() < MAX_COLUMNS - 1
    {
        if cols.is_empty() && schema.properties.contains_key("name") {
            cols.push(field("name", "name"));
        }
        for (name, prop) in &schema.properties {
            if cols.len() >= MAX_COLUMNS - 1 {
                break;
            }
            let taken = cols.iter().any(|c| c.api_fields().contains(&name.as_str()));
            if taken || SKIP.contains(&name.as_str()) || name.ends_with("_prn") || prop.write_only || !prop.is_scalar()
            {
                continue;
            }
            let key = match prop.format.as_deref() {
                Some("uri") => Href(name.clone()),
                Some("date-time") => Age(name.clone()),
                _ => Field(name.clone()),
            };
            cols.push(Column::new(field_header(name), key));
        }
    }
    if cols.is_empty() {
        cols.push(Column::new("HREF", Field("pulp_href".into())));
    }
    if has("pulp_created") {
        cols.push(Column::new(col("age"), Age("pulp_created".into())));
    }
    cols
}

pub fn tasks() -> Vec<Column> {
    vec![
        Column::new(col("state"), ColKey::TaskState),
        Column::new(col("name"), ColKey::TaskName),
        Column::new(col("resources"), ColKey::TaskResources),
        Column::new(col("worker"), ColKey::Href("worker".into())),
        Column::new(col("duration"), ColKey::TaskDuration),
        Column::new(col("age"), ColKey::Age("pulp_created".into())),
    ]
}
