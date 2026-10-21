//! Scrollable list of styled lines with a cursor; lines may carry an href to follow.

use crossterm::event::{KeyCode, KeyEvent};
use pulply_api::models::{is_href, version_number};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use serde_json::Value;

use super::Ctx;
use crate::format;

pub struct LineItem {
    pub line: Line<'static>,
    pub href: Option<String>,
}

impl LineItem {
    pub fn plain(line: impl Into<Line<'static>>) -> Self {
        Self { line: line.into(), href: None }
    }
}

#[derive(Default)]
pub struct LineList {
    pub items: Vec<LineItem>,
    state: ListState,
    page: usize,
}

impl LineList {
    pub fn set(&mut self, items: Vec<LineItem>) {
        self.items = items;
        let max = self.items.len().saturating_sub(1);
        self.state.select(Some(self.state.selected().unwrap_or(0).min(max)));
    }

    pub fn selected_href(&self) -> Option<&str> {
        self.state.selected().and_then(|i| self.items.get(i)).and_then(|l| l.href.as_deref())
    }

    /// Cursor movement; returns false when the key is not a movement key.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        let len = self.items.len();
        if len == 0 {
            return false;
        }
        let cur = self.state.selected().unwrap_or(0);
        let page = self.page.max(1);
        let next = match key.code {
            KeyCode::Down | KeyCode::Char('j') => (cur + 1).min(len - 1),
            KeyCode::Up | KeyCode::Char('k') => cur.saturating_sub(1),
            KeyCode::PageDown => (cur + page).min(len - 1),
            KeyCode::PageUp => cur.saturating_sub(page),
            KeyCode::Home | KeyCode::Char('g') => 0,
            KeyCode::End | KeyCode::Char('G') => len - 1,
            _ => return false,
        };
        self.state.select(Some(next));
        true
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.page = area.height as usize;
        let items: Vec<ListItem> = self
            .items
            .iter()
            .map(|l| {
                let mut line = l.line.clone();
                if l.href.is_some() {
                    line.spans.push(Span::styled(" ↵", ctx.theme.muted()));
                }
                ListItem::new(line)
            })
            .collect();
        let list = List::new(items).highlight_style(ctx.theme.selected());
        f.render_stateful_widget(list, area, &mut self.state);
    }
}

/// Fields shown first when describing a resource.
const LEADING: &[&str] = &["name", "pulp_href", "state", "number", "base_path", "base_url", "url"];

/// YAML-like rendering of a resource: keys colored, hrefs followable and named,
/// timestamps shown in local time with their age.
pub fn json_lines(value: &Value, ctx: &Ctx) -> Vec<LineItem> {
    let mut out = Vec::new();
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = LEADING.iter().filter_map(|k| map.get_key_value(*k).map(|(k, _)| k)).collect();
            keys.extend(map.keys().filter(|k| !LEADING.contains(&k.as_str())));
            for k in keys {
                push_value(&mut out, 0, Some(k), &map[k], ctx);
            }
        }
        other => push_value(&mut out, 0, None, other, ctx),
    }
    out
}

/// Followable link: an href, or an API list URL with a query (content summaries).
fn is_link(s: &str) -> bool {
    is_href(s) || (s.contains("/api/v3/") && s.contains('?'))
}

fn push_value(out: &mut Vec<LineItem>, depth: usize, key: Option<&str>, value: &Value, ctx: &Ctx) {
    let theme = ctx.theme;
    let indent = "  ".repeat(depth);
    let mut spans: Vec<Span<'static>> = vec![Span::raw(indent.clone())];
    match key {
        Some(k) => spans.push(Span::styled(format!("{k}: "), theme.accent())),
        None => spans.push(Span::styled("- ", theme.muted())),
    }
    match value {
        Value::Object(map) if !map.is_empty() => {
            out.push(LineItem::plain(Line::from(spans)));
            for (k, v) in map {
                push_value(out, depth + 1, Some(k), v, ctx);
            }
        }
        Value::Array(items) if !items.is_empty() => {
            spans.push(Span::styled(format!("({})", items.len()), theme.muted()));
            out.push(LineItem::plain(Line::from(spans)));
            for v in items {
                push_value(out, depth + 1, None, v, ctx);
            }
        }
        Value::String(s) if is_link(s) => {
            spans.push(Span::raw(s.clone()));
            if ctx.names.get(s).is_some() || version_number(s).is_some() {
                spans.push(Span::styled(format!("  ({})", format::href(s, ctx.names)), theme.muted()));
            }
            out.push(LineItem { line: Line::from(spans), href: Some(s.clone()) });
        }
        Value::String(s) if s.contains('\n') => {
            // Multi-line strings (tracebacks, certificates) as an indented block.
            out.push(LineItem::plain(Line::from(spans)));
            out.extend(s.lines().map(|l| LineItem::plain(Line::from(format!("{indent}    {l}")))));
        }
        Value::String(s) => {
            match format::parse_time(s) {
                Some(t) => {
                    spans.push(Span::raw(format::local_time(s)));
                    let ago = format::compact_duration((ctx.now - t).num_seconds());
                    spans.push(Span::styled(format!("  ({ago})"), theme.muted()));
                }
                None => spans.push(Span::raw(s.clone())),
            }
            out.push(LineItem::plain(Line::from(spans)));
        }
        Value::Null => {
            spans.push(Span::styled("null", theme.muted()));
            out.push(LineItem::plain(Line::from(spans)));
        }
        Value::Object(_) | Value::Array(_) => {
            spans.push(Span::styled(if value.is_object() { "{}" } else { "[]" }, theme.muted()));
            out.push(LineItem::plain(Line::from(spans)));
        }
        other => {
            spans.push(Span::styled(other.to_string(), Style::new().fg(theme.info)));
            out.push(LineItem::plain(Line::from(spans)));
        }
    }
}
