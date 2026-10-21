//! Task detail: state, timings, progress reports, created resources, children, error.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures::FutureExt;
use pulply_api::models::is_href;
use pulply_api::{Error, ResourceExt};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use rust_i18n::t;
use serde_json::Value;

use super::lines::{LineItem, LineList};
use super::{Ctx, Fetch, FetchKind, Hint, Outcome, Payload, View, actions, detail, open_href};
use crate::format;
use crate::theme::Theme;

const BAR_WIDTH: usize = 24;

pub struct TaskView {
    href: String,
    task: Option<Value>,
    children: Vec<Value>,
    lines: LineList,
    error: Option<String>,
}

impl TaskView {
    pub fn new(href: String) -> Self {
        Self { href, task: None, children: Vec::new(), lines: LineList::default(), error: None }
    }

    fn state(&self) -> Option<&str> {
        self.task.as_ref().and_then(|t| t.str_field("state"))
    }
}

pub fn is_active(state: &str) -> bool {
    matches!(state, "waiting" | "running" | "canceling")
}

/// `▕██████░░░░▏ 60%` for a progress report with a known total.
pub fn progress_bar(done: u64, total: u64) -> String {
    let ratio = if total == 0 { 1.0 } else { (done as f64 / total as f64).min(1.0) };
    let filled = (ratio * BAR_WIDTH as f64).round() as usize;
    format!("▕{}{}▏ {:>3}%", "█".repeat(filled), "░".repeat(BAR_WIDTH - filled), (ratio * 100.0).round() as u64)
}

fn section(title: String, theme: &Theme) -> LineItem {
    LineItem::plain(Line::styled(title, theme.accent().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)))
}

fn field(label: String, value: impl Into<Span<'static>>, theme: &Theme) -> LineItem {
    LineItem::plain(Line::from(vec![Span::styled(format!("  {label:<16}"), theme.accent()), value.into()]))
}

fn link(prefix: String, href: &str, ctx: &Ctx) -> LineItem {
    LineItem {
        line: Line::from(vec![Span::raw(format!("  {prefix}")), Span::raw(format::href(href, ctx.names))]),
        href: Some(href.to_owned()),
    }
}

fn build_lines(task: &Value, children: &[Value], ctx: &Ctx) -> Vec<LineItem> {
    let theme = ctx.theme;
    let s = |k: &str| task.str_field(k);
    let mut out = Vec::new();
    let state = s("state").unwrap_or_default();
    out.push(section(t!("task.overview").into_owned(), theme));
    out.push(field(t!("task.name").into_owned(), s("name").unwrap_or_default().to_owned(), theme));
    out.push(field(
        t!("task.state").into_owned(),
        Span::styled(state.to_owned(), theme.task_state(state).add_modifier(Modifier::BOLD)),
        theme,
    ));
    for (label, key) in
        [("task.created", "pulp_created"), ("task.started", "started_at"), ("task.finished", "finished_at")]
    {
        if let Some(ts) = s(key) {
            out.push(field(t!(label).into_owned(), format::local_time(ts), theme));
        }
    }
    out.push(field(
        t!("task.duration").into_owned(),
        format::elapsed(s("started_at"), s("finished_at"), ctx.now),
        theme,
    ));
    if let Some(cid) = s("logging_cid") {
        out.push(field(t!("task.cid").into_owned(), cid.to_owned(), theme));
    }
    for (label, key) in [
        ("task.worker", "worker"),
        ("task.parent", "parent_task"),
        ("task.group", "task_group"),
        ("task.created_by", "created_by"),
    ] {
        if let Some(h) = s(key) {
            out.push(link(format!("{:<16}", t!(label)), h, ctx));
        }
    }

    let reports = task.get("progress_reports").and_then(Value::as_array).filter(|r| !r.is_empty());
    if let Some(reports) = reports {
        out.push(LineItem::plain(""));
        out.push(section(t!("task.progress").into_owned(), theme));
        for r in reports {
            let done = r.get("done").and_then(Value::as_u64).unwrap_or_default();
            let total = r.get("total").and_then(Value::as_u64);
            let rstate = r.str_field("state").unwrap_or_default();
            let message = r.str_field("message").unwrap_or_default();
            let mut spans = vec![Span::raw("  ")];
            match total {
                Some(total) => {
                    spans.push(Span::styled(progress_bar(done, total), theme.task_state(rstate)));
                    spans.push(Span::raw(format!(" {message}  {done}/{total}")));
                }
                None => spans.push(Span::raw(format!("{message}  {done}"))),
            }
            spans.push(Span::styled(format!("  ({rstate})"), theme.muted()));
            out.push(LineItem::plain(Line::from(spans)));
        }
    }

    for (title, key) in
        [("task.created_resources", "created_resources"), ("task.reserved", "reserved_resources_record")]
    {
        let Some(items) = task.get(key).and_then(Value::as_array).filter(|a| !a.is_empty()) else { continue };
        out.push(LineItem::plain(""));
        out.push(section(t!(title).into_owned(), theme));
        for item in items.iter().filter_map(Value::as_str) {
            let target = item.strip_prefix("shared:").unwrap_or(item);
            let prefix = if item.starts_with("shared:") { format!("{} ", t!("task.shared")) } else { String::new() };
            if is_href(target) {
                out.push(link(prefix, target, ctx));
            } else {
                let label = format::resource_record(target, ctx.names);
                let raw = if label == target { String::new() } else { format!("  ({target})") };
                out.push(LineItem::plain(Line::from(vec![
                    Span::raw(format!("  {prefix}{label}")),
                    Span::styled(raw, theme.muted()),
                ])));
            }
        }
    }

    if !children.is_empty() {
        out.push(LineItem::plain(""));
        out.push(section(t!("task.children").into_owned(), theme));
        for child in children {
            let cstate = child.str_field("state").unwrap_or_default();
            out.push(LineItem {
                line: Line::from(vec![
                    Span::styled(format!("  {cstate:<10}"), theme.task_state(cstate)),
                    Span::raw(child.str_field("name").map(format::task_name).unwrap_or_default()),
                ]),
                href: child.href().map(str::to_owned),
            });
        }
    }

    if let Some(err) = task.get("error").filter(|e| !e.is_null()) {
        out.push(LineItem::plain(""));
        out.push(section(t!("task.error").into_owned(), theme));
        let description = err.str_field("description").map(str::to_owned).unwrap_or_else(|| err.to_string());
        for l in description.lines() {
            out.push(LineItem::plain(Line::styled(format!("  {l}"), theme.error())));
        }
        if let Some(tb) = err.str_field("traceback") {
            for l in tb.lines() {
                out.push(LineItem::plain(Line::styled(format!("  {l}"), Style::new().fg(theme.muted))));
            }
        }
    }
    out
}

impl View for TaskView {
    fn title(&self) -> String {
        self.task
            .as_ref()
            .and_then(|t| t.str_field("name"))
            .map(format::task_name)
            .unwrap_or_else(|| format::short_href(&self.href))
    }

    fn info(&self, _ctx: &Ctx) -> String {
        self.state().unwrap_or_default().to_owned()
    }

    fn hints(&self, ctx: &Ctx) -> Vec<Hint> {
        let mut h = vec![Hint::new("↵", t!("hint.follow")), Hint::new("d", t!("hint.describe"))];
        if !ctx.readonly() && self.state().is_some_and(is_active) {
            h.push(Hint::new("ctrl-k", t!("hint.cancel_task")));
        }
        h.push(Hint::new("esc", t!("hint.close")));
        h
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome {
        if self.lines.handle_key(key) {
            return Outcome::None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter => match self.lines.selected_href().and_then(|h| open_href(h, ctx)) {
                Some(v) => Outcome::Push(v),
                None => Outcome::None,
            },
            KeyCode::Char('k') if ctrl && self.state().is_some_and(is_active) => {
                actions::cancel_tasks(vec![self.href.clone()], ctx)
            }
            KeyCode::Char('d') if !ctrl => Outcome::Push(Box::new(detail::DetailView::new(self.href.clone()))),
            _ => Outcome::Ignored,
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        if let Some(err) = &self.error {
            f.render_widget(Paragraph::new(Line::styled(err.clone(), ctx.theme.error())), area);
            return;
        }
        let Some(task) = &self.task else {
            f.render_widget(Paragraph::new(Line::styled(t!("list.loading").into_owned(), ctx.theme.muted())), area);
            return;
        };
        self.lines.set(build_lines(task, &self.children, ctx));
        self.lines.render(f, area, ctx);
    }

    fn fetch(&mut self, _kind: FetchKind, ctx: &Ctx) -> Option<Fetch> {
        let client = ctx.client()?.clone();
        let href = self.href.clone();
        let tasks = client.api_path("tasks/");
        Some(
            async move {
                let task = client.get(&href, &[]).await?;
                let query = [
                    ("parent_task".to_owned(), href.clone()),
                    ("fields".to_owned(), "pulp_href,name,state".to_owned()),
                    ("limit".to_owned(), "200".to_owned()),
                ];
                let children = client.list(&tasks, &query).await.map(|p| p.results).unwrap_or_default();
                Ok(Payload::Task { task, children })
            }
            .boxed(),
        )
    }

    fn on_data(&mut self, _kind: FetchKind, result: Result<Payload, Error>, ctx: &Ctx) -> Outcome {
        match result {
            Ok(Payload::Task { task, children }) => {
                let first = self.task.is_none();
                let resolve = if first { detail::unresolved_hrefs(&task, ctx.names) } else { Vec::new() };
                self.task = Some(task);
                self.children = children;
                self.error = None;
                if resolve.is_empty() { Outcome::None } else { Outcome::Resolve(resolve) }
            }
            Ok(_) => Outcome::None,
            Err(e) => {
                self.error = Some(super::error_text(&e));
                Outcome::None
            }
        }
    }

    fn poll_interval(&self, _default: Duration) -> Option<Duration> {
        match self.state() {
            None => Some(Duration::from_secs(1)),
            Some(s) if is_active(s) => Some(Duration::from_secs(1)),
            Some(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_bounds() {
        assert!(progress_bar(0, 10).ends_with("  0%"));
        assert!(progress_bar(5, 10).ends_with(" 50%"));
        assert!(progress_bar(15, 10).ends_with("100%"), "overshoot is clamped");
        assert!(progress_bar(0, 0).ends_with("100%"), "empty total counts as done");
    }
}
