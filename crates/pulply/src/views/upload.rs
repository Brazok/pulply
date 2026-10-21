//! Upload (R9): local file picker, then chunked uploads turned into content units
//! (optionally added to a repository), with per-file progress.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};
use pulply_api::{Registry, ResourceType};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use rust_i18n::t;

use super::task::progress_bar;
use super::{Ctx, Hint, Outcome, View};
use crate::format;
use crate::labels;

/// Content types of `plugin` whose create endpoint accepts an `upload`.
pub fn upload_types<'a>(plugin: &str, reg: &'a Registry) -> Vec<&'a ResourceType> {
    let mut types: Vec<&ResourceType> = reg
        .types()
        .iter()
        .filter(|t| t.category == "content" && t.plugin == plugin && t.ops.create)
        .filter(|t| {
            t.create_schema.as_deref().and_then(|s| reg.schema(s)).is_some_and(|s| s.properties.contains_key("upload"))
        })
        .collect();
    // Main artifact types first: what people upload most.
    types.sort_by_key(|t| {
        (!matches!(t.name.as_str(), "packages" | "files" | "collection_versions" | "gem"), t.name.clone())
    });
    types
}

/// Everything needed to run an upload batch.
#[derive(Debug, Clone)]
pub struct UploadJob {
    pub files: Vec<PathBuf>,
    /// Content create endpoint.
    pub create_path: String,
    /// The endpoint requires `relative_path` (file name is used).
    pub relative_path: bool,
    /// Repository receiving the new content units.
    pub repository: Option<String>,
}

impl UploadJob {
    /// Form fields creating content from upload `href` of `file`.
    pub fn create_fields(&self, file: &Path, upload_href: &str) -> Vec<(String, String)> {
        let mut fields = vec![("upload".to_owned(), upload_href.to_owned())];
        if self.relative_path {
            let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            fields.push(("relative_path".to_owned(), name));
        }
        if let Some(repo) = &self.repository {
            fields.push(("repository".to_owned(), repo.clone()));
        }
        fields
    }
}

struct Entry {
    name: String,
    dir: bool,
    size: u64,
}

pub struct FilePickerView {
    dir: PathBuf,
    entries: Vec<Entry>,
    state: ListState,
    marked: BTreeSet<PathBuf>,
    filter: String,
    show_hidden: bool,
    error: Option<String>,
    /// Upload-capable content type ids, cycled with `t`.
    types: Vec<String>,
    type_idx: usize,
    repository: Option<String>,
}

impl FilePickerView {
    pub fn new(types: Vec<String>, repository: Option<String>) -> Self {
        let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let mut view = Self {
            dir,
            entries: Vec::new(),
            state: ListState::default().with_selected(Some(0)),
            marked: BTreeSet::new(),
            filter: String::new(),
            show_hidden: false,
            error: None,
            types,
            type_idx: 0,
            repository,
        };
        view.read_dir();
        view
    }

    fn read_dir(&mut self) {
        self.error = None;
        let mut entries: Vec<Entry> = match std::fs::read_dir(&self.dir) {
            Ok(rd) => rd
                .filter_map(Result::ok)
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let meta = e.metadata().ok()?;
                    Some(Entry { name, dir: meta.is_dir(), size: meta.len() })
                })
                .filter(|e| self.show_hidden || !e.name.starts_with('.'))
                .collect(),
            Err(e) => {
                self.error = Some(e.to_string());
                Vec::new()
            }
        };
        entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.cmp(&b.name)));
        self.entries = entries;
        self.state.select(Some(0));
    }

    fn visible(&self) -> Vec<&Entry> {
        let needle = self.filter.to_lowercase();
        self.entries.iter().filter(|e| needle.is_empty() || e.name.to_lowercase().contains(&needle)).collect()
    }

    fn current(&self) -> Option<&Entry> {
        self.visible().get(self.state.selected().unwrap_or(0)).copied()
    }

    fn submit(&self, ctx: &Ctx) -> Outcome {
        if ctx.readonly() {
            return Outcome::readonly();
        }
        let files: Vec<PathBuf> = if self.marked.is_empty() {
            match self.current() {
                Some(e) if !e.dir => vec![self.dir.join(&e.name)],
                _ => return Outcome::None,
            }
        } else {
            self.marked.iter().cloned().collect()
        };
        let Some(reg) = ctx.registry() else { return Outcome::None };
        let Some(t) = self.types.get(self.type_idx).and_then(|id| reg.get(id)) else { return Outcome::None };
        let schema = t.create_schema.as_deref().and_then(|s| reg.schema(s));
        let has = |f: &str| schema.is_some_and(|s| s.properties.contains_key(f));
        let job = UploadJob {
            files,
            create_path: t.list_path.clone().unwrap_or_default(),
            relative_path: has("relative_path"),
            repository: self.repository.clone().filter(|_| has("repository")),
        };
        Outcome::Batch(vec![Outcome::Pop, Outcome::Upload(job)])
    }
}

impl View for FilePickerView {
    fn title(&self) -> String {
        t!("upload.pick_title").into_owned()
    }

    fn info(&self, _ctx: &Ctx) -> String {
        if self.marked.is_empty() { String::new() } else { t!("upload.marked", count = self.marked.len()).into_owned() }
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        let mut h = vec![
            Hint::new("↵", t!("hint.upload")),
            Hint::new("space", t!("hint.mark")),
            Hint::new("⌫", t!("hint.parent")),
            Hint::new(".", t!("hint.hidden")),
        ];
        if self.types.len() > 1 {
            h.push(Hint::new("t", t!("hint.content_type")));
        }
        h.push(Hint::new("esc", t!("hint.cancel")));
        h
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome {
        let len = self.visible().len();
        let cur = self.state.selected().unwrap_or(0);
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.state.select(Some((cur + 1).min(len.saturating_sub(1)))),
            KeyCode::Up | KeyCode::Char('k') => self.state.select(Some(cur.saturating_sub(1))),
            KeyCode::Home | KeyCode::Char('g') => self.state.select(Some(0)),
            KeyCode::End | KeyCode::Char('G') => self.state.select(Some(len.saturating_sub(1))),
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
                if let Some(parent) = self.dir.parent() {
                    self.dir = parent.to_path_buf();
                    self.filter.clear();
                    self.read_dir();
                }
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => match self.current() {
                Some(e) if e.dir => {
                    self.dir = self.dir.join(&e.name);
                    self.filter.clear();
                    self.read_dir();
                }
                _ if key.code == KeyCode::Enter => return self.submit(ctx),
                _ => {}
            },
            KeyCode::Char(' ') => {
                if let Some(e) = self.current().filter(|e| !e.dir) {
                    let path = self.dir.join(&e.name);
                    if !self.marked.remove(&path) {
                        self.marked.insert(path);
                    }
                }
                self.state.select(Some((cur + 1).min(len.saturating_sub(1))));
            }
            KeyCode::Char('.') => {
                self.show_hidden = !self.show_hidden;
                self.read_dir();
            }
            KeyCode::Char('t') if !self.types.is_empty() => self.type_idx = (self.type_idx + 1) % self.types.len(),
            KeyCode::Esc if !self.filter.is_empty() => self.filter.clear(),
            _ => return Outcome::Ignored,
        }
        Outcome::None
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let theme = ctx.theme;
        let [head, body, foot] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(1), Constraint::Length(1)]).areas(area);
        let reg = ctx.registry();
        let type_label = self
            .types
            .get(self.type_idx)
            .and_then(|id| reg.and_then(|r| r.get(id)))
            .map(labels::type_title)
            .unwrap_or_else(|| "–".into());
        let target = self.repository.as_deref().map(|r| format::href(r, ctx.names)).unwrap_or_else(|| "–".into());
        let label = |k: &str| Span::styled(format!("{:<14}", labels::tr(k)), theme.accent());
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![label("upload.content_type"), Span::raw(type_label)]),
                Line::from(vec![label("upload.repository"), Span::raw(target)]),
                Line::from(vec![label("upload.directory"), Span::raw(self.dir.display().to_string())]),
            ]),
            head,
        );
        let items: Vec<ListItem> = self
            .visible()
            .iter()
            .map(|e| {
                let path = self.dir.join(&e.name);
                let mark = if self.marked.contains(&path) { "● " } else { "  " };
                if e.dir {
                    ListItem::new(Line::styled(format!("{mark}{}/", e.name), theme.accent()))
                } else {
                    ListItem::new(Line::from(vec![
                        Span::raw(format!("{mark}{:<50} ", e.name)),
                        Span::styled(format::bytes(e.size), theme.muted()),
                    ]))
                }
            })
            .collect();
        f.render_stateful_widget(List::new(items).highlight_style(theme.selected()), body, &mut self.state);
        let footer = match &self.error {
            Some(e) => Line::styled(e.clone(), theme.error()),
            None => Line::styled(
                t!("upload.chunk", size = format::bytes(ctx.config.upload_chunk_size as u64)).into_owned(),
                theme.muted(),
            ),
        };
        f.render_widget(Paragraph::new(footer), foot);
    }

    fn poll_interval(&self, _default: Duration) -> Option<Duration> {
        None
    }

    fn filterable(&self) -> bool {
        true
    }

    fn filter(&self) -> &str {
        &self.filter
    }

    fn set_filter(&mut self, input: &str, _commit: bool) -> Outcome {
        self.filter = input.trim().to_owned();
        self.state.select(Some(0));
        Outcome::None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Queued,
    Uploading,
    Creating,
    Done,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct FileProgress {
    pub name: String,
    pub sent: u64,
    pub total: u64,
    pub phase: Phase,
    pub started: Option<Instant>,
    pub finished: Option<Instant>,
}

/// Progress shared between the upload task (writer) and [`UploadView`] (reader).
pub type SharedProgress = Arc<Mutex<Vec<FileProgress>>>;

pub fn shared_progress(files: &[PathBuf]) -> SharedProgress {
    Arc::new(Mutex::new(
        files
            .iter()
            .map(|p| FileProgress {
                name: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                sent: 0,
                total: 0,
                phase: Phase::Queued,
                started: None,
                finished: None,
            })
            .collect(),
    ))
}

pub struct UploadView {
    progress: SharedProgress,
}

impl UploadView {
    pub fn new(progress: SharedProgress) -> Self {
        Self { progress }
    }
}

impl View for UploadView {
    fn title(&self) -> String {
        t!("upload.title").into_owned()
    }

    fn info(&self, _ctx: &Ctx) -> String {
        let files = self.progress.lock().expect("upload progress poisoned");
        let done = files.iter().filter(|f| f.phase == Phase::Done).count();
        format!("{done}/{}", files.len())
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        vec![Hint::new("esc", t!("hint.background"))]
    }

    fn handle_key(&mut self, _key: KeyEvent, _ctx: &Ctx) -> Outcome {
        Outcome::Ignored
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let theme = ctx.theme;
        let files = self.progress.lock().expect("upload progress poisoned").clone();
        let lines: Vec<Line> = files
            .iter()
            .map(|p| {
                let (status, style) = match &p.phase {
                    Phase::Queued => (t!("upload.queued").into_owned(), theme.muted()),
                    Phase::Uploading => (t!("upload.uploading").into_owned(), theme.task_state("running")),
                    Phase::Creating => (t!("upload.creating").into_owned(), theme.task_state("running")),
                    Phase::Done => (t!("upload.done").into_owned(), theme.success()),
                    Phase::Failed(e) => (format!("{}: {e}", t!("upload.failed")), theme.error()),
                };
                let elapsed = p.started.map(|s| p.finished.unwrap_or_else(Instant::now) - s).unwrap_or_default();
                let speed = if elapsed.as_secs_f64() > 0.0 {
                    format!("{}/s", format::bytes((p.sent as f64 / elapsed.as_secs_f64()) as u64))
                } else {
                    String::new()
                };
                Line::from(vec![
                    Span::raw(format!("{:<36} ", p.name.chars().take(36).collect::<String>())),
                    // Size is unknown until the upload starts: empty bar, not 0/0 = 100%.
                    Span::styled(
                        match (p.total, &p.phase) {
                            (_, Phase::Done) => progress_bar(1, 1),
                            (0, _) => progress_bar(0, 1),
                            (total, _) => progress_bar(p.sent, total),
                        },
                        style,
                    ),
                    Span::raw(format!(" {:>10} {:>12}  ", format::bytes(p.total), speed)),
                    Span::styled(status, style),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(lines).style(Style::new()), area);
    }

    fn poll_interval(&self, _default: Duration) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn upload_capable_types_put_main_artifacts_first() {
        let reg = testutil::registry();
        let ids = |p: &str| upload_types(p, &reg).iter().map(|t| t.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids("rpm")[0], "content/rpm/packages");
        assert!(ids("rpm").contains(&"content/rpm/advisories".to_owned()));
        assert_eq!(ids("file"), ["content/file/files"]);
        assert!(ids("container").is_empty(), "container content is pushed, not uploaded");
    }

    #[test]
    fn create_fields_include_relative_path_and_repository_when_needed() {
        let job = UploadJob {
            files: vec![],
            create_path: "/c/".into(),
            relative_path: true,
            repository: Some("/repo/".into()),
        };
        let fields = job.create_fields(Path::new("/tmp/a/b.iso"), "/up/1/");
        assert_eq!(
            fields,
            [
                ("upload".into(), "/up/1/".into()),
                ("relative_path".into(), "b.iso".into()),
                ("repository".into(), "/repo/".into())
            ]
        );
        let plain = UploadJob { relative_path: false, repository: None, ..job };
        assert_eq!(plain.create_fields(Path::new("x.rpm"), "/up/2/"), [("upload".to_owned(), "/up/2/".to_owned())]);
    }
}
