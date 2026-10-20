//! Builders for modifying workflows: each returns the [`Outcome`] to apply
//! (usually a confirmation overlay wrapping one or more mutations).

use pulply_api::{Method, PulpClient, Registry};
use rust_i18n::t;
use serde_json::json;

use super::confirm::confirm;
use super::form::FormView;
use super::{Ctx, Mutation, Outcome};
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
