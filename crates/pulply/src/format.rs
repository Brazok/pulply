//! Human-friendly rendering of Pulp values.

use chrono::{DateTime, Utc};
use pulply_api::models::{is_href, version_number, version_repository};

use crate::names::NameCache;

pub fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

/// Compact duration like k9s: `42s`, `5m`, `3h`, `12d`, `2y`.
pub fn compact_duration(secs: i64) -> String {
    let secs = secs.max(0);
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m{}s", s / 60, s % 60).replace("m0s", "m"),
        s if s < 86_400 => format!("{}h{}m", s / 3600, (s % 3600) / 60).replace("h0m", "h"),
        s if s < 365 * 86_400 => format!("{}d", s / 86_400),
        s => format!("{}y", s / (365 * 86_400)),
    }
}

pub fn age(ts: &str, now: DateTime<Utc>) -> String {
    parse_time(ts).map(|t| compact_duration((now - t).num_seconds())).unwrap_or_default()
}

/// Elapsed time between two timestamps; `end = None` means still running.
pub fn elapsed(start: Option<&str>, end: Option<&str>, now: DateTime<Utc>) -> String {
    let Some(start) = start.and_then(parse_time) else { return String::new() };
    let end = end.and_then(parse_time).unwrap_or(now);
    compact_duration((end - start).num_seconds())
}

pub fn local_time(ts: &str) -> String {
    parse_time(ts)
        .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| ts.to_owned())
}

pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 { format!("{n} B") } else { format!("{value:.1} {}", UNITS[unit]) }
}

/// Display text for an href: cached name, `repo@N` for versions, else `type:shortid`.
pub fn href(href: &str, names: &NameCache) -> String {
    if let Some(name) = names.get(href) {
        return name.to_owned();
    }
    if let (Some(n), Some(repo)) = (version_number(href), version_repository(href)) {
        return match names.get(repo) {
            Some(name) => format!("{name}@{n}"),
            None => format!("{}@{n}", short_href(repo)),
        };
    }
    short_href(href)
}

/// `/pulp/api/v3/remotes/file/file/0192abcd-…/` → `remotes/file:0192abcd`.
pub fn short_href(href: &str) -> String {
    let rel = href.split("/api/v3/").nth(1).unwrap_or(href).trim_end_matches('/');
    let mut segs: Vec<&str> = rel.split('/').collect();
    match segs.pop() {
        Some(id) if segs.is_empty() => id.to_owned(),
        Some(id) => {
            let short: String = id.chars().take(8).collect();
            let kind = segs.iter().take(2).copied().collect::<Vec<_>>().join("/");
            format!("{kind}:{short}")
        }
        None => href.to_owned(),
    }
}

/// `pulp_file.app.tasks.synchronizing.synchronize` → `file: synchronize`.
pub fn task_name(name: &str) -> String {
    let plugin = name.split('.').next().unwrap_or_default();
    let plugin = plugin.strip_prefix("pulp_").unwrap_or(plugin);
    let plugin = if plugin == "pulpcore" { "core" } else { plugin };
    match name.rsplit('.').next() {
        Some(func) if func != name => format!("{plugin}: {func}"),
        _ => name.to_owned(),
    }
}

/// Renders a reserved resource record (`shared:/pulp/…`, `/pulp/…`, `prn:…`):
/// cached name when known, else `model:shortid` for PRNs.
pub fn resource_record(record: &str, names: &NameCache) -> String {
    let (prefix, rest) = match record.strip_prefix("shared:") {
        Some(rest) => ("~", rest),
        None => ("", record),
    };
    if let Some(name) = names.get(rest) {
        return format!("{prefix}{name}");
    }
    if is_href(rest) {
        return format!("{prefix}{}", href(rest, names));
    }
    match rest.strip_prefix("prn:").and_then(|p| p.split_once(':')) {
        Some((model, id)) => {
            let model = model.rsplit('.').next().unwrap_or(model);
            format!("{prefix}{model}:{}", id.chars().take(8).collect::<String>())
        }
        None => format!("{prefix}{rest}"),
    }
}

/// Domain locks are taken by every task: noise in resource columns.
pub fn is_domain_record(record: &str) -> bool {
    record.contains("prn:core.domain:") || record.contains("/api/v3/domains/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_durations() {
        assert_eq!(compact_duration(5), "5s");
        assert_eq!(compact_duration(125), "2m5s");
        assert_eq!(compact_duration(120), "2m");
        assert_eq!(compact_duration(3 * 3600 + 60), "3h1m");
        assert_eq!(compact_duration(3 * 86_400), "3d");
        assert_eq!(compact_duration(-4), "0s");
    }

    #[test]
    fn hrefs_render_with_cached_names() {
        let mut names = NameCache::default();
        let repo = "/pulp/api/v3/repositories/file/file/0192abcd-1111-2222/";
        assert_eq!(href(repo, &names), "repositories/file:0192abcd");
        names.insert(repo, "iso");
        assert_eq!(href(repo, &names), "iso");
        assert_eq!(href(&format!("{repo}versions/4/"), &names), "iso@4");
        assert_eq!(resource_record(&format!("shared:{repo}"), &names), "~iso");
        let prn = "prn:file.filerepository:01a15afc-fc12-714f";
        assert_eq!(resource_record(prn, &names), "filerepository:01a15afc");
        names.insert(prn, "empty-files");
        assert_eq!(resource_record(prn, &names), "empty-files");
    }

    #[test]
    fn task_names_are_shortened() {
        assert_eq!(task_name("pulp_file.app.tasks.synchronizing.synchronize"), "file: synchronize");
        assert_eq!(task_name("pulpcore.app.tasks.orphan.orphan_cleanup"), "core: orphan_cleanup");
        assert_eq!(task_name("plain"), "plain");
    }

    #[test]
    fn bytes_are_humanized() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1536), "1.5 KiB");
        assert_eq!(bytes(5 * 1024 * 1024 * 1024), "5.0 GiB");
    }
}
