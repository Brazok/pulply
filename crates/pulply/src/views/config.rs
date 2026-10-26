//! `:config` — configuration files with their state; Enter edits one in `$EDITOR` (R14.6).

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Modifier;
use ratatui::text::Span;
use ratatui::widgets::{Cell, Row, Table, TableState};
use rust_i18n::t;

use super::{Ctx, Fetch, FetchKind, Hint, Outcome, View};
use crate::config::files::{self, ConfigFile, Kind, Status};
use crate::labels::tr;

#[derive(Default)]
pub struct ConfigView {
    state: TableState,
    files: Vec<ConfigFile>,
}

impl ConfigView {
    pub fn new(ctx: &Ctx) -> Self {
        Self { state: TableState::default().with_selected(Some(0)), files: files::list(ctx.config) }
    }
}

/// Skin of the current context.
fn active_skin<'a>(ctx: &'a Ctx) -> &'a str {
    let config = ctx.config;
    config.context(ctx.current_ctx).and_then(|c| c.cfg.skin.as_deref()).unwrap_or(&config.skin)
}

impl View for ConfigView {
    fn title(&self) -> String {
        t!("config.title").into_owned()
    }

    fn info(&self, ctx: &Ctx) -> String {
        ctx.config.dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default()
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        vec![Hint::new("↵/e", t!("hint.edit"))]
    }

    fn handle_key(&mut self, key: KeyEvent, _ctx: &Ctx) -> Outcome {
        let cur = self.state.selected().unwrap_or(0);
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.state.select(Some((cur + 1).min(self.files.len().saturating_sub(1))))
            }
            KeyCode::Up | KeyCode::Char('k') => self.state.select(Some(cur.saturating_sub(1))),
            KeyCode::Home | KeyCode::Char('g') => self.state.select(Some(0)),
            KeyCode::End | KeyCode::Char('G') => self.state.select(Some(self.files.len().saturating_sub(1))),
            KeyCode::Enter | KeyCode::Char('e') => {
                return self.files.get(cur).map_or(Outcome::None, |f| Outcome::EditConfig(f.clone()));
            }
            _ => return Outcome::Ignored,
        }
        Outcome::None
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let theme = ctx.theme;
        let bold = theme.accent().add_modifier(Modifier::BOLD);
        let header = Row::new(["", "file", "description", "status", "path"].map(|k| {
            Cell::from(Span::styled(if k.is_empty() { String::new() } else { tr(&format!("config.{k}")) }, bold))
        }));
        let active = active_skin(ctx);
        let rows = self.files.iter().map(|file| {
            let in_use = matches!(&file.kind, Kind::Skin(n) if n == active);
            let status = match file.status(ctx.config) {
                Status::Present => Span::styled(t!("config.present").into_owned(), theme.success()),
                Status::Defaults => Span::styled(t!("config.defaults").into_owned(), theme.muted()),
                Status::Builtin => Span::styled(t!("config.builtin").into_owned(), theme.muted()),
                Status::Ignored => Span::styled(t!("config.ignored").into_owned(), theme.warning()),
            };
            Row::new(vec![
                Cell::from(Span::styled(if in_use { "*" } else { "" }, theme.accent())),
                Cell::from(file.label()),
                Cell::from(tr(&format!("config.desc.{}", file.desc_key()))),
                Cell::from(status),
                Cell::from(Span::styled(file.path.display().to_string(), theme.muted())),
            ])
        });
        let widths = [
            Constraint::Length(1),
            Constraint::Length(20),
            Constraint::Min(28),
            Constraint::Length(14),
            Constraint::Min(24),
        ];
        let table = Table::new(rows, widths).header(header).column_spacing(2).row_highlight_style(theme.selected());
        f.render_stateful_widget(table, area, &mut self.state);
    }

    /// Rescans the files (a skin added outside pulply, a file created by an edit).
    fn fetch(&mut self, _kind: FetchKind, ctx: &Ctx) -> Option<Fetch> {
        let selected = self.state.selected().and_then(|i| self.files.get(i)).map(ConfigFile::key);
        self.files = files::list(ctx.config);
        let pos = selected.and_then(|k| self.files.iter().position(|f| f.key() == k)).unwrap_or(0);
        self.state.select(Some(pos.min(self.files.len().saturating_sub(1))));
        None
    }
}
