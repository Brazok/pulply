//! Plugin-specific presentation (R11): table columns and usage hints on top of the
//! describe view. Plugins without an entry, and types a plugin does not customize,
//! fall back to the generic, schema-derived views.

use pulply_api::{ResourceExt, ResourceType};
use serde_json::Value;

use super::columns::{ColKey, Column, col};

pub trait PluginView: Sync {
    /// Columns for `t`; `None` keeps the generic ones. Fields absent from the server
    /// schema are dropped by the caller, an AGE column is appended.
    fn columns(&self, _t: &ResourceType) -> Option<Vec<Column>> {
        None
    }

    /// Usage lines shown above a resource's fields (pull/install commands, certificates).
    fn describe_header(&self, _t: &ResourceType, _resource: &Value) -> Vec<String> {
        Vec::new()
    }
}

pub fn for_plugin(plugin: &str) -> Option<&'static dyn PluginView> {
    Some(match plugin {
        "rpm" => &Rpm,
        "container" => &Container,
        "file" => &File,
        "python" => &Python,
        "ansible" => &Ansible,
        "deb" => &Deb,
        "ostree" => &Ostree,
        "certguard" => &Certguard,
        _ => return None,
    })
}

fn field(header: &str, f: &str) -> Column {
    Column::new(header, ColKey::Field(f.into()))
}

fn named(key: &str, f: &str) -> Column {
    Column::new(col(key), ColKey::Field(f.into()))
}

fn href(header: &str, f: &str) -> Column {
    Column::new(header, ColKey::Href(f.into()))
}

fn bytes(f: &str) -> Column {
    Column::new(col("size"), ColKey::Bytes(f.into()))
}

fn join(header: &str, fields: &[&str], sep: &'static str) -> Column {
    Column::new(header, ColKey::Join(fields.iter().map(|f| (*f).to_owned()).collect(), sep))
}

fn is(t: &ResourceType, category: &str, name: &str) -> bool {
    t.category == category && t.name == name
}

/// `base_url` of a distribution, with a trailing slash.
fn base_url(resource: &Value) -> Option<String> {
    let url = resource.str_field("base_url")?;
    Some(if url.ends_with('/') { url.to_owned() } else { format!("{url}/") })
}

struct Rpm;

impl PluginView for Rpm {
    fn columns(&self, t: &ResourceType) -> Option<Vec<Column>> {
        if t.category != "content" {
            return None;
        }
        Some(match t.name.as_str() {
            "packages" => vec![Column::new("NEVRA", ColKey::Nevra), bytes("size_package"), field("SUMMARY", "summary")],
            "advisories" => vec![
                field("ID", "id"),
                field("TYPE", "type"),
                field("SEVERITY", "severity"),
                field("ISSUED", "issued_date"),
                field("TITLE", "title"),
            ],
            "modulemds" => vec![
                named("name", "name"),
                field("STREAM", "stream"),
                named("version", "version"),
                field("CONTEXT", "context"),
                field("ARCH", "arch"),
            ],
            "distribution_trees" => {
                vec![join("RELEASE", &["release_name", "release_version"], " "), field("ARCH", "arch")]
            }
            "packagegroups" | "packageenvironments" | "packagecategories" => {
                vec![field("ID", "id"), named("name", "name"), field("PACKAGES", "packages")]
            }
            _ => return None,
        })
    }

    fn describe_header(&self, t: &ResourceType, resource: &Value) -> Vec<String> {
        match (t.category.as_str(), base_url(resource)) {
            ("distributions", Some(url)) => vec![
                format!("dnf config-manager --add-repo {url}"),
                format!("[{}]  baseurl={url}", resource.name().unwrap_or("pulp")),
            ],
            _ => Vec::new(),
        }
    }
}

struct Container;

impl PluginView for Container {
    fn columns(&self, t: &ResourceType) -> Option<Vec<Column>> {
        Some(match (t.category.as_str(), t.name.as_str()) {
            ("content", "tags") => vec![field("TAG", "name"), href("MANIFEST", "tagged_manifest")],
            ("content", "manifests") => vec![
                field("DIGEST", "digest"),
                field("MEDIA TYPE", "media_type"),
                field("ARCH", "architecture"),
                field("OS", "os"),
                bytes("compressed_image_size"),
                field("LISTED", "listed_manifests"),
            ],
            ("content", "blobs") => vec![field("DIGEST", "digest")],
            ("distributions", _) => vec![
                named("name", "name"),
                field("REGISTRY PATH", "registry_path"),
                Column::new(col("serves"), ColKey::AnyHref(vec!["repository".into(), "repository_version".into()])),
                field("PRIVATE", "private"),
            ],
            _ => return None,
        })
    }

    fn describe_header(&self, t: &ResourceType, resource: &Value) -> Vec<String> {
        match (t.category.as_str(), resource.str_field("registry_path")) {
            ("distributions", Some(path)) => vec![format!("podman pull {path}:<tag>")],
            _ => Vec::new(),
        }
    }
}

struct File;

impl PluginView for File {
    fn columns(&self, t: &ResourceType) -> Option<Vec<Column>> {
        if is(t, "content", "files") {
            return Some(vec![field("RELATIVE PATH", "relative_path"), field("SHA256", "sha256")]);
        }
        if t.category == "publications" {
            return Some(vec![
                Column::new(col("version"), ColKey::Href("repository_version".into())),
                field("MANIFEST", "manifest"),
            ]);
        }
        None
    }

    fn describe_header(&self, t: &ResourceType, resource: &Value) -> Vec<String> {
        match (t.category.as_str(), base_url(resource)) {
            ("distributions", Some(url)) => vec![format!("curl {url}PULP_MANIFEST")],
            _ => Vec::new(),
        }
    }
}

struct Python;

impl PluginView for Python {
    fn columns(&self, t: &ResourceType) -> Option<Vec<Column>> {
        is(t, "content", "packages").then(|| {
            vec![
                named("name", "name"),
                named("version", "version"),
                field("TYPE", "packagetype"),
                field("FILENAME", "filename"),
                bytes("size"),
            ]
        })
    }

    fn describe_header(&self, t: &ResourceType, resource: &Value) -> Vec<String> {
        let (true, Some(url)) = (t.category == "distributions", base_url(resource)) else { return Vec::new() };
        let mut lines = vec![format!("pip install --index-url {url}simple/ <package>")];
        if resource.get("allow_uploads").and_then(Value::as_bool) == Some(true) {
            lines.push(format!("twine upload --repository-url {url}legacy/ dist/*"));
        }
        lines
    }
}

struct Ansible;

impl PluginView for Ansible {
    fn columns(&self, t: &ResourceType) -> Option<Vec<Column>> {
        Some(match (t.category.as_str(), t.name.as_str()) {
            ("content", "collection_versions") => vec![
                join("COLLECTION", &["namespace", "name"], "."),
                named("version", "version"),
                field("REQUIRES ANSIBLE", "requires_ansible"),
            ],
            ("content", "roles") => vec![join("ROLE", &["namespace", "name"], "."), named("version", "version")],
            ("distributions", _) => {
                vec![named("name", "name"), named("base_path", "base_path"), field("CLIENT URL", "client_url")]
            }
            _ => return None,
        })
    }

    fn describe_header(&self, t: &ResourceType, resource: &Value) -> Vec<String> {
        match (t.category.as_str(), resource.str_field("client_url")) {
            ("distributions", Some(url)) => {
                vec![format!("ansible-galaxy collection install <namespace.name> -s {url}")]
            }
            _ => Vec::new(),
        }
    }
}

struct Deb;

impl PluginView for Deb {
    fn columns(&self, t: &ResourceType) -> Option<Vec<Column>> {
        if t.category != "content" {
            return None;
        }
        Some(match t.name.as_str() {
            "releases" => vec![
                field("CODENAME", "codename"),
                field("SUITE", "suite"),
                field("DISTRIBUTION", "distribution"),
                named("version", "version"),
            ],
            "release_components" => vec![field("COMPONENT", "component"), field("DISTRIBUTION", "distribution")],
            "release_architectures" => {
                vec![field("ARCHITECTURE", "architecture"), field("DISTRIBUTION", "distribution")]
            }
            "packages" => vec![
                field("PACKAGE", "package"),
                named("version", "version"),
                field("ARCHITECTURE", "architecture"),
                field("SECTION", "section"),
                field("RELATIVE PATH", "relative_path"),
            ],
            _ => return None,
        })
    }

    fn describe_header(&self, t: &ResourceType, resource: &Value) -> Vec<String> {
        match (t.category.as_str(), base_url(resource)) {
            ("distributions", Some(url)) => vec![format!("deb {url} <distribution> <component>")],
            _ => Vec::new(),
        }
    }
}

/// Not shipped in the reference container: field names follow the pulp_ostree API and
/// are dropped automatically when the server schema differs.
struct Ostree;

impl PluginView for Ostree {
    fn columns(&self, t: &ResourceType) -> Option<Vec<Column>> {
        if t.category != "content" {
            return None;
        }
        Some(match t.name.as_str() {
            "refs" => vec![named("name", "name"), field("CHECKSUM", "checksum"), href("COMMIT", "commit")],
            "commits" => vec![
                field("CHECKSUM", "checksum"),
                field("RELATIVE PATH", "relative_path"),
                href("PARENT", "parent_commit"),
            ],
            "configs" => vec![field("RELATIVE PATH", "relative_path")],
            _ => return None,
        })
    }

    fn describe_header(&self, t: &ResourceType, resource: &Value) -> Vec<String> {
        match (t.category.as_str(), base_url(resource)) {
            ("distributions", Some(url)) => {
                vec![format!("ostree remote add --no-gpg-verify {} {url}", resource.name().unwrap_or("pulp"))]
            }
            _ => Vec::new(),
        }
    }
}

struct Certguard;

impl PluginView for Certguard {
    fn columns(&self, t: &ResourceType) -> Option<Vec<Column>> {
        (t.category == "contentguards").then(|| vec![named("name", "name"), field("DESCRIPTION", "description")])
    }

    fn describe_header(&self, _t: &ResourceType, resource: &Value) -> Vec<String> {
        resource.str_field("ca_certificate").map(certificate_summary).unwrap_or_default()
    }
}

/// Subject, issuer and validity of every certificate in a PEM bundle.
pub fn certificate_summary(pem: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, block) in x509_parser::pem::Pem::iter_from_buffer(pem.as_bytes()).enumerate() {
        let parsed = block.map_err(|e| e.to_string()).and_then(|p| {
            let cert = p.parse_x509().map_err(|e| e.to_string())?;
            let validity = cert.validity();
            let state = if validity.is_valid() { "valid" } else { "EXPIRED or not yet valid" };
            Ok(vec![
                format!("certificate #{}: {}", i + 1, cert.subject()),
                format!("  issuer: {}", cert.issuer()),
                format!("  valid: {} → {} ({state})", validity.not_before, validity.not_after),
            ])
        });
        match parsed {
            Ok(lines) => out.extend(lines),
            Err(e) => out.push(format!("certificate #{}: unreadable ({e})", i + 1)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;
    use serde_json::json;

    #[test]
    fn plugin_columns_are_resolved_and_filtered_by_schema() {
        let reg = testutil::registry();
        let packages = reg.get("content/rpm/packages").unwrap();
        let cols = super::super::columns::for_type(packages, reg.read_schema(packages));
        let headers: Vec<&str> = cols.iter().map(|c| c.header.as_str()).collect();
        assert_eq!(headers[0], "NEVRA");
        assert!(headers.contains(&"SUMMARY"));

        // Types a plugin does not customize keep the generic columns.
        let remotes = reg.get("remotes/rpm/rpm").unwrap();
        assert!(Rpm.columns(remotes).is_none());
        // Unknown plugins have no view.
        assert!(for_plugin("gem").is_none());
    }

    #[test]
    fn nevra_omits_zero_epoch() {
        let p = json!({"name": "bash", "epoch": "0", "version": "5.2", "release": "3.fc40", "arch": "x86_64"});
        assert_eq!(super::super::columns::nevra(&p), "bash-5.2-3.fc40.x86_64");
        let p = json!({"name": "bash", "epoch": "1", "version": "5.2", "release": "3", "arch": "noarch"});
        assert_eq!(super::super::columns::nevra(&p), "bash-1:5.2-3.noarch");
    }

    #[test]
    fn distribution_usage_lines() {
        let reg = testutil::registry();
        let dist = reg.get("distributions/python/pypi").unwrap();
        let lines =
            Python.describe_header(dist, &json!({"base_url": "http://h/pulp/content/py", "allow_uploads": true}));
        assert_eq!(lines[0], "pip install --index-url http://h/pulp/content/py/simple/ <package>");
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn certificate_summary_reports_unreadable_blocks() {
        let lines = certificate_summary("-----BEGIN CERTIFICATE-----\nnotbase64!!\n-----END CERTIFICATE-----\n");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("unreadable"));
    }
}
