//! Application state, event loop, polling and navigation.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use color_eyre::eyre::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use futures::future::join_all;
use pulply_api::{Error, PulpClient, Registry, ResourceExt, Status};
use ratatui::DefaultTerminal;
use rust_i18n::t;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::AbortHandle;

use crate::commands::{self, Target};
use crate::config::{Config, netrc};
use crate::names::NameCache;
use crate::session::{self, Session};
use crate::theme::Theme;
use crate::ui;
use crate::ui::sidebar::{self, Sidebar};
use crate::views::connect::{ConnectView, State as ConnectState};
use crate::views::contexts::ContextView;
use crate::views::dashboard::DashboardView;
use crate::views::list::ListView;
use crate::views::{FetchKind, Level, Outcome, Payload, View};

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
const NOTICE_TTL: Duration = Duration::from_secs(6);

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
}

enum Msg {
    Connected { generation: u64, result: Result<(PulpClient, Status), Error> },
    Registry { generation: u64, result: Result<Registry, Error> },
    Data { generation: u64, view: u64, kind: FetchKind, result: Result<Payload, Error> },
    Names { generation: u64, entries: Vec<(String, String)> },
    TaskCounts { generation: u64, running: u64, waiting: u64 },
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
    pending_command: Option<String>,
    netrc: Vec<netrc::Entry>,
    password: Option<String>,
    tx: UnboundedSender<Msg>,
    rx: UnboundedReceiver<Msg>,
    generation: u64,
    next_id: u64,
    background: Vec<AbortHandle>,
    quit: bool,
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
            pending_command: None,
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
        }
        Ok(())
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
        self.ctx_name = name.clone();
        self.sidebar = Sidebar::build(None, Some(&self.sidebar));
        self.rebuild_completions();
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
            let result = session::connect(&ctx.cfg, creds, readonly).await;
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
                let Some(idx) = self.stack.iter().position(|s| s.id == view) else { return };
                match &result {
                    Ok(Payload::Page(page)) => page.results.iter().for_each(|r| self.names.learn(r)),
                    Ok(Payload::Object(obj)) => self.names.learn(obj),
                    _ => {}
                }
                let ctx = ctx!(self);
                let slot = &mut self.stack[idx];
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
            Msg::TaskCounts { .. } => self.counts_inflight = false,
            _ => {} // stale message from a previous context
        }
    }

    fn on_connected(&mut self, result: Result<(PulpClient, Status), Error>) {
        let Some(ctx) = self.config.context(&self.ctx_name).cloned() else { return };
        let url = ctx.cfg.base_url.clone();
        match result {
            Ok((client, status)) => {
                if !status.supported() {
                    let v = status.core_version().map(|v| v.to_string()).unwrap_or_else(|| "?".into());
                    self.notify(Level::Error, t!("notice.unsupported", version = v).into_owned());
                }
                let refresh = ctx.cfg.refresh(self.opts.refresh.unwrap_or(self.config.refresh));
                self.session = Some(Session { client, status, registry: None, refresh });
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

    // ------------------------------------------------------------ polling

    fn on_tick(&mut self) {
        if self.notice.as_ref().is_some_and(|n| n.at.elapsed() > NOTICE_TTL) {
            self.notice = None;
        }
        if self.paused {
            return;
        }
        let refresh = self.refresh_interval();
        if let Some(slot) = self.stack.last() {
            let due =
                slot.view.poll_interval(refresh).is_some_and(|iv| slot.last_poll.is_none_or(|t| t.elapsed() >= iv));
            if due && slot.inflight.is_empty() {
                self.fetch_top();
            }
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
        if let Some(idx) = self.stack.len().checked_sub(1) {
            self.fetch_slot(idx, FetchKind::Refresh);
        }
    }

    fn fetch_slot(&mut self, idx: usize, kind: FetchKind) {
        let ctx = ctx!(self);
        let slot = &mut self.stack[idx];
        if slot.inflight.contains(&kind) {
            return;
        }
        let Some(fut) = slot.view.fetch(kind, &ctx) else {
            slot.last_poll = Some(Instant::now());
            return;
        };
        slot.inflight.insert(kind);
        let (tx, generation, view) = (self.tx.clone(), self.generation, slot.id);
        let handle = tokio::spawn(async move {
            let result = fut.await;
            let _ = tx.send(Msg::Data { generation, view, kind, result });
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
            let fields = ("fields".to_owned(), "pulp_href,prn,name,username".to_owned());
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
                let Some(name) = v.name().or_else(|| v.str_field("username")) else { continue };
                for key in [v.href(), v.str_field("prn")].into_iter().flatten() {
                    entries.push((key.to_owned(), name.to_owned()));
                }
            }
            Msg::Names { generation, entries }
        });
    }

    // ------------------------------------------------------------ navigation

    fn new_slot(&mut self, view: Box<dyn View>) -> Slot {
        self.next_id += 1;
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
            Target::Quit => self.quit = true,
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
        match commands::parse(input, reg.as_deref()) {
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

    fn apply(&mut self, outcome: Outcome, view_id: u64) {
        match outcome {
            Outcome::Ignored | Outcome::None => {}
            Outcome::Push(v) => self.push(v),
            Outcome::Fetch(kind) => {
                if let Some(idx) = self.stack.iter().position(|s| s.id == view_id) {
                    self.fetch_slot(idx, kind);
                }
            }
            Outcome::Open(target) => self.open(target),
            Outcome::SwitchContext(name) => self.start_connect(name),
            Outcome::SubmitPassword(pw) => {
                self.password = Some(pw);
                self.start_connect(self.ctx_name.clone());
            }
            Outcome::Retry => self.start_connect(self.ctx_name.clone()),
            Outcome::Resolve(hrefs) => self.resolve(hrefs),
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
