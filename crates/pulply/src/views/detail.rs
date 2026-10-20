//! Describe view: every field of one resource, hrefs followable.

use std::collections::BTreeSet;

use crossterm::event::{KeyCode, KeyEvent};
use futures::FutureExt;
use pulply_api::models::{is_href, version_number, version_repository};
use pulply_api::{Error, ResourceExt};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use rust_i18n::t;
use serde_json::Value;

use super::lines::{LineList, json_lines};
use super::{Ctx, Fetch, FetchKind, Hint, Outcome, Payload, View, open_href};
use crate::format;
use crate::names::NameCache;

/// Max hrefs resolved to names per payload.
const MAX_RESOLVE: usize = 25;

pub struct DetailView {
    href: String,
    /// Name, or href rendered with known names once data arrived.
    label: Option<String>,
    data: Option<Value>,
    lines: LineList,
    error: Option<String>,
    resolved: bool,
}

impl DetailView {
    pub fn new(href: String) -> Self {
        Self { href, label: None, data: None, lines: LineList::default(), error: None, resolved: false }
    }
}

/// Hrefs inside `value` whose display name is unknown (repository versions resolve
/// through their repository).
pub fn unresolved_hrefs(value: &Value, names: &NameCache) -> Vec<String> {
    fn walk(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::String(s) if is_href(s) => {
                let target = match (version_number(s), version_repository(s)) {
                    (Some(_), Some(repo)) => repo,
                    _ => s.as_str(),
                };
                out.insert(target.to_owned());
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut set = BTreeSet::new();
    walk(value, &mut set);
    let own = value.href();
    set.into_iter()
        .filter(|h| names.get(h).is_none() && Some(h.as_str()) != own && !h.ends_with("/versions/"))
        .take(MAX_RESOLVE)
        .collect()
}

impl View for DetailView {
    fn title(&self) -> String {
        self.label.clone().unwrap_or_else(|| format::short_href(&self.href))
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        vec![Hint::new("↵", t!("hint.follow")), Hint::new("j/k", t!("hint.scroll"))]
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome {
        if self.lines.handle_key(key) {
            return Outcome::None;
        }
        match key.code {
            KeyCode::Enter => match self.lines.selected_href().and_then(|h| open_href(h, ctx)) {
                Some(view) => Outcome::Push(view),
                None => Outcome::None,
            },
            _ => Outcome::Ignored,
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        match (&self.data, &self.error) {
            (_, Some(err)) => f.render_widget(Paragraph::new(Line::styled(err.clone(), ctx.theme.error())), area),
            (None, None) => {
                f.render_widget(Paragraph::new(Line::styled(t!("list.loading").into_owned(), ctx.theme.muted())), area)
            }
            (Some(data), None) => {
                // Rebuilt each frame so names resolved later show up.
                self.lines.set(json_lines(data, ctx));
                self.lines.render(f, area, ctx);
            }
        }
    }

    fn fetch(&mut self, _kind: FetchKind, ctx: &Ctx) -> Option<Fetch> {
        let client = ctx.client()?.clone();
        let href = self.href.clone();
        Some(async move { client.get(&href, &[]).await.map(Payload::Object) }.boxed())
    }

    fn on_data(&mut self, _kind: FetchKind, result: Result<Payload, Error>, ctx: &Ctx) -> Outcome {
        match result {
            Ok(Payload::Object(v)) => {
                self.error = None;
                let resolve = if self.resolved { Vec::new() } else { unresolved_hrefs(&v, ctx.names) };
                self.resolved = true;
                self.label = Some(v.name().map(str::to_owned).unwrap_or_else(|| format::href(&self.href, ctx.names)));
                self.data = Some(v);
                if resolve.is_empty() { Outcome::None } else { Outcome::Resolve(resolve) }
            }
            Ok(_) => Outcome::None,
            Err(e) => {
                self.error = Some(e.to_string());
                Outcome::None
            }
        }
    }
}
