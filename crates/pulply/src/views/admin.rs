//! Administration views (R12): generic action menu, role assignments of a resource,
//! and the current user (`:me`).

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};
use futures::FutureExt;
use pulply_api::registry::{Action, Method as OpMethod};
use pulply_api::{Error, Method, ResourceExt};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, List, ListItem, ListState, Paragraph, Row, Table, TableState};
use rust_i18n::t;
use serde_json::{Value, json};

use super::confirm::{centered, confirm};
use super::form::FormView;
use super::lines::{LineItem, LineList, json_lines};
use super::raw::RawView;
use super::{Ctx, Fetch, FetchKind, Hint, Mutation, Outcome, Payload, View, actions, error_text};
use crate::format;
use crate::perms::Access;

/// Actions offered by the menu: POST actions (those with a dedicated key stay available
/// here too) and GET actions returning JSON (`metrics/`, `profile_artifacts/`…).
pub struct ActionMenuView {
    href: String,
    name: String,
    items: Vec<Action>,
    state: ListState,
}

/// GET actions with their own view (role assignments, permissions).
const DEDICATED_GETS: &[&str] = &["list_roles", "my_permissions"];

/// The user may run the POST action `verb` on `href`: its own permission when Pulp has
/// one (`sync`, `modify`, `repair`, `replicate`…; `manage_roles` for role changes), else
/// the right to change the resource (labels, reset, exports…).
fn action_allowed(verb: &str, href: &str, ctx: &Ctx) -> bool {
    let own = match verb {
        "add_role" | "remove_role" => "manage_roles",
        "set_label" | "unset_label" => "change",
        v => v,
    };
    actions::can_on(own, href, ctx) || (own != "manage_roles" && actions::can_on("change", href, ctx))
}

/// Menu entries for `href`: POST actions the user may run (none in read-only mode),
/// then read-only GET actions.
fn menu_items(href: &str, ctx: &Ctx) -> Vec<Action> {
    let Some(t) = ctx.registry().and_then(|r| r.type_for_href(href)) else { return Vec::new() };
    let mut items: Vec<Action> = t
        .actions
        .iter()
        .filter(|a| match a.method {
            OpMethod::Post => !ctx.readonly() && action_allowed(&a.verb, href, ctx),
            OpMethod::Get => a.response_schema.is_some() && !DEDICATED_GETS.contains(&a.verb.as_str()),
            _ => false,
        })
        .cloned()
        .collect();
    items.sort_by(|a, b| (a.method != OpMethod::Post, &a.verb).cmp(&(b.method != OpMethod::Post, &b.verb)));
    items
}

/// The resource at `href` has actions the user may run or read.
pub fn has_actions(href: &str, ctx: &Ctx) -> bool {
    !menu_items(href, ctx).is_empty()
}

impl ActionMenuView {
    /// Menu of the actions of the resource at `href`, if any.
    pub fn for_resource(href: &str, name: String, ctx: &Ctx) -> Option<Self> {
        let items = menu_items(href, ctx);
        (!items.is_empty()).then(|| Self {
            href: href.to_owned(),
            name,
            items,
            state: ListState::default().with_selected(Some(0)),
        })
    }

    fn run(&self, ctx: &Ctx) -> Outcome {
        let (Some(action), Some(reg)) = (self.state.selected().and_then(|i| self.items.get(i)), ctx.registry()) else {
            return Outcome::None;
        };
        let verb = &action.verb;
        let path = format!("{}{verb}/", self.href);
        let title = t!("admin.action_title", verb = verb, name = self.name).into_owned();
        if action.method == OpMethod::Get {
            return Outcome::Batch(vec![
                Outcome::CloseOverlay,
                Outcome::Push(Box::new(RawView::fetching(title, path))),
            ]);
        }
        if ctx.readonly() {
            return Outcome::readonly();
        }
        let label = t!("action.run", verb = verb, name = self.name).into_owned();
        let schema = action
            .body_schema
            .as_deref()
            .and_then(|s| reg.schema(s))
            .filter(|s| s.properties.values().any(|p| !p.read_only));
        let next = match schema {
            Some(schema) => {
                let plugin = reg.type_for_href(&self.href).map(|t| t.plugin.clone()).unwrap_or_default();
                let form = FormView::action(title.clone(), label, path, Some(schema), reg, &plugin)
                    .with_multipart(action.multipart)
                    .with_result(title);
                Outcome::Push(Box::new(form))
            }
            None => {
                let mut mutation = Mutation::new(Method::POST, path, Some(json!({})), label);
                if action.multipart {
                    mutation = mutation.multipart(Vec::new());
                }
                confirm(t!("confirm.action", verb = verb, name = self.name), Outcome::Mutate(mutation))
            }
        };
        Outcome::Batch(vec![Outcome::CloseOverlay, next])
    }
}

impl View for ActionMenuView {
    fn title(&self) -> String {
        t!("admin.actions", name = self.name).into_owned()
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        vec![Hint::new("↵", t!("hint.run")), Hint::new("esc", t!("hint.cancel"))]
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome {
        let cur = self.state.selected().unwrap_or(0);
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.state.select(Some((cur + 1).min(self.items.len() - 1))),
            KeyCode::Up | KeyCode::Char('k') => self.state.select(Some(cur.saturating_sub(1))),
            KeyCode::Enter => return self.run(ctx),
            _ => return Outcome::Ignored,
        }
        Outcome::None
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let items: Vec<ListItem> = self
            .items
            .iter()
            .map(|a| {
                let suffix = match a.method {
                    OpMethod::Get => format!("  ({})", t!("admin.view")),
                    _ if a.body_schema.is_some() => "  …".to_owned(),
                    _ => String::new(),
                };
                ListItem::new(Line::from(vec![Span::raw(a.verb.clone()), Span::styled(suffix, ctx.theme.muted())]))
            })
            .collect();
        f.render_stateful_widget(List::new(items).highlight_style(ctx.theme.selected()), area, &mut self.state);
    }

    fn poll_interval(&self, _default: Duration) -> Option<Duration> {
        None
    }

    fn overlay_size(&self, area: Rect) -> Rect {
        centered(area, 50, self.items.len() as u16 + 2)
    }
}

/// Role assignments on one resource (`list_roles/`), with add/remove (`add_role/`, `remove_role/`).
pub struct RolesView {
    href: String,
    name: String,
    roles: Vec<Value>,
    state: TableState,
    loaded: bool,
    error: Option<String>,
}

impl RolesView {
    pub fn new(href: String, name: String) -> Self {
        Self {
            href,
            name,
            roles: Vec::new(),
            state: TableState::default().with_selected(Some(0)),
            loaded: false,
            error: None,
        }
    }

    fn form(&self, verb: &str, prefill: bool, ctx: &Ctx) -> Outcome {
        if ctx.readonly() {
            return Outcome::readonly();
        }
        let Some(reg) = ctx.registry() else { return Outcome::None };
        let Some(action) = reg.type_for_href(&self.href).and_then(|t| t.actions.iter().find(|a| a.verb == verb)) else {
            return Outcome::None;
        };
        let schema = action.body_schema.as_deref().and_then(|s| reg.schema(s));
        let title = t!("admin.action_title", verb = verb, name = self.name).into_owned();
        let label = t!("action.run", verb = verb, name = self.name).into_owned();
        let mut form = FormView::action(title, label, format!("{}{verb}/", self.href), schema, reg, "core");
        if prefill && let Some(r) = self.state.selected().and_then(|i| self.roles.get(i)) {
            let join = |k: &str| {
                r.get(k)
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))
            };
            form = form.with_values(&[
                ("role", r.str_field("role").unwrap_or_default().to_owned()),
                ("users", join("users").unwrap_or_default()),
                ("groups", join("groups").unwrap_or_default()),
            ]);
        }
        Outcome::Push(Box::new(form))
    }
}

/// The resource has role assignments the user may manage.
pub fn can_manage_roles(href: &str, ctx: &Ctx) -> bool {
    actions::can_on("manage_roles", href, ctx)
}

impl View for RolesView {
    fn title(&self) -> String {
        t!("admin.roles_title", name = self.name).into_owned()
    }

    fn info(&self, _ctx: &Ctx) -> String {
        self.roles.len().to_string()
    }

    fn hints(&self, ctx: &Ctx) -> Vec<Hint> {
        if ctx.readonly() {
            return Vec::new();
        }
        vec![Hint::new("a", t!("hint.add_role")), Hint::new("r", t!("hint.remove_role"))]
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome {
        let cur = self.state.selected().unwrap_or(0);
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.state.select(Some((cur + 1).min(self.roles.len().saturating_sub(1))))
            }
            KeyCode::Up | KeyCode::Char('k') => self.state.select(Some(cur.saturating_sub(1))),
            KeyCode::Char('a') => return self.form("add_role", false, ctx),
            KeyCode::Char('r') if !self.roles.is_empty() => return self.form("remove_role", true, ctx),
            _ => return Outcome::Ignored,
        }
        Outcome::None
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let theme = ctx.theme;
        if let Some(e) = &self.error {
            f.render_widget(Paragraph::new(Line::styled(e.clone(), theme.error())), area);
            return;
        }
        if self.roles.is_empty() {
            let msg = if self.loaded { t!("admin.no_roles") } else { t!("list.loading") };
            f.render_widget(Paragraph::new(Line::styled(msg.into_owned(), theme.muted())), area);
            return;
        }
        let bold = theme.accent().add_modifier(Modifier::BOLD);
        let header = Row::new(["ROLE", "USERS", "GROUPS"].map(|h| Cell::from(Span::styled(h, bold))));
        let join = |r: &Value, k: &str| {
            r.get(k)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))
                .unwrap_or_default()
        };
        let rows = self.roles.iter().map(|r| {
            Row::new([
                Cell::from(r.str_field("role").unwrap_or_default().to_owned()),
                Cell::from(join(r, "users")),
                Cell::from(join(r, "groups")),
            ])
        });
        let widths = [Constraint::Percentage(40), Constraint::Percentage(30), Constraint::Percentage(30)];
        f.render_stateful_widget(
            Table::new(rows, widths).header(header).row_highlight_style(theme.selected()),
            area,
            &mut self.state,
        );
    }

    fn fetch(&mut self, _kind: FetchKind, ctx: &Ctx) -> Option<Fetch> {
        let client = ctx.client()?.clone();
        let path = format!("{}list_roles/", self.href);
        Some(async move { client.get(&path, &[]).await.map(Payload::Object) }.boxed())
    }

    fn on_data(&mut self, _kind: FetchKind, result: Result<Payload, Error>, _ctx: &Ctx) -> Outcome {
        self.loaded = true;
        match result {
            Ok(Payload::Object(v)) => {
                self.error = None;
                self.roles = v.get("roles").and_then(Value::as_array).cloned().unwrap_or_default();
            }
            Ok(_) => {}
            Err(e) => self.error = Some(error_text(&e)),
        }
        Outcome::None
    }
}

/// `:me` — the connected user, how pulply filters its actions, and its roles when readable.
#[derive(Default)]
pub struct MeView {
    data: Option<Value>,
    lines: LineList,
    error: Option<String>,
}

impl MeView {
    pub fn new() -> Self {
        Self::default()
    }
}

impl View for MeView {
    fn title(&self) -> String {
        t!("admin.me").into_owned()
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        vec![Hint::new("↵", t!("hint.follow"))]
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome {
        if self.lines.handle_key(key) {
            return Outcome::None;
        }
        match key.code {
            KeyCode::Enter => match self.lines.selected_href().and_then(|h| super::open_href(h, ctx)) {
                Some(v) => Outcome::Push(v),
                None => Outcome::None,
            },
            _ => Outcome::Ignored,
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let theme = ctx.theme;
        let [head, body] = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(area);
        let perms = ctx.session.map(|s| &s.perms);
        let access = match perms.map(|p| &p.access) {
            Some(Access::All) | None => t!("admin.access_all"),
            Some(Access::Limited(g)) => t!("admin.access_limited", count = g.len()),
            Some(Access::ObjectsOnly) => t!("admin.access_objects"),
        };
        let user = perms.and_then(|p| p.username.clone()).unwrap_or_else(|| "–".into());
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![Span::styled(format!("{:<12}", t!("admin.user")), theme.accent()), Span::raw(user)]),
                Line::from(vec![
                    Span::styled(format!("{:<12}", t!("admin.access")), theme.accent()),
                    Span::raw(access.into_owned()),
                ]),
            ]),
            head,
        );
        match (&self.data, &self.error) {
            (_, Some(e)) => f.render_widget(Paragraph::new(Line::styled(e.clone(), theme.error())), body),
            (None, None) => {
                f.render_widget(Paragraph::new(Line::styled(t!("list.loading").into_owned(), theme.muted())), body)
            }
            (Some(data), None) => {
                let mut lines = json_lines(data, ctx);
                if lines.is_empty() {
                    lines.push(LineItem::plain(Line::styled(t!("admin.no_details").into_owned(), theme.muted())));
                }
                self.lines.set(lines);
                self.lines.render(f, body, ctx);
            }
        }
    }

    fn fetch(&mut self, _kind: FetchKind, ctx: &Ctx) -> Option<Fetch> {
        let client = ctx.client()?.clone();
        Some(
            async move {
                let me = client.get(&client.api_path("login/"), &[]).await?;
                let Some(href) = me.href().map(str::to_owned) else { return Ok(Payload::Object(me)) };
                // Non-admin users usually cannot read their own record nor roles: show what is readable.
                let roles_query = [("limit".to_owned(), "1000".to_owned())];
                let roles_path = format!("{href}roles/");
                let (user, roles) = futures::join!(client.get(&href, &[]), client.list(&roles_path, &roles_query));
                let mut out = json!({ "pulp_href": href });
                match user {
                    Ok(u) => out["user"] = u,
                    Err(e) => out["user"] = Value::from(error_text(&e)),
                }
                match roles {
                    Ok(page) => {
                        out["roles"] = Value::Array(
                            page.results
                                .iter()
                                .map(|r| json!({"role": r.get("role"), "content_object": r.get("content_object")}))
                                .collect(),
                        )
                    }
                    Err(e) => out["roles"] = Value::from(error_text(&e)),
                }
                Ok(Payload::Object(out))
            }
            .boxed(),
        )
    }

    fn on_data(&mut self, _kind: FetchKind, result: Result<Payload, Error>, _ctx: &Ctx) -> Outcome {
        match result {
            Ok(Payload::Object(v)) => self.data = Some(v),
            Ok(_) => {}
            Err(e) => self.error = Some(error_text(&e)),
        }
        Outcome::None
    }

    fn poll_interval(&self, _default: Duration) -> Option<Duration> {
        None
    }
}

/// Display name for menus: resource name or rendered href.
pub fn display_name(row: Option<&Value>, href: &str, ctx: &Ctx) -> String {
    row.and_then(crate::names::NameCache::label).map(str::to_owned).unwrap_or_else(|| format::href(href, ctx.names))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::NameCache;
    use crate::testutil;
    use crate::theme::Theme;

    fn verbs(href: &str, readonly: bool) -> Vec<String> {
        let (names, theme, config) = (NameCache::default(), Theme::default(), testutil::config());
        let session = testutil::session(readonly);
        let ctx = Ctx {
            session: Some(&session),
            names: &names,
            theme: &theme,
            config: &config,
            current_ctx: "test",
            now: chrono::Utc::now(),
        };
        menu_items(href, &ctx).into_iter().map(|a| a.verb).collect()
    }

    #[test]
    fn menu_offers_json_get_actions_even_read_only() {
        let maven = "/pulp/api/v3/repositories/maven/maven/0192/";
        let all = verbs(maven, false);
        for v in ["metrics", "packages", "path_index_status", "set_label"] {
            assert!(all.iter().any(|a| a == v), "{v} missing from {all:?}");
        }
        assert!(!all.iter().any(|a| a == "list_roles" || a == "my_permissions"), "dedicated views");
        assert_eq!(verbs(maven, true), ["metrics", "packages", "path_index_status"]);
        assert!(verbs("/pulp/api/v3/tasks/0192/", true).contains(&"profile_artifacts".to_owned()));
        let namespace = "/pulp/api/v3/content/ansible/namespaces/0192/";
        assert!(!verbs(namespace, true).contains(&"avatar".to_owned()), "redirect without JSON");
    }
}
