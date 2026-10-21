//! Views stacked in the main area. Each view owns its state, declares what to
//! fetch, and receives the result; the app runs fetches off the render loop.

pub mod actions;
pub mod columns;
pub mod confirm;
pub mod connect;
pub mod contexts;
pub mod dashboard;
pub mod detail;
pub mod diff;
pub mod form;
pub mod lines;
pub mod list;
pub mod plugins;
pub mod summary;
pub mod task;
pub mod upload;

use std::time::Duration;

use chrono::{DateTime, Utc};
use crossterm::event::KeyEvent;
use futures::future::BoxFuture;
use pulply_api::{Error, Method, Page, PulpClient, Registry};
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
    Count(u64),
    Dashboard(Box<dashboard::DashboardData>),
    Task { task: Value, children: Vec<Value> },
    Reachability(Vec<(String, Result<String, String>)>),
    Diff(Box<diff::DiffData>),
    DiffDetail(Vec<diff::DiffLine>),
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
    Success,
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
    /// Mutations are disabled (`--readonly` or `readonly = true` on the context).
    pub fn readonly(&self) -> bool {
        self.client().is_none_or(PulpClient::readonly)
    }
}

/// Follow-up once the task spawned by a mutation completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AfterTask {
    None,
    /// Show the base URL of the distribution created or updated by the task.
    ShowDistribution,
}

/// A modifying request. Task responses are tracked; other results notify and refresh.
#[derive(Debug, Clone)]
pub struct Mutation {
    pub method: Method,
    pub path: String,
    pub body: Option<Value>,
    /// Human description used in notifications and the task bar (`sync iso`).
    pub label: String,
    pub after: AfterTask,
    /// View whose [`View::on_mutated`] receives the result (the form that submitted it).
    /// Explicit because the mutation may be applied from another layer (confirmation overlay).
    pub reply_to: Option<u64>,
}

impl Mutation {
    pub fn new(method: Method, path: impl Into<String>, body: Option<Value>, label: impl Into<String>) -> Self {
        Self { method, path: path.into(), body, label: label.into(), after: AfterTask::None, reply_to: None }
    }

    pub fn after(mut self, after: AfterTask) -> Self {
        self.after = after;
        self
    }

    pub fn reply_to(mut self, view: u64) -> Self {
        self.reply_to = Some(view);
        self
    }
}

pub enum Outcome {
    /// Key not handled: the app applies its global fallback.
    Ignored,
    None,
    Push(Box<dyn View>),
    Pop,
    /// Modal view drawn above the stack (confirmations, task progress).
    Overlay(Box<dyn View>),
    CloseOverlay,
    Fetch(FetchKind),
    Open(Target),
    SwitchContext(String),
    SubmitPassword(String),
    Retry,
    /// Fetch display names for these hrefs.
    Resolve(Vec<String>),
    Mutate(Mutation),
    /// Hand a value to the view with this id (picker result for a form field).
    Deliver {
        view: u64,
        key: String,
        value: Value,
    },
    /// Edit a resource as YAML in `$EDITOR`.
    EditExternal(String),
    /// Run an upload batch (progress overlay, content created from each file).
    Upload(upload::UploadJob),
    Notify(Level, String),
    Batch(Vec<Outcome>),
}

impl Outcome {
    /// Refusal shown when a mutation key is pressed in read-only mode.
    pub fn readonly() -> Self {
        Outcome::Notify(Level::Error, rust_i18n::t!("notice.readonly").into_owned())
    }
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

    /// Called once when the view gets its slot id (needed to receive deliveries).
    fn set_id(&mut self, _id: u64) {}

    /// Area used when shown as an overlay (default: 80% × 75% of the screen).
    fn overlay_size(&self, area: Rect) -> Rect {
        confirm::centered(area, area.width * 4 / 5, area.height * 3 / 4)
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome;

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx);

    fn fetch(&mut self, _kind: FetchKind, _ctx: &Ctx) -> Option<Fetch> {
        None
    }

    fn on_data(&mut self, _kind: FetchKind, _result: Result<Payload, Error>, _ctx: &Ctx) -> Outcome {
        Outcome::None
    }

    /// Result of a [`Mutation`] emitted with `reply`. Return `Ignored` to let the app
    /// notify as for any other mutation.
    fn on_mutated(&mut self, _result: &Result<Value, Error>) -> Outcome {
        Outcome::Ignored
    }

    /// Value delivered by [`Outcome::Deliver`].
    fn receive(&mut self, _key: &str, _value: Value) {}

    /// Hrefs of the marked rows, or of the row under the cursor.
    fn selection(&self) -> Vec<String> {
        Vec::new()
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
