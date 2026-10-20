//! Views stacked in the main area. Each view owns its state, declares what to
//! fetch, and receives the result; the app runs fetches off the render loop.

pub mod columns;
pub mod connect;
pub mod contexts;
pub mod dashboard;
pub mod detail;
pub mod lines;
pub mod list;
pub mod summary;
pub mod task;

use std::time::Duration;

use chrono::{DateTime, Utc};
use crossterm::event::KeyEvent;
use futures::future::BoxFuture;
use pulply_api::{Error, Page, PulpClient, Registry};
use ratatui::Frame;
use ratatui::layout::Rect;
use serde_json::Value;

use crate::commands::Target;
use crate::config::Config;
use crate::names::NameCache;
use crate::session::Session;
use crate::theme::Theme;

pub type Fetch = BoxFuture<'static, Result<Payload, Error>>;

pub enum Payload {
    Page(Page),
    Object(Value),
    Dashboard(Box<dashboard::DashboardData>),
    Task { task: Value, children: Vec<Value> },
    Reachability(Vec<(String, Result<String, String>)>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FetchKind {
    /// Reload the current data (initial load, polling, Ctrl-R).
    Refresh,
    /// Next page of a paginated list.
    More,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Error,
}

/// Read-only state handed to views.
pub struct Ctx<'a> {
    pub session: Option<&'a Session>,
    pub names: &'a NameCache,
    pub theme: &'a Theme,
    pub config: &'a Config,
    pub current_ctx: &'a str,
    pub now: DateTime<Utc>,
}

impl Ctx<'_> {
    pub fn client(&self) -> Option<&PulpClient> {
        self.session.map(|s| &s.client)
    }
    pub fn registry(&self) -> Option<&Registry> {
        self.session.and_then(|s| s.registry.as_deref())
    }
}

pub enum Outcome {
    /// Key not handled: the app applies its global fallback.
    Ignored,
    None,
    Push(Box<dyn View>),
    Fetch(FetchKind),
    Open(Target),
    SwitchContext(String),
    SubmitPassword(String),
    Retry,
    /// Fetch display names for these hrefs.
    Resolve(Vec<String>),
}

pub struct Hint {
    pub key: &'static str,
    pub label: String,
}

impl Hint {
    pub fn new(key: &'static str, label: impl Into<String>) -> Self {
        Self { key, label: label.into() }
    }
}

pub trait View: Send {
    /// Breadcrumb segment.
    fn title(&self) -> String;

    /// Right-aligned text in the frame title (counts, filter, sort).
    fn info(&self, _ctx: &Ctx) -> String {
        String::new()
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        Vec::new()
    }

    /// When true, every key except Ctrl-C goes to the view first (text inputs).
    fn captures_input(&self) -> bool {
        false
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome;

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx);

    fn fetch(&mut self, _kind: FetchKind, _ctx: &Ctx) -> Option<Fetch> {
        None
    }

    fn on_data(&mut self, _kind: FetchKind, _result: Result<Payload, Error>, _ctx: &Ctx) -> Outcome {
        Outcome::None
    }

    /// Polling period while this view is on top; `None` disables polling.
    fn poll_interval(&self, default: Duration) -> Option<Duration> {
        Some(default)
    }

    fn filterable(&self) -> bool {
        false
    }

    fn filter(&self) -> &str {
        ""
    }

    /// `commit` is false while typing (client-side only) and true on Enter.
    fn set_filter(&mut self, _input: &str, _commit: bool) -> Outcome {
        Outcome::None
    }
}

/// Picks the view for an href: list endpoints, versions, tasks, or a describe view.
pub fn open_href(href: &str, ctx: &Ctx) -> Option<Box<dyn View>> {
    let reg = ctx.registry();
    if href.contains('?') {
        return Some(Box::new(list::ListView::from_url(href, reg)));
    }
    if pulply_api::models::version_number(href).is_some() {
        return Some(Box::new(summary::SummaryView::new(href.to_owned())));
    }
    if let Some(reg) = reg
        && let Some(t) = reg.type_for_href(href)
    {
        if t.list_path.as_deref() == Some(href) {
            return Some(Box::new(list::ListView::for_type(t, reg, None)));
        }
        if t.id == "tasks" {
            return Some(Box::new(task::TaskView::new(href.to_owned())));
        }
        let rel = href.strip_prefix(t.list_path.as_deref().unwrap_or_default()).unwrap_or_default();
        // `<id>/versions/` → nested collection of the owning resource.
        if let Some((_, sub)) = rel.trim_end_matches('/').split_once('/')
            && let Some(child) = t.children.iter().filter_map(|c| reg.get(c)).find(|c| c.name == sub)
        {
            let owner = &href[..href.len() - sub.len() - 1];
            return Some(Box::new(list::ListView::children(child, reg, owner, ctx)));
        }
    }
    pulply_api::models::is_href(href).then(|| Box::new(detail::DetailView::new(href.to_owned())) as Box<dyn View>)
}
