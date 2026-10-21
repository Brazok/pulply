//! `:` command mode: aliases, resource resolution and completion.

use pulply_api::{Registry, ResourceType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Dashboard,
    Contexts,
    Context(String),
    /// Registry type id (`repositories/rpm/rpm`, `tasks`).
    Resource(String),
    Help,
    /// Connected user and its permissions.
    Me,
    Quit,
    RefreshSchema,
    /// Global operation form (`tasks/purge/`, `orphans/cleanup/`, `repositories/reclaim_space/`, `repair/`).
    Operation(Op),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Purge,
    Orphans,
    Reclaim,
    Repair,
}

impl Op {
    /// Path relative to the API prefix.
    pub fn rel_path(self) -> &'static str {
        match self {
            Op::Purge => "tasks/purge/",
            Op::Orphans => "orphans/cleanup/",
            Op::Reclaim => "repositories/reclaim_space/",
            Op::Repair => "repair/",
        }
    }

    /// Locale key suffix (`op.<key>.title`, `op.<key>.confirm`).
    pub fn key(self) -> &'static str {
        match self {
            Op::Purge => "purge",
            Op::Orphans => "orphans",
            Op::Reclaim => "reclaim",
            Op::Repair => "repair",
        }
    }
}

/// (canonical category/core id, aliases; the first alias is used for completion).
const ALIASES: &[(&str, &[&str])] = &[
    ("repositories", &["repo", "repos", "repository", "repositories"]),
    ("remotes", &["remote", "remotes", "rm"]),
    ("distributions", &["dist", "dists", "distribution", "distributions"]),
    ("publications", &["pub", "pubs", "publication", "publications"]),
    ("content", &["content", "co"]),
    ("acs", &["acs"]),
    ("contentguards", &["guard", "guards", "contentguard", "contentguards"]),
    ("exporters", &["exporter", "exporters"]),
    ("importers", &["importer", "importers"]),
    ("tasks", &["task", "tasks"]),
    ("task-groups", &["taskgroup", "taskgroups", "tg"]),
    ("task-schedules", &["schedule", "schedules"]),
    ("workers", &["worker", "workers", "wk"]),
    ("users", &["user", "users"]),
    ("groups", &["group", "groups"]),
    ("roles", &["role", "roles"]),
    ("artifacts", &["artifact", "artifacts"]),
    ("uploads", &["upload", "uploads"]),
    ("access_policies", &["policy", "policies", "ap"]),
    ("signing-services", &["signing", "signing-services"]),
    ("upstream-pulps", &["upstream", "upstreams"]),
    ("domains", &["domain", "domains"]),
    ("repository_versions", &["version", "versions", "rv"]),
];

const BUILTINS: &[&str] =
    &["dashboard", "ctx", "help", "me", "quit", "refresh-schema", "purge", "orphans", "reclaim", "repair"];

fn canonical(word: &str) -> Option<&'static str> {
    ALIASES.iter().find(|(_, al)| al.contains(&word)).map(|(c, _)| *c)
}

fn short_alias(category: &str) -> Option<&'static str> {
    ALIASES.iter().find(|(c, _)| *c == category).map(|(_, al)| al[0])
}

/// Parses a command line (without the leading `:`).
pub fn parse(input: &str, reg: Option<&Registry>) -> Result<Target, String> {
    let mut words = input.split_whitespace();
    let Some(head) = words.next() else { return Err(String::new()) };
    let arg = words.collect::<Vec<_>>().join("/");
    match head {
        "dash" | "dashboard" | "home" => return Ok(Target::Dashboard),
        "ctx" | "context" | "contexts" => {
            return Ok(if arg.is_empty() { Target::Contexts } else { Target::Context(arg) });
        }
        "q" | "q!" | "quit" | "exit" => return Ok(Target::Quit),
        "help" | "?" => return Ok(Target::Help),
        "me" | "whoami" => return Ok(Target::Me),
        "refresh-schema" => return Ok(Target::RefreshSchema),
        "purge" | "task-purge" => return Ok(Target::Operation(Op::Purge)),
        "orphans" | "orphan" => return Ok(Target::Operation(Op::Orphans)),
        "reclaim" => return Ok(Target::Operation(Op::Reclaim)),
        "repair" => return Ok(Target::Operation(Op::Repair)),
        _ => {}
    }
    let Some(reg) = reg else { return Err(format!("schema not loaded yet: {input}")) };
    if reg.get(head).is_some() && arg.is_empty() {
        return Ok(Target::Resource(head.to_owned()));
    }
    let category = canonical(head).unwrap_or(head);
    resolve(reg, category, &arg)
        .map(|t| Target::Resource(t.id.clone()))
        .ok_or_else(|| format!("unknown resource: {input}"))
}

fn resolve<'a>(reg: &'a Registry, category: &str, arg: &str) -> Option<&'a ResourceType> {
    let top = || reg.types().iter().filter(move |t| t.category == category && t.parent.is_none());
    if arg.is_empty() {
        // Core endpoint, cross-plugin list, or the only type of the category.
        return reg.get(category).or_else(|| {
            let mut it = top();
            let first = it.next()?;
            it.next().is_none().then_some(first)
        });
    }
    let (plugin, name) = arg.split_once('/').map_or((arg, None), |(p, n)| (p, Some(n)));
    let mut candidates = top().filter(|t| t.plugin == plugin);
    match name {
        Some(n) => candidates.find(|t| t.name == n),
        None => {
            let all: Vec<_> = candidates.collect();
            all.iter()
                .find(|t| t.name == plugin)
                .or_else(|| all.iter().find(|t| t.name == "packages"))
                .or_else(|| all.first())
                .copied()
        }
    }
}

/// Every completable command, shortest forms first.
pub fn completions(reg: Option<&Registry>, contexts: &[String]) -> Vec<String> {
    let mut out: Vec<String> = BUILTINS.iter().map(|s| (*s).to_owned()).collect();
    out.extend(contexts.iter().map(|c| format!("ctx {c}")));
    if let Some(reg) = reg {
        for t in reg.types().iter().filter(|t| t.parent.is_none()) {
            match (t.is_core() || t.category == "other", short_alias(&t.category)) {
                (true, Some(alias)) if t.generic || t.id == t.category => out.push(alias.to_owned()),
                (true, _) => out.push(t.id.clone()),
                (false, Some(alias)) => {
                    out.push(format!("{alias} {}", t.plugin));
                    if t.name != t.plugin {
                        out.push(format!("{alias} {}/{}", t.plugin, t.name));
                    }
                }
                (false, None) => out.push(t.id.clone()),
            }
        }
    }
    out.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    out.dedup();
    out
}

/// First completion extending `input`, used as inline ghost text.
pub fn suggest<'a>(input: &str, candidates: &'a [String]) -> Option<&'a str> {
    if input.is_empty() {
        return None;
    }
    candidates.iter().map(String::as_str).find(|c| c.len() > input.len() && c.starts_with(input))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::registry;

    #[test]
    fn resolves_aliases_with_plugin_qualifiers() {
        let reg = registry();
        let p = |s| parse(s, Some(&reg));
        assert_eq!(p("repo"), Ok(Target::Resource("repositories".into())));
        assert_eq!(p("repo rpm"), Ok(Target::Resource("repositories/rpm/rpm".into())));
        assert_eq!(
            p("repo container container-push"),
            Ok(Target::Resource("repositories/container/container-push".into()))
        );
        assert_eq!(p("content rpm"), Ok(Target::Resource("content/rpm/packages".into())));
        assert_eq!(p("content container/tags"), Ok(Target::Resource("content/container/tags".into())));
        assert_eq!(p("task"), Ok(Target::Resource("tasks".into())));
        assert_eq!(p("remotes/file/git"), Ok(Target::Resource("remotes/file/git".into())));
        assert_eq!(p("ctx lab"), Ok(Target::Context("lab".into())));
        assert!(p("repo nope").is_err());
    }

    #[test]
    fn builtins_work_without_schema() {
        assert_eq!(parse("ctx", None), Ok(Target::Contexts));
        assert_eq!(parse("q", None), Ok(Target::Quit));
        assert!(parse("repo", None).is_err());
        assert_eq!(parse("orphan", None), Ok(Target::Operation(Op::Orphans)));
        assert_eq!(parse("task-purge", None), Ok(Target::Operation(Op::Purge)));
    }

    #[test]
    fn completion_offers_short_plugin_forms() {
        let reg = registry();
        let all = completions(Some(&reg), &["lab".into()]);
        for expected in
            ["repo rpm", "content rpm/advisories", "task", "worker", "ctx lab", "dist container/pull-through"]
        {
            assert!(all.iter().any(|c| c == expected), "missing {expected}");
        }
        assert_eq!(suggest("repo r", &all), Some("repo rpm"));
        assert_eq!(suggest("", &all), None);
    }
}
