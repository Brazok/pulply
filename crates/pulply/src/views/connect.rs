//! Connection screen: connecting, password prompt, or failure with recovery actions.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use rust_i18n::t;

use super::{Ctx, Hint, Outcome, View, contexts::ContextView};

pub enum State {
    Connecting,
    Password { input: String, error: Option<String> },
    Failed(String),
}

pub struct ConnectView {
    ctx_name: String,
    url: String,
    pub state: State,
}

impl ConnectView {
    pub fn new(ctx_name: String, url: String, state: State) -> Self {
        Self { ctx_name, url, state }
    }
}

impl View for ConnectView {
    fn title(&self) -> String {
        self.ctx_name.clone()
    }

    fn captures_input(&self) -> bool {
        matches!(self.state, State::Password { .. })
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        match self.state {
            State::Connecting => vec![Hint::new("c", t!("hint.contexts"))],
            State::Password { .. } => vec![Hint::new("↵", t!("hint.submit")), Hint::new("esc", t!("hint.contexts"))],
            State::Failed(_) => vec![
                Hint::new("r", t!("hint.retry")),
                Hint::new("c", t!("hint.contexts")),
                Hint::new("q", t!("hint.quit")),
            ],
        }
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome {
        match &mut self.state {
            State::Password { input, .. } => match key.code {
                KeyCode::Enter => Outcome::SubmitPassword(std::mem::take(input)),
                KeyCode::Esc => Outcome::Push(Box::new(ContextView::new(ctx))),
                KeyCode::Backspace => {
                    input.pop();
                    Outcome::None
                }
                KeyCode::Char(c) => {
                    input.push(c);
                    Outcome::None
                }
                _ => Outcome::None,
            },
            State::Connecting | State::Failed(_) => match key.code {
                KeyCode::Char('r') if matches!(self.state, State::Failed(_)) => Outcome::Retry,
                KeyCode::Char('c') => Outcome::Push(Box::new(ContextView::new(ctx))),
                _ => Outcome::Ignored,
            },
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let theme = ctx.theme;
        let [area] = Layout::vertical([Constraint::Length(9)]).flex(Flex::Center).areas(area);
        let [area] = Layout::horizontal([Constraint::Max(80)]).flex(Flex::Center).areas(area);
        let target = Line::from(vec![
            Span::styled(format!("{}: ", t!("connect.context")), theme.muted()),
            Span::styled(self.ctx_name.clone(), theme.accent().add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {}", self.url), theme.muted()),
        ]);
        let mut lines = vec![target, Line::raw("")];
        let title = match &self.state {
            State::Connecting => {
                lines.push(Line::styled(t!("connect.connecting").into_owned(), theme.accent()));
                t!("connect.title")
            }
            State::Password { input, error } => {
                if let Some(e) = error {
                    lines.push(Line::styled(e.clone(), theme.error()));
                }
                lines.push(Line::from(vec![
                    Span::raw(format!("{}: ", t!("connect.password"))),
                    Span::raw("•".repeat(input.chars().count())),
                    Span::styled("▏", theme.accent()),
                ]));
                t!("connect.auth_title")
            }
            State::Failed(err) => {
                lines.push(Line::styled(t!("connect.failed").into_owned(), theme.error().add_modifier(Modifier::BOLD)));
                lines.push(Line::styled(err.clone(), theme.error()));
                t!("connect.title")
            }
        };
        let block = Block::bordered().title(format!(" {title} ")).border_style(theme.accent());
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }).block(block), area);
    }

    fn poll_interval(&self, _default: Duration) -> Option<Duration> {
        None
    }
}
