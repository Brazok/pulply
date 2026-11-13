//! Forms generated from OpenAPI request schemas: create, edit (PATCH of changed
//! fields only) and actions (sync, publish, distribute, global operations).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures::FutureExt;
use pulply_api::registry::{Property, Schema};
use pulply_api::{Error, Method, Registry, ResourceType};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};
use rust_i18n::t;
use serde_json::{Map, Value};

use super::confirm::confirm;
use super::list::ListView;
use super::{AfterTask, Ctx, Fetch, FetchKind, Hint, Mutation, Outcome, Payload, View};
use crate::format;
use crate::labels;

const LABEL_WIDTH: usize = 26;

#[derive(Debug, Clone, PartialEq)]
pub enum FieldKind {
    Text,
    /// `writeOnly` string (passwords, keys): masked.
    Secret,
    Integer,
    Number,
    Bool,
    Enum(Vec<String>),
    /// Comma-separated values; allowed values listed when the items are an enum.
    List(Vec<String>),
    Json,
    /// Href; `Some(type id)` when a picker can list candidates.
    Relation(Option<String>),
    /// Binary field: path of a local file sent as a multipart part.
    File,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub kind: FieldKind,
    pub required: bool,
    pub nullable: bool,
    pub help: Option<String>,
    pub placeholder: Option<String>,
    pub value: String,
}

impl Field {
    fn is_text(&self) -> bool {
        matches!(
            self.kind,
            FieldKind::Text
                | FieldKind::Secret
                | FieldKind::Integer
                | FieldKind::Number
                | FieldKind::List(_)
                | FieldKind::Json
                | FieldKind::Relation(_)
                | FieldKind::File
        )
    }

    /// Parsed JSON value; `Ok(None)` when empty.
    pub fn json(&self) -> Result<Option<Value>, String> {
        let raw = self.value.trim();
        if raw.is_empty() {
            return Ok(None);
        }
        let v = match &self.kind {
            FieldKind::Text | FieldKind::Secret | FieldKind::Relation(_) | FieldKind::Enum(_) | FieldKind::File => {
                Value::String(raw.to_owned())
            }
            FieldKind::Integer => {
                raw.parse::<i64>().map(Value::from).map_err(|_| t!("form.not_integer").into_owned())?
            }
            FieldKind::Number => raw.parse::<f64>().map(Value::from).map_err(|_| t!("form.not_number").into_owned())?,
            FieldKind::Bool => Value::Bool(raw == "true"),
            FieldKind::List(_) => {
                Value::Array(raw.split(',').map(str::trim).filter(|s| !s.is_empty()).map(Value::from).collect())
            }
            FieldKind::Json => serde_json::from_str(raw).map_err(|e| t!("form.bad_json", error = e).into_owned())?,
        };
        Ok(Some(v))
    }
}

/// Text form of an API value for editing.
fn to_text(v: &Value, kind: &FieldKind) -> String {
    match (v, kind) {
        (Value::Null, _) => String::new(),
        (Value::String(s), _) => s.clone(),
        (Value::Bool(b), _) => b.to_string(),
        (Value::Number(n), _) => n.to_string(),
        (Value::Array(items), FieldKind::List(_)) if items.iter().all(Value::is_string) => {
            items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")
        }
        (other, _) => other.to_string(),
    }
}

/// Picker target for an href field, by naming convention of Pulp serializers.
fn relation_target(field: &str, plugin: &str, reg: &Registry) -> Option<String> {
    let category = match field {
        "remote" => "remotes",
        "repository" => "repositories",
        "publication" => "publications",
        "content_guard" => return reg.get("contentguards").map(|t| t.id.clone()),
        "repository_version" | "base_version" => return reg.get("repository_versions").map(|t| t.id.clone()),
        f if f.ends_with("signing_service") => return reg.get("signing-services").map(|t| t.id.clone()),
        _ => return None,
    };
    let candidates: Vec<&ResourceType> =
        reg.types().iter().filter(|t| t.category == category && t.plugin == plugin && t.parent.is_none()).collect();
    candidates.iter().find(|t| t.name == plugin).or(candidates.first()).map(|t| t.id.clone())
}

/// Writable fields of a request schema, required ones first.
pub fn fields_from_schema(schema: &Schema, reg: &Registry, plugin: &str) -> Vec<Field> {
    let mut fields: Vec<Field> = schema
        .properties
        .iter()
        .filter(|(_, p)| !p.read_only)
        .map(|(name, p)| build_field(name, p, schema, reg, plugin))
        .collect();
    fields.sort_by_key(|f| !f.required);
    fields
}

fn build_field(name: &str, p: &Property, schema: &Schema, reg: &Registry, plugin: &str) -> Field {
    let choices = reg.enum_choices(p);
    let kind = match p.ty.as_deref() {
        _ if p.format.as_deref() == Some("binary") => FieldKind::File,
        Some("array") => FieldKind::List(choices),
        _ if !choices.is_empty() => FieldKind::Enum(choices),
        Some("boolean") => FieldKind::Bool,
        Some("integer") => FieldKind::Integer,
        Some("number") => FieldKind::Number,
        Some("object") => FieldKind::Json,
        _ if p.format.as_deref() == Some("uri") => FieldKind::Relation(relation_target(name, plugin, reg)),
        _ if p.write_only => FieldKind::Secret,
        _ => FieldKind::Text,
    };
    let help = p.description.as_deref().map(|d| d.split("\n\n").next().unwrap_or(d).replace('\n', " "));
    Field {
        name: name.to_owned(),
        placeholder: p.default.as_ref().map(|d| to_text(d, &kind)).filter(|s| !s.is_empty()),
        kind,
        required: schema.required.iter().any(|r| r == name),
        nullable: p.nullable,
        help,
        value: String::new(),
    }
}

/// Local path typed in a file field, with `~/` expanded.
fn local_path(raw: &str) -> PathBuf {
    match raw.strip_prefix("~/").zip(directories::BaseDirs::new()) {
        Some((rest, dirs)) => dirs.home_dir().join(rest),
        None => PathBuf::from(raw),
    }
}

/// List query whose `count` is shown on top of the form, with `key` as message.
#[derive(Clone)]
struct CountQuery {
    path: String,
    query: Vec<(String, String)>,
    key: &'static str,
}

pub struct FormView {
    id: u64,
    title: String,
    method: Method,
    path: String,
    fields: Vec<Field>,
    focus: usize,
    /// Edit mode: resource to load, then compared against to send only changes.
    load: Option<String>,
    original: Option<Value>,
    /// Estimated count shown on top (`?orphaned_for=0` before an orphan cleanup).
    count: Option<CountQuery>,
    intro: Option<String>,
    errors: BTreeMap<String, Vec<String>>,
    general: Option<String>,
    submitting: bool,
    label: String,
    after: AfterTask,
    confirm: Option<String>,
    /// Send as `multipart/form-data` even without files (endpoint refuses JSON).
    multipart: bool,
    /// Title of the view showing a synchronous, non-task response (`import-check/`).
    show_result: Option<String>,
    state: ListState,
}

impl FormView {
    fn new(title: String, label: String, method: Method, path: String, fields: Vec<Field>) -> Self {
        Self {
            id: 0,
            title,
            method,
            path,
            fields,
            focus: 0,
            load: None,
            original: None,
            count: None,
            intro: None,
            errors: BTreeMap::new(),
            general: None,
            submitting: false,
            label,
            after: AfterTask::None,
            confirm: None,
            multipart: false,
            show_result: None,
            state: ListState::default(),
        }
    }

    /// Creation form for a resource type (`c` in a list).
    pub fn create(t: &ResourceType, reg: &Registry) -> Option<Self> {
        let schema = reg.schema(t.create_schema.as_deref()?)?;
        let title = t!("form.create", what = labels::type_title(t)).into_owned();
        let label = t!("action.create", name = labels::type_title(t)).into_owned();
        let fields = fields_from_schema(schema, reg, &t.plugin);
        Some(Self::new(title, label, Method::POST, t.list_path.clone()?, fields).with_multipart(t.create_multipart))
    }

    /// Edit form for an existing resource (`u`); only changed fields are PATCHed.
    pub fn edit(t: &ResourceType, href: &str, name: &str, reg: &Registry) -> Option<Self> {
        let schema = reg.schema(t.update_schema.as_deref()?)?;
        let title = t!("form.edit", what = name).into_owned();
        let label = t!("action.update", name = name).into_owned();
        let mut form =
            Self::new(title, label, Method::PATCH, href.to_owned(), fields_from_schema(schema, reg, &t.plugin));
        form.load = Some(href.to_owned());
        Some(form)
    }

    /// POST form for an action or operation body schema (sync, publish, purge…).
    pub fn action(
        title: String,
        label: String,
        path: String,
        schema: Option<&Schema>,
        reg: &Registry,
        plugin: &str,
    ) -> Self {
        let fields = schema.map(|s| fields_from_schema(s, reg, plugin)).unwrap_or_default();
        Self::new(title, label, Method::POST, path, fields)
    }

    /// Pre-fills fields that exist in the schema.
    pub fn with_values(mut self, values: &[(&str, String)]) -> Self {
        for (name, value) in values {
            if let Some(f) = self.fields.iter_mut().find(|f| f.name == *name) {
                f.value = value.clone();
            }
        }
        self
    }

    pub fn with_after(mut self, after: AfterTask) -> Self {
        self.after = after;
        self
    }

    pub fn with_multipart(mut self, multipart: bool) -> Self {
        self.multipart = multipart;
        self
    }

    /// Shows a synchronous response (neither a task nor empty) in a view titled `title`.
    pub fn with_result(mut self, title: String) -> Self {
        self.show_result = Some(title);
        self
    }

    pub fn with_confirm(mut self, message: String) -> Self {
        self.confirm = Some(message);
        self
    }

    /// Label used in notifications and the task bar (`publish iso`).
    pub fn with_label(mut self, label: String) -> Self {
        self.label = label;
        self
    }

    /// Shows `count` of a list query on top, with `key` (`%{count}`) as message.
    pub fn with_count(mut self, path: String, query: Vec<(String, String)>, key: &'static str) -> Self {
        self.count = Some(CountQuery { path, query, key });
        self
    }

    fn body(&mut self) -> Option<Value> {
        self.errors.clear();
        self.general = None;
        let mut body = Map::new();
        for f in &self.fields {
            if f.kind == FieldKind::File {
                let raw = f.value.trim();
                if raw.is_empty() {
                    if f.required && self.original.is_none() {
                        self.errors.insert(f.name.clone(), vec![t!("form.required").into_owned()]);
                    }
                } else if !local_path(raw).is_file() {
                    self.errors.insert(f.name.clone(), vec![t!("form.no_file").into_owned()]);
                }
                continue;
            }
            let value = match f.json() {
                Ok(v) => v,
                Err(e) => {
                    self.errors.insert(f.name.clone(), vec![e]);
                    continue;
                }
            };
            match &self.original {
                // Edit: send changes only; clearing a nullable field sends null.
                Some(orig) => {
                    // Normalize the stored value through the same text round-trip as the input.
                    let old = orig
                        .get(&f.name)
                        .filter(|v| !v.is_null())
                        .and_then(|v| Field { value: to_text(v, &f.kind), ..f.clone() }.json().ok().flatten());
                    if value == old {
                        continue;
                    }
                    match value {
                        Some(v) => body.insert(f.name.clone(), v),
                        None if f.nullable => body.insert(f.name.clone(), Value::Null),
                        None => None,
                    };
                }
                None => match value {
                    Some(v) => {
                        body.insert(f.name.clone(), v);
                    }
                    None if f.required => {
                        self.errors.insert(f.name.clone(), vec![t!("form.required").into_owned()]);
                    }
                    None => {}
                },
            }
        }
        self.errors.is_empty().then_some(Value::Object(body))
    }

    /// Filled file fields, as multipart parts.
    fn files(&self) -> Vec<(String, PathBuf)> {
        self.fields
            .iter()
            .filter(|f| f.kind == FieldKind::File && !f.value.trim().is_empty())
            .map(|f| (f.name.clone(), local_path(f.value.trim())))
            .collect()
    }

    fn submit(&mut self, ctx: &Ctx) -> Outcome {
        if ctx.readonly() {
            return Outcome::readonly();
        }
        if self.submitting || (self.load.is_some() && self.original.is_none()) {
            return Outcome::None;
        }
        let Some(body) = self.body() else { return Outcome::None };
        if self.original.is_some() && body.as_object().is_some_and(Map::is_empty) {
            return Outcome::Batch(vec![
                Outcome::Pop,
                Outcome::Notify(super::Level::Info, t!("form.no_changes").into_owned()),
            ]);
        }
        let mut mutation = Mutation::new(self.method.clone(), self.path.clone(), Some(body), self.label.clone())
            .after(self.after.clone())
            .reply_to(self.id);
        let files = self.files();
        if self.multipart || !files.is_empty() {
            mutation = mutation.multipart(files);
        }
        self.submitting = true;
        match &self.confirm {
            Some(msg) => {
                self.submitting = false;
                confirm(msg.clone(), Outcome::Mutate(mutation))
            }
            None => Outcome::Mutate(mutation),
        }
    }

    fn cycle(&mut self, forward: bool) {
        let Some(f) = self.fields.get_mut(self.focus) else { return };
        match &f.kind {
            FieldKind::Bool => f.value = if f.value == "true" { "false".into() } else { "true".into() },
            FieldKind::Enum(choices) => {
                // "" (unset) is part of the cycle unless the field is required.
                let mut options: Vec<&str> = choices.iter().map(String::as_str).collect();
                if !f.required {
                    options.insert(0, "");
                }
                let pos = options.iter().position(|o| *o == f.value).unwrap_or(0);
                let len = options.len();
                let next = if forward { (pos + 1) % len } else { (pos + len - 1) % len };
                f.value = options[next].to_owned();
            }
            _ => {}
        }
    }

    fn field_line(&self, i: usize, f: &Field, ctx: &Ctx) -> ListItem<'static> {
        let theme = ctx.theme;
        let focused = i == self.focus;
        let star = if f.required { "*" } else { " " };
        let mut spans = vec![Span::styled(format!("{star}{:<w$} ", f.name, w = LABEL_WIDTH - 1), theme.accent())];
        let empty = f.value.is_empty();
        match &f.kind {
            FieldKind::Bool => spans.push(Span::raw(match f.value.as_str() {
                "true" => "[x]",
                "false" => "[ ]",
                _ => "[-]", // unset: the server default applies
            })),
            FieldKind::Enum(_) => spans.push(Span::raw(format!("‹ {} ›", if empty { "–" } else { &f.value }))),
            FieldKind::Secret => spans.push(Span::raw("•".repeat(f.value.chars().count()))),
            _ => spans.push(Span::raw(f.value.clone())),
        }
        if focused && f.is_text() {
            spans.push(Span::styled("▏", theme.accent()));
        }
        if empty && let Some(p) = &f.placeholder {
            spans.push(Span::styled(format!(" ({} {p})", t!("form.default")), theme.muted()));
        }
        if let FieldKind::Relation(target) = &f.kind {
            if !empty && format::href(&f.value, ctx.names) != format::short_href(&f.value) {
                spans.push(Span::styled(format!("  ({})", format::href(&f.value, ctx.names)), theme.muted()));
            }
            if target.is_some() && focused {
                spans.push(Span::styled(format!("  ↵ {}", t!("form.pick")), theme.muted()));
            }
        }
        let mut lines = vec![Line::from(spans)];
        for e in self.errors.get(&f.name).into_iter().flatten() {
            lines.push(Line::styled(format!("{:w$}↳ {e}", "", w = LABEL_WIDTH + 1), theme.error()));
        }
        ListItem::new(Text::from(lines))
    }
}

impl View for FormView {
    fn title(&self) -> String {
        self.title.clone()
    }

    fn captures_input(&self) -> bool {
        true
    }

    fn set_id(&mut self, id: u64) {
        self.id = id;
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        vec![
            Hint::new("↑↓", t!("hint.field")),
            Hint::new("←→", t!("hint.choose")),
            Hint::new("ctrl-s", t!("hint.submit")),
            Hint::new("ctrl-u", t!("hint.clear")),
            Hint::new("esc", t!("hint.cancel")),
        ]
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let last = self.fields.len(); // index of the submit button
        match key.code {
            KeyCode::Esc => return Outcome::Pop,
            KeyCode::Char('s') if ctrl => return self.submit(ctx),
            KeyCode::Up | KeyCode::BackTab => self.focus = self.focus.saturating_sub(1),
            KeyCode::Down | KeyCode::Tab => self.focus = (self.focus + 1).min(last),
            KeyCode::Enter if self.focus == last => return self.submit(ctx),
            KeyCode::Char('u') if ctrl => {
                if let Some(f) = self.fields.get_mut(self.focus) {
                    f.value.clear();
                }
            }
            _ => {
                let Some(f) = self.fields.get_mut(self.focus) else { return Outcome::None };
                match (&f.kind, key.code) {
                    (FieldKind::Relation(Some(target)), KeyCode::Enter) => {
                        let Some(reg) = ctx.registry() else { return Outcome::None };
                        let Some(t) = reg.get(target) else { return Outcome::None };
                        return Outcome::Push(Box::new(ListView::picker(t, reg, self.id, &f.name)));
                    }
                    (FieldKind::Bool | FieldKind::Enum(_), KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right) => {
                        self.cycle(true)
                    }
                    (FieldKind::Bool | FieldKind::Enum(_), KeyCode::Left) => self.cycle(false),
                    (FieldKind::Bool | FieldKind::Enum(_), KeyCode::Backspace) if !f.required => f.value.clear(),
                    (_, KeyCode::Enter) => self.focus = (self.focus + 1).min(last),
                    (_, KeyCode::Backspace) if f.is_text() => {
                        f.value.pop();
                    }
                    (_, KeyCode::Char(c)) if f.is_text() && !ctrl => f.value.push(c),
                    _ => {}
                }
            }
        }
        Outcome::None
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let theme = ctx.theme;
        let mut top: Vec<Line> = Vec::new();
        if let Some(intro) = &self.intro {
            top.push(Line::styled(intro.clone(), theme.accent()));
        }
        if let Some(g) = &self.general {
            top.push(Line::styled(g.clone(), theme.error()));
        }
        if self.load.is_some() && self.original.is_none() {
            top.push(Line::styled(t!("list.loading").into_owned(), theme.muted()));
        }
        let [head, body, help] =
            Layout::vertical([Constraint::Length(top.len() as u16), Constraint::Min(3), Constraint::Length(3)])
                .areas(area);
        f.render_widget(Paragraph::new(top), head);

        let mut items: Vec<ListItem> =
            self.fields.iter().enumerate().map(|(i, fld)| self.field_line(i, fld, ctx)).collect();
        let button = if self.submitting { t!("form.submitting") } else { t!("form.submit") };
        items.push(ListItem::new(Line::from(vec![
            Span::raw(" ".repeat(LABEL_WIDTH + 1)),
            Span::styled(format!("[ {button} ]"), theme.success().add_modifier(Modifier::BOLD)),
        ])));
        self.state.select(Some(self.focus));
        let list = List::new(items).highlight_style(Style::new().bg(theme.selection_bg));
        f.render_stateful_widget(list, body, &mut self.state);

        let help_text = match self.fields.get(self.focus) {
            Some(fld) => {
                let mut text = fld.help.clone().unwrap_or_default();
                if fld.kind == FieldKind::File {
                    text = format!("[{}] {text}", t!("form.file_help"));
                }
                if let FieldKind::Enum(c) | FieldKind::List(c) = &fld.kind
                    && !c.is_empty()
                {
                    text = format!("[{}] {text}", c.join(" | "));
                }
                text
            }
            None => t!("form.submit_help").into_owned(),
        };
        f.render_widget(Paragraph::new(Line::styled(help_text, theme.muted())).wrap(Wrap { trim: true }), help);
    }

    fn fetch(&mut self, _kind: FetchKind, ctx: &Ctx) -> Option<Fetch> {
        let client = ctx.client()?.clone();
        if let Some(href) = self.load.clone().filter(|_| self.original.is_none()) {
            return Some(async move { client.get(&href, &[]).await.map(Payload::Object) }.boxed());
        }
        let CountQuery { path, query, .. } = self.count.clone()?;
        Some(async move { client.count(&path, &query).await.map(Payload::Count) }.boxed())
    }

    fn on_data(&mut self, _kind: FetchKind, result: Result<Payload, Error>, _ctx: &Ctx) -> Outcome {
        match result {
            Ok(Payload::Object(resource)) => {
                for fld in &mut self.fields {
                    if let Some(v) = resource.get(&fld.name) {
                        fld.value = to_text(v, &fld.kind);
                    }
                }
                self.original = Some(resource);
            }
            Ok(Payload::Count(n)) => {
                if let Some(CountQuery { key, .. }) = &self.count {
                    self.intro = Some(labels::tr_count(key, n));
                }
            }
            Ok(_) => {}
            Err(e) => self.general = Some(super::error_text(&e)),
        }
        Outcome::None
    }

    fn on_mutated(&mut self, result: &Result<Value, Error>) -> Outcome {
        self.submitting = false;
        match result {
            Ok(v) => match &self.show_result {
                Some(title)
                    if pulply_api::models::task_href(v).is_none() && v.as_object().is_some_and(|o| !o.is_empty()) =>
                {
                    Outcome::Batch(vec![
                        Outcome::Pop,
                        Outcome::Push(Box::new(super::raw::RawView::value(title.clone(), v.clone()))),
                    ])
                }
                _ => Outcome::Pop,
            },
            Err(Error::Validation(fields)) => {
                let mut fields = fields.clone();
                let general: Vec<String> =
                    ["non_field_errors", "detail"].iter().filter_map(|k| fields.remove(*k)).flatten().collect();
                // Errors on fields the form does not show go to the general line.
                let (known, unknown): (BTreeMap<_, _>, BTreeMap<_, _>) =
                    fields.into_iter().partition(|(k, _)| self.fields.iter().any(|f| &f.name == k));
                let mut general = general;
                general.extend(unknown.into_iter().map(|(k, v)| format!("{k}: {}", v.join(", "))));
                self.general = (!general.is_empty()).then(|| general.join("; "));
                self.errors = known;
                if let Some(pos) = self.fields.iter().position(|f| self.errors.contains_key(&f.name)) {
                    self.focus = pos;
                }
                Outcome::None
            }
            Err(e) => {
                self.general = Some(super::error_text(e));
                Outcome::None
            }
        }
    }

    fn receive(&mut self, key: &str, value: Value) {
        if let Some(f) = self.fields.iter_mut().find(|f| f.name == key) {
            f.value = to_text(&value, &f.kind);
            self.errors.remove(key);
        }
    }

    fn poll_interval(&self, _default: Duration) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn field(name: &str, kind: FieldKind, value: &str) -> Field {
        Field {
            name: name.into(),
            kind,
            required: false,
            nullable: true,
            help: None,
            placeholder: None,
            value: value.into(),
        }
    }

    fn form(fields: Vec<Field>, original: Option<Value>) -> FormView {
        let mut f = FormView::new("t".into(), "l".into(), Method::PATCH, "/x/".into(), fields);
        f.original = original;
        f
    }

    #[test]
    fn values_are_typed_from_text() {
        assert_eq!(field("n", FieldKind::Integer, " 42 ").json(), Ok(Some(json!(42))));
        assert!(field("n", FieldKind::Integer, "4x").json().is_err());
        assert_eq!(field("l", FieldKind::List(vec![]), "a, b,,c").json(), Ok(Some(json!(["a", "b", "c"]))));
        assert_eq!(field("j", FieldKind::Json, r#"{"k":"v"}"#).json(), Ok(Some(json!({"k": "v"}))));
        assert_eq!(field("b", FieldKind::Bool, "false").json(), Ok(Some(json!(false))));
        assert_eq!(field("t", FieldKind::Text, "   ").json(), Ok(None));
    }

    #[test]
    fn edit_sends_only_changed_fields_and_null_for_cleared_nullable() {
        let original =
            json!({"name": "iso", "retain_repo_versions": 3, "description": "old", "pulp_labels": {"a": "b"}});
        let mut f = form(
            vec![
                field("name", FieldKind::Text, "iso"),
                field("retain_repo_versions", FieldKind::Integer, "5"),
                field("description", FieldKind::Text, ""),
                field("pulp_labels", FieldKind::Json, r#"{"a": "b"}"#),
            ],
            Some(original),
        );
        assert_eq!(f.body(), Some(json!({"retain_repo_versions": 5, "description": null})));
    }

    #[test]
    fn create_requires_required_fields() {
        let mut name = field("name", FieldKind::Text, "");
        name.required = true;
        let mut f = form(vec![name, field("url", FieldKind::Text, "http://x")], None);
        assert_eq!(f.body(), None);
        assert!(f.errors.contains_key("name"));
        f.fields[0].value = "iso".into();
        assert_eq!(f.body(), Some(json!({"name": "iso", "url": "http://x"})));
    }

    #[test]
    fn file_fields_become_parts_not_json() {
        let dir = tempfile::tempdir().unwrap();
        let comps = dir.path().join("comps.xml");
        std::fs::write(&comps, "<comps/>").unwrap();
        let mut file = field("file", FieldKind::File, comps.to_str().unwrap());
        file.required = true;
        let mut f = form(vec![file, field("repository", FieldKind::Text, "/r/")], None);
        assert_eq!(f.body(), Some(json!({"repository": "/r/"})));
        assert_eq!(f.files(), [("file".to_owned(), comps)]);

        f.fields[0].value = dir.path().join("missing").to_string_lossy().into_owned();
        assert_eq!(f.body(), None);
        f.fields[0].value.clear();
        assert_eq!(f.body(), None, "required");
        assert!(f.errors.contains_key("file"));
    }

    #[test]
    fn content_creation_is_sent_as_multipart() {
        let reg = crate::testutil::registry();
        let packages = FormView::create(reg.get("content/rpm/packages").unwrap(), &reg).unwrap();
        assert!(packages.multipart && packages.fields.iter().any(|f| f.name == "file" && f.kind == FieldKind::File));
        assert!(!FormView::create(reg.get("repositories/rpm/rpm").unwrap(), &reg).unwrap().multipart);
    }

    #[test]
    fn binary_schema_properties_are_file_fields() {
        let reg = crate::testutil::registry();
        let fields = fields_from_schema(reg.schema("CompsXml").unwrap(), &reg, "rpm");
        let file = fields.iter().find(|f| f.name == "file").unwrap();
        assert!(file.kind == FieldKind::File && file.required);
    }

    #[test]
    fn enum_cycle_includes_unset_for_optional_fields() {
        let mut f =
            form(vec![field("policy", FieldKind::Enum(vec!["immediate".into(), "on_demand".into()]), "")], None);
        f.cycle(true);
        assert_eq!(f.fields[0].value, "immediate");
        f.cycle(true);
        f.cycle(true);
        assert_eq!(f.fields[0].value, "", "wraps back to unset");
        f.cycle(false);
        assert_eq!(f.fields[0].value, "on_demand");
    }
}
