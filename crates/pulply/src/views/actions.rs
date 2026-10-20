//! Builders for modifying workflows: each returns the [`Outcome`] to apply
//! (usually a confirmation overlay wrapping one or more mutations).

use pulply_api::Method;
use rust_i18n::t;
use serde_json::json;

use super::confirm::confirm;
use super::{Ctx, Mutation, Outcome};
use crate::format;

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
