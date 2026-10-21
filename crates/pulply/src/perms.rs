//! What the connected user may do (R12.3), to only offer permitted actions.
//!
//! - Staff users (`UserResponse` exposes no `is_superuser`) see everything.
//! - Users allowed to read their own roles get global permissions from the roles of
//!   the user and its groups.
//! - Otherwise (the default Pulp policy for non-admins), only object permissions are
//!   known, from `my_permissions/`, which lists object-level roles and nothing granted
//!   globally: creation stays offered (the server refuses with 403 if needed).
//!
//! The server remains the authority; `rbac_filter = false` on a context disables this.

use std::collections::{HashMap, HashSet};

use pulply_api::{Error, PulpClient, ResourceExt, ResourceType};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    /// Staff user, anonymous session or filtering disabled: no filtering.
    All,
    /// Codenames (`file.add_filerepository`) granted globally, plus object permissions.
    Limited(HashSet<String>),
    /// Global permissions unreadable: object permissions decide, creation is not filtered.
    ObjectsOnly,
}

#[derive(Debug, Clone)]
pub struct Permissions {
    pub access: Access,
    pub username: Option<String>,
    objects: HashMap<String, HashSet<String>>,
    pending: HashSet<String>,
}

impl Default for Permissions {
    fn default() -> Self {
        Self { access: Access::All, username: None, objects: HashMap::new(), pending: HashSet::new() }
    }
}

/// `file.FileRepositoryResponse` → `file.filerepository`.
pub fn model_of_type_id(read_schema: &str) -> Option<String> {
    Some(read_schema.strip_suffix("Response")?.to_lowercase())
}

/// Model of a type from its read schema.
pub fn model_of_type(t: &ResourceType) -> Option<String> {
    model_of_type_id(t.read_schema.as_deref()?)
}

/// `("change", "file.filerepository")` → `file.change_filerepository`.
pub fn codename(action: &str, model: &str) -> Option<String> {
    let (app, name) = model.split_once('.')?;
    Some(format!("{app}.{action}_{name}"))
}

impl Permissions {
    pub fn new(access: Access, username: Option<String>) -> Self {
        Self { access, username, ..Self::default() }
    }

    /// May the user perform `action` (`add`, `change`, `delete`, `sync`, `modify`,
    /// `manage_roles`) on `model`, globally (`href: None`) or on `href`? Object
    /// permissions not loaded yet count as missing.
    pub fn allows(&self, action: &str, model: Option<&str>, href: Option<&str>) -> bool {
        let global = match &self.access {
            Access::All => return true,
            Access::ObjectsOnly if href.is_none() => return true,
            Access::ObjectsOnly => None,
            Access::Limited(global) => Some(global),
        };
        let Some(perm) = model.and_then(|m| codename(action, m)) else { return false };
        global.is_some_and(|g| g.contains(&perm))
            || href.and_then(|h| self.objects.get(h)).is_some_and(|p| p.contains(&perm))
    }

    /// Object permissions of `href` should be fetched (filtering active, not loaded or loading).
    pub fn needs(&self, href: &str) -> bool {
        !matches!(self.access, Access::All) && !self.objects.contains_key(href) && !self.pending.contains(href)
    }

    pub fn mark_pending(&mut self, href: &str) {
        self.pending.insert(href.to_owned());
    }

    pub fn record(&mut self, href: String, perms: HashSet<String>) {
        self.pending.remove(&href);
        self.objects.insert(href, perms);
    }
}

fn role_permissions(page: &[Value]) -> impl Iterator<Item = String> + '_ {
    page.iter()
        .filter(|r| r.get("content_object").is_none_or(Value::is_null))
        .flat_map(|r| r.get("permissions").and_then(Value::as_array).into_iter().flatten())
        .filter_map(|p| p.as_str().map(str::to_owned))
}

/// Reads the user behind the session's credentials and what it may do.
pub async fn load(client: &PulpClient) -> Permissions {
    let Ok(me) = client.get(&client.api_path("login/"), &[]).await else { return Permissions::default() };
    let username = me.str_field("username").map(str::to_owned);
    let Some(href) = me.href().map(str::to_owned) else { return Permissions::new(Access::All, username) };
    let user = match client.get(&href, &[]).await {
        Ok(u) => u,
        Err(Error::Forbidden) => return Permissions::new(Access::ObjectsOnly, username),
        Err(_) => return Permissions::new(Access::All, username),
    };
    if user.get("is_staff").and_then(Value::as_bool) == Some(true) {
        return Permissions::new(Access::All, username);
    }
    let all = [("limit".to_owned(), "1000".to_owned())];
    let mut sources = vec![format!("{href}roles/")];
    for g in user.get("groups").and_then(Value::as_array).into_iter().flatten() {
        if let Some(gh) = g.href() {
            sources.push(format!("{gh}roles/"));
        }
    }
    let mut global = HashSet::new();
    for source in sources {
        match client.list(&source, &all).await {
            Ok(page) => global.extend(role_permissions(&page.results)),
            Err(_) => return Permissions::new(Access::ObjectsOnly, username),
        }
    }
    Permissions::new(Access::Limited(global), username)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MODEL: Option<&str> = Some("file.filerepository");

    #[test]
    fn global_roles_only_count_without_content_object() {
        let roles = [
            json!({"role": "file.filerepository_creator", "content_object": null, "permissions": ["file.add_filerepository"]}),
            json!({"role": "file.filerepository_owner", "content_object": "/r/1/", "permissions": ["file.delete_filerepository"]}),
        ];
        let perms: HashSet<String> = role_permissions(&roles).collect();
        assert_eq!(perms, HashSet::from(["file.add_filerepository".to_owned()]));
    }

    #[test]
    fn limited_access_combines_global_and_object_permissions() {
        let mut p = Permissions::new(Access::Limited(HashSet::from(["file.add_filerepository".to_owned()])), None);
        assert!(p.allows("add", MODEL, None));
        assert!(!p.allows("delete", MODEL, Some("/r/1/")), "unknown object permissions hide the action");
        assert!(p.needs("/r/1/"));
        p.mark_pending("/r/1/");
        assert!(!p.needs("/r/1/"));
        p.record("/r/1/".into(), HashSet::from(["file.delete_filerepository".to_owned()]));
        assert!(p.allows("delete", MODEL, Some("/r/1/")));
        assert!(!p.allows("delete", MODEL, Some("/r/2/")));
        assert!(!p.allows("sync", None, Some("/r/1/")), "no model, no permission");
    }

    #[test]
    fn objects_only_filters_object_actions_but_not_creation() {
        let mut p = Permissions::new(Access::ObjectsOnly, Some("viewer".into()));
        assert!(p.allows("add", MODEL, None), "global permissions unknown: creation offered");
        assert!(!p.allows("change", MODEL, Some("/r/1/")));
        p.record("/r/1/".into(), HashSet::from(["file.change_filerepository".to_owned()]));
        p.record("/r/2/".into(), HashSet::new());
        assert!(p.allows("change", MODEL, Some("/r/1/")));
        assert!(!p.allows("change", MODEL, Some("/r/2/")));
    }

    #[test]
    fn full_access_is_not_filtered() {
        let p = Permissions::new(Access::All, None);
        assert!(p.allows("delete", None, Some("/r/1/")));
        assert!(!p.needs("/r/1/"));
    }

    #[test]
    fn codenames_from_models() {
        assert_eq!(model_of_type_id("rpm.RpmRepositoryResponse").as_deref(), Some("rpm.rpmrepository"));
        assert_eq!(codename("sync", "rpm.rpmrepository").as_deref(), Some("rpm.sync_rpmrepository"));
        assert_eq!(codename("sync", "nodot"), None);
    }
}
