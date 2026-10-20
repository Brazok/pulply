//! `:ctx` — configured contexts with reachability; Enter switches.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};
use futures::FutureExt;
use futures::future::join_all;
use pulply_api::{Error, PulpClient};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Modifier;
use ratatui::text::Span;
use ratatui::widgets::{Cell, Row, Table, TableState};
use rust_i18n::t;

use super::{Ctx, Fetch, FetchKind, Hint, Outcome, Payload, View};
use crate::config::Origin;

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct ContextView {
    state: TableState,
    /// name → Ok(pulpcore version) | Err(message)
    reachability: Vec<(String, Result<String, String>)>,
    probing: bool,
}

impl ContextView {
    pub fn new(ctx: &Ctx) -> Self {
        let pos = ctx.config.contexts.iter().position(|c| c.name == ctx.current_ctx).unwrap_or(0);
        Self { state: TableState::default().with_selected(Some(pos)), ..Self::default() }
    }
}

impl View for ContextView {
    fn title(&self) -> String {
        t!("ctx.title").into_owned()
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        vec![Hint::new("↵", t!("hint.switch"))]
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome {
        let len = ctx.config.contexts.len();
        let cur = self.state.selected().unwrap_or(0);
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.state.select(Some((cur + 1).min(len.saturating_sub(1)))),
            KeyCode::Up | KeyCode::Char('k') => self.state.select(Some(cur.saturating_sub(1))),
            KeyCode::Enter => {
                return match ctx.config.contexts.get(cur) {
                    Some(c) => Outcome::SwitchContext(c.name.clone()),
                    None => Outcome::None,
                };
            }
            _ => return Outcome::Ignored,
        }
        Outcome::None
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let theme = ctx.theme;
        let bold = theme.accent().add_modifier(Modifier::BOLD);
        let header = Row::new(["", "name", "url", "user", "mode", "source", "status"].map(|k| {
            Cell::from(Span::styled(
                if k.is_empty() { String::new() } else { crate::labels::tr(&format!("ctx.{k}")) },
                bold,
            ))
        }));
        let rows = ctx.config.contexts.iter().map(|c| {
            let current = if c.name == ctx.current_ctx { "*" } else { "" };
            let mode = if c.cfg.readonly { t!("ctx.readonly") } else { t!("ctx.readwrite") };
            let source = match c.origin {
                Origin::Pulply => "pulply",
                Origin::PulpCli => "pulp-cli",
            };
            let status = match self.reachability.iter().find(|(n, _)| n == &c.name).map(|(_, r)| r) {
                Some(Ok(v)) => Span::styled(format!("pulpcore {v}"), theme.success()),
                Some(Err(e)) => Span::styled(e.clone(), theme.error()),
                None if self.probing => Span::styled("…", theme.muted()),
                None => Span::raw(""),
            };
            Row::new(vec![
                Cell::from(Span::styled(current, theme.accent())),
                Cell::from(c.name.clone()),
                Cell::from(c.cfg.base_url.clone()),
                Cell::from(c.cfg.username.clone().unwrap_or_default()),
                Cell::from(mode.into_owned()),
                Cell::from(source),
                Cell::from(status),
            ])
        });
        let widths = [
            Constraint::Length(1),
            Constraint::Length(16),
            Constraint::Min(24),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(9),
            Constraint::Min(16),
        ];
        let table = Table::new(rows, widths).header(header).column_spacing(2).row_highlight_style(theme.selected());
        f.render_stateful_widget(table, area, &mut self.state);
    }

    /// Anonymous `status/` probe of every context (status needs no credentials).
    fn fetch(&mut self, _kind: FetchKind, ctx: &Ctx) -> Option<Fetch> {
        self.probing = true;
        let probes: Vec<(String, Result<PulpClient, String>)> = ctx
            .config
            .contexts
            .iter()
            .map(|c| {
                let client = c.cfg.client_config(None, true).map_err(|e| e.to_string()).and_then(|mut cfg| {
                    cfg.timeout = PROBE_TIMEOUT;
                    PulpClient::new(cfg).map_err(|e| e.to_string())
                });
                (c.name.clone(), client)
            })
            .collect();
        Some(
            async move {
                let results = join_all(probes.into_iter().map(|(name, client)| async move {
                    let res = match client {
                        Ok(c) => c
                            .status()
                            .await
                            .map(|s| s.core_version().map(|v| v.to_string()).unwrap_or_else(|| "?".into()))
                            .map_err(|e| e.to_string()),
                        Err(e) => Err(e),
                    };
                    (name, res)
                }))
                .await;
                Ok(Payload::Reachability(results))
            }
            .boxed(),
        )
    }

    fn on_data(&mut self, _kind: FetchKind, result: Result<Payload, Error>, _ctx: &Ctx) -> Outcome {
        self.probing = false;
        if let Ok(Payload::Reachability(r)) = result {
            self.reachability = r;
        }
        Outcome::None
    }

    fn poll_interval(&self, _default: Duration) -> Option<Duration> {
        None
    }
}
