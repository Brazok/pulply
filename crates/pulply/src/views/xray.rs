//! Relations tree (R10.3): distribution → publication → version → repository → remote
//! (+ content guard), or from a repository: its remote, latest version, and what serves it.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};
use futures::FutureExt;
use futures::future::{BoxFuture, join_all};
use pulply_api::models::version_number;
use pulply_api::{Error, PulpClient, ResourceExt};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use rust_i18n::t;
use serde_json::Value;

use super::lines::{LineItem, LineList};
use super::{Ctx, Fetch, FetchKind, Hint, Outcome, Payload, View, open_href};
use crate::format;
use crate::labels::tr;
use crate::names::NameCache;

/// Related resources listed per kind (publications/distributions of a repository).
const MAX_SERVING: usize = 20;

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Locale key suffix (`xray.<kind>`).
    pub kind: &'static str,
    pub label: String,
    pub href: Option<String>,
    pub children: Vec<Node>,
}

impl Node {
    fn leaf(kind: &'static str, label: String, href: Option<String>) -> Self {
        Self { kind, label, href, children: Vec::new() }
    }
}

/// Kind of resource an href points at, for the starting node.
fn kind_of(href: &str) -> &'static str {
    if version_number(href).is_some() {
        "version"
    } else if href.contains("/distributions/") {
        "distribution"
    } else if href.contains("/publications/") {
        "publication"
    } else if href.contains("/remotes/") {
        "remote"
    } else if href.contains("/contentguards/") {
        "guard"
    } else {
        "repository"
    }
}

/// Can a relations tree start from this href?
pub fn supports(href: &str) -> bool {
    ["/distributions/", "/publications/", "/repositories/"].iter().any(|p| href.contains(p))
}

fn label_of(v: &Value) -> String {
    let href = v.href().unwrap_or_default();
    match (version_number(href), v.str_field("repository")) {
        (Some(n), _) => format!("#{n}"),
        _ => NameCache::label(v).map(str::to_owned).unwrap_or_else(|| format::short_href(href)),
    }
}

async fn simple(client: &PulpClient, href: &str) -> Node {
    let kind = kind_of(href);
    match client.get(href, &[]).await {
        Ok(v) => Node::leaf(kind, label_of(&v), Some(href.to_owned())),
        Err(e) => Node::leaf(kind, format!("{} ({e})", format::short_href(href)), Some(href.to_owned())),
    }
}

/// Repository with its remote; `serving` adds the publications and distributions using it.
fn repository<'a>(client: &'a PulpClient, href: &'a str, serving: bool) -> BoxFuture<'a, Node> {
    async move {
        let repo = match client.get(href, &[]).await {
            Ok(v) => v,
            Err(e) => {
                return Node::leaf("repository", format!("{} ({e})", format::short_href(href)), Some(href.into()));
            }
        };
        let mut node = Node::leaf("repository", label_of(&repo), Some(href.to_owned()));
        if let Some(remote) = repo.str_field("remote") {
            node.children.push(simple(client, remote).await);
        }
        if let Some(latest) = repo.str_field("latest_version_href") {
            let n = version_number(latest).map(|n| format!("#{n}")).unwrap_or_default();
            node.children.push(Node::leaf("version", n, Some(latest.to_owned())));
        }
        if serving {
            let query = vec![("repository".to_owned(), href.to_owned()), ("limit".to_owned(), MAX_SERVING.to_string())];
            let (pubs_path, dists_path) = (client.api_path("publications/"), client.api_path("distributions/"));
            let (pubs, dists) = futures::join!(client.list(&pubs_path, &query), client.list(&dists_path, &query));
            for p in pubs.map(|p| p.results).unwrap_or_default() {
                let label = p.str_field("repository_version").and_then(version_number).map(|n| format!("→ #{n}"));
                node.children.push(Node::leaf("publication", label.unwrap_or_default(), p.href().map(str::to_owned)));
            }
            for d in dists.map(|p| p.results).unwrap_or_default() {
                let url = d.str_field("base_url").or_else(|| d.str_field("registry_path")).unwrap_or_default();
                node.children.push(Node::leaf(
                    "serves",
                    format!("{}  {url}", label_of(&d)),
                    d.href().map(str::to_owned),
                ));
            }
        }
        node
    }
    .boxed()
}

async fn version(client: &PulpClient, href: &str) -> Node {
    let mut node =
        Node::leaf("version", version_number(href).map(|n| format!("#{n}")).unwrap_or_default(), Some(href.into()));
    if let Some(repo) = pulply_api::models::version_repository(href) {
        node.children.push(repository(client, repo, false).await);
    }
    node
}

async fn publication(client: &PulpClient, href: &str) -> Node {
    let mut node = Node::leaf("publication", format::short_href(href), Some(href.into()));
    match client.get(href, &[]).await {
        Ok(p) => {
            if let Some(v) = p.str_field("repository_version") {
                node.label = format!("→ #{}", version_number(v).unwrap_or_default());
                node.children.push(version(client, v).await);
            }
        }
        Err(e) => node.label = format!("{} ({e})", node.label),
    }
    node
}

async fn distribution(client: &PulpClient, href: &str) -> Node {
    let dist = match client.get(href, &[]).await {
        Ok(v) => v,
        Err(e) => return Node::leaf("distribution", format!("{} ({e})", format::short_href(href)), Some(href.into())),
    };
    let mut node = Node::leaf("distribution", label_of(&dist), Some(href.to_owned()));
    if let Some(url) = dist.str_field("base_url").or_else(|| dist.str_field("registry_path")) {
        node.label = format!("{}  {url}", node.label);
    }
    let field = |k: &str| dist.str_field(k).map(str::to_owned);
    let mut children: Vec<BoxFuture<Node>> = Vec::new();
    if let Some(g) = field("content_guard") {
        children.push(async move { simple(client, &g).await }.boxed());
    }
    if let Some(p) = field("publication") {
        children.push(async move { publication(client, &p).await }.boxed());
    }
    if let Some(v) = field("repository_version") {
        children.push(async move { version(client, &v).await }.boxed());
    }
    if let Some(r) = field("repository") {
        children.push(async move { repository(client, &r, false).await }.boxed());
    }
    if let Some(r) = field("remote") {
        children.push(async move { simple(client, &r).await }.boxed());
    }
    node.children = join_all(children).await;
    node
}

/// Builds the tree rooted at `href`.
pub async fn build(client: &PulpClient, href: &str) -> Node {
    match kind_of(href) {
        "distribution" => distribution(client, href).await,
        "publication" => publication(client, href).await,
        "version" => version(client, href).await,
        _ => repository(client, href, true).await,
    }
}

/// Tree lines with box-drawing prefixes; every node keeps its href.
pub fn flatten(node: &Node, ctx: &Ctx) -> Vec<LineItem> {
    fn walk(node: &Node, prefix: &str, last: bool, root: bool, ctx: &Ctx, out: &mut Vec<LineItem>) {
        let theme = ctx.theme;
        let branch = if root {
            ""
        } else if last {
            "└─ "
        } else {
            "├─ "
        };
        let label = match &node.href {
            Some(h) if node.label.is_empty() => format::href(h, ctx.names),
            _ => node.label.clone(),
        };
        out.push(LineItem {
            line: Line::from(vec![
                Span::styled(format!("{prefix}{branch}"), theme.muted()),
                Span::styled(
                    format!("{:<14}", tr(&format!("xray.{}", node.kind))),
                    theme.accent().add_modifier(Modifier::BOLD),
                ),
                Span::raw(label),
            ]),
            href: node.href.clone(),
        });
        let child_prefix = if root { String::new() } else { format!("{prefix}{}", if last { "   " } else { "│  " }) };
        for (i, child) in node.children.iter().enumerate() {
            walk(child, &child_prefix, i + 1 == node.children.len(), false, ctx, out);
        }
    }
    let mut out = Vec::new();
    walk(node, "", true, true, ctx, &mut out);
    out
}

pub struct XrayView {
    href: String,
    tree: Option<Node>,
    lines: LineList,
    error: Option<String>,
}

impl XrayView {
    pub fn new(href: String) -> Self {
        Self { href, tree: None, lines: LineList::default(), error: None }
    }
}

impl View for XrayView {
    fn title(&self) -> String {
        t!("xray.title").into_owned()
    }

    fn info(&self, ctx: &Ctx) -> String {
        format::href(&self.href, ctx.names)
    }

    fn hints(&self, _ctx: &Ctx) -> Vec<Hint> {
        vec![Hint::new("↵", t!("hint.follow")), Hint::new("j/k", t!("hint.scroll"))]
    }

    fn handle_key(&mut self, key: KeyEvent, ctx: &Ctx) -> Outcome {
        if self.lines.handle_key(key) {
            return Outcome::None;
        }
        match key.code {
            KeyCode::Enter => match self.lines.selected_href().and_then(|h| open_href(h, ctx)) {
                Some(v) => Outcome::Push(v),
                None => Outcome::None,
            },
            _ => Outcome::Ignored,
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        match (&self.tree, &self.error) {
            (_, Some(e)) => f.render_widget(Paragraph::new(Line::styled(e.clone(), ctx.theme.error())), area),
            (None, None) => {
                f.render_widget(Paragraph::new(Line::styled(t!("xray.loading").into_owned(), ctx.theme.muted())), area)
            }
            (Some(tree), None) => {
                self.lines.set(flatten(tree, ctx));
                self.lines.render(f, area, ctx);
            }
        }
    }

    fn fetch(&mut self, _kind: FetchKind, ctx: &Ctx) -> Option<Fetch> {
        let client = ctx.client()?.clone();
        let href = self.href.clone();
        Some(async move { Ok(Payload::Xray(Box::new(build(&client, &href).await))) }.boxed())
    }

    fn on_data(&mut self, _kind: FetchKind, result: Result<Payload, Error>, _ctx: &Ctx) -> Outcome {
        match result {
            Ok(Payload::Xray(tree)) => self.tree = Some(*tree),
            Ok(_) => {}
            Err(e) => self.error = Some(super::error_text(&e)),
        }
        Outcome::None
    }

    fn poll_interval(&self, default: Duration) -> Option<Duration> {
        Some(default * 3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::NameCache;
    use crate::testutil;
    use crate::theme::Theme;

    #[test]
    fn flatten_draws_branches_and_keeps_links() {
        let tree = Node {
            kind: "distribution",
            label: "iso".into(),
            href: Some("/d/".into()),
            children: vec![
                Node::leaf("guard", "g".into(), Some("/g/".into())),
                Node {
                    kind: "repository",
                    label: "repo".into(),
                    href: Some("/r/".into()),
                    children: vec![Node::leaf("remote", "rm".into(), Some("/rm/".into()))],
                },
            ],
        };
        let (names, theme, config) = (NameCache::default(), Theme::default(), testutil::config());
        let ctx = Ctx {
            session: None,
            names: &names,
            theme: &theme,
            config: &config,
            current_ctx: "t",
            now: chrono::Utc::now(),
        };
        let lines = flatten(&tree, &ctx);
        let text: Vec<String> =
            lines.iter().map(|l| l.line.spans.iter().map(|s| s.content.as_ref()).collect::<String>()).collect();
        assert!(text[0].ends_with("iso"));
        assert!(text[1].starts_with("├─ ") && text[1].ends_with('g'));
        assert!(text[2].starts_with("└─ "));
        assert!(text[3].starts_with("   └─ ") && text[3].ends_with("rm"));
        assert_eq!(lines.iter().filter_map(|l| l.href.as_deref()).collect::<Vec<_>>(), ["/d/", "/g/", "/r/", "/rm/"]);
    }

    #[test]
    fn starting_points() {
        assert!(supports("/pulp/api/v3/distributions/file/file/1/"));
        assert!(supports("/pulp/api/v3/repositories/file/file/1/versions/2/"));
        assert!(!supports("/pulp/api/v3/remotes/file/file/1/"));
        assert_eq!(kind_of("/pulp/api/v3/repositories/file/file/1/versions/2/"), "version");
    }
}
