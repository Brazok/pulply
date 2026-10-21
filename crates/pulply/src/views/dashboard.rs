//! Server overview: health, task activity, storage, components, repositories.

use crossterm::event::{KeyCode, KeyEvent};
use futures::FutureExt;
use futures::future::join_all;
use pulply_api::{Error, Status};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Gauge, Paragraph};
use rust_i18n::t;

use super::{Ctx, Fetch, FetchKind, Hint, Outcome, Payload, View, list};
use crate::commands::Target;
use crate::format;
use crate::theme::Theme;

#[derive(Debug, Default)]
pub struct DashboardData {
    pub status: Status,
    pub running: Option<u64>,
    pub waiting: Option<u64>,
    pub failed_24h: Option<u64>,
    pub completed_24h: Option<u64>,
    pub missing_workers: Option<u64>,
    /// (label, count) per repository type.
    pub repositories: Vec<(String, u64)>,
}

#[derive(Default)]
pub struct DashboardView {
    data: Option<DashboardData>,
    error: Option<String>,
}

impl DashboardView {
    pub fn new() -> Self {
        Self::default()
    }
}

fn count_line(label: String, value: Option<u64>, style: Style, theme: &Theme) -> Line<'static> {
    let v = value.map(|v| v.to_string()).unwrap_or_else(|| "?".into());
    Line::from(vec![Span::styled(format!("{label:<22}"), theme.muted()), Span::styled(v, style)])
}

fn check(ok: Option<bool>, theme: &Theme) -> Span<'static> {
    match ok {
        Some(true) => Span::styled(format!("✓ {}", t!("dash.ok")), theme.success()),
        Some(false) => Span::styled(format!("✗ {}", t!("dash.down")), theme.error()),
        None => Span::styled("–", theme.muted()),
    }
}

fn panel(title: String, ctx: &Ctx) -> Block<'static> {
    Block::bordered()
        .title(Span::styled(format!(" {title} "), ctx.theme.accent().add_modifier(Modifier::BOLD)))
        .border_style(Style::new().fg(ctx.theme.border))
}

impl DashboardView {
    fn render_health(&self, f: &mut Frame, area: Rect, d: &DashboardData, ctx: &Ctx) {
        let theme = ctx.theme;
        let s = &d.status;
        let label = |k: &str| Span::styled(format!("{:<22}", t!(k)), theme.muted());
        let missing = d.missing_workers.unwrap_or(0);
        let workers_style = if s.online_workers.is_empty() || missing > 0 { theme.error() } else { theme.success() };
        let mut workers = vec![label("dash.workers"), Span::styled(s.online_workers.len().to_string(), workers_style)];
        if missing > 0 {
            workers.push(Span::styled(format!("  ({missing} {})", t!("dash.missing")), theme.error()));
        }
        let apps = |n: usize| Span::styled(n.to_string(), if n == 0 { theme.error() } else { theme.success() });
        let mut lines = vec![
            Line::from(vec![label("dash.database"), check(s.database_connection.as_ref().map(|c| c.connected), theme)]),
            Line::from(vec![
                label("dash.redis"),
                // Redis is optional in Pulp: "not connected" is a warning, not an outage.
                match s.redis_connection.as_ref().map(|c| c.connected) {
                    Some(false) => Span::styled(format!("– {}", t!("dash.not_connected")), theme.warning()),
                    other => check(other, theme),
                },
            ]),
            Line::from(workers),
            Line::from(vec![label("dash.api_apps"), apps(s.online_api_apps.len())]),
            Line::from(vec![label("dash.content_apps"), apps(s.online_content_apps.len())]),
        ];
        if let Some(origin) = s.content_settings.as_ref().and_then(|c| c.content_origin.clone()) {
            lines.push(Line::from(vec![label("dash.content_origin"), Span::raw(origin)]));
        }
        if s.domain_enabled {
            lines.push(Line::from(vec![
                label("dash.domains"),
                Span::styled(t!("dash.enabled").into_owned(), theme.accent()),
            ]));
        }
        f.render_widget(Paragraph::new(lines).block(panel(t!("dash.health").into_owned(), ctx)), area);
    }

    fn render_tasks(&self, f: &mut Frame, area: Rect, d: &DashboardData, ctx: &Ctx) {
        let theme = ctx.theme;
        let failed_style = if d.failed_24h.unwrap_or(0) > 0 { theme.error() } else { theme.success() };
        let lines = vec![
            count_line(t!("dash.running").into_owned(), d.running, theme.task_state("running"), theme),
            count_line(t!("dash.waiting").into_owned(), d.waiting, theme.task_state("waiting"), theme),
            count_line(t!("dash.failed_24h").into_owned(), d.failed_24h, failed_style, theme),
            count_line(t!("dash.completed_24h").into_owned(), d.completed_24h, theme.success(), theme),
        ];
        f.render_widget(Paragraph::new(lines).block(panel(t!("dash.tasks").into_owned(), ctx)), area);
    }

    fn render_storage(&self, f: &mut Frame, area: Rect, d: &DashboardData, ctx: &Ctx) {
        let block = panel(t!("dash.storage").into_owned(), ctx);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let [gauge_area, text_area, versions_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)]).areas(inner);
        match d.status.storage.as_ref().and_then(|s| Some((s.total?, s.used?, s.free.unwrap_or(0)))) {
            Some((total, used, free)) if total > 0 => {
                let ratio = used as f64 / total as f64;
                let style = if ratio > 0.9 {
                    ctx.theme.error()
                } else if ratio > 0.75 {
                    ctx.theme.warning()
                } else {
                    ctx.theme.success()
                };
                f.render_widget(Gauge::default().ratio(ratio.clamp(0.0, 1.0)).gauge_style(style), gauge_area);
                let text = format!(
                    "{} / {}  ({} {})",
                    format::bytes(used),
                    format::bytes(total),
                    format::bytes(free),
                    t!("dash.free")
                );
                f.render_widget(Paragraph::new(text), text_area);
            }
            _ => f.render_widget(
                Paragraph::new(Line::styled(t!("dash.storage_unknown").into_owned(), ctx.theme.muted())),
                gauge_area,
            ),
        }
        let mut versions: Vec<Line> = vec![Line::styled(t!("dash.components").into_owned(), ctx.theme.accent())];
        versions.extend(d.status.versions.iter().map(|v| {
            Line::from(vec![
                Span::styled(format!("  {:<16}", v.component), ctx.theme.muted()),
                Span::raw(v.version.clone()),
            ])
        }));
        f.render_widget(Paragraph::new(versions), versions_area);
    }

    fn render_repositories(&self, f: &mut Frame, area: Rect, d: &DashboardData, ctx: &Ctx) {
        let block = panel(t!("dash.repositories").into_owned(), ctx);
        if d.repositories.is_empty() {
            let msg = if ctx.registry().is_some() { t!("dash.no_repositories") } else { t!("dash.schema_loading") };
            f.render_widget(Paragraph::new(Line::styled(msg.into_owned(), ctx.theme.muted())).block(block), area);
            return;
        }
        let max = d.repositories.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1);
        let label_w = d.repositories.iter().map(|(l, _)| l.chars().count()).max().unwrap_or(10).min(30);
        let bar_w = (area.width as usize).saturating_sub(label_w + 12).max(5);
        let lines: Vec<Line> = d
            .repositories
            .iter()
            .map(|(label, count)| {
                let filled = ((*count as f64 / max as f64) * bar_w as f64).round() as usize;
                Line::from(vec![
                    Span::raw(format!("{:<label_w$} ", label.chars().take(label_w).collect::<String>())),
                    Span::styled("█".repeat(filled.max(usize::from(*count > 0))), ctx.theme.accent()),
                    Span::raw(format!(" {count}")),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(lines).block(block), area);
    }
}

impl View for DashboardView {
    fn title(&self) -> String {
        t!("dash.title").into_owned()
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        vec![
            Hint::new("t", t!("hint.tasks")),
            Hint::new("F", t!("hint.failed_tasks")),
            Hint::new("w", t!("hint.workers")),
            Hint::new("r", t!("hint.repositories")),
        ]
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome {
        let open = |id: &str| Outcome::Open(Target::Resource(id.into()));
        match key.code {
            KeyCode::Char('t') => open("tasks"),
            KeyCode::Char('w') => open("workers"),
            KeyCode::Char('r') => open("repositories"),
            KeyCode::Char('F') => match ctx.registry().and_then(|r| r.get("tasks").map(|t| (r, t))) {
                Some((reg, t)) => {
                    Outcome::Push(Box::new(list::ListView::for_type(t, reg, None).with_task_state("failed")))
                }
                None => Outcome::None,
            },
            _ => Outcome::Ignored,
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let Some(d) = &self.data else {
            let msg = self.error.clone().unwrap_or_else(|| t!("list.loading").into_owned());
            f.render_widget(Paragraph::new(Line::styled(msg, ctx.theme.muted())), area);
            return;
        };
        let [top, bottom] = Layout::vertical([Constraint::Length(9), Constraint::Min(6)]).areas(area);
        let [health, tasks] = Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(top);
        let [storage, repos] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(bottom);
        self.render_health(f, health, d, ctx);
        self.render_tasks(f, tasks, d, ctx);
        self.render_storage(f, storage, d, ctx);
        self.render_repositories(f, repos, d, ctx);
        if let Some(err) = &self.error {
            let line = Rect { y: area.bottom().saturating_sub(1), height: 1, ..area };
            f.render_widget(Paragraph::new(Line::styled(err.clone(), ctx.theme.error())), line);
        }
    }

    fn fetch(&mut self, _kind: FetchKind, ctx: &Ctx) -> Option<Fetch> {
        let client = ctx.client()?.clone();
        let repo_types: Vec<(String, String)> = ctx
            .registry()
            .map(|r| {
                r.types()
                    .iter()
                    .filter(|t| t.category == "repositories" && !t.generic && t.parent.is_none())
                    .filter_map(|t| {
                        let label =
                            if t.name == t.plugin { t.plugin.clone() } else { format!("{}/{}", t.plugin, t.name) };
                        Some((label, t.list_path.clone()?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let since = (ctx.now - chrono::Duration::hours(24)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        Some(
            async move {
                let tasks = client.api_path("tasks/");
                let workers = client.api_path("workers/");
                let q = |pairs: &[(&str, &str)]| {
                    pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect::<Vec<_>>()
                };
                let (q_running, q_waiting) = (q(&[("state", "running")]), q(&[("state", "waiting")]));
                let q_failed = q(&[("state", "failed"), ("finished_at__gte", &since)]);
                let q_completed = q(&[("state", "completed"), ("finished_at__gte", &since)]);
                let q_missing = q(&[("missing", "true")]);
                let (status, running, waiting, failed, completed, missing, repos) = futures::join!(
                    client.status(),
                    client.count(&tasks, &q_running),
                    client.count(&tasks, &q_waiting),
                    client.count(&tasks, &q_failed),
                    client.count(&tasks, &q_completed),
                    client.count(&workers, &q_missing),
                    join_all(repo_types.iter().map(|(_, path)| client.count(path, &[]))),
                );
                let mut repositories: Vec<(String, u64)> = repo_types
                    .into_iter()
                    .zip(repos)
                    .filter_map(|((label, _), count)| count.ok().filter(|c| *c > 0).map(|c| (label, c)))
                    .collect();
                repositories.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                Ok(Payload::Dashboard(Box::new(DashboardData {
                    status: status?,
                    running: running.ok(),
                    waiting: waiting.ok(),
                    failed_24h: failed.ok(),
                    completed_24h: completed.ok(),
                    missing_workers: missing.ok(),
                    repositories,
                })))
            }
            .boxed(),
        )
    }

    fn on_data(&mut self, _kind: FetchKind, result: Result<Payload, Error>, _ctx: &Ctx) -> Outcome {
        match result {
            Ok(Payload::Dashboard(d)) => {
                self.data = Some(*d);
                self.error = None;
            }
            Ok(_) => {}
            Err(e) => self.error = Some(super::error_text(&e)),
        }
        Outcome::None
    }
}
