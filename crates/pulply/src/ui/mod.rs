//! Frame layout: header, optional prompt, sidebar + main view, status bar, key hints.

pub mod sidebar;

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use rust_i18n::t;

use crate::app::{App, PromptKind, ctx};
use crate::commands;
use crate::views::{Hint, Level};

const SIDEBAR_WIDTH: u16 = 30;

pub fn draw(f: &mut Frame, app: &mut App) {
    let prompt_h = u16::from(app.prompt.is_some());
    let [header, prompt, body, status, hints] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(prompt_h),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(f.area());

    draw_header(f, header, app);
    if app.prompt.is_some() {
        draw_prompt(f, prompt, app);
    }

    let main = if app.sidebar_visible {
        let [side, main] = Layout::horizontal([Constraint::Length(SIDEBAR_WIDTH), Constraint::Min(20)]).areas(body);
        app.sidebar.render(f, side, app.sidebar_focus, app.active_target.as_ref(), &app.theme);
        main
    } else {
        body
    };

    let ctx = ctx!(app);
    let breadcrumb = app.stack.iter().map(|s| s.view.title()).collect::<Vec<_>>().join(" › ");
    let info = app.stack.last().map(|s| s.view.info(&ctx)).unwrap_or_default();
    // Hints follow the focused layer: the overlay when one is open.
    let mut view_hints = app.overlay.as_ref().or(app.stack.last()).map(|s| s.view.hints(&ctx)).unwrap_or_default();
    view_hints.extend(app.plugin_hints());
    let border = if app.sidebar_focus { app.theme.border } else { app.theme.border_focus };
    let block = Block::bordered()
        .border_style(Style::new().fg(border))
        .title(Line::from(Span::styled(format!(" {breadcrumb} "), app.theme.accent().add_modifier(Modifier::BOLD))))
        .title(Line::from(Span::styled(format!(" {info} "), app.theme.muted())).right_aligned());
    let inner = block.inner(main);
    f.render_widget(block, main);
    if let Some(slot) = app.stack.last_mut() {
        slot.view.render(f, inner, &ctx);
    }
    if let Some(slot) = app.overlay.as_mut() {
        let area = slot.view.overlay_size(f.area());
        f.render_widget(Clear, area);
        let block = Block::bordered()
            .border_style(Style::new().fg(app.theme.border_focus))
            .title(Line::from(Span::styled(
                format!(" {} ", slot.view.title()),
                app.theme.accent().add_modifier(Modifier::BOLD),
            )))
            .title(Line::from(Span::styled(format!(" {} ", slot.view.info(&ctx)), app.theme.muted())).right_aligned());
        let inner = block.inner(area);
        f.render_widget(block, area);
        slot.view.render(f, inner, &ctx);
    }

    draw_status(f, status, app);
    draw_hints(f, hints, app, view_hints);
    if app.help {
        draw_help(f, app);
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let base = theme.header();
    let sep = Span::styled(" │ ", base.fg(theme.muted));
    let mut spans = vec![
        Span::styled(" pulply ", base.add_modifier(Modifier::BOLD).fg(theme.accent)),
        sep.clone(),
        Span::styled(format!("{} {}", t!("header.context"), app.ctx_name), base.add_modifier(Modifier::BOLD)),
    ];
    if let Some(s) = &app.session {
        let user = s.client.username().map(|u| format!("{u}@")).unwrap_or_default();
        spans.push(sep.clone());
        spans.push(Span::styled(format!("{user}{}", s.client.base_url()), base));
        if let Some(v) = s.status.core_version() {
            spans.push(sep.clone());
            spans.push(Span::styled(format!("pulpcore {v}"), base));
        }
        if !s.status.supported() {
            spans.push(sep.clone());
            spans.push(Span::styled(
                t!("header.unsupported").into_owned(),
                base.fg(theme.warning).add_modifier(Modifier::BOLD),
            ));
        }
        if s.client.readonly() {
            spans.push(sep.clone());
            spans.push(Span::styled(
                " RO ",
                Style::new().bg(theme.warning).fg(ratatui::style::Color::Black).add_modifier(Modifier::BOLD),
            ));
        }
        spans.push(sep.clone());
        let refresh = if app.paused {
            Span::styled(format!("⏸ {}", t!("header.paused")), base.fg(theme.warning))
        } else {
            Span::styled(format!("⟳ {}s", s.refresh.as_secs()), base)
        };
        spans.push(refresh);
    }
    f.render_widget(Paragraph::new(Line::from(spans)).style(base), area);
}

fn draw_prompt(f: &mut Frame, area: Rect, app: &App) {
    let Some(p) = &app.prompt else { return };
    let theme = &app.theme;
    let (sigil, ghost) = match p.kind {
        PromptKind::Command => {
            (":", commands::suggest(&p.input, &app.completions).map(|s| s[p.input.len()..].to_owned()))
        }
        PromptKind::Filter => ("/", None),
    };
    let line = Line::from(vec![
        Span::styled(format!(" {sigil}"), theme.key()),
        Span::raw(p.input.clone()),
        Span::styled(ghost.unwrap_or_default(), theme.muted()),
        Span::styled("▏", theme.accent()),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_status(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let mut left = Vec::new();
    if !app.tracked.is_empty() {
        // Tasks started from pulply take precedence over server-wide counters.
        left.push(Span::styled(" ⚙", theme.task_state("running")));
        for t in &app.tracked {
            let progress = match t.progress {
                Some((done, total)) if total > 0 => format!(" {}%", done * 100 / total),
                _ => String::new(),
            };
            left.push(Span::styled(format!(" {}{progress}", t.label), theme.task_state(&t.state)));
            left.push(Span::styled(" ·", theme.muted()));
        }
        left.pop();
    } else if let Some((running, waiting)) = app.task_counts {
        let style = if running + waiting > 0 { theme.task_state("running") } else { theme.muted() };
        left.push(Span::styled(
            format!(" ⚙ {running} {} · {waiting} {}", t!("status.running"), t!("status.waiting")),
            style,
        ));
    }
    let notice_w = app.notice.as_ref().map_or(0, |n| (n.text.chars().count() as u16 + 1).min(area.width * 7 / 10));
    let [l, r] = Layout::horizontal([Constraint::Min(0), Constraint::Length(notice_w)]).areas(area);
    f.render_widget(Paragraph::new(Line::from(left)), l);
    if let Some(n) = &app.notice {
        let style = match n.level {
            Level::Error => theme.error(),
            Level::Success => theme.success(),
            Level::Info => theme.accent(),
        };
        f.render_widget(Paragraph::new(Line::styled(format!("{} ", n.text), style)).right_aligned(), r);
    }
}

fn hint_spans(hints: &[Hint], app: &App) -> Vec<Span<'static>> {
    hints
        .iter()
        .flat_map(|h| [Span::styled(format!(" <{}>", h.key), app.theme.key()), Span::raw(format!(" {}", h.label))])
        .collect()
}

fn draw_hints(f: &mut Frame, area: Rect, app: &App, view_hints: Vec<Hint>) {
    let mut hints = view_hints;
    hints.push(Hint::new(":", t!("hint.command")));
    hints.push(Hint::new("?", t!("hint.help")));
    f.render_widget(Paragraph::new(Line::from(hint_spans(&hints, app))), area);
}

fn global_hints() -> Vec<Hint> {
    vec![
        Hint::new(":", t!("help.command")),
        Hint::new("/", t!("help.filter")),
        Hint::new("tab", t!("help.sidebar_focus")),
        Hint::new("ctrl-b", t!("help.sidebar_toggle")),
        Hint::new("ctrl-r", t!("help.refresh")),
        Hint::new("p", t!("help.pause")),
        Hint::new("esc", t!("help.back")),
        Hint::new("[ ]", t!("help.history")),
        Hint::new("q", t!("help.quit")),
    ]
}

fn draw_help(f: &mut Frame, app: &App) {
    let ctx = ctx!(app);
    let mut view_hints = app.stack.last().map(|s| s.view.hints(&ctx)).unwrap_or_default();
    view_hints.extend(app.plugin_hints());
    let section = |title: String| Line::styled(title, app.theme.accent().add_modifier(Modifier::BOLD));
    let row = |h: &Hint| {
        Line::from(vec![Span::styled(format!("  {:<10}", h.key), app.theme.key()), Span::raw(h.label.clone())])
    };
    let mut lines = vec![section(t!("help.global").into_owned())];
    lines.extend(global_hints().iter().map(row));
    if !view_hints.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section(t!("help.view").into_owned()));
        lines.extend(view_hints.iter().map(row));
    }
    lines.push(Line::raw(""));
    lines.push(section(t!("help.commands").into_owned()));
    for example in ["repo rpm", "content container/tags", "dist", "task", "ctx", "config", "dashboard", "q"] {
        lines.push(Line::from(vec![Span::styled(format!("  :{example}"), app.theme.key())]));
    }
    let height = (lines.len() as u16 + 2).min(f.area().height);
    let [area] = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center).areas(f.area());
    let [area] = Layout::horizontal([Constraint::Length(56)]).flex(Flex::Center).areas(area);
    f.render_widget(Clear, area);
    let block = Block::bordered().title(format!(" {} ", t!("help.title"))).border_style(app.theme.accent());
    f.render_widget(Paragraph::new(lines).block(block), area);
}
