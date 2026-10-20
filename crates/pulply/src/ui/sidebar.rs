//! Collapsible tree: Core, then one group per installed plugin.

use crossterm::event::{KeyCode, KeyEvent};
use pulply_api::{Registry, ResourceType};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use rust_i18n::t;

use crate::commands::Target;
use crate::labels;
use crate::theme::Theme;

/// Order of core endpoints in the Core group; others follow alphabetically.
const CORE_ORDER: &[&str] = &[
    "workers",
    "task-groups",
    "task-schedules",
    "repositories",
    "remotes",
    "distributions",
    "publications",
    "content",
    "repository_versions",
    "artifacts",
    "uploads",
    "contentguards",
    "users",
    "groups",
    "roles",
    "access_policies",
    "domains",
    "signing-services",
    "upstream-pulps",
];

const CATEGORY_ORDER: &[&str] = &[
    "repositories",
    "remotes",
    "distributions",
    "publications",
    "content",
    "acs",
    "contentguards",
    "exporters",
    "importers",
    "other",
];

pub struct Group {
    key: String,
    label: String,
    expanded: bool,
    items: Vec<(String, Target)>,
}

enum RowRef {
    Group(usize),
    Item(usize, usize),
}

pub enum Action {
    None,
    Ignored,
    Open(Target),
}

pub struct Sidebar {
    groups: Vec<Group>,
    state: ListState,
    loading: bool,
}

fn rank(order: &[&str], key: &str) -> usize {
    order.iter().position(|k| *k == key).unwrap_or(order.len())
}

impl Sidebar {
    /// Rebuilds from the registry, keeping groups the user expanded.
    pub fn build(reg: Option<&Registry>, previous: Option<&Sidebar>) -> Self {
        let was_expanded =
            |key: &str| previous.and_then(|p| p.groups.iter().find(|g| g.key == key)).map(|g| g.expanded);
        let mut core = vec![
            (t!("sidebar.dashboard").into_owned(), Target::Dashboard),
            (labels::category("tasks"), Target::Resource("tasks".into())),
        ];
        let mut groups = Vec::new();
        if let Some(reg) = reg {
            // Single-segment endpoints (tasks, users, …) stay in Core; pulpcore's own content
            // types (`contentguards/core/rbac`, `content/core/openpgp_*`) get a group like plugins.
            let mut core_types: Vec<&ResourceType> = reg
                .types()
                .iter()
                .filter(|t| t.is_core() && t.parent.is_none() && t.id != "tasks" && !t.id.contains('/'))
                .collect();
            core_types.sort_by_key(|t| (rank(CORE_ORDER, &t.id), t.id.clone()));
            core.extend(core_types.into_iter().map(|t| {
                let label = if t.generic {
                    format!("{} ({})", labels::type_label(t), t!("sidebar.all"))
                } else {
                    labels::type_label(t)
                };
                (label, Target::Resource(t.id.clone()))
            }));
            for plugin in std::iter::once("core").chain(reg.plugins()) {
                let mut types: Vec<&ResourceType> = reg
                    .types()
                    .iter()
                    .filter(|t| t.plugin == plugin && t.parent.is_none() && (plugin != "core" || t.id.contains('/')))
                    .collect();
                if types.is_empty() {
                    continue;
                }
                types.sort_by_key(|t| (rank(CATEGORY_ORDER, &t.category), t.name != t.plugin, t.name.clone()));
                let key = if plugin == "core" { "pulpcore" } else { plugin };
                groups.push(Group {
                    key: key.to_owned(),
                    label: key.to_owned(),
                    expanded: was_expanded(key).unwrap_or(false),
                    items: types.into_iter().map(|t| (labels::type_label(t), Target::Resource(t.id.clone()))).collect(),
                });
            }
        }
        groups.insert(
            0,
            Group {
                key: "core".into(),
                label: t!("sidebar.core").into_owned(),
                expanded: was_expanded("core").unwrap_or(true),
                items: core,
            },
        );
        let mut state = ListState::default();
        state.select(Some(previous.and_then(|p| p.state.selected()).unwrap_or(1)));
        Self { groups, state, loading: reg.is_none() }
    }

    fn rows(&self) -> Vec<RowRef> {
        let mut rows = Vec::new();
        for (gi, g) in self.groups.iter().enumerate() {
            rows.push(RowRef::Group(gi));
            if g.expanded {
                rows.extend((0..g.items.len()).map(|ii| RowRef::Item(gi, ii)));
            }
        }
        rows
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        let rows = self.rows();
        let len = rows.len();
        let cur = self.state.selected().unwrap_or(0).min(len.saturating_sub(1));
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.state.select(Some((cur + 1).min(len.saturating_sub(1)))),
            KeyCode::Up | KeyCode::Char('k') => self.state.select(Some(cur.saturating_sub(1))),
            KeyCode::Home | KeyCode::Char('g') => self.state.select(Some(0)),
            KeyCode::End | KeyCode::Char('G') => self.state.select(Some(len.saturating_sub(1))),
            KeyCode::Enter
            | KeyCode::Char(' ')
            | KeyCode::Right
            | KeyCode::Char('l')
            | KeyCode::Left
            | KeyCode::Char('h') => {
                match rows.get(cur) {
                    Some(RowRef::Group(gi)) => {
                        let g = &mut self.groups[*gi];
                        g.expanded = match key.code {
                            KeyCode::Right | KeyCode::Char('l') => true,
                            KeyCode::Left | KeyCode::Char('h') => false,
                            _ => !g.expanded,
                        };
                    }
                    Some(RowRef::Item(gi, ii)) => match key.code {
                        KeyCode::Left | KeyCode::Char('h') => {
                            // Collapse the parent group and land on its header.
                            self.groups[*gi].expanded = false;
                            let header = self.rows().iter().position(|r| matches!(r, RowRef::Group(g) if g == gi));
                            self.state.select(header);
                        }
                        _ => return Action::Open(self.groups[*gi].items[*ii].1.clone()),
                    },
                    None => {}
                }
            }
            _ => return Action::Ignored,
        }
        Action::None
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, focused: bool, active: Option<&Target>, theme: &Theme) {
        let rows = self.rows();
        let mut items: Vec<ListItem> = rows
            .iter()
            .map(|r| match r {
                RowRef::Group(gi) => {
                    let g = &self.groups[*gi];
                    let arrow = if g.expanded { "▾" } else { "▸" };
                    ListItem::new(Line::from(Span::styled(
                        format!("{arrow} {}", g.label),
                        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
                    )))
                }
                RowRef::Item(gi, ii) => {
                    let (label, target) = &self.groups[*gi].items[*ii];
                    let style = if Some(target) == active { theme.accent() } else { Style::new() };
                    ListItem::new(Line::from(Span::styled(format!("  {label}"), style)))
                }
            })
            .collect();
        if self.loading {
            items.push(ListItem::new(Line::styled(format!("  {}", t!("sidebar.loading")), theme.muted())));
        }
        let border = if focused { theme.border_focus } else { theme.border };
        let list = List::new(items)
            .block(Block::new().borders(Borders::RIGHT).border_style(Style::new().fg(border)))
            .highlight_style(if focused { theme.selected() } else { Style::new() });
        f.render_stateful_widget(list, area, &mut self.state);
    }
}
