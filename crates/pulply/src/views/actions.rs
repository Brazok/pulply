//! Builders for modifying workflows: each returns the [`Outcome`] to apply
//! (usually a confirmation overlay wrapping one or more mutations).

use pulply_api::{Method, PulpClient, Registry, ResourceExt, ResourceType};
use rust_i18n::t;
use serde_json::{Value, json};

use super::confirm::confirm;
use super::form::FormView;
use super::{AfterTask, Ctx, Mutation, Outcome};
use crate::commands::Op;
use crate::format;
use crate::labels::tr;
use crate::perms::model_of_type;

fn describe(hrefs: &[String], ctx: &Ctx) -> String {
    match hrefs {
        [one] => format::href(one, ctx.names),
        many => t!("action.n_resources", count = many.len()).into_owned(),
    }
}

/// Confirmed DELETE of every href (repositories, remotes, versions, content guards…).
/// `pop_after` leaves the current view once confirmed (describe view of the deleted resource).
pub fn delete(hrefs: Vec<String>, ctx: &Ctx, pop_after: bool) -> Outcome {
    if hrefs.is_empty() {
        return Outcome::None;
    }
    if ctx.readonly() {
        return Outcome::readonly();
    }
    let what = describe(&hrefs, ctx);
    let mut outcomes: Vec<Outcome> = hrefs
        .iter()
        .map(|h| {
            let label = t!("action.delete", name = format::href(h, ctx.names)).into_owned();
            Outcome::Mutate(Mutation::new(Method::DELETE, h.clone(), None, label))
        })
        .collect();
    if pop_after {
        outcomes.push(Outcome::Pop);
    }
    confirm(t!("confirm.delete", what = what), Outcome::Batch(outcomes))
}

/// Confirmed cancellation of waiting/running tasks.
pub fn cancel_tasks(hrefs: Vec<String>, ctx: &Ctx) -> Outcome {
    if hrefs.is_empty() {
        return Outcome::None;
    }
    if ctx.readonly() {
        return Outcome::readonly();
    }
    let mutations = hrefs
        .iter()
        .map(|h| {
            let label = t!("action.cancel", name = format::href(h, ctx.names)).into_owned();
            Outcome::Mutate(Mutation::new(Method::PATCH, h.clone(), Some(json!({"state": "canceled"})), label))
        })
        .collect();
    confirm(t!("confirm.cancel", what = describe(&hrefs, ctx)), Outcome::Batch(mutations))
}

/// Confirmed form for the global operation at `rel` (`repair/`, `rpm/copy/`).
/// `selection` (hrefs of the current view) pre-fills the repositories to reclaim or prune.
pub fn operation(rel: &str, reg: &Registry, client: &PulpClient, selection: Vec<String>) -> Option<FormView> {
    let operation = reg.operation(rel)?;
    let schema = operation.body_schema.as_deref().and_then(|s| reg.schema(s));
    let (title, label, confirm) = match Op::from_rel(rel) {
        Some(op) => {
            let key = op.key();
            (tr(&format!("op.{key}.title")), tr(&format!("op.{key}.label")), tr(&format!("op.{key}.confirm")))
        }
        None => {
            let name = rel.trim_end_matches('/').to_owned();
            let title = operation.summary.clone().map_or_else(|| name.clone(), |s| format!("{s} · {name}"));
            let confirm = t!("confirm.operation", what = name).into_owned();
            (title, name, confirm)
        }
    };
    // Plugin of `rpm/copy/`, `remotes/python/…` for relation pickers.
    let plugin = match rel.split('/').collect::<Vec<_>>().as_slice() {
        [cat, plugin, ..] if reg.types().iter().any(|t| t.category == *cat && t.plugin == *plugin) => *plugin,
        [plugin, ..] if reg.plugins().contains(plugin) => *plugin,
        _ => "core",
    };
    let mut form = FormView::action(title.clone(), label, operation.path.clone(), schema, reg, plugin)
        .with_confirm(confirm)
        .with_multipart(operation.multipart)
        .with_result(title);
    let repos: Vec<String> = selection.into_iter().filter(|h| h.contains("/repositories/")).collect();
    match Op::from_rel(rel) {
        Some(Op::Orphans) => {
            let query = vec![("orphaned_for".to_owned(), "0".to_owned())];
            form = form.with_count(client.api_path("content/"), query, "op.orphans.count");
        }
        Some(Op::Purge | Op::Repair) => {}
        // `repositories/reclaim_space/`, `rpm/prune/`, `rpm/comps/`…
        Some(Op::Reclaim) | None => {
            if let Some(first) = repos.first() {
                form = form.with_values(&[("repo_hrefs", repos.join(", ")), ("repository", first.clone())]);
            }
        }
    }
    Some(form)
}

/// What a publication or distribution points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Repository,
    Version,
    Publication,
}

fn source_of(href: &str) -> Option<Source> {
    if pulply_api::models::version_number(href).is_some() {
        Some(Source::Version)
    } else if href.contains("/publications/") {
        Some(Source::Publication)
    } else if href.contains("/repositories/") {
        Some(Source::Repository)
    } else {
        None
    }
}

/// Creatable type of `category` for `plugin`, preferring the one named like the plugin.
fn plugin_type<'a>(reg: &'a Registry, category: &str, plugin: &str) -> Option<&'a ResourceType> {
    let candidates: Vec<&ResourceType> = reg
        .types()
        .iter()
        .filter(|t| t.category == category && t.plugin == plugin && t.parent.is_none() && t.ops.create)
        .collect();
    candidates.iter().find(|t| t.name == plugin).or(candidates.first()).copied()
}

fn push(form: Option<FormView>) -> Outcome {
    match form {
        Some(f) => Outcome::Push(Box::new(f)),
        None => Outcome::None,
    }
}

/// Can `href` be synced (repository whose type has a `sync` action, user allowed)?
pub fn can_sync(href: &str, ctx: &Ctx) -> bool {
    source_of(href) == Some(Source::Repository)
        && ctx
            .registry()
            .and_then(|r| r.type_for_href(href))
            .is_some_and(|t| t.has_action("sync") && ctx.allowed("sync", model_of_type(t).as_deref(), Some(href)))
}

/// The user may create resources of `t` (model-level `add_` permission).
pub fn can_create(t: &ResourceType, ctx: &Ctx) -> bool {
    t.ops.create && ctx.allowed("add", model_of_type(t).as_deref(), None)
}

/// The user may run `action` (`change`, `delete`, `manage_roles`…) on resource `href`.
pub fn can_on(action: &str, href: &str, ctx: &Ctx) -> bool {
    let Some(t) = ctx.registry().and_then(|r| r.type_for_href(href)) else { return false };
    let supported = match action {
        "change" => t.ops.partial_update,
        "delete" => t.ops.delete,
        "manage_roles" => t.has_action("list_roles"),
        _ => true,
    };
    supported && ctx.allowed(action, model_of_type(t).as_deref(), Some(href))
}

/// Sync form (R8.1): remote pre-filled from the repository, plugin options from the schema.
pub fn sync(repo: &Value, ctx: &Ctx) -> Outcome {
    if ctx.readonly() {
        return Outcome::readonly();
    }
    let (Some(reg), Some(href)) = (ctx.registry(), repo.href()) else { return Outcome::None };
    let Some(t) = reg.type_for_href(href) else { return Outcome::None };
    let Some(action) = t.actions.iter().find(|a| a.verb == "sync") else { return Outcome::None };
    let name = repo.name().unwrap_or(href);
    let schema = action.body_schema.as_deref().and_then(|s| reg.schema(s));
    let form = FormView::action(
        t!("form.sync", what = name).into_owned(),
        t!("action.sync", name = name).into_owned(),
        format!("{href}sync/"),
        schema,
        reg,
        &t.plugin,
    )
    .with_multipart(action.multipart)
    .with_values(&[("remote", repo.str_field("remote").unwrap_or_default().to_owned())]);
    Outcome::Push(Box::new(form))
}

/// Publication type able to publish `href` (repository or version), if the plugin has one.
fn publication_type<'a>(href: &str, reg: &'a Registry) -> Option<&'a ResourceType> {
    matches!(source_of(href), Some(Source::Repository | Source::Version)).then_some(())?;
    plugin_type(reg, "publications", &reg.type_for_href(href)?.plugin)
}

pub fn can_publish(href: &str, ctx: &Ctx) -> bool {
    ctx.registry().and_then(|r| publication_type(href, r)).is_some_and(|t| can_create(t, ctx))
}

/// Publication form (R8.2) for a repository (latest version) or a specific version.
pub fn publish(href: &str, ctx: &Ctx) -> Outcome {
    if ctx.readonly() {
        return Outcome::readonly();
    }
    let Some(reg) = ctx.registry() else { return Outcome::None };
    let Some(t) = publication_type(href, reg) else { return Outcome::None };
    let field = if source_of(href) == Some(Source::Version) { "repository_version" } else { "repository" };
    let name = format::href(href, ctx.names);
    push(
        FormView::create(t, reg).map(|f| {
            f.with_values(&[(field, href.to_owned())]).with_label(t!("action.publish", name = name).into_owned())
        }),
    )
}

fn distribution_type<'a>(href: &str, reg: &'a Registry) -> Option<&'a ResourceType> {
    source_of(href)?;
    let plugin = &reg.type_for_href(href)?.plugin;
    plugin_type(reg, "distributions", plugin)
}

pub fn can_distribute(href: &str, ctx: &Ctx) -> bool {
    ctx.registry().and_then(|r| distribution_type(href, r)).is_some_and(|t| can_create(t, ctx))
}

/// Distribution form (R8.3) serving a repository, version or publication; the base URL
/// is shown once the creation task completes.
pub fn distribute(href: &str, name: &str, ctx: &Ctx) -> Outcome {
    if ctx.readonly() {
        return Outcome::readonly();
    }
    let Some(reg) = ctx.registry() else { return Outcome::None };
    let (Some(t), Some(source)) = (distribution_type(href, reg), source_of(href)) else { return Outcome::None };
    let field = match source {
        Source::Repository => "repository",
        Source::Version => "repository_version",
        Source::Publication => "publication",
    };
    let base = name.replace(['@', ' '], "-");
    push(FormView::create(t, reg).map(|f| {
        f.with_values(&[("name", base.clone()), ("base_path", base.clone()), (field, href.to_owned())])
            .with_label(t!("action.distribute", name = name).into_owned())
            .with_after(AfterTask::ShowDistribution)
    }))
}

/// The repository supports `modify/` (add/remove content, rollback) and the user may use it.
pub fn can_modify(repo: &str, ctx: &Ctx) -> bool {
    ctx.registry()
        .and_then(|r| r.type_for_href(repo))
        .is_some_and(|t| t.has_action("modify") && ctx.allowed("modify", model_of_type(t).as_deref(), Some(repo)))
}

fn modify(repo: &str, body: Value, label: String) -> Mutation {
    Mutation::new(Method::POST, format!("{repo}modify/"), Some(body), label)
}

/// Rollback (R8.5): new latest version with the content of `version`.
pub fn rollback(version: &str, ctx: &Ctx) -> Outcome {
    if ctx.readonly() {
        return Outcome::readonly();
    }
    let Some(repo) = pulply_api::models::version_repository(version) else { return Outcome::None };
    let (repo_name, version_name) = (format::href(repo, ctx.names), format::href(version, ctx.names));
    let mutation =
        modify(repo, json!({"base_version": version}), t!("action.rollback", name = version_name).into_owned());
    confirm(t!("confirm.rollback", repo = repo_name, version = version_name), Outcome::Mutate(mutation))
}

/// Confirmed removal of content units, based on `version` (R8.4).
pub fn remove_content(repo: &str, version: &str, units: Vec<String>, ctx: &Ctx) -> Outcome {
    if units.is_empty() {
        return Outcome::None;
    }
    if ctx.readonly() {
        return Outcome::readonly();
    }
    let repo_name = format::href(repo, ctx.names);
    let label = t!("action.remove_content", count = units.len(), name = repo_name).into_owned();
    let what = describe(&units, ctx);
    let mutation = modify(repo, json!({"remove_content_units": units, "base_version": version}), label);
    confirm(t!("confirm.remove_content", what = what, repo = repo_name), Outcome::Mutate(mutation))
}

/// Adds content units to `repo`, on top of `base_version` or the latest version (R8.4).
pub fn add_content(repo: &str, base_version: Option<&str>, units: Vec<String>, ctx: &Ctx) -> Outcome {
    if units.is_empty() {
        return Outcome::None;
    }
    if ctx.readonly() {
        return Outcome::readonly();
    }
    let label = t!("action.add_content", count = units.len(), name = format::href(repo, ctx.names)).into_owned();
    let mut body = json!({"add_content_units": units});
    if let Some(v) = base_version {
        body["base_version"] = Value::from(v);
    }
    Outcome::Mutate(modify(repo, body, label))
}

/// Repository type receiving content of `content_type`'s plugin.
pub fn repository_type_for<'a>(content_type: &ResourceType, reg: &'a Registry) -> Option<&'a ResourceType> {
    plugin_type(reg, "repositories", &content_type.plugin).filter(|t| t.has_action("modify"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn operation_forms_cover_every_global_operation() {
        let (reg, session) = (testutil::registry(), testutil::session(false));
        for rel in reg.operation_paths() {
            assert!(operation(rel, &reg, &session.client, Vec::new()).is_some(), "{rel}");
        }
        assert!(operation("nope/", &reg, &session.client, Vec::new()).is_none());
    }
}
