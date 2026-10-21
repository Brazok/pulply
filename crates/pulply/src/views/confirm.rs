//! Yes/No confirmation overlay; "No" is selected by default (R13.2).

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use rust_i18n::t;

use super::{Ctx, Hint, Outcome, View};

pub struct ConfirmView {
    message: String,
    then: Option<Outcome>,
    yes: bool,
}

impl ConfirmView {
    pub fn new(message: impl Into<String>, then: Outcome) -> Self {
        Self { message: message.into(), then: Some(then), yes: false }
    }

    fn accept(&mut self) -> Outcome {
        match self.then.take() {
            Some(then) => Outcome::Batch(vec![Outcome::CloseOverlay, then]),
            None => Outcome::CloseOverlay,
        }
    }
}

/// Overlay asking to confirm `then`.
pub fn confirm(message: impl Into<String>, then: Outcome) -> Outcome {
    Outcome::Overlay(Box::new(ConfirmView::new(message, then)))
}

impl View for ConfirmView {
    fn title(&self) -> String {
        t!("confirm.title").into_owned()
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        vec![Hint::new("y/n", t!("hint.confirm")), Hint::new("←→", t!("hint.choose"))]
    }

    fn handle_key(&mut self, key: KeyEvent, _ctx: &Ctx) -> Outcome {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => self.accept(),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Char('q') => Outcome::CloseOverlay,
            KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::Char('h') | KeyCode::Char('l') => {
                self.yes = !self.yes;
                Outcome::None
            }
            KeyCode::Enter if self.yes => self.accept(),
            KeyCode::Enter => Outcome::CloseOverlay,
            _ => Outcome::None,
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let theme = ctx.theme;
        let [text, buttons] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        f.render_widget(Paragraph::new(self.message.clone()).wrap(Wrap { trim: true }), text);
        let button = |label: String, active: bool, style: Style| {
            if active {
                Span::styled(format!(" {label} "), style.add_modifier(Modifier::REVERSED | Modifier::BOLD))
            } else {
                Span::styled(format!(" {label} "), style)
            }
        };
        let line = Line::from(vec![
            button(t!("confirm.yes").into_owned(), self.yes, theme.error()),
            Span::raw("   "),
            button(t!("confirm.no").into_owned(), !self.yes, theme.accent()),
        ]);
        let [centered] =
            Layout::horizontal([Constraint::Length(line.width() as u16)]).flex(Flex::Center).areas(buttons);
        f.render_widget(Paragraph::new(line), centered);
    }

    fn poll_interval(&self, _default: std::time::Duration) -> Option<std::time::Duration> {
        None
    }

    fn overlay_size(&self, area: Rect) -> Rect {
        centered(area, 64, 8)
    }
}

/// `width` × `height` rectangle centered in `area` (clamped).
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [r] = Layout::vertical([Constraint::Length(height.min(area.height))]).flex(Flex::Center).areas(area);
    let [r] = Layout::horizontal([Constraint::Length(width.min(area.width))]).flex(Flex::Center).areas(r);
    r
}
