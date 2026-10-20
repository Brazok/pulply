//! Generic paginated table for any list endpoint (also used for tasks).

use std::collections::HashSet;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures::FutureExt;
use pulply_api::{Error, Registry, ResourceExt, ResourceType};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState};
use rust_i18n::t;
use serde_json::Value;

use super::columns::{self, Column, SortValue};
use super::{Ctx, Fetch, FetchKind, Hint, Outcome, Payload, View, actions, detail, form, summary, task};
use crate::format;
use crate::labels;

const PAGE_SIZE: usize = 100;
/// Upper bound of rows reloaded by a refresh (pages loaded so far).
const MAX_REFRESH: usize = 1000;
/// Prefetch the next page when the cursor gets this close to the end.
const PREFETCH_MARGIN: usize = 10;
const TASK_STATES: [&str; 7] = ["", "running", "waiting", "failed", "completed", "canceled", "skipped"];
/// Max hrefs sent for name resolution per page.
const MAX_RESOLVE: usize = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Generic,
    Tasks,
}

pub struct ListView {
    title: String,
    kind: Kind,
    path: String,
    base_query: Vec<(String, String)>,
    columns: Vec<Column>,
    /// Endpoint accepts `?fields=` (lighter payloads for big content types).
    fields_param: bool,
    ordering_fields: Vec<String>,
    rows: Vec<Value>,
    count: u64,
    /// Indices into `rows` after client filter/sort, rebuilt on render.
    visible: Vec<usize>,
    state: TableState,
    page_height: usize,
    filter: String,
    client_filter: String,
    server_query: Vec<(String, String)>,
    sort: Option<(usize, bool)>,
    server_ordering: Option<String>,
    loading: bool,
    loaded: bool,
    error: Option<String>,
    task_state: usize,
    /// Resource to re-select once `visible` is rebuilt after a refresh.
    keep_href: Option<String>,
    /// Hrefs already sent for name resolution.
    requested: HashSet<String>,
    /// Rows marked with Space for bulk actions.
    marked: HashSet<String>,
    /// Own resource type (create form, actions).
    type_id: Option<String>,
    /// Picker mode: Enter hands the selected href to a form field instead of opening it.
    pick: Option<(u64, String)>,
}

impl ListView {
    fn new(
        title: String,
        path: String,
        base_query: Vec<(String, String)>,
        t: Option<&ResourceType>,
        reg: Option<&Registry>,
    ) -> Self {
        let kind = if t.is_some_and(|t| t.id == "tasks") { Kind::Tasks } else { Kind::Generic };
        let columns = match t {
            Some(t) => columns::for_type(t, reg.and_then(|r| r.read_schema(t))),
            None => vec![
                Column::new("NAME", columns::ColKey::Field("name".into())),
                Column::new("HREF", columns::ColKey::Field("pulp_href".into())),
            ],
        };
        Self {
            title,
            kind,
            path,
            base_query,
            columns,
            fields_param: t.is_some_and(|t| t.param("fields").is_some()),
            ordering_fields: t.map(|t| t.ordering_fields().to_vec()).unwrap_or_default(),
            rows: Vec::new(),
            count: 0,
            visible: Vec::new(),
            state: TableState::default().with_selected(Some(0)),
            page_height: 20,
            filter: String::new(),
            client_filter: String::new(),
            server_query: Vec::new(),
            sort: None,
            server_ordering: None,
            loading: false,
            loaded: false,
            error: None,
            task_state: 0,
            keep_href: None,
            requested: HashSet::new(),
            marked: HashSet::new(),
            type_id: t.map(|t| t.id.clone()),
            pick: None,
        }
    }

    /// Picker for a form's href field: Enter returns the selected row's href.
    pub fn picker(t: &ResourceType, reg: &Registry, form: u64, field: &str) -> Self {
        let title = t!("list.pick", what = field).into_owned();
        let mut list = Self::for_type(t, reg, Some(title));
        list.pick = Some((form, field.to_owned()));
        list
    }

    pub fn for_type(t: &ResourceType, reg: &Registry, title: Option<String>) -> Self {
        let path = t.list_path.clone().unwrap_or_default();
        Self::new(title.unwrap_or_else(|| labels::type_title(t)), path, Vec::new(), Some(t), Some(reg))
    }

    /// Nested collection of `owner_href` (e.g. a repository's versions).
    pub fn children(child: &ResourceType, reg: &Registry, owner_href: &str, ctx: &Ctx) -> Self {
        let sub = child.parent.as_ref().map(|(_, s)| s.as_str()).unwrap_or_default();
        let owner = format::href(owner_href, ctx.names);
        let title = format!("{owner} · {}", labels::titleize(&child.name));
        Self::new(title, format!("{owner_href}{sub}"), Vec::new(), Some(child), Some(reg))
    }

    /// List at an API URL carrying a query (content summary links).
    pub fn from_url(href: &str, reg: Option<&Registry>) -> Self {
        let (path, query) = href.split_once('?').unwrap_or((href, ""));
        let base_query: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes()).into_owned().collect();
        let t = reg.and_then(|r| r.type_for_href(path));
        let title = t.map(labels::type_title).unwrap_or_else(|| path.to_owned());
        Self::new(title, path.to_owned(), base_query, t, reg)
    }

    /// Task list pre-filtered on a state (`failed`, `running`, …).
    pub fn with_task_state(mut self, state: &str) -> Self {
        self.task_state = TASK_STATES.iter().position(|s| *s == state).unwrap_or(0);
        self
    }

    fn query(&self, offset: usize, limit: usize) -> Vec<(String, String)> {
        let mut q = self.base_query.clone();
        q.extend(self.server_query.iter().cloned());
        q.push(("limit".into(), limit.to_string()));
        q.push(("offset".into(), offset.to_string()));
        if let Some(o) = &self.server_ordering {
            q.push(("ordering".into(), o.clone()));
        }
        if self.kind == Kind::Tasks && self.task_state > 0 {
            q.push(("state".into(), TASK_STATES[self.task_state].into()));
        }
        if self.fields_param {
            let mut fields = vec!["pulp_href", "name", "prn"];
            for c in &self.columns {
                fields.extend(c.api_fields());
            }
            fields.sort_unstable();
            fields.dedup();
            q.push(("fields".into(), fields.join(",")));
        }
        q
    }

    fn selected(&self) -> Option<&Value> {
        self.state.selected().and_then(|i| self.visible.get(i)).and_then(|&i| self.rows.get(i))
    }

    fn rebuild_visible(&mut self, ctx: &Ctx) {
        let needle = self.client_filter.to_lowercase();
        let mut visible: Vec<usize> = (0..self.rows.len())
            .filter(|&i| {
                needle.is_empty()
                    || self.columns.iter().any(|c| c.text(&self.rows[i], ctx).0.to_lowercase().contains(&needle))
            })
            .collect();
        if let (Some((col, desc)), None) = (self.sort, &self.server_ordering)
            && let Some(column) = self.columns.get(col)
        {
            let mut keyed: Vec<(SortValue, usize)> =
                visible.iter().map(|&i| (column.sort_value(&self.rows[i], ctx), i)).collect();
            keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            if desc {
                keyed.reverse();
            }
            visible = keyed.into_iter().map(|(_, i)| i).collect();
        }
        self.visible = visible;
        if let Some(href) = self.keep_href.take()
            && let Some(pos) = self.visible.iter().position(|&i| self.rows[i].href() == Some(href.as_str()))
        {
            self.state.select(Some(pos));
        }
        let max = self.visible.len().saturating_sub(1);
        if self.state.selected().is_none_or(|s| s > max) {
            self.state.select(Some(max));
        }
    }

    fn move_cursor(&mut self, key: KeyEvent) -> Option<Outcome> {
        let len = self.visible.len();
        let cur = self.state.selected().unwrap_or(0);
        let page = self.page_height.max(1);
        let next = match key.code {
            KeyCode::Down | KeyCode::Char('j') => cur + 1,
            KeyCode::Up | KeyCode::Char('k') => cur.saturating_sub(1),
            KeyCode::PageDown => cur + page,
            KeyCode::PageUp => cur.saturating_sub(page),
            KeyCode::Home | KeyCode::Char('g') => 0,
            KeyCode::End | KeyCode::Char('G') => usize::MAX,
            _ => return None,
        };
        self.state.select(Some(next.min(len.saturating_sub(1))));
        let near_end = next.saturating_add(PREFETCH_MARGIN) >= len;
        if near_end && (self.rows.len() as u64) < self.count && !self.loading && self.client_filter.is_empty() {
            return Some(Outcome::Fetch(FetchKind::More));
        }
        Some(Outcome::None)
    }

    fn open_selected(&self, ctx: &Ctx, describe: bool) -> Outcome {
        let Some(href) = self.selected().and_then(|r| r.href()).map(str::to_owned) else { return Outcome::None };
        if self.kind == Kind::Tasks {
            return Outcome::Push(Box::new(task::TaskView::new(href)));
        }
        if describe {
            return Outcome::Push(Box::new(detail::DetailView::new(href)));
        }
        if pulply_api::models::version_number(&href).is_some() {
            return Outcome::Push(Box::new(summary::SummaryView::new(href)));
        }
        if let Some(reg) = ctx.registry()
            && let Some(t) = reg.type_for_href(&href)
            && let Some(versions) = t.children.iter().filter_map(|c| reg.get(c)).find(|c| c.name == "versions")
        {
            return Outcome::Push(Box::new(ListView::children(versions, reg, &href, ctx)));
        }
        Outcome::Push(Box::new(detail::DetailView::new(href)))
    }

    fn toggle_sort(&mut self, letter: char) -> Outcome {
        let Some(col) =
            self.columns.iter().position(|c| c.header.chars().next().is_some_and(|h| h.eq_ignore_ascii_case(&letter)))
        else {
            return Outcome::Ignored;
        };
        let desc = matches!(self.sort, Some((c, d)) if c == col && !d);
        self.sort = Some((col, desc));
        let server = self.columns[col].sort_field().filter(|f| self.ordering_fields.iter().any(|o| o == f));
        let previous = self.server_ordering.take();
        self.server_ordering = server.map(|f| if desc { format!("-{f}") } else { f.to_owned() });
        if self.server_ordering.is_some() || previous.is_some() {
            Outcome::Fetch(FetchKind::Refresh)
        } else {
            Outcome::None
        }
    }

    fn own_type<'a>(&self, ctx: &'a Ctx) -> Option<&'a ResourceType> {
        ctx.registry()?.get(self.type_id.as_deref()?)
    }

    /// Type of the resource under the cursor (differs from `own_type` in cross-plugin lists).
    fn selected_type<'a>(&self, ctx: &'a Ctx) -> Option<&'a ResourceType> {
        ctx.registry()?.type_for_href(self.selected()?.href()?)
    }

    /// The resource under the cursor supports DELETE.
    fn can_delete(&self, ctx: &Ctx) -> bool {
        self.selected_type(ctx).is_some_and(|t| t.ops.delete)
    }

    fn has_running_tasks(&self) -> bool {
        self.kind == Kind::Tasks
            && self.rows.iter().any(|r| matches!(r.str_field("state"), Some("running" | "waiting" | "canceling")))
    }

    /// Hrefs (href columns) and PRNs/hrefs (task resources) whose name is unknown and
    /// not yet requested.
    fn unresolved_hrefs(&mut self, ctx: &Ctx) -> Vec<String> {
        let fields: Vec<&str> = self
            .columns
            .iter()
            .filter(|c| matches!(c.key, columns::ColKey::Href(_) | columns::ColKey::AnyHref(_)))
            .flat_map(|c| c.api_fields())
            .collect();
        let mut out = Vec::new();
        for row in &self.rows {
            let records = row
                .get("reserved_resources_record")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|r| !format::is_domain_record(r))
                .map(|r| r.strip_prefix("shared:").unwrap_or(r));
            for key in fields.iter().filter_map(|f| row.str_field(f)).chain(records) {
                let target = match pulply_api::models::version_repository(key) {
                    Some(repo) if pulply_api::models::version_number(key).is_some() => repo,
                    _ => key,
                };
                if ctx.names.get(target).is_none() && self.requested.insert(target.to_owned()) {
                    out.push(target.to_owned());
                }
            }
            if out.len() >= MAX_RESOLVE {
                break;
            }
        }
        out
    }
}

/// Splits filter input into client text and server params (`-q expr`, `-f key=value`).
fn parse_filter(input: &str) -> (String, Vec<(String, String)>) {
    let mut client = Vec::new();
    let mut server = Vec::new();
    let mut words = input.split_whitespace().peekable();
    while let Some(w) = words.next() {
        match w {
            "-q" => {
                let mut expr = Vec::new();
                while let Some(next) = words.next_if(|n| *n != "-q" && *n != "-f") {
                    expr.push(next);
                }
                if !expr.is_empty() {
                    server.push(("q".to_owned(), expr.join(" ")));
                }
            }
            "-f" => {
                if let Some((k, v)) = words.next().and_then(|kv| kv.split_once('=')) {
                    server.push((k.to_owned(), v.to_owned()));
                }
            }
            other => client.push(other),
        }
    }
    (client.join(" "), server)
}

impl View for ListView {
    fn title(&self) -> String {
        self.title.clone()
    }

    fn info(&self, _ctx: &Ctx) -> String {
        let mut parts = vec![format!("{}/{}", self.visible.len(), self.count)];
        if self.kind == Kind::Tasks && self.task_state > 0 {
            parts.push(format!("{}={}", t!("list.state"), TASK_STATES[self.task_state]));
        }
        if !self.filter.is_empty() {
            parts.push(format!("/{}", self.filter));
        }
        if self.loading {
            parts.push("…".into());
        }
        parts.join("  ")
    }

    fn hints(&self, ctx: &Ctx) -> Vec<Hint> {
        if self.pick.is_some() {
            return vec![
                Hint::new("↵", t!("hint.select")),
                Hint::new("/", t!("hint.filter")),
                Hint::new("esc", t!("hint.cancel")),
            ];
        }
        let mut h = vec![
            Hint::new("↵", t!("hint.open")),
            Hint::new("d", t!("hint.describe")),
            Hint::new("/", t!("hint.filter")),
        ];
        if self.kind == Kind::Tasks {
            h.push(Hint::new("f", t!("hint.task_state")));
        }
        if !ctx.readonly() {
            if self.own_type(ctx).is_some_and(|t| t.ops.create) {
                h.push(Hint::new("c", t!("hint.create")));
            }
            if self.selected_type(ctx).is_some_and(|t| t.ops.partial_update) {
                h.push(Hint::new("u", t!("hint.edit")));
                h.push(Hint::new("e", t!("hint.yaml")));
            }
            h.push(Hint::new("space", t!("hint.mark")));
            if self.kind == Kind::Tasks {
                h.push(Hint::new("ctrl-k", t!("hint.cancel_task")));
            } else if self.can_delete(ctx) {
                h.push(Hint::new("ctrl-d", t!("hint.delete")));
            }
        }
        h.push(Hint::new("⇧A-Z", t!("hint.sort")));
        h
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome {
        if let Some(o) = self.move_cursor(key) {
            return o;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if let Some((form, field)) = &self.pick {
            return match key.code {
                KeyCode::Enter => match self.selected().and_then(|r| r.href()) {
                    Some(href) => Outcome::Batch(vec![
                        Outcome::Pop,
                        Outcome::Deliver { view: *form, key: field.clone(), value: Value::from(href) },
                    ]),
                    None => Outcome::None,
                },
                KeyCode::Esc if !self.filter.is_empty() => self.set_filter("", true),
                _ => Outcome::Ignored,
            };
        }
        let mutating = matches!(key.code, KeyCode::Char('c' | 'u' | 'e' | ' '))
            || (ctrl && matches!(key.code, KeyCode::Char('d' | 'k')));
        if mutating && ctx.readonly() {
            return Outcome::readonly();
        }
        match key.code {
            KeyCode::Enter => self.open_selected(ctx, false),
            KeyCode::Char('d') if !ctrl => self.open_selected(ctx, true),
            KeyCode::Char('f') if self.kind == Kind::Tasks => {
                self.task_state = (self.task_state + 1) % TASK_STATES.len();
                Outcome::Fetch(FetchKind::Refresh)
            }
            KeyCode::Char('c') => match (ctx.registry(), self.own_type(ctx)) {
                (Some(reg), Some(t)) => match form::FormView::create(t, reg) {
                    Some(f) => Outcome::Push(Box::new(f)),
                    None => Outcome::None,
                },
                _ => Outcome::None,
            },
            KeyCode::Char('e') if self.selected_type(ctx).is_some_and(|t| t.ops.partial_update) => {
                self.selected().and_then(|r| r.href()).map_or(Outcome::None, |h| Outcome::EditExternal(h.to_owned()))
            }
            KeyCode::Char('u') => {
                let (Some(reg), Some(t), Some(row)) = (ctx.registry(), self.selected_type(ctx), self.selected()) else {
                    return Outcome::None;
                };
                let href = row.href().unwrap_or_default();
                let name = row.name().map(str::to_owned).unwrap_or_else(|| format::href(href, ctx.names));
                match form::FormView::edit(t, href, &name, reg) {
                    Some(f) => Outcome::Push(Box::new(f)),
                    None => Outcome::None,
                }
            }
            KeyCode::Char(' ') => {
                if let Some(href) = self.selected().and_then(|r| r.href()).map(str::to_owned)
                    && !self.marked.remove(&href)
                {
                    self.marked.insert(href);
                }
                self.move_cursor(KeyEvent::from(KeyCode::Down)).unwrap_or(Outcome::None)
            }
            KeyCode::Char('d') if ctrl && self.kind != Kind::Tasks && self.can_delete(ctx) => {
                actions::delete(self.selection(), ctx, false)
            }
            KeyCode::Char('k') if ctrl && self.kind == Kind::Tasks => {
                let active: Vec<String> = self
                    .selection()
                    .into_iter()
                    .filter(|h| {
                        self.rows
                            .iter()
                            .any(|r| r.href() == Some(h) && r.str_field("state").is_some_and(task::is_active))
                    })
                    .collect();
                actions::cancel_tasks(active, ctx)
            }
            KeyCode::Esc if !self.filter.is_empty() => self.set_filter("", true),
            KeyCode::Char(c) if c.is_ascii_uppercase() && !ctrl => self.toggle_sort(c),
            _ => Outcome::Ignored,
        }
    }

    fn selection(&self) -> Vec<String> {
        if self.marked.is_empty() {
            self.selected().and_then(|r| r.href()).map(|h| vec![h.to_owned()]).unwrap_or_default()
        } else {
            let mut marked: Vec<String> = self.marked.iter().cloned().collect();
            marked.sort();
            marked
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.rebuild_visible(ctx);
        let [table_area, status_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(u16::from(self.error.is_some()))]).areas(area);
        self.page_height = table_area.height.saturating_sub(1) as usize;
        if let Some(err) = &self.error {
            f.render_widget(Paragraph::new(Line::styled(err.clone(), ctx.theme.error())), status_area);
        }
        if self.rows.is_empty() {
            let msg = if self.loaded { t!("list.empty") } else { t!("list.loading") };
            f.render_widget(Paragraph::new(Line::styled(msg.into_owned(), ctx.theme.muted())), table_area);
            return;
        }
        let texts: Vec<Vec<(String, Style)>> = self
            .visible
            .iter()
            .map(|&i| {
                let row = &self.rows[i];
                let marked = row.href().is_some_and(|h| self.marked.contains(h));
                let mut cells: Vec<(String, Style)> = self.columns.iter().map(|c| c.text(row, ctx)).collect();
                if let Some(first) = cells.first_mut() {
                    first.0.insert_str(0, if marked { "● " } else { "  " });
                    if marked {
                        first.1 = ctx.theme.accent();
                    }
                }
                cells
            })
            .collect();
        let widths: Vec<Constraint> = self
            .columns
            .iter()
            .enumerate()
            .map(|(ci, c)| {
                let content = texts
                    .iter()
                    .take(500)
                    .map(|r| unicode_width::UnicodeWidthStr::width(r[ci].0.as_str()))
                    .max()
                    .unwrap_or(0);
                let header_w = c.header.len() + 1 + if ci == 0 { 2 } else { 0 };
                Constraint::Length((content.max(header_w) as u16).clamp(4, 60))
            })
            .collect();
        let header = Row::new(self.columns.iter().enumerate().map(|(i, c)| {
            let arrow = match self.sort {
                Some((col, true)) if col == i => "↓",
                Some((col, false)) if col == i => "↑",
                _ => "",
            };
            let pad = if i == 0 { "  " } else { "" };
            Cell::from(Span::styled(
                format!("{pad}{}{arrow}", c.header),
                ctx.theme.accent().add_modifier(Modifier::BOLD),
            ))
        }));
        let rows = texts
            .into_iter()
            .map(|cells| Row::new(cells.into_iter().map(|(text, style)| Cell::from(Span::styled(text, style)))));
        let table = Table::new(rows, widths).header(header).column_spacing(2).row_highlight_style(ctx.theme.selected());
        f.render_stateful_widget(table, table_area, &mut self.state);
    }

    fn fetch(&mut self, kind: FetchKind, ctx: &Ctx) -> Option<Fetch> {
        let client = ctx.client()?.clone();
        let query = match kind {
            FetchKind::Refresh => self.query(0, self.rows.len().clamp(PAGE_SIZE, MAX_REFRESH)),
            FetchKind::More => self.query(self.rows.len(), PAGE_SIZE),
        };
        self.loading = true;
        let path = self.path.clone();
        Some(async move { client.list(&path, &query).await.map(Payload::Page) }.boxed())
    }

    fn on_data(&mut self, kind: FetchKind, result: Result<Payload, Error>, ctx: &Ctx) -> Outcome {
        self.loading = false;
        self.loaded = true;
        match result {
            Ok(Payload::Page(page)) => {
                self.error = None;
                self.count = page.count;
                match kind {
                    FetchKind::Refresh => {
                        // Keep the cursor on the same resource across refreshes.
                        self.keep_href = self.selected().and_then(|r| r.href()).map(str::to_owned);
                        self.rows = page.results;
                        let rows = &self.rows;
                        self.marked.retain(|h| rows.iter().any(|r| r.href() == Some(h.as_str())));
                    }
                    FetchKind::More => self.rows.extend(page.results),
                }
                let resolve = self.unresolved_hrefs(ctx);
                if !resolve.is_empty() {
                    return Outcome::Resolve(resolve);
                }
            }
            Ok(_) => {}
            // Some plugin endpoints crash on `?fields=` (deb source_packages, gem): retry without it.
            Err(Error::Http { status: 500, .. }) if self.fields_param => {
                self.fields_param = false;
                return Outcome::Fetch(kind);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        Outcome::None
    }

    fn poll_interval(&self, default: Duration) -> Option<Duration> {
        Some(if self.has_running_tasks() { Duration::from_secs(1) } else { default })
    }

    fn filterable(&self) -> bool {
        true
    }

    fn filter(&self) -> &str {
        &self.filter
    }

    fn set_filter(&mut self, input: &str, commit: bool) -> Outcome {
        let (client, server) = parse_filter(input);
        self.client_filter = client;
        self.state.select(Some(0));
        if !commit {
            return Outcome::None;
        }
        self.filter = input.trim().to_owned();
        if server != self.server_query {
            self.server_query = server;
            return Outcome::Fetch(FetchKind::Refresh);
        }
        Outcome::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_input_splits_client_and_server_parts() {
        assert_eq!(parse_filter("iso"), ("iso".into(), vec![]));
        let (client, server) = parse_filter("rhel -q name__contains=\"x\" -f state=failed nine");
        assert_eq!(client, "rhel nine");
        assert_eq!(server, vec![("q".into(), "name__contains=\"x\"".into()), ("state".into(), "failed".into())]);
        assert_eq!(parse_filter("-f broken"), (String::new(), vec![]));
    }
}
