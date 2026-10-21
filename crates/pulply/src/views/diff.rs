//! Diff of two repository versions (R10): per-type summary, then the added and
//! removed units of the selected type, loaded on demand.

use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};
use futures::FutureExt;
use futures::future::try_join_all;
use pulply_api::models::version_number;
use pulply_api::{Error, PulpClient, ResourceExt};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, TableState};
use rust_i18n::t;
use serde_json::Value;

use super::columns::nevra;
use super::lines::{LineItem, LineList};
use super::{Ctx, Fetch, FetchKind, Hint, Outcome, Payload, View, open_href};
use crate::format;
use crate::names::NameCache;

const PAGE: usize = 1000;
/// Units listed per side in the detail pane.
const DETAIL_LIMIT: usize = 200;

#[derive(Debug, Clone, Default)]
pub struct DiffRow {
    /// Pulp content type (`rpm.package`).
    pub ty: String,
    /// Typed content list path (without query).
    pub list_path: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// Consecutive mode: counts from `content_summary`, hrefs fetched on demand.
    pub added_count: u64,
    pub removed_count: u64,
}

#[derive(Debug, Clone)]
pub struct DiffData {
    pub older: String,
    pub newer: String,
    /// `newer` was created from `older`: its summary and `repository_version_added/removed` apply.
    pub consecutive: bool,
    pub rows: Vec<DiffRow>,
}

/// One unit of the detail pane.
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub added: bool,
    pub label: String,
    pub href: String,
}

pub struct DiffView {
    a: String,
    b: String,
    data: Option<DiffData>,
    state: TableState,
    detail: LineList,
    detail_for: Option<usize>,
    loading_detail: bool,
    error: Option<String>,
}

impl DiffView {
    /// Diff of two versions of the same repository, in any order.
    pub fn new(a: String, b: String) -> Self {
        Self {
            a,
            b,
            data: None,
            state: TableState::default().with_selected(Some(0)),
            detail: LineList::default(),
            detail_for: None,
            loading_detail: false,
            error: None,
        }
    }

    /// Version this one is compared to by default: its base version, else number − 1.
    pub fn previous_of(version: &Value) -> Option<String> {
        if let Some(base) = version.str_field("base_version") {
            return Some(base.to_owned());
        }
        let href = version.href()?;
        previous_href(href)
    }
}

/// `…/versions/N/` → `…/versions/N-1/`.
pub fn previous_href(href: &str) -> Option<String> {
    let n = version_number(href)?.checked_sub(1)?;
    let repo = pulply_api::models::version_repository(href)?;
    Some(format!("{repo}versions/{n}/"))
}

/// Path part of a summary href (`/pulp/api/v3/content/file/files/?repository_version=…`).
fn path_of(href: &str) -> String {
    href.split('?').next().unwrap_or(href).to_owned()
}

/// Every `pulp_href` of `list_path` matching `filter=value`.
async fn all_hrefs(client: &PulpClient, list_path: &str, filter: &str, value: &str) -> Result<HashSet<String>, Error> {
    let mut out = HashSet::new();
    loop {
        let query = vec![
            (filter.to_owned(), value.to_owned()),
            ("fields".to_owned(), "pulp_href".to_owned()),
            ("limit".to_owned(), PAGE.to_string()),
            ("offset".to_owned(), out.len().to_string()),
        ];
        let page = client.list(list_path, &query).await?;
        let before = out.len();
        out.extend(page.results.iter().filter_map(|r| r.href().map(str::to_owned)));
        if out.len() as u64 >= page.count || out.len() == before {
            return Ok(out);
        }
    }
}

/// Builds the per-type summary of `older` → `newer`.
pub fn summarize(older: &Value, newer: &Value) -> DiffData {
    let (o, n) = (older.href().unwrap_or_default(), newer.href().unwrap_or_default());
    let consecutive = match newer.str_field("base_version") {
        Some(base) => base == o,
        None => version_number(n).zip(version_number(o)).is_some_and(|(nb, ob)| nb == ob + 1),
    };
    let section =
        |v: &Value, key: &str| v.get("content_summary").and_then(|s| s.get(key)).and_then(Value::as_object).cloned();
    let mut rows: BTreeMap<String, DiffRow> = BTreeMap::new();
    if consecutive {
        for (key, counted) in [("added", true), ("removed", false)] {
            for (ty, entry) in section(newer, key).unwrap_or_default() {
                let row = rows.entry(ty.clone()).or_insert_with(|| DiffRow { ty: ty.clone(), ..DiffRow::default() });
                let count = entry.get("count").and_then(Value::as_u64).unwrap_or_default();
                if row.list_path.is_empty() {
                    row.list_path = entry.get("href").and_then(Value::as_str).map(path_of).unwrap_or_default();
                }
                if counted { row.added_count = count } else { row.removed_count = count }
            }
        }
    } else {
        for v in [older, newer] {
            for (ty, entry) in section(v, "present").unwrap_or_default() {
                let path = entry.get("href").and_then(Value::as_str).map(path_of).unwrap_or_default();
                rows.entry(ty.clone()).or_insert(DiffRow { ty, list_path: path, ..DiffRow::default() });
            }
        }
    }
    DiffData { older: o.to_owned(), newer: n.to_owned(), consecutive, rows: rows.into_values().collect() }
}

/// Fills `added`/`removed` of every row by set difference (arbitrary versions).
async fn compute_sets(client: &PulpClient, data: &mut DiffData) -> Result<(), Error> {
    let lists: Vec<_> = data
        .rows
        .iter()
        .map(|row| {
            let (path, older, newer) = (row.list_path.clone(), data.older.clone(), data.newer.clone());
            async move {
                let (a, b) = futures::try_join!(
                    all_hrefs(client, &path, "repository_version", &older),
                    all_hrefs(client, &path, "repository_version", &newer),
                )?;
                Ok::<_, Error>((a, b))
            }
        })
        .collect();
    let sets = try_join_all(lists).await?;
    for (row, (a, b)) in data.rows.iter_mut().zip(sets) {
        row.added = b.difference(&a).cloned().collect();
        row.removed = a.difference(&b).cloned().collect();
        row.added.sort();
        row.removed.sort();
        row.added_count = row.added.len() as u64;
        row.removed_count = row.removed.len() as u64;
    }
    data.rows.retain(|r| r.added_count + r.removed_count > 0);
    Ok(())
}

/// Readable label of a content unit: NEVRA for RPM-like packages, else its name.
fn unit_label(v: &Value) -> String {
    if v.get("release").is_some() && v.get("arch").is_some() {
        return nevra(v);
    }
    let base =
        NameCache::label(v).map(str::to_owned).unwrap_or_else(|| format::short_href(v.href().unwrap_or_default()));
    match v.str_field("version") {
        Some(ver) => format!("{base} {ver}"),
        None => base,
    }
}

impl View for DiffView {
    fn title(&self) -> String {
        t!("diff.title").into_owned()
    }

    fn info(&self, ctx: &Ctx) -> String {
        match &self.data {
            Some(d) => format!("{} → {}", format::href(&d.older, ctx.names), format::href(&d.newer, ctx.names)),
            None => String::new(),
        }
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        vec![Hint::new("↵", t!("hint.diff_units")), Hint::new("tab", t!("hint.diff_pane"))]
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome {
        let in_detail = self.detail_for.is_some() && self.detail_for == self.state.selected() && !self.loading_detail;
        if in_detail && self.detail.handle_key(key) {
            return Outcome::None;
        }
        let len = self.data.as_ref().map_or(0, |d| d.rows.len());
        let cur = self.state.selected().unwrap_or(0);
        match key.code {
            KeyCode::Char('J') | KeyCode::Tab => self.state.select(Some((cur + 1).min(len.saturating_sub(1)))),
            KeyCode::Char('K') | KeyCode::BackTab => self.state.select(Some(cur.saturating_sub(1))),
            KeyCode::Down | KeyCode::Up if !in_detail => {
                let next = if key.code == KeyCode::Down {
                    (cur + 1).min(len.saturating_sub(1))
                } else {
                    cur.saturating_sub(1)
                };
                self.state.select(Some(next));
            }
            KeyCode::Enter if in_detail => {
                return match self.detail.selected_href().and_then(|h| open_href(h, ctx)) {
                    Some(v) => Outcome::Push(v),
                    None => Outcome::None,
                };
            }
            KeyCode::Enter if len > 0 => {
                self.detail_for = Some(cur);
                self.loading_detail = true;
                self.detail.set(Vec::new());
                return Outcome::Fetch(FetchKind::More);
            }
            _ => return Outcome::Ignored,
        }
        Outcome::None
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let theme = ctx.theme;
        if let Some(err) = &self.error {
            f.render_widget(Paragraph::new(Line::styled(err.clone(), theme.error())), area);
            return;
        }
        let Some(data) = &self.data else {
            f.render_widget(Paragraph::new(Line::styled(t!("diff.computing").into_owned(), theme.muted())), area);
            return;
        };
        let table_h = (data.rows.len() as u16 + 2).min(area.height / 2).max(3);
        let [head, table, detail] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(table_h), Constraint::Min(1)]).areas(area);
        let mode = if data.consecutive { t!("diff.consecutive") } else { t!("diff.sets") };
        f.render_widget(Paragraph::new(Line::styled(mode.into_owned(), theme.muted())), head);
        if data.rows.is_empty() {
            f.render_widget(Paragraph::new(Line::styled(t!("diff.identical").into_owned(), theme.success())), table);
            return;
        }
        let bold = theme.accent().add_modifier(Modifier::BOLD);
        let header = Row::new(
            ["summary.type", "summary.added", "summary.removed"]
                .map(|k| Cell::from(Span::styled(crate::labels::tr(k), bold))),
        );
        let rows = data.rows.iter().map(|r| {
            Row::new([
                Cell::from(r.ty.clone()),
                Cell::from(Span::styled(format!("+{}", r.added_count), theme.success())),
                Cell::from(Span::styled(format!("-{}", r.removed_count), theme.error())),
            ])
        });
        let widths = [Constraint::Min(30), Constraint::Length(10), Constraint::Length(10)];
        f.render_stateful_widget(
            Table::new(rows, widths).header(header).row_highlight_style(theme.selected()),
            table,
            &mut self.state,
        );
        if self.loading_detail {
            f.render_widget(Paragraph::new(Line::styled(t!("list.loading").into_owned(), theme.muted())), detail);
        } else if self.detail_for.is_some() {
            self.detail.render(f, detail, ctx);
        } else {
            f.render_widget(Paragraph::new(Line::styled(t!("diff.select").into_owned(), theme.muted())), detail);
        }
    }

    fn fetch(&mut self, kind: FetchKind, ctx: &Ctx) -> Option<Fetch> {
        let client = ctx.client()?.clone();
        match kind {
            FetchKind::Refresh => {
                if self.data.is_some() {
                    return None; // versions are immutable: compute once
                }
                let (a, b) = (self.a.clone(), self.b.clone());
                Some(
                    async move {
                        let (va, vb) = futures::try_join!(client.get(&a, &[]), client.get(&b, &[]))?;
                        let na = va.get("number").and_then(Value::as_u64).unwrap_or_default();
                        let nb = vb.get("number").and_then(Value::as_u64).unwrap_or_default();
                        let (older, newer) = if na <= nb { (va, vb) } else { (vb, va) };
                        let mut data = summarize(&older, &newer);
                        if !data.consecutive {
                            compute_sets(&client, &mut data).await?;
                        }
                        Ok(Payload::Diff(Box::new(data)))
                    }
                    .boxed(),
                )
            }
            FetchKind::More => {
                let data = self.data.as_ref()?;
                let row = data.rows.get(self.detail_for?)?.clone();
                let (consecutive, newer) = (data.consecutive, data.newer.clone());
                Some(
                    async move {
                        let mut lines = Vec::new();
                        for (added, hrefs, filter) in [
                            (true, &row.added, "repository_version_added"),
                            (false, &row.removed, "repository_version_removed"),
                        ] {
                            let units = if consecutive {
                                let query = vec![
                                    (filter.to_owned(), newer.clone()),
                                    ("limit".to_owned(), DETAIL_LIMIT.to_string()),
                                ];
                                client.list(&row.list_path, &query).await?.results
                            } else if hrefs.is_empty() {
                                Vec::new()
                            } else {
                                let batch: Vec<&str> = hrefs.iter().take(DETAIL_LIMIT).map(String::as_str).collect();
                                let query = vec![
                                    ("pulp_href__in".to_owned(), batch.join(",")),
                                    ("limit".to_owned(), DETAIL_LIMIT.to_string()),
                                ];
                                client.list(&row.list_path, &query).await?.results
                            };
                            lines.extend(units.iter().map(|u| DiffLine {
                                added,
                                label: unit_label(u),
                                href: u.href().unwrap_or_default().to_owned(),
                            }));
                        }
                        Ok(Payload::DiffDetail(lines))
                    }
                    .boxed(),
                )
            }
        }
    }

    fn on_data(&mut self, kind: FetchKind, result: Result<Payload, Error>, ctx: &Ctx) -> Outcome {
        match (kind, result) {
            (_, Err(e)) => {
                self.loading_detail = false;
                self.error = Some(super::error_text(&e));
            }
            (_, Ok(Payload::Diff(data))) => self.data = Some(*data),
            (_, Ok(Payload::DiffDetail(lines))) => {
                self.loading_detail = false;
                let theme = ctx.theme;
                let row = self.detail_for.and_then(|i| self.data.as_ref()?.rows.get(i));
                let shown_added = lines.iter().filter(|l| l.added).count() as u64;
                let shown_removed = lines.len() as u64 - shown_added;
                let mut items: Vec<LineItem> = lines
                    .into_iter()
                    .map(|l| {
                        let (sign, style) = if l.added { ("+", theme.success()) } else { ("-", theme.error()) };
                        LineItem { line: Line::styled(format!("{sign} {}", l.label), style), href: Some(l.href) }
                    })
                    .collect();
                if let Some(r) = row {
                    let hidden = (r.added_count + r.removed_count).saturating_sub(shown_added + shown_removed);
                    if hidden > 0 {
                        items.push(LineItem::plain(Line::styled(
                            t!("diff.more", count = hidden).into_owned(),
                            theme.muted(),
                        )));
                    }
                }
                self.detail.set(items);
            }
            _ => {}
        }
        Outcome::None
    }

    fn poll_interval(&self, _default: Duration) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const REPO: &str = "/pulp/api/v3/repositories/file/file/r1/";

    fn version(n: u64, base: Option<u64>, summary: Value) -> Value {
        json!({
            "pulp_href": format!("{REPO}versions/{n}/"),
            "number": n,
            "base_version": base.map(|b| format!("{REPO}versions/{b}/")),
            "content_summary": summary,
        })
    }

    #[test]
    fn consecutive_versions_use_the_newer_summary() {
        let v1 = version(1, None, json!({"present": {}}));
        let v2 = version(
            2,
            None,
            json!({
                "added": {"file.file": {"count": 2, "href": "/pulp/api/v3/content/file/files/?repository_version_added=x"}},
                "removed": {"file.file": {"count": 1, "href": "/pulp/api/v3/content/file/files/?repository_version_removed=x"}},
                "present": {}
            }),
        );
        let d = summarize(&v1, &v2);
        assert!(d.consecutive);
        assert_eq!(d.rows.len(), 1);
        assert_eq!((d.rows[0].added_count, d.rows[0].removed_count), (2, 1));
        assert_eq!(d.rows[0].list_path, "/pulp/api/v3/content/file/files/");
    }

    #[test]
    fn rollback_versions_are_not_consecutive_with_their_predecessor() {
        // v3 was created from v1 (rollback): v2 → v3 needs a set difference.
        let present = json!({"present": {"file.file": {"count": 3, "href": "/pulp/api/v3/content/file/files/?repository_version=y"}}});
        let d = summarize(&version(2, None, present.clone()), &version(3, Some(1), present.clone()));
        assert!(!d.consecutive);
        assert_eq!(d.rows[0].ty, "file.file");
        assert!(summarize(&version(1, None, present.clone()), &version(3, Some(1), present)).consecutive);
    }

    #[test]
    fn previous_version_prefers_base_version() {
        assert_eq!(previous_href(&format!("{REPO}versions/4/")), Some(format!("{REPO}versions/3/")));
        assert_eq!(previous_href(&format!("{REPO}versions/0/")), None);
        let v = version(5, Some(2), json!({}));
        assert_eq!(DiffView::previous_of(&v), Some(format!("{REPO}versions/2/")));
    }
}
