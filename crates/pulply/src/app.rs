//! Application state, event loop, polling, navigation and mutations.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use color_eyre::eyre::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use futures::future::join_all;
use pulply_api::models::task_href;
use pulply_api::{Error, Registry, ResourceExt};
use ratatui::DefaultTerminal;
use rust_i18n::t;
use serde_json::Value;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::AbortHandle;

use crate::commands::{self, Target};
use crate::config::{Config, netrc};
use crate::custom::Plugin;
use crate::editor::{self, EditOutcome};
use crate::format;
use crate::names::NameCache;
use crate::session::{self, Session};
use crate::shell::PluginRun;
use crate::theme::Theme;
use crate::ui;
use crate::ui::sidebar::{self, Sidebar};
use crate::views::actions;
use crate::views::connect::{ConnectView, State as ConnectState};
use crate::views::contexts::ContextView;
use crate::views::dashboard::DashboardView;
use crate::views::list::ListView;
use crate::views::task::{TaskView, is_active};
use crate::views::upload::{FileProgress, Phase, UploadJob, UploadView, shared_progress};
use crate::views::{AfterTask, FetchKind, Hint, Level, Mutation, Outcome, Payload, View, error_text};

/// Builds a [`crate::views::Ctx`] from disjoint fields so a view can be borrowed mutably alongside.
macro_rules! ctx {
    ($app:expr) => {
        $crate::views::Ctx {
            session: $app.session.as_ref(),
            names: &$app.names,
            theme: &$app.theme,
            config: &$app.config,
            current_ctx: &$app.ctx_name,
            now: chrono::Utc::now(),
        }
    };
}
pub(crate) use ctx;

const TICK: Duration = Duration::from_millis(250);
const NOTICE_TTL: Duration = Duration::from_secs(8);
const TRACK_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Command,
    Filter,
}

pub struct Prompt {
    pub kind: PromptKind,
    pub input: String,
    history_pos: Option<usize>,
}

pub struct Notice {
    pub level: Level,
    pub text: String,
    at: Instant,
}

pub struct Slot {
    pub id: u64,
    pub view: Box<dyn View>,
    last_poll: Option<Instant>,
    inflight: HashSet<FetchKind>,
    tasks: Vec<AbortHandle>,
}

impl Slot {
    fn abort(&mut self) {
        for t in self.tasks.drain(..) {
            t.abort();
        }
    }

    fn due(&self, refresh: Duration) -> bool {
        self.inflight.is_empty()
            && self.view.poll_interval(refresh).is_some_and(|iv| self.last_poll.is_none_or(|t| t.elapsed() >= iv))
    }
}

/// A task spawned by one of our mutations, polled until it finishes.
pub struct Tracked {
    pub href: String,
    pub label: String,
    pub state: String,
    pub progress: Option<(u64, u64)>,
    after: AfterTask,
    /// Path the mutation targeted (an updated distribution, for instance).
    subject: String,
}

enum Msg {
    Connected { generation: u64, result: Result<Box<session::Connected>, Error> },
    Registry { generation: u64, result: Result<Registry, Error> },
    Data { generation: u64, view: u64, kind: FetchKind, result: Result<Payload, Error> },
    Names { generation: u64, entries: Vec<(String, String)> },
    TaskCounts { generation: u64, running: u64, waiting: u64 },
    Mutated { generation: u64, mutation: Mutation, result: Result<Value, Error> },
    Tracked { generation: u64, results: Vec<(String, Result<Value, Error>)> },
    Notice { generation: u64, level: Level, text: String },
    Perms { generation: u64, href: String, perms: HashSet<String> },
}

pub struct Options {
    pub context: String,
    pub readonly: bool,
    pub refresh: Option<u64>,
    pub start: String,
}

pub struct App {
    pub config: Config,
    opts: Options,
    pub theme: Theme,
    pub names: NameCache,
    pub session: Option<Session>,
    pub ctx_name: String,
    pub stack: Vec<Slot>,
    pub overlay: Option<Slot>,
    forward: Vec<Box<dyn View>>,
    pub sidebar: Sidebar,
    pub sidebar_visible: bool,
    pub sidebar_focus: bool,
    pub active_target: Option<Target>,
    pub prompt: Option<Prompt>,
    history: Vec<String>,
    pub completions: Vec<String>,
    pub help: bool,
    pub paused: bool,
    pub notice: Option<Notice>,
    pub task_counts: Option<(u64, u64)>,
    last_counts: Option<Instant>,
    counts_inflight: bool,
    pub tracked: Vec<Tracked>,
    last_track: Option<Instant>,
    track_inflight: bool,
    pending_command: Option<String>,
    /// Resource to open in `$EDITOR` once the current event is handled.
    pending_edit: Option<String>,
    /// Foreground shell plugin to run once the current event is handled.
    pending_plugin: Option<PluginRun>,
    netrc: Vec<netrc::Entry>,
    password: Option<String>,
    tx: UnboundedSender<Msg>,
    rx: UnboundedReceiver<Msg>,
    generation: u64,
    next_id: u64,
    background: Vec<AbortHandle>,
    quit: bool,
}

/// Slot with this id, in the stack or the overlay.
fn find_slot<'a>(stack: &'a mut [Slot], overlay: &'a mut Option<Slot>, id: u64) -> Option<&'a mut Slot> {
    match overlay {
        Some(o) if o.id == id => Some(o),
        _ => stack.iter_mut().find(|s| s.id == id),
    }
}

impl App {
    pub fn new(config: Config, opts: Options) -> Self {
        let (tx, rx) = unbounded_channel();
        let netrc = netrc::default_path().map(|p| netrc::load(&p)).unwrap_or_default();
        let ctx_name = opts.context.clone();
        let mut app = Self {
            config,
            opts,
            theme: Theme::default(),
            names: NameCache::default(),
            session: None,
            ctx_name,
            stack: Vec::new(),
            overlay: None,
            forward: Vec::new(),
            sidebar: Sidebar::build(None, None),
            sidebar_visible: true,
            sidebar_focus: false,
            active_target: Some(Target::Dashboard),
            prompt: None,
            history: Vec::new(),
            completions: Vec::new(),
            help: false,
            paused: false,
            notice: None,
            task_counts: None,
            last_counts: None,
            counts_inflight: false,
            tracked: Vec::new(),
            last_track: None,
            track_inflight: false,
            pending_command: None,
            pending_edit: None,
            pending_plugin: None,
            netrc,
            password: None,
            tx,
            rx,
            generation: 0,
            next_id: 0,
            background: Vec::new(),
            quit: false,
        };
        app.rebuild_completions();
        app
    }

    pub async fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let mut events = EventStream::new();
        let mut tick = tokio::time::interval(TICK);
        self.start_connect(self.ctx_name.clone());
        while !self.quit {
            terminal.draw(|f| ui::draw(f, &mut self))?;
            tokio::select! {
                ev = events.next() => match ev {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => self.on_key(key),
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e.into()),
                    None => break,
                },
                Some(msg) = self.rx.recv() => self.on_msg(msg),
                _ = tick.tick() => self.on_tick(),
            }
            if let Some(href) = self.pending_edit.take() {
                // The event reader would steal the editor's keystrokes: stop it meanwhile.
                drop(events);
                self.run_editor(terminal, href).await;
                events = EventStream::new();
            }
            if let Some(run) = self.pending_plugin.take() {
                drop(events);
                let result = run.run_foreground(terminal).await;
                events = EventStream::new();
                if let Err(e) = result {
                    self.notify(Level::Error, e);
                }
            }
        }
        Ok(())
    }

    async fn run_editor(&mut self, terminal: &mut DefaultTerminal, href: String) {
        let Some(s) = &self.session else { return };
        if s.client.readonly() {
            self.notify(Level::Error, t!("notice.readonly").into_owned());
            return;
        }
        let writable: Vec<String> = s
            .registry
            .as_deref()
            .and_then(|r| {
                let t = r.type_for_href(&href)?;
                r.schema(t.update_schema.as_deref()?)
            })
            .map(|schema| schema.properties.iter().filter(|(_, p)| !p.read_only).map(|(k, _)| k.clone()).collect())
            .unwrap_or_default();
        if writable.is_empty() {
            self.notify(Level::Error, t!("editor.not_editable").into_owned());
            return;
        }
        let client = s.client.clone();
        let name = format::href(&href, &self.names);
        match editor::edit(terminal, &client, &href, &writable).await {
            Ok(EditOutcome::Applied(body)) => {
                let label = t!("action.update", name = name).into_owned();
                let mutation = Mutation::new(pulply_api::Method::PATCH, href, None, label);
                self.on_mutated(mutation, Ok(body));
            }
            Ok(EditOutcome::Unchanged) => self.notify(Level::Info, t!("form.no_changes").into_owned()),
            Ok(EditOutcome::Cancelled) => self.notify(Level::Info, t!("editor.cancelled").into_owned()),
            Err(e) => self.notify(Level::Error, e),
        }
    }

    // ------------------------------------------------------------ sessions

    fn refresh_interval(&self) -> Duration {
        self.session.as_ref().map(|s| s.refresh).unwrap_or(Duration::from_secs(self.config.refresh))
    }

    fn spawn(&mut self, fut: impl Future<Output = Msg> + Send + 'static) {
        let tx = self.tx.clone();
        let handle = tokio::spawn(async move {
            let _ = tx.send(fut.await);
        });
        self.background.retain(|h| !h.is_finished());
        self.background.push(handle.abort_handle());
    }

    fn start_connect(&mut self, name: String) {
        let Some(ctx) = self.config.context(&name).cloned() else {
            self.notify(Level::Error, t!("notice.unknown_context", name = name).into_owned());
            return;
        };
        if name != self.ctx_name {
            self.password = None;
        }
        self.generation += 1;
        for h in self.background.drain(..) {
            h.abort();
        }
        self.session = None;
        self.names.clear();
        self.task_counts = None;
        self.tracked.clear();
        self.track_inflight = false;
        self.close_overlay();
        self.ctx_name = name.clone();
        self.sidebar = Sidebar::build(None, Some(&self.sidebar));
        self.rebuild_completions();
        let skin = ctx.cfg.skin.as_deref().unwrap_or(&self.config.skin);
        match Theme::load(skin, self.config.dir.as_deref()) {
            Ok(theme) => self.theme = theme,
            Err(e) => {
                self.theme = Theme::default();
                self.notify(Level::Error, e);
            }
        }
        let url = ctx.cfg.base_url.clone();
        let creds = ctx.cfg.credentials(&self.netrc).map(|mut c| {
            if c.password.is_none() {
                c.password = self.password.clone();
            }
            c
        });
        if creds.as_ref().is_some_and(|c| c.password.is_none()) {
            let state = ConnectState::Password { input: String::new(), error: None };
            self.set_root(Box::new(ConnectView::new(name, url, state)));
            return;
        }
        self.set_root(Box::new(ConnectView::new(name, url, ConnectState::Connecting)));
        let generation = self.generation;
        let readonly = self.opts.readonly;
        self.spawn(async move {
            let result = session::connect(&ctx.cfg, creds, readonly).await.map(Box::new);
            Msg::Connected { generation, result }
        });
    }

    fn load_registry(&mut self, force: bool) {
        let Some(s) = &self.session else { return };
        let (client, status, generation) = (s.client.clone(), s.status.clone(), self.generation);
        self.spawn(async move {
            let result = session::load_registry(client, status, force).await;
            Msg::Registry { generation, result }
        });
    }

    fn rebuild_completions(&mut self) {
        let names: Vec<String> = self.config.contexts.iter().map(|c| c.name.clone()).collect();
        let reg = self.session.as_ref().and_then(|s| s.registry.as_deref());
        self.completions = commands::completions(reg, &names);
        self.completions.extend(self.config.custom.aliases.keys().cloned());
    }

    // ------------------------------------------------------------ messages

    fn on_msg(&mut self, msg: Msg) {
        match msg {
            Msg::Connected { generation, result } if generation == self.generation => self.on_connected(result),
            Msg::Registry { generation, result } if generation == self.generation => match result {
                Ok(reg) => {
                    if let Some(s) = &mut self.session {
                        s.registry = Some(Arc::new(reg));
                    }
                    let reg = self.session.as_ref().and_then(|s| s.registry.clone());
                    self.sidebar = Sidebar::build(reg.as_deref(), Some(&self.sidebar));
                    self.rebuild_completions();
                    match self.pending_command.take() {
                        Some(cmd) => self.run_command(&cmd),
                        None => self.fetch_top(),
                    }
                }
                Err(e) => self.notify(Level::Error, t!("notice.schema_failed", error = e.to_string()).into_owned()),
            },
            Msg::Data { generation, view, kind, result } if generation == self.generation => {
                match &result {
                    Ok(Payload::Page(page)) => page.results.iter().for_each(|r| self.names.learn(r)),
                    Ok(Payload::Object(obj)) => self.names.learn(obj),
                    _ => {}
                }
                let ctx = ctx!(self);
                let Some(slot) = find_slot(&mut self.stack, &mut self.overlay, view) else { return };
                slot.inflight.remove(&kind);
                if kind == FetchKind::Refresh {
                    slot.last_poll = Some(Instant::now());
                }
                let outcome = slot.view.on_data(kind, result, &ctx);
                self.apply(outcome, view);
            }
            Msg::Names { generation, entries } if generation == self.generation => {
                for (href, name) in entries {
                    self.names.insert(href, name);
                }
            }
            Msg::TaskCounts { generation, running, waiting } if generation == self.generation => {
                self.counts_inflight = false;
                self.task_counts = Some((running, waiting));
            }
            Msg::Mutated { generation, mutation, result } if generation == self.generation => {
                self.on_mutated(mutation, result);
            }
            Msg::Tracked { generation, results } if generation == self.generation => self.on_tracked(results),
            Msg::Notice { generation, level, text } if generation == self.generation => self.notify(level, text),
            Msg::Perms { generation, href, perms } if generation == self.generation => {
                if let Some(s) = &mut self.session {
                    s.perms.record(href, perms);
                }
            }
            _ => {} // stale message from a previous context
        }
    }

    /// `my_permissions/` of `href` (object-level permissions), once per resource.
    fn load_object_perms(&mut self, href: String) {
        let Some(s) = &mut self.session else { return };
        if !s.perms.needs(&href) {
            return;
        }
        let supported =
            s.registry.as_deref().and_then(|r| r.type_for_href(&href)).is_some_and(|t| t.has_action("my_permissions"));
        if !supported {
            s.perms.record(href, HashSet::new());
            return;
        }
        s.perms.mark_pending(&href);
        let (client, generation) = (s.client.clone(), self.generation);
        self.spawn(async move {
            let perms = client
                .get(&format!("{href}my_permissions/"), &[])
                .await
                .ok()
                .and_then(|v| v.get("permissions").and_then(Value::as_array).cloned())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|p| p.as_str().map(str::to_owned))
                .collect();
            Msg::Perms { generation, href, perms }
        });
    }

    fn on_connected(&mut self, result: Result<Box<session::Connected>, Error>) {
        let Some(ctx) = self.config.context(&self.ctx_name).cloned() else { return };
        let url = ctx.cfg.base_url.clone();
        match result {
            Ok(connected) => {
                let session::Connected { client, status, perms, ignored_domain } = *connected;
                if let Some(domain) = ignored_domain {
                    self.notify(Level::Info, t!("notice.domain_ignored", domain = domain).into_owned());
                }
                if !status.supported() {
                    let v = status.core_version().map(|v| v.to_string()).unwrap_or_else(|| "?".into());
                    self.notify(Level::Error, t!("notice.unsupported", version = v).into_owned());
                }
                let refresh = ctx.cfg.refresh(self.opts.refresh.unwrap_or(self.config.refresh));
                self.session = Some(Session { client, status, registry: None, refresh, perms });
                self.set_root(Box::new(DashboardView::new()));
                self.active_target = Some(Target::Dashboard);
                let start = self.opts.start.trim();
                self.pending_command = (!start.is_empty() && commands::parse(start, None) != Ok(Target::Dashboard))
                    .then(|| start.to_owned());
                self.load_registry(false);
            }
            Err(e @ (Error::Unauthorized { .. } | Error::Forbidden))
                if ctx.cfg.username.is_some() || self.password.is_some() =>
            {
                self.password = None;
                let state = ConnectState::Password { input: String::new(), error: Some(e.to_string()) };
                self.set_root(Box::new(ConnectView::new(self.ctx_name.clone(), url, state)));
            }
            Err(e) => {
                self.set_root(Box::new(ConnectView::new(
                    self.ctx_name.clone(),
                    url,
                    ConnectState::Failed(e.to_string()),
                )));
            }
        }
    }

    // ------------------------------------------------------------ mutations

    fn mutate(&mut self, mutation: Mutation) {
        let Some(s) = &self.session else { return };
        if s.client.readonly() {
            self.notify(Level::Error, t!("notice.readonly").into_owned());
            return;
        }
        let (client, generation) = (s.client.clone(), self.generation);
        tracing::info!(method = %mutation.method, path = mutation.path, "mutation");
        self.spawn(async move {
            let result = client.mutate(mutation.method.clone(), &mutation.path, mutation.body.as_ref()).await;
            Msg::Mutated { generation, mutation, result }
        });
    }

    fn on_mutated(&mut self, mutation: Mutation, result: Result<Value, Error>) {
        let mut handled = false;
        if let Some(view) = mutation.reply_to
            && let Some(slot) = find_slot(&mut self.stack, &mut self.overlay, view)
        {
            let outcome = slot.view.on_mutated(&result);
            handled = !matches!(outcome, Outcome::Ignored);
            self.apply(outcome, view);
        }
        match result {
            Ok(body) => match task_href(&body) {
                Some(task) => {
                    self.tracked.push(Tracked {
                        href: task.to_owned(),
                        label: mutation.label.clone(),
                        state: "waiting".into(),
                        progress: None,
                        after: mutation.after.clone(),
                        subject: mutation.path.clone(),
                    });
                    self.last_track = None;
                    if self.overlay.is_none() {
                        self.open_overlay(Box::new(TaskView::new(task.to_owned())));
                    }
                }
                None => {
                    self.notify(Level::Success, t!("notice.done", label = mutation.label).into_owned());
                    self.refresh_visible();
                }
            },
            Err(_) if handled => {}
            Err(e) => self.notify(Level::Error, format!("{}: {}", mutation.label, error_text(&e))),
        }
    }

    fn poll_tracked(&mut self) {
        let Some(s) = &self.session else { return };
        let hrefs: Vec<String> = self.tracked.iter().map(|t| t.href.clone()).collect();
        if hrefs.is_empty() {
            return;
        }
        self.track_inflight = true;
        self.last_track = Some(Instant::now());
        let (client, generation) = (s.client.clone(), self.generation);
        self.spawn(async move {
            let query = [("fields".to_owned(), "pulp_href,state,progress_reports,error,created_resources".to_owned())];
            let results = join_all(hrefs.iter().map(|h| client.get(h, &query))).await;
            Msg::Tracked { generation, results: hrefs.into_iter().zip(results).collect() }
        });
    }

    fn on_tracked(&mut self, results: Vec<(String, Result<Value, Error>)>) {
        self.track_inflight = false;
        let mut finished = Vec::new();
        for (href, result) in results {
            let Some(pos) = self.tracked.iter().position(|t| t.href == href) else { continue };
            let task = match result {
                Ok(task) => task,
                Err(e) => {
                    let t = self.tracked.remove(pos);
                    self.notify(Level::Error, format!("{}: {}", t.label, error_text(&e)));
                    continue;
                }
            };
            let tracked = &mut self.tracked[pos];
            tracked.state = task.str_field("state").unwrap_or_default().to_owned();
            tracked.progress = task
                .get("progress_reports")
                .and_then(Value::as_array)
                .and_then(|reports| {
                    reports.iter().find(|r| r.str_field("state") == Some("running")).or_else(|| reports.first())
                })
                .and_then(|r| Some((r.get("done")?.as_u64()?, r.get("total")?.as_u64()?)));
            if !is_active(&tracked.state) {
                finished.push((self.tracked.remove(pos), task));
            }
        }
        for (tracked, task) in finished {
            self.on_task_finished(tracked, &task);
        }
    }

    fn on_task_finished(&mut self, tracked: Tracked, task: &Value) {
        match tracked.state.as_str() {
            "completed" => {
                self.notify(Level::Success, t!("notice.task_completed", label = tracked.label).into_owned());
                if tracked.after == AfterTask::ShowDistribution {
                    let created = task.get("created_resources").and_then(Value::as_array).into_iter().flatten();
                    let dist = created
                        .filter_map(Value::as_str)
                        .find(|h| h.contains("/distributions/"))
                        .map(str::to_owned)
                        .or_else(|| tracked.subject.contains("/distributions/").then(|| tracked.subject.clone()));
                    if let (Some(dist), Some(s)) = (dist, &self.session) {
                        let (client, generation) = (s.client.clone(), self.generation);
                        self.spawn(async move {
                            let (level, text) = match client.get(&dist, &[]).await {
                                Ok(d) => {
                                    let url = d.str_field("base_url").unwrap_or_default();
                                    (Level::Success, t!("notice.distribution_url", url = url).into_owned())
                                }
                                Err(e) => (Level::Error, e.to_string()),
                            };
                            Msg::Notice { generation, level, text }
                        });
                    }
                }
            }
            "failed" => {
                let reason = task.get("error").and_then(|e| e.str_field("description")).unwrap_or_default();
                self.notify(Level::Error, t!("notice.task_failed", label = tracked.label, error = reason).into_owned());
            }
            state => self.notify(Level::Info, format!("{}: {state}", tracked.label)),
        }
        self.refresh_visible();
    }

    // ------------------------------------------------------------ polling

    fn on_tick(&mut self) {
        if self.notice.as_ref().is_some_and(|n| n.at.elapsed() > NOTICE_TTL) {
            self.notice = None;
        }
        if !self.track_inflight
            && !self.tracked.is_empty()
            && self.last_track.is_none_or(|t| t.elapsed() >= TRACK_INTERVAL)
        {
            self.poll_tracked();
        }
        if self.paused {
            return;
        }
        let refresh = self.refresh_interval();
        if let Some(id) = self.stack.last().filter(|s| s.due(refresh)).map(|s| s.id) {
            self.fetch_view(id, FetchKind::Refresh);
        }
        if let Some(id) = self.overlay.as_ref().filter(|s| s.due(refresh)).map(|s| s.id) {
            self.fetch_view(id, FetchKind::Refresh);
        }
        if let Some(s) = &self.session
            && !self.counts_inflight
            && self.last_counts.is_none_or(|t| t.elapsed() >= refresh)
        {
            self.counts_inflight = true;
            self.last_counts = Some(Instant::now());
            let (client, generation) = (s.client.clone(), self.generation);
            self.spawn(async move {
                let tasks = client.api_path("tasks/");
                let (q_running, q_waiting) = (
                    vec![("state".to_owned(), "running".to_owned())],
                    vec![("state".to_owned(), "waiting".to_owned())],
                );
                let (running, waiting) =
                    futures::join!(client.count(&tasks, &q_running), client.count(&tasks, &q_waiting));
                Msg::TaskCounts { generation, running: running.unwrap_or(0), waiting: waiting.unwrap_or(0) }
            });
        }
    }

    fn fetch_top(&mut self) {
        if let Some(id) = self.stack.last().map(|s| s.id) {
            self.fetch_view(id, FetchKind::Refresh);
        }
    }

    /// Refreshes what is on screen after a change (top view and overlay).
    fn refresh_visible(&mut self) {
        self.fetch_top();
        if let Some(id) = self.overlay.as_ref().map(|s| s.id) {
            self.fetch_view(id, FetchKind::Refresh);
        }
    }

    fn fetch_view(&mut self, id: u64, kind: FetchKind) {
        let ctx = ctx!(self);
        let Some(slot) = find_slot(&mut self.stack, &mut self.overlay, id) else { return };
        if slot.inflight.contains(&kind) {
            return;
        }
        let Some(fut) = slot.view.fetch(kind, &ctx) else {
            slot.last_poll = Some(Instant::now());
            return;
        };
        slot.inflight.insert(kind);
        let (tx, generation) = (self.tx.clone(), self.generation);
        let handle = tokio::spawn(async move {
            let result = fut.await;
            let _ = tx.send(Msg::Data { generation, view: id, kind, result });
        });
        slot.tasks.retain(|h| !h.is_finished());
        slot.tasks.push(handle.abort_handle());
    }

    /// Fetches display names: hrefs one GET each, PRNs grouped per type with `?prn__in=`.
    fn resolve(&mut self, keys: Vec<String>) {
        let Some(s) = &self.session else { return };
        let (client, generation) = (s.client.clone(), self.generation);
        let (prns, hrefs): (Vec<String>, Vec<String>) = keys.into_iter().partition(|k| k.starts_with("prn:"));
        let mut by_type: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
        if let Some(reg) = s.registry.as_deref() {
            for prn in prns {
                if let Some(path) = reg.type_for_prn(&prn).and_then(|t| t.list_path.clone()) {
                    by_type.entry(path).or_default().push(prn);
                }
            }
        }
        self.spawn(async move {
            let fields = ("fields".to_owned(), format!("pulp_href,prn,{}", crate::names::LABEL_FIELDS.join(",")));
            let single = [fields.clone()];
            let (objects, pages) = futures::join!(
                join_all(hrefs.iter().map(|h| client.get(h, &single))),
                join_all(by_type.iter().map(|(path, prns)| {
                    let query = vec![
                        fields.clone(),
                        ("prn__in".to_owned(), prns.join(",")),
                        ("limit".to_owned(), prns.len().to_string()),
                    ];
                    let client = client.clone();
                    async move { client.list(path, &query).await }
                })),
            );
            let mut entries = Vec::new();
            let items = objects.into_iter().flatten().chain(pages.into_iter().flatten().flat_map(|p| p.results));
            for v in items {
                let Some(name) = NameCache::label(&v) else { continue };
                for key in [v.href(), v.str_field("prn")].into_iter().flatten() {
                    entries.push((key.to_owned(), name.to_owned()));
                }
            }
            Msg::Names { generation, entries }
        });
    }

    // ------------------------------------------------------------ navigation

    fn new_slot(&mut self, mut view: Box<dyn View>) -> Slot {
        self.next_id += 1;
        view.set_id(self.next_id);
        Slot { id: self.next_id, view, last_poll: None, inflight: HashSet::new(), tasks: Vec::new() }
    }

    fn set_root(&mut self, view: Box<dyn View>) {
        for mut s in self.stack.drain(..) {
            s.abort();
        }
        self.forward.clear();
        let slot = self.new_slot(view);
        self.stack.push(slot);
        self.fetch_top();
    }

    fn push(&mut self, view: Box<dyn View>) {
        self.forward.clear();
        self.push_keep_forward(view);
    }

    fn push_keep_forward(&mut self, view: Box<dyn View>) {
        let slot = self.new_slot(view);
        self.stack.push(slot);
        self.fetch_top();
    }

    fn pop(&mut self) {
        if self.stack.len() <= 1 {
            return;
        }
        if let Some(mut slot) = self.stack.pop() {
            slot.abort();
            self.forward.push(slot.view);
        }
        self.fetch_top();
    }

    fn open_overlay(&mut self, view: Box<dyn View>) {
        self.close_overlay();
        let slot = self.new_slot(view);
        let id = slot.id;
        self.overlay = Some(slot);
        self.fetch_view(id, FetchKind::Refresh);
    }

    fn close_overlay(&mut self) {
        if let Some(mut slot) = self.overlay.take() {
            slot.abort();
        }
    }

    /// Uploads each file in chunks, then creates its content unit (task tracked like any
    /// mutation). Progress is shared with the overlay; the job keeps running when hidden.
    fn start_upload(&mut self, job: UploadJob) {
        let Some(s) = &self.session else { return };
        if s.client.readonly() {
            self.notify(Level::Error, t!("notice.readonly").into_owned());
            return;
        }
        let progress = shared_progress(&job.files);
        let (client, generation, tx) = (s.client.clone(), self.generation, self.tx.clone());
        let chunk = self.config.upload_chunk_size;
        let shared = progress.clone();
        let handle = tokio::spawn(async move {
            let set = |i: usize, f: &dyn Fn(&mut FileProgress)| {
                if let Some(p) = shared.lock().expect("upload progress poisoned").get_mut(i) {
                    f(p);
                }
            };
            for (i, path) in job.files.iter().enumerate() {
                set(i, &|p| {
                    p.phase = Phase::Uploading;
                    p.started = Some(Instant::now());
                });
                let uploaded = client
                    .upload_file(path, chunk, |sent, total| {
                        set(i, &|p| {
                            p.sent = sent;
                            p.total = total;
                        })
                    })
                    .await;
                let result = match uploaded {
                    Ok(up) => {
                        set(i, &|p| p.phase = Phase::Creating);
                        client.post_form(&job.create_path, &job.create_fields(path, &up.href)).await
                    }
                    Err(e) => Err(e),
                };
                let phase = match &result {
                    Ok(_) => Phase::Done,
                    Err(e) => Phase::Failed(e.to_string()),
                };
                set(i, &|p| {
                    p.phase = phase.clone();
                    p.finished = Some(Instant::now());
                });
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let label = t!("action.upload", name = name).into_owned();
                let mutation = Mutation::new(pulply_api::Method::POST, job.create_path.clone(), None, label);
                let _ = tx.send(Msg::Mutated { generation, mutation, result });
            }
        });
        self.background.push(handle.abort_handle());
        self.open_overlay(Box::new(UploadView::new(progress)));
    }

    fn open(&mut self, target: Target) {
        match target {
            Target::Dashboard => {
                if self.session.is_some() {
                    self.set_root(Box::new(DashboardView::new()));
                    self.active_target = Some(Target::Dashboard);
                }
            }
            Target::Contexts => {
                let view = ContextView::new(&ctx!(self));
                self.push(Box::new(view));
            }
            Target::Context(name) => self.start_connect(name),
            Target::Help => self.help = true,
            Target::Me => {
                if self.session.is_some() {
                    self.push(Box::new(crate::views::admin::MeView::new()));
                }
            }
            Target::Quit => self.quit = true,
            Target::Operation(op) => {
                let Some(s) = &self.session else { return };
                if s.client.readonly() {
                    self.notify(Level::Error, t!("notice.readonly").into_owned());
                    return;
                }
                let Some(reg) = s.registry.clone() else {
                    self.notify(Level::Info, t!("notice.schema_loading").into_owned());
                    return;
                };
                let selection = self.stack.last().map(|slot| slot.view.selection()).unwrap_or_default();
                match actions::operation(op, &reg, &s.client, selection) {
                    Some(form) => self.push(Box::new(form)),
                    None => self.notify(Level::Error, t!("notice.unknown_resource", name = op.rel_path()).into_owned()),
                }
            }
            Target::RefreshSchema => {
                self.notify(Level::Info, t!("notice.schema_reload").into_owned());
                self.load_registry(true);
            }
            Target::Resource(id) => {
                let Some(reg) = self.session.as_ref().and_then(|s| s.registry.clone()) else {
                    self.notify(Level::Info, t!("notice.schema_loading").into_owned());
                    return;
                };
                let Some(t) = reg.get(&id) else {
                    self.notify(Level::Error, t!("notice.unknown_resource", name = id).into_owned());
                    return;
                };
                self.set_root(Box::new(ListView::for_type(t, &reg, None)));
                self.active_target = Some(Target::Resource(id));
            }
        }
    }

    fn run_command(&mut self, input: &str) {
        let input = input.trim();
        if input.is_empty() {
            return;
        }
        let reg = self.session.as_ref().and_then(|s| s.registry.clone());
        let expanded = self.config.custom.expand(input);
        match commands::parse(&expanded, reg.as_deref()) {
            Ok(target) => {
                self.history.retain(|h| h != input);
                self.history.push(input.to_owned());
                self.open(target);
            }
            Err(msg) => self.notify(Level::Error, msg),
        }
    }

    fn notify(&mut self, level: Level, text: String) {
        match level {
            Level::Error => tracing::warn!("{text}"),
            _ => tracing::info!("{text}"),
        }
        self.notice = Some(Notice { level, text, at: Instant::now() });
    }

    /// Plugins applicable to the resource selected in the top view (no overlay open).
    fn scoped_plugins(&self) -> Vec<(&Plugin, &Value)> {
        let (Some(top), None, Some(reg)) =
            (self.stack.last(), &self.overlay, self.session.as_ref().and_then(|s| s.registry.as_deref()))
        else {
            return Vec::new();
        };
        let Some(item) = top.view.selected_item() else { return Vec::new() };
        let Some(t) = item.href().and_then(|h| reg.type_for_href(h)) else { return Vec::new() };
        self.config
            .custom
            .plugins
            .iter()
            .filter(|p| p.in_scope(&t.id, &t.category, &t.plugin))
            .map(|p| (p, item))
            .collect()
    }

    pub fn plugin_hints(&self) -> Vec<Hint> {
        self.scoped_plugins().into_iter().map(|(p, _)| Hint::new(p.key_label.clone(), p.description.clone())).collect()
    }

    fn plugin_for(&self, key: &KeyEvent) -> Option<PluginRun> {
        let (plugin, item) = self.scoped_plugins().into_iter().find(|(p, _)| p.key.matches(key))?;
        let session = self.session.as_ref()?;
        let href = item.href().unwrap_or_default();
        let reg = session.registry.as_deref();
        let name = NameCache::label(item).map(str::to_owned).unwrap_or_else(|| format::href(href, &self.names));
        let base_url = self.config.context(&self.ctx_name).map(|c| c.cfg.base_url.clone()).unwrap_or_default();
        let vars = HashMap::from([
            ("HREF", href.to_owned()),
            ("NAME", name),
            ("TYPE", reg.and_then(|r| r.type_for_href(href)).map(|t| t.id.clone()).unwrap_or_default()),
            ("PRN", item.str_field("prn").unwrap_or_default().to_owned()),
            ("BASE_URL", base_url),
            ("API_URL", session.client.url(href)),
            ("CONTEXT", self.ctx_name.clone()),
            ("DIST_URL", item.str_field("base_url").unwrap_or_default().to_owned()),
            ("USER", session.perms.username.clone().unwrap_or_default()),
        ]);
        Some(PluginRun::new(plugin, &vars))
    }

    fn start_plugin(&mut self, run: PluginRun) {
        if !run.background {
            self.pending_plugin = Some(run);
            return;
        }
        self.notify(Level::Info, t!("plugin.started", name = run.name).into_owned());
        let generation = self.generation;
        tokio::spawn({
            let tx = self.tx.clone();
            async move {
                if let Err(text) = run.run_background().await {
                    let _ = tx.send(Msg::Notice { generation, level: Level::Error, text });
                }
            }
        });
    }

    fn apply(&mut self, outcome: Outcome, view_id: u64) {
        match outcome {
            Outcome::Ignored | Outcome::None => {}
            Outcome::Push(v) => self.push(v),
            Outcome::Pop => self.pop(),
            Outcome::Overlay(v) => self.open_overlay(v),
            Outcome::CloseOverlay => self.close_overlay(),
            Outcome::Fetch(kind) => self.fetch_view(view_id, kind),
            Outcome::Open(target) => self.open(target),
            Outcome::SwitchContext(name) => self.start_connect(name),
            Outcome::SubmitPassword(pw) => {
                self.password = Some(pw);
                self.start_connect(self.ctx_name.clone());
            }
            Outcome::Retry => self.start_connect(self.ctx_name.clone()),
            Outcome::Resolve(hrefs) => self.resolve(hrefs),
            Outcome::Permissions(href) => self.load_object_perms(href),
            Outcome::Mutate(m) => self.mutate(m),
            Outcome::Deliver { view, key, value } => {
                if let Some(slot) = find_slot(&mut self.stack, &mut self.overlay, view) {
                    slot.view.receive(&key, value);
                }
            }
            Outcome::EditExternal(href) => self.pending_edit = Some(href),
            Outcome::Upload(job) => self.start_upload(job),
            Outcome::Notify(level, text) => self.notify(level, text),
            Outcome::Batch(all) => {
                for o in all {
                    self.apply(o, view_id);
                }
            }
        }
    }

    // ------------------------------------------------------------ keys

    fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.help {
            self.help = false;
            return;
        }
        if self.prompt.is_some() {
            self.prompt_key(key);
            return;
        }
        if self.overlay.is_some() {
            self.overlay_key(key);
            return;
        }
        let Some(top) = self.stack.last() else { return };
        let top_id = top.id;
        if top.view.captures_input() {
            let ctx = ctx!(self);
            let outcome = self.stack.last_mut().map(|s| s.view.handle_key(key, &ctx)).unwrap_or(Outcome::None);
            self.apply(outcome, top_id);
            return;
        }
        match key.code {
            KeyCode::Char(':') => {
                self.prompt = Some(Prompt { kind: PromptKind::Command, input: String::new(), history_pos: None });
                return;
            }
            KeyCode::Char('/') if top.view.filterable() => {
                let input = top.view.filter().to_owned();
                self.prompt = Some(Prompt { kind: PromptKind::Filter, input, history_pos: None });
                return;
            }
            KeyCode::Char('?') => {
                self.help = true;
                return;
            }
            KeyCode::Char('b') if ctrl => {
                self.sidebar_visible = !self.sidebar_visible;
                self.sidebar_focus &= self.sidebar_visible;
                return;
            }
            KeyCode::Char('r') if ctrl => {
                self.fetch_top();
                return;
            }
            KeyCode::Tab if self.sidebar_visible => {
                self.sidebar_focus = !self.sidebar_focus;
                return;
            }
            _ => {}
        }
        // User hotkeys (hotkeys.toml) run a `:` command and take precedence over view keys.
        if let Some(cmd) = self.config.custom.hotkey(&key).map(str::to_owned) {
            self.run_command(&cmd);
            return;
        }
        if let Some(run) = self.plugin_for(&key) {
            self.start_plugin(run);
            return;
        }
        if self.sidebar_focus {
            match self.sidebar.handle_key(key) {
                sidebar::Action::Open(target) => {
                    self.sidebar_focus = false;
                    self.open(target);
                    return;
                }
                sidebar::Action::None => return,
                sidebar::Action::Ignored => {}
            }
        }
        let ctx = ctx!(self);
        let outcome = self.stack.last_mut().map(|s| s.view.handle_key(key, &ctx)).unwrap_or(Outcome::None);
        if !matches!(outcome, Outcome::Ignored) {
            self.apply(outcome, top_id);
            return;
        }
        match key.code {
            KeyCode::Char('p') => {
                self.paused = !self.paused;
                let text = if self.paused { t!("notice.paused") } else { t!("notice.resumed") };
                self.notify(Level::Info, text.into_owned());
            }
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc | KeyCode::Char('[') => self.pop(),
            KeyCode::Char(']') => {
                if let Some(view) = self.forward.pop() {
                    self.push_keep_forward(view);
                }
            }
            _ => {}
        }
    }

    /// Keys go to the overlay; Esc/q close it (a task modal keeps being tracked in the
    /// status bar). Views it opens replace it on the main stack.
    fn overlay_key(&mut self, key: KeyEvent) {
        let ctx = ctx!(self);
        let Some(slot) = self.overlay.as_mut() else { return };
        let id = slot.id;
        match slot.view.handle_key(key, &ctx) {
            Outcome::Ignored if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) => self.close_overlay(),
            Outcome::Push(view) => {
                self.close_overlay();
                self.push(view);
            }
            outcome => self.apply(outcome, id),
        }
    }

    fn prompt_key(&mut self, key: KeyEvent) {
        let Some(prompt) = self.prompt.as_mut() else { return };
        let kind = prompt.kind;
        match key.code {
            KeyCode::Esc => {
                self.prompt = None;
                if kind == PromptKind::Filter
                    && let Some(slot) = self.stack.last_mut()
                {
                    // Drop uncommitted typing: re-apply the committed filter.
                    let committed = slot.view.filter().to_owned();
                    slot.view.set_filter(&committed, false);
                }
                return;
            }
            KeyCode::Enter => {
                let input = std::mem::take(&mut prompt.input);
                self.prompt = None;
                match kind {
                    PromptKind::Command => self.run_command(&input),
                    PromptKind::Filter => {
                        if let Some(slot) = self.stack.last_mut() {
                            let (id, outcome) = (slot.id, slot.view.set_filter(&input, true));
                            self.apply(outcome, id);
                        }
                    }
                }
                return;
            }
            KeyCode::Tab | KeyCode::Right if kind == PromptKind::Command => {
                if let Some(s) = commands::suggest(&prompt.input, &self.completions) {
                    prompt.input = s.to_owned();
                }
            }
            KeyCode::Up if kind == PromptKind::Command && !self.history.is_empty() => {
                let pos = prompt.history_pos.map_or(self.history.len() - 1, |p| p.saturating_sub(1));
                prompt.history_pos = Some(pos);
                prompt.input = self.history[pos].clone();
            }
            KeyCode::Down if kind == PromptKind::Command => {
                if let Some(p) = prompt.history_pos {
                    let next = p + 1;
                    prompt.history_pos = (next < self.history.len()).then_some(next);
                    prompt.input = self.history.get(next).cloned().unwrap_or_default();
                }
            }
            KeyCode::Backspace => {
                prompt.input.pop();
            }
            KeyCode::Char(c) => prompt.input.push(c),
            _ => return,
        }
        if kind == PromptKind::Filter {
            let input = self.prompt.as_ref().map(|p| p.input.clone()).unwrap_or_default();
            if let Some(slot) = self.stack.last_mut() {
                slot.view.set_filter(&input, false);
            }
        }
    }
}
