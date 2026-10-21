//! Content of a repository version, per content type (from `content_summary`).

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};
use futures::FutureExt;
use pulply_api::Error;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState};
use rust_i18n::t;
use serde_json::Value;

use super::{Ctx, Fetch, FetchKind, Hint, Outcome, Payload, View, detail, diff, list};
use crate::format;

#[derive(Debug, Default, Clone)]
struct TypeRow {
    ty: String,
    counts: [u64; 3],
    hrefs: [Option<String>; 3],
}

const PRESENT: usize = 0;
const ADDED: usize = 1;
const REMOVED: usize = 2;

pub struct SummaryView {
    href: String,
    version: Option<Value>,
    rows: Vec<TypeRow>,
    state: TableState,
    error: Option<String>,
}

impl SummaryView {
    pub fn new(href: String) -> Self {
        Self { href, version: None, rows: Vec::new(), state: TableState::default().with_selected(Some(0)), error: None }
    }

    fn open(&self, which: usize, ctx: &Ctx) -> Outcome {
        let href = self.state.selected().and_then(|i| self.rows.get(i)).and_then(|r| r.hrefs[which].clone());
        match href {
            Some(h) => Outcome::Push(Box::new(list::ListView::from_url(&h, ctx.registry()))),
            None => Outcome::None,
        }
    }
}

fn parse_rows(version: &Value) -> Vec<TypeRow> {
    let mut rows: Vec<TypeRow> = Vec::new();
    let summary = version.get("content_summary");
    for (idx, key) in ["present", "added", "removed"].into_iter().enumerate() {
        let Some(types) = summary.and_then(|s| s.get(key)).and_then(Value::as_object) else { continue };
        for (ty, entry) in types {
            let pos = match rows.iter().position(|r| &r.ty == ty) {
                Some(p) => p,
                None => {
                    rows.push(TypeRow { ty: ty.clone(), ..TypeRow::default() });
                    rows.len() - 1
                }
            };
            rows[pos].counts[idx] = entry.get("count").and_then(Value::as_u64).unwrap_or_default();
            rows[pos].hrefs[idx] = entry.get("href").and_then(Value::as_str).map(str::to_owned);
        }
    }
    rows.sort_by(|a, b| a.ty.cmp(&b.ty));
    rows
}

impl View for SummaryView {
    fn title(&self) -> String {
        t!("summary.title").into_owned()
    }

    fn info(&self, ctx: &Ctx) -> String {
        format::href(&self.href, ctx.names)
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        let mut h = vec![
            Hint::new("↵", t!("hint.present")),
            Hint::new("+", t!("hint.added")),
            Hint::new("-", t!("hint.removed")),
            Hint::new("d", t!("hint.describe")),
        ];
        if self.version.as_ref().and_then(diff::DiffView::previous_of).is_some() {
            h.push(Hint::new("x", t!("hint.diff")));
        }
        h
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome {
        let len = self.rows.len();
        let cur = self.state.selected().unwrap_or(0);
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.state.select(Some((cur + 1).min(len.saturating_sub(1)))),
            KeyCode::Up | KeyCode::Char('k') => self.state.select(Some(cur.saturating_sub(1))),
            KeyCode::Enter => return self.open(PRESENT, ctx),
            KeyCode::Char('+') => return self.open(ADDED, ctx),
            KeyCode::Char('-') => return self.open(REMOVED, ctx),
            KeyCode::Char('d') => return Outcome::Push(Box::new(detail::DetailView::new(self.href.clone()))),
            KeyCode::Char('x') => {
                return match self.version.as_ref().and_then(diff::DiffView::previous_of) {
                    Some(prev) => Outcome::Push(Box::new(diff::DiffView::new(prev, self.href.clone()))),
                    None => Outcome::None,
                };
            }
            _ => return Outcome::Ignored,
        }
        Outcome::None
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let theme = ctx.theme;
        if let Some(err) = &self.error {
            f.render_widget(Paragraph::new(Line::styled(err.clone(), theme.error())), area);
            return;
        }
        let Some(version) = &self.version else {
            f.render_widget(Paragraph::new(Line::styled(t!("list.loading").into_owned(), theme.muted())), area);
            return;
        };
        let [head, body] = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(area);
        let base = version.get("base_version").and_then(Value::as_str).map(|b| format::href(b, ctx.names));
        let created = version.get("pulp_created").and_then(Value::as_str).map(format::local_time).unwrap_or_default();
        let mut spans = vec![Span::styled(format!("{}: ", t!("summary.created")), theme.accent()), Span::raw(created)];
        if let Some(b) = base {
            spans.push(Span::styled(format!("   {}: ", t!("summary.base")), theme.accent()));
            spans.push(Span::raw(b));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), head);
        if self.rows.is_empty() {
            f.render_widget(Paragraph::new(Line::styled(t!("summary.empty").into_owned(), theme.muted())), body);
            return;
        }
        let bold = theme.accent().add_modifier(Modifier::BOLD);
        let header = Row::new(
            ["type", "present", "added", "removed"]
                .map(|k| Cell::from(Span::styled(crate::labels::tr(&format!("summary.{k}")), bold))),
        );
        let rows = self.rows.iter().map(|r| {
            Row::new([
                Cell::from(r.ty.clone()),
                Cell::from(r.counts[PRESENT].to_string()),
                Cell::from(Span::styled(format!("+{}", r.counts[ADDED]), theme.success())),
                Cell::from(Span::styled(format!("-{}", r.counts[REMOVED]), theme.error())),
            ])
        });
        let widths = [Constraint::Min(30), Constraint::Length(10), Constraint::Length(10), Constraint::Length(10)];
        let table = Table::new(rows, widths).header(header).row_highlight_style(theme.selected());
        f.render_stateful_widget(table, body, &mut self.state);
    }

    fn fetch(&mut self, _kind: FetchKind, ctx: &Ctx) -> Option<Fetch> {
        let client = ctx.client()?.clone();
        let href = self.href.clone();
        Some(async move { client.get(&href, &[]).await.map(Payload::Object) }.boxed())
    }

    fn on_data(&mut self, _kind: FetchKind, result: Result<Payload, Error>, ctx: &Ctx) -> Outcome {
        match result {
            Ok(Payload::Object(v)) => {
                self.rows = parse_rows(&v);
                let resolve = detail::unresolved_hrefs(&v, ctx.names);
                self.version = Some(v);
                self.error = None;
                if resolve.is_empty() { Outcome::None } else { Outcome::Resolve(resolve) }
            }
            Ok(_) => Outcome::None,
            Err(e) => {
                self.error = Some(super::error_text(&e));
                Outcome::None
            }
        }
    }

    /// Repository versions are immutable.
    fn poll_interval(&self, _default: Duration) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merges_summary_sections_per_type() {
        let v = json!({"content_summary": {
            "present": {"file.file": {"count": 3, "href": "/p?repository_version=x"}},
            "added": {"file.file": {"count": 1, "href": "/a"}, "rpm.package": {"count": 2, "href": "/b"}},
            "removed": {}
        }});
        let rows = parse_rows(&v);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].ty, "file.file");
        assert_eq!(rows[0].counts, [3, 1, 0]);
        assert_eq!(rows[1].counts, [0, 2, 0]);
        assert_eq!(rows[1].hrefs[PRESENT], None);
    }
}
