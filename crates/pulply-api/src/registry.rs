//! Resource registry built from the server's OpenAPI document.
//!
//! Every list endpoint under the API prefix becomes a [`ResourceType`]; detail,
//! update, delete and action operations are attached through the `{xxx_href}`
//! path parameter of their read operation.

use std::collections::HashMap;

use indexmap::IndexMap;
use serde::Deserialize;

use crate::error::{Error, Result};

/// Categories whose list paths are `category/plugin/type/`.
const PLUGIN_CATEGORIES: &[&str] = &[
    "repositories",
    "remotes",
    "distributions",
    "publications",
    "content",
    "acs",
    "contentguards",
    "exporters",
    "importers",
];

// ---------------------------------------------------------------- raw OpenAPI

#[derive(Deserialize)]
struct Doc {
    paths: IndexMap<String, PathItem>,
    #[serde(default)]
    components: Components,
}

#[derive(Deserialize, Default)]
struct Components {
    #[serde(default)]
    schemas: HashMap<String, Schema>,
}

#[derive(Deserialize)]
struct PathItem {
    get: Option<Operation>,
    post: Option<Operation>,
    put: Option<Operation>,
    patch: Option<Operation>,
    delete: Option<Operation>,
}

impl PathItem {
    fn operations(&self) -> impl Iterator<Item = (Method, &Operation)> {
        [
            (Method::Get, &self.get),
            (Method::Post, &self.post),
            (Method::Put, &self.put),
            (Method::Patch, &self.patch),
            (Method::Delete, &self.delete),
        ]
        .into_iter()
        .filter_map(|(m, op)| op.as_ref().map(|o| (m, o)))
    }
}

#[derive(Deserialize)]
struct Operation {
    #[serde(rename = "operationId")]
    operation_id: String,
    #[serde(default)]
    parameters: Vec<RawParam>,
    #[serde(rename = "requestBody")]
    request_body: Option<Body>,
    #[serde(default)]
    responses: HashMap<String, Body>,
}

#[derive(Deserialize)]
struct RawParam {
    name: String,
    #[serde(rename = "in")]
    location: String,
    #[serde(default)]
    schema: Option<Property>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
struct Body {
    #[serde(default)]
    content: IndexMap<String, MediaType>,
}

impl Body {
    fn schema_ref(&self) -> Option<&str> {
        let media = self.content.get("application/json").or_else(|| self.content.values().next())?;
        media.schema.as_ref()?.reference.as_deref().map(strip_ref)
    }
}

#[derive(Deserialize)]
struct MediaType {
    schema: Option<Property>,
}

fn strip_ref(r: &str) -> &str {
    r.rsplit('/').next().unwrap_or(r)
}

// ---------------------------------------------------------------- public model

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Schema {
    #[serde(default)]
    pub properties: IndexMap<String, Property>,
    #[serde(default)]
    pub required: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Property {
    #[serde(rename = "type")]
    pub ty: Option<String>,
    pub format: Option<String>,
    #[serde(rename = "$ref")]
    pub reference: Option<String>,
    #[serde(rename = "readOnly", default)]
    pub read_only: bool,
    #[serde(rename = "writeOnly", default)]
    pub write_only: bool,
    #[serde(default)]
    pub nullable: bool,
    #[serde(rename = "enum", default)]
    pub enum_values: Vec<serde_json::Value>,
    pub description: Option<String>,
    pub items: Option<Box<Property>>,
    #[serde(rename = "allOf", default)]
    pub all_of: Vec<Property>,
    #[serde(rename = "oneOf", default)]
    pub one_of: Vec<Property>,
}

impl Property {
    /// Scalar = renderable in a single table cell.
    pub fn is_scalar(&self) -> bool {
        matches!(self.ty.as_deref(), Some("string" | "integer" | "number" | "boolean"))
            || !self.all_of.is_empty()
            || !self.one_of.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpSet {
    pub list: bool,
    pub create: bool,
    pub read: bool,
    pub update: bool,
    pub partial_update: bool,
    pub delete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    /// Path suffix after the href, without slash (`sync`, `modify`, `add_role`).
    pub verb: String,
    pub method: Method,
    pub body_schema: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamType {
    String,
    Integer,
    Number,
    Boolean,
    Array,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: ParamType,
    pub enum_values: Vec<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResourceType {
    /// Path-derived id: `repositories/rpm/rpm`, `tasks`, `repositories/rpm/rpm/versions`.
    pub id: String,
    pub category: String,
    /// `core` for pulpcore resources.
    pub plugin: String,
    /// Type segment(s): `rpm`, `packages`, `maven/index-page`; equals `category` for core resources.
    pub name: String,
    /// Absolute list path for top-level types; `None` for nested ones.
    pub list_path: Option<String>,
    /// Parent type id and sub-path (`versions/`) for nested collections.
    pub parent: Option<(String, String)>,
    /// Cross-plugin list (`/repositories/`, `/content/`, …).
    pub generic: bool,
    pub ops: OpSet,
    pub actions: Vec<Action>,
    pub children: Vec<String>,
    pub list_params: Vec<Param>,
    pub read_schema: Option<String>,
    pub create_schema: Option<String>,
    pub update_schema: Option<String>,
}

impl ResourceType {
    pub fn param(&self, name: &str) -> Option<&Param> {
        self.list_params.iter().find(|p| p.name == name)
    }

    /// Server-side ordering fields accepted by `?ordering=`.
    pub fn ordering_fields(&self) -> &[String] {
        self.param("ordering").map(|p| p.enum_values.as_slice()).unwrap_or(&[])
    }

    pub fn has_action(&self, verb: &str) -> bool {
        self.actions.iter().any(|a| a.verb == verb)
    }

    pub fn is_core(&self) -> bool {
        self.plugin == "core"
    }
}

#[derive(Debug, Default)]
pub struct Registry {
    types: Vec<ResourceType>,
    by_id: HashMap<String, usize>,
    schemas: HashMap<String, Schema>,
}

struct ListOp<'a> {
    path: &'a str,
    op: &'a Operation,
    prefix: &'a str,
}

impl Registry {
    /// Parses an OpenAPI document. `api_prefix` is the client's resolved prefix
    /// (`/pulp/api/v3/` or `/pulp/<domain>/api/v3/`); `{pulp_domain}` placeholders are
    /// substituted with `domain`.
    pub fn from_json(bytes: &[u8], api_prefix: &str, domain: Option<&str>) -> Result<Self> {
        let doc: Doc = serde_json::from_slice(bytes).map_err(|e| Error::Decode(format!("OpenAPI schema: {e}")))?;
        let domain = domain.unwrap_or("default");
        let paths: Vec<(String, &PathItem)> =
            doc.paths.iter().map(|(p, item)| (p.replace("{pulp_domain}", domain), item)).collect();

        // href parameter name -> operationId prefix of its read operation.
        let mut href_owner: HashMap<&str, &str> = HashMap::new();
        for (path, item) in &paths {
            if let (Some(param), Some(get)) = (href_param(path).filter(|(_, rest)| rest.is_empty()), &item.get)
                && let Some(prefix) = get.operation_id.strip_suffix("_read")
            {
                href_owner.insert(param.0, prefix);
            }
        }

        let mut lists: Vec<ListOp> = Vec::new();
        for (path, item) in &paths {
            if let Some(get) = &item.get
                && let Some(prefix) = get.operation_id.strip_suffix("_list")
            {
                lists.push(ListOp { path, op: get, prefix });
            }
        }

        let mut types: Vec<ResourceType> = Vec::new();
        let mut by_prefix: HashMap<String, usize> = HashMap::new();
        // Top-level types first so nested ones can resolve their parent.
        for l in &lists {
            let Some(rel) = l.path.strip_prefix(api_prefix) else { continue };
            if rel.contains('{') || rel.is_empty() {
                continue;
            }
            let segs: Vec<&str> = rel.trim_end_matches('/').split('/').collect();
            let (category, plugin, name, generic) = classify(&segs);
            let id = segs.join("/");
            by_prefix.insert(l.prefix.to_owned(), types.len());
            types.push(new_type(id, category, plugin, name, Some(l.path.to_owned()), None, generic, l.op));
        }
        for l in &lists {
            let Some((param, sub)) = href_param(l.path) else { continue };
            if sub.is_empty() || sub.matches('/').count() != 1 {
                continue;
            }
            let Some(parent_prefix) = href_owner.get(param) else { continue };
            // Skip foreign endpoints hanging off another plugin's href (e.g. Galaxy v1 APIs).
            if !l.prefix.starts_with(parent_prefix) {
                continue;
            }
            let Some(&parent_idx) = by_prefix.get(*parent_prefix) else { continue };
            let parent = &types[parent_idx];
            let sub_name = sub.trim_end_matches('/');
            let id = format!("{}/{sub_name}", parent.id);
            let (category, plugin) = (parent.category.clone(), parent.plugin.clone());
            let parent_id = parent.id.clone();
            by_prefix.insert(l.prefix.to_owned(), types.len());
            types[parent_idx].children.push(id.clone());
            types.push(new_type(
                id,
                category,
                plugin,
                sub_name.to_owned(),
                None,
                Some((parent_id, sub.to_owned())),
                false,
                l.op,
            ));
        }

        // Attach create/read/update/delete/actions.
        for (path, item) in &paths {
            for (method, op) in item.operations() {
                if let Some(rel) = path.strip_prefix(api_prefix)
                    && !rel.contains('{')
                {
                    if method == Method::Post
                        && let Some(prefix) = op.operation_id.strip_suffix("_create")
                        && let Some(&idx) = by_prefix.get(prefix)
                    {
                        types[idx].ops.create = true;
                        types[idx].create_schema =
                            op.request_body.as_ref().and_then(|b| b.schema_ref()).map(str::to_owned);
                    }
                    continue;
                }
                let Some((param, rest)) = href_param(path) else { continue };
                let Some(owner) = href_owner.get(param) else { continue };
                let Some(&idx) = by_prefix.get(*owner) else { continue };
                let t = &mut types[idx];
                if rest.is_empty() {
                    match method {
                        Method::Get => {
                            t.ops.read = true;
                            if let Some(s) = op.responses.get("200").and_then(Body::schema_ref) {
                                t.read_schema = Some(s.to_owned());
                            }
                        }
                        Method::Put => t.ops.update = true,
                        Method::Patch => {
                            t.ops.partial_update = true;
                            t.update_schema = op.request_body.as_ref().and_then(|b| b.schema_ref()).map(str::to_owned);
                        }
                        Method::Delete => t.ops.delete = true,
                        Method::Post => {}
                    }
                } else if rest.matches('/').count() == 1 && !op.operation_id.ends_with("_list") {
                    t.actions.push(Action {
                        verb: rest.trim_end_matches('/').to_owned(),
                        method,
                        body_schema: op.request_body.as_ref().and_then(|b| b.schema_ref()).map(str::to_owned),
                    });
                }
            }
        }

        let schemas = doc.components.schemas;
        let mut reg = Registry { types, by_id: HashMap::new(), schemas };
        reg.fill_list_read_schemas(&lists, &by_prefix);
        reg.types.sort_by(|a, b| a.id.cmp(&b.id));
        reg.by_id = reg.types.iter().enumerate().map(|(i, t)| (t.id.clone(), i)).collect();
        Ok(reg)
    }

    fn fill_list_read_schemas(&mut self, lists: &[ListOp], by_prefix: &HashMap<String, usize>) {
        for l in lists {
            let Some(&idx) = by_prefix.get(l.prefix) else { continue };
            if self.types[idx].read_schema.is_some() {
                continue;
            }
            let item_ref =
                l.op.responses
                    .get("200")
                    .and_then(Body::schema_ref)
                    .and_then(|page| self.schemas.get(page))
                    .and_then(|page| page.properties.get("results"))
                    .and_then(|results| results.items.as_ref())
                    .and_then(|items| items.reference.as_deref())
                    .map(|r| strip_ref(r).to_owned());
            self.types[idx].read_schema = item_ref;
        }
    }

    pub fn types(&self) -> &[ResourceType] {
        &self.types
    }

    pub fn get(&self, id: &str) -> Option<&ResourceType> {
        self.by_id.get(id).map(|&i| &self.types[i])
    }

    pub fn schema(&self, name: &str) -> Option<&Schema> {
        self.schemas.get(name)
    }

    pub fn read_schema(&self, t: &ResourceType) -> Option<&Schema> {
        t.read_schema.as_deref().and_then(|s| self.schema(s))
    }

    /// Top-level type whose list path prefixes `href` (longest match).
    pub fn type_for_href(&self, href: &str) -> Option<&ResourceType> {
        if let Some(repo) = crate::models::version_repository(href)
            && crate::models::version_number(href).is_some()
        {
            let parent = self.type_for_href(repo)?;
            return parent.children.iter().filter_map(|c| self.get(c)).find(|c| c.name == "versions");
        }
        self.types
            .iter()
            .filter(|t| !t.generic)
            .filter_map(|t| t.list_path.as_deref().filter(|p| href.starts_with(p)).map(|p| (p.len(), t)))
            .max_by_key(|(len, _)| *len)
            .map(|(_, t)| t)
    }

    /// Top-level type of a PRN (`prn:file.filerepository:<id>`), matched on its read schema
    /// (`file.FileRepositoryResponse`). Usable with `?prn__in=` on the type's list path.
    pub fn type_for_prn(&self, prn: &str) -> Option<&ResourceType> {
        let model = prn.strip_prefix("prn:")?.split(':').next()?;
        self.types.iter().filter(|t| !t.generic && t.list_path.is_some()).find(|t| {
            t.read_schema
                .as_deref()
                .and_then(|s| s.strip_suffix("Response"))
                .is_some_and(|s| s.eq_ignore_ascii_case(model))
        })
    }

    /// Plugins with at least one resource type, sorted, `core` excluded.
    pub fn plugins(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self.types.iter().map(|t| t.plugin.as_str()).filter(|p| *p != "core").collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// Splits `{param}rest` into `(param, rest)`.
fn href_param(path: &str) -> Option<(&str, &str)> {
    let inner = path.strip_prefix('{')?;
    let end = inner.find('}')?;
    Some((&inner[..end], &inner[end + 1..]))
}

fn classify(segs: &[&str]) -> (String, String, String, bool) {
    match segs {
        [cat] if PLUGIN_CATEGORIES.contains(cat) => ((*cat).into(), "core".into(), (*cat).into(), true),
        [single] => ((*single).into(), "core".into(), (*single).into(), false),
        [cat, plugin, rest @ ..] if PLUGIN_CATEGORIES.contains(cat) && !rest.is_empty() => {
            ((*cat).into(), (*plugin).into(), rest.join("/"), false)
        }
        // `pulp_container/namespaces`, `ansible/collections`, …
        [first, rest @ ..] => {
            let plugin = first.strip_prefix("pulp_").unwrap_or(first);
            ("other".into(), plugin.into(), rest.join("/"), false)
        }
        [] => unreachable!("empty relative path filtered earlier"),
    }
}

#[allow(clippy::too_many_arguments)]
fn new_type(
    id: String,
    category: String,
    plugin: String,
    name: String,
    list_path: Option<String>,
    parent: Option<(String, String)>,
    generic: bool,
    list_op: &Operation,
) -> ResourceType {
    let list_params = list_op
        .parameters
        .iter()
        .filter(|p| p.location == "query" && p.name != "limit" && p.name != "offset")
        .map(|p| {
            let schema = p.schema.as_ref();
            let ty = match schema.and_then(|s| s.ty.as_deref()) {
                Some("integer") => ParamType::Integer,
                Some("number") => ParamType::Number,
                Some("boolean") => ParamType::Boolean,
                Some("array") => ParamType::Array,
                _ => ParamType::String,
            };
            let enum_source = match schema {
                Some(s) if !s.enum_values.is_empty() => &s.enum_values,
                Some(s) => s.items.as_ref().map(|i| &i.enum_values).unwrap_or(&s.enum_values),
                None => &Vec::new(),
            };
            Param {
                name: p.name.clone(),
                ty,
                enum_values: enum_source.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect(),
                description: p.description.clone(),
            }
        })
        .collect();
    ResourceType {
        id,
        category,
        plugin,
        name,
        list_path,
        parent,
        generic,
        ops: OpSet { list: true, ..OpSet::default() },
        actions: Vec::new(),
        children: Vec::new(),
        list_params,
        read_schema: None,
        create_schema: None,
        update_schema: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn fixture() -> Registry {
        let gz = include_bytes!("../tests/fixtures/api.json.gz");
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(&gz[..]).read_to_end(&mut bytes).unwrap();
        Registry::from_json(&bytes, "/pulp/api/v3/", None).unwrap()
    }

    #[test]
    fn classifies_plugin_and_core_types() {
        let reg = fixture();
        let rpm = reg.get("repositories/rpm/rpm").unwrap();
        assert_eq!((rpm.category.as_str(), rpm.plugin.as_str(), rpm.name.as_str()), ("repositories", "rpm", "rpm"));
        assert_eq!(rpm.list_path.as_deref(), Some("/pulp/api/v3/repositories/rpm/rpm/"));
        assert!(rpm.ops.create && rpm.ops.read && rpm.ops.partial_update && rpm.ops.delete);
        assert!(rpm.has_action("sync") && rpm.has_action("modify"));
        assert_eq!(rpm.read_schema.as_deref(), Some("rpm.RpmRepositoryResponse"));
        assert!(rpm.ordering_fields().iter().any(|f| f == "-pulp_created"));

        let tasks = reg.get("tasks").unwrap();
        assert!(tasks.is_core() && !tasks.generic);
        assert!(reg.get("repositories").unwrap().generic);
        assert_eq!(reg.get("content/maven/maven/index-page").unwrap().name, "maven/index-page");
        assert_eq!(reg.get("other/namespaces").map(|t| t.plugin.as_str()), None);
        assert_eq!(reg.get("pulp_container/namespaces").unwrap().plugin, "container");
    }

    #[test]
    fn nested_collections_hang_off_their_parent() {
        let reg = fixture();
        let versions = reg.get("repositories/file/file/versions").unwrap();
        assert_eq!(versions.parent, Some(("repositories/file/file".into(), "versions/".into())));
        assert!(versions.list_path.is_none());
        assert!(versions.ops.read && versions.ops.delete);
        assert!(reg.get("repositories/file/file").unwrap().children.contains(&versions.id));
        // Galaxy v1 endpoint on a content href must not become a child of content roles.
        assert!(reg.get("content/ansible/roles/versions").is_none());
    }

    #[test]
    fn resolves_type_from_prn() {
        let reg = fixture();
        assert_eq!(reg.type_for_prn("prn:file.filerepository:01a1").unwrap().id, "repositories/file/file");
        assert_eq!(reg.type_for_prn("prn:rpm.rpmremote:01a1").unwrap().id, "remotes/rpm/rpm");
        assert!(reg.type_for_prn("prn:nope.thing:1").is_none());
        assert!(reg.type_for_prn("/pulp/api/v3/tasks/").is_none());
    }

    #[test]
    fn resolves_type_from_href() {
        let reg = fixture();
        let repo = "/pulp/api/v3/repositories/container/container-push/0192-aa/";
        assert_eq!(reg.type_for_href(repo).unwrap().id, "repositories/container/container-push");
        let version = format!("{repo}versions/3/");
        assert_eq!(reg.type_for_href(&version).unwrap().id, "repositories/container/container-push/versions");
        assert_eq!(reg.type_for_href("/pulp/api/v3/tasks/0192-aa/").unwrap().id, "tasks");
    }

    #[test]
    fn substitutes_domain_placeholder() {
        let doc = br#"{"paths":{
            "/pulp/{pulp_domain}/api/v3/tasks/":{"get":{"operationId":"tasks_list"}},
            "{task_href}":{"get":{"operationId":"tasks_read"},"delete":{"operationId":"tasks_delete"}}
        }}"#;
        let reg = Registry::from_json(doc, "/pulp/acme/api/v3/", Some("acme")).unwrap();
        let tasks = reg.get("tasks").unwrap();
        assert_eq!(tasks.list_path.as_deref(), Some("/pulp/acme/api/v3/tasks/"));
        assert!(tasks.ops.read && tasks.ops.delete);
    }

    #[test]
    fn lists_installed_plugins() {
        let reg = fixture();
        let plugins = reg.plugins();
        for p in
            ["ansible", "certguard", "container", "deb", "file", "gem", "hugging_face", "maven", "npm", "python", "rpm"]
        {
            assert!(plugins.contains(&p), "{p} missing from {plugins:?}");
        }
        assert!(!plugins.contains(&"core"));
    }
}
