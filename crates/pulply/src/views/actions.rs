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

/// Confirmed form for a global operation. `selection` (hrefs of the current view)
/// pre-fills the repositories to reclaim.
pub fn operation(op: Op, reg: &Registry, client: &PulpClient, selection: Vec<String>) -> Option<FormView> {
    let operation = reg.operation(op.rel_path())?;
    let schema = operation.body_schema.as_deref().and_then(|s| reg.schema(s));
    let key = op.key();
    let mut form = FormView::action(
        tr(&format!("op.{key}.title")),
        tr(&format!("op.{key}.label")),
        operation.path.clone(),
        schema,
        reg,
        "core",
    )
    .with_confirm(tr(&format!("op.{key}.confirm")));
    match op {
        Op::Orphans => {
            let query = vec![("orphaned_for".to_owned(), "0".to_owned())];
            form = form.with_count(client.api_path("content/"), query, "op.orphans.count");
        }
        Op::Reclaim => {
            let repos: Vec<String> = selection.into_iter().filter(|h| h.contains("/repositories/")).collect();
            if !repos.is_empty() {
                form = form.with_values(&[("repo_hrefs", repos.join(", "))]);
            }
        }
        Op::Purge | Op::Repair => {}
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

/// Can `href` be synced (repository whose type has a `sync` action)?
pub fn can_sync(href: &str, ctx: &Ctx) -> bool {
    source_of(href) == Some(Source::Repository)
        && ctx.registry().and_then(|r| r.type_for_href(href)).is_some_and(|t| t.has_action("sync"))
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
    .with_values(&[("remote", repo.str_field("remote").unwrap_or_default().to_owned())]);
    Outcome::Push(Box::new(form))
}

/// Publication type able to publish `href` (repository or version), if the plugin has one.
fn publication_type<'a>(href: &str, reg: &'a Registry) -> Option<&'a ResourceType> {
    matches!(source_of(href), Some(Source::Repository | Source::Version)).then_some(())?;
    plugin_type(reg, "publications", &reg.type_for_href(href)?.plugin)
}

pub fn can_publish(href: &str, ctx: &Ctx) -> bool {
    ctx.registry().and_then(|r| publication_type(href, r)).is_some()
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
    ctx.registry().and_then(|r| distribution_type(href, r)).is_some()
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
