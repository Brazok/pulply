//! Read-only JSON view: response of a GET action (`metrics/`, `profile_artifacts/`…)
//! or the synchronous result of an operation (`import-check/`).

use crossterm::event::{KeyCode, KeyEvent};
use futures::FutureExt;
use pulply_api::Error;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use rust_i18n::t;
use serde_json::Value;

use super::detail::unresolved_hrefs;
use super::lines::{LineList, json_lines};
use super::{Ctx, Fetch, FetchKind, Hint, Outcome, Payload, View, open_href};

pub struct RawView {
    title: String,
    /// Endpoint fetched (and refreshed); `None` for a fixed value.
    path: Option<String>,
    data: Option<Value>,
    lines: LineList,
    error: Option<String>,
    resolved: bool,
}

impl RawView {
    /// JSON returned by `GET path`.
    pub fn fetching(title: String, path: String) -> Self {
        Self { title, path: Some(path), data: None, lines: LineList::default(), error: None, resolved: false }
    }

    /// A value already received (operation result).
    pub fn value(title: String, value: Value) -> Self {
        Self { title, path: None, data: Some(value), lines: LineList::default(), error: None, resolved: false }
    }
}

impl View for RawView {
    fn title(&self) -> String {
        self.title.clone()
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
                self.lines.set(json_lines(data, ctx));
                self.lines.render(f, area, ctx);
            }
        }
    }

    fn fetch(&mut self, _kind: FetchKind, ctx: &Ctx) -> Option<Fetch> {
        let (client, path) = (ctx.client()?.clone(), self.path.clone()?);
        Some(async move { client.get(&path, &[]).await.map(Payload::Object) }.boxed())
    }

    fn on_data(&mut self, _kind: FetchKind, result: Result<Payload, Error>, ctx: &Ctx) -> Outcome {
        match result {
            Ok(Payload::Object(v)) => {
                self.error = None;
                let resolve = if self.resolved { Vec::new() } else { unresolved_hrefs(&v, ctx.names) };
                self.resolved = true;
                self.data = Some(v);
                if resolve.is_empty() { Outcome::None } else { Outcome::Resolve(resolve) }
            }
            Ok(_) => Outcome::None,
            Err(e) => {
                self.error = Some(super::error_text(&e));
                Outcome::None
            }
        }
    }

    fn poll_interval(&self, default: std::time::Duration) -> Option<std::time::Duration> {
        self.path.as_ref().map(|_| default)
    }
}
