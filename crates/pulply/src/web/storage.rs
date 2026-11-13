//! Disk usage breakdown, computed on the pulply server in the background.
//!
//! Pulp has no aggregation endpoint: sizes live on artifacts, and only content units know
//! which artifacts they use. On a production server this means reading every artifact and
//! every content unit, so the scan runs as a job (one per context, started on demand,
//! cancellable, with progress) instead of in the browser. Memory stays compact: one entry
//! per artifact (UUID as `u128` → size and owning content type). The last report is kept on
//! disk so a reload or a restart of `pulply serve` does not lose it.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use futures::{StreamExt, TryStreamExt, stream};
use pulply_api::{Method, PulpClient, Registry};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Rows per request: large pages keep the number of (offset-paginated) requests low.
const PAGE: u64 = 1000;
/// Pages fetched at the same time.
const PARALLEL: usize = 4;
/// Largest content units kept per plugin.
const TOP: usize = 3;
/// Repositories listed in the report, largest first.
const TOP_REPOS: usize = 50;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Usage {
    pub size: u64,
    pub count: u64,
}

impl Usage {
    fn add(&mut self, size: u64) {
        self.size += size;
        self.count += 1;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeUsage {
    pub id: String,
    #[serde(flatten)]
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TopItem {
    pub href: String,
    pub type_id: String,
    pub label: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginUsage {
    pub plugin: String,
    #[serde(flatten)]
    pub usage: Usage,
    pub types: Vec<TypeUsage>,
    /// Largest content units of the plugin (size of all their artifacts).
    pub top: Vec<TopItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoUsage {
    pub href: String,
    pub name: String,
    pub plugin: String,
    #[serde(flatten)]
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    /// RFC 3339.
    pub computed_at: String,
    pub duration_secs: u64,
    pub total: Usage,
    pub orphaned: Usage,
    pub unattributed: Usage,
    pub plugins: Vec<PluginUsage>,
    pub repositories: Vec<RepoUsage>,
    pub repositories_total: u64,
    /// Content types that could not be read.
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Progress {
    /// 1-based step and number of steps.
    pub step: u32,
    pub steps: u32,
    pub label: String,
    pub done: u64,
    pub total: u64,
}

#[derive(Default)]
struct Inner {
    running: bool,
    started_at: Option<String>,
    progress: Progress,
    error: Option<String>,
    report: Option<Arc<Report>>,
    loaded: bool,
}

/// The job of one connected context.
#[derive(Default)]
pub struct StorageJob {
    inner: Mutex<Inner>,
    cancel: AtomicBool,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub running: bool,
    pub started_at: Option<String>,
    pub progress: Option<Progress>,
    pub error: Option<String>,
    /// Shared with the job, not copied.
    pub report: Option<Arc<Report>>,
}

impl StorageJob {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Current state; the last report is read from disk the first time.
    pub fn snapshot(&self, cache: Option<&PathBuf>) -> Snapshot {
        let mut inner = self.lock();
        if !inner.loaded {
            inner.loaded = true;
            if inner.report.is_none()
                && let Some(file) = cache
                && let Ok(bytes) = std::fs::read(file)
                && let Ok(report) = serde_json::from_slice::<Report>(&bytes)
            {
                inner.report = Some(Arc::new(report));
            }
        }
        Snapshot {
            running: inner.running,
            started_at: inner.started_at.clone(),
            progress: inner.running.then(|| inner.progress.clone()),
            error: inner.error.clone(),
            report: inner.report.clone(),
        }
    }

    /// Starts a scan unless one is running. Returns false when already running.
    pub fn start(self: &Arc<Self>, client: PulpClient, registry: Arc<Registry>, cache: Option<PathBuf>) -> bool {
        {
            let mut inner = self.lock();
            if inner.running {
                return false;
            }
            inner.running = true;
            inner.started_at = Some(Utc::now().to_rfc3339());
            inner.error = None;
            inner.progress = Progress::default();
        }
        self.cancel.store(false, Ordering::SeqCst);
        let job = self.clone();
        tokio::spawn(async move {
            let started = Instant::now();
            let result = scan(&job, &client, &registry).await;
            let mut inner = job.lock();
            inner.running = false;
            match result {
                Ok(mut report) => {
                    report.duration_secs = started.elapsed().as_secs();
                    if let Some(file) = &cache {
                        if let Some(dir) = file.parent() {
                            let _ = std::fs::create_dir_all(dir);
                        }
                        if let Ok(bytes) = serde_json::to_vec(&report)
                            && let Err(e) = std::fs::write(file, bytes)
                        {
                            tracing::warn!("cannot save the storage report: {e}");
                        }
                    }
                    inner.report = Some(Arc::new(report));
                }
                Err(e) => {
                    tracing::warn!("storage scan: {e}");
                    inner.error = Some(e);
                }
            }
        });
        true
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    fn cancelled(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::SeqCst) { Err("Cancelled".into()) } else { Ok(()) }
    }

    fn set_step(&self, step: u32, steps: u32, label: impl Into<String>, total: u64) {
        let mut inner = self.lock();
        inner.progress = Progress { step, steps, label: label.into(), done: 0, total };
    }

    fn advance(&self, rows: u64) {
        self.lock().progress.done += rows;
    }
}

/// Last path segment of an href (`…/artifacts/0192-…/`) as a number, for compact maps.
pub fn href_id(href: &str) -> Option<u128> {
    let seg = href.trim_end_matches('/').rsplit('/').next()?;
    let hex: String = seg.chars().filter(|c| *c != '-').collect();
    if hex.len() == 32 { u128::from_str_radix(&hex, 16).ok() } else { None }
}

/// Artifact hrefs referenced by a content unit (`artifact`, or the values of `artifacts`).
pub fn artifact_refs(content: &Value) -> Vec<&str> {
    let mut out = Vec::new();
    if let Some(a) = content.get("artifact").and_then(Value::as_str) {
        out.push(a);
    }
    if let Some(map) = content.get("artifacts").and_then(Value::as_object) {
        out.extend(map.values().filter_map(Value::as_str));
    }
    out
}

/// One page of a list, retried on transient errors.
async fn page(client: &PulpClient, path: &str, query: &[(String, String)]) -> Result<Vec<Value>, String> {
    let mut delay = Duration::from_millis(500);
    for attempt in 1..=4 {
        let result = async {
            let resp = client
                .raw(Method::GET, path)
                .map_err(|e| e.to_string())?
                .query(query)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("{path}: HTTP {status}: {}", body.chars().take(200).collect::<String>()));
            }
            let page: pulply_api::Page = resp.json().await.map_err(|e| format!("{path}: {e}"))?;
            Ok(page.results)
        }
        .await;
        match result {
            Ok(rows) => return Ok(rows),
            Err(e) if attempt == 4 => return Err(e),
            Err(e) => {
                tracing::debug!("storage scan, retrying: {e}");
                tokio::time::sleep(delay).await;
                delay *= 2;
            }
        }
    }
    unreachable!()
}

/// `count` of a list. Without `fields`: some plugins answer 500 to `fields=` on their content.
async fn count(client: &PulpClient, path: &str, params: &[(&str, &str)]) -> Result<u64, String> {
    let mut q: Vec<(String, String)> =
        params.iter().filter(|(k, _)| *k != "fields").map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
    q.push(("limit".into(), "1".into()));
    let resp =
        client.raw(Method::GET, path).map_err(|e| e.to_string())?.query(&q).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("{path}: HTTP {status}"));
    }
    let page: pulply_api::Page = resp.json().await.map_err(|e| format!("{path}: {e}"))?;
    Ok(page.count)
}

/// Every row of a list, `PARALLEL` pages at a time, handed to `each` page by page.
/// `ordered`: the list accepts `ordering=pulp_created`, which keeps offsets stable.
async fn walk(
    job: &StorageJob,
    client: &PulpClient,
    path: &str,
    params: &[(&str, &str)],
    ordered: bool,
    mut each: impl FnMut(Vec<Value>),
) -> Result<u64, String> {
    let base: Vec<(String, String)> = params.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
    let count = count(client, path, params).await?;
    let offsets: Vec<u64> = (0..count.div_ceil(PAGE)).map(|i| i * PAGE).collect();
    let mut pages = stream::iter(offsets)
        .map(|offset| {
            let mut q = base.clone();
            // Stable order, so that offsets do not skip or repeat rows.
            if ordered {
                q.push(("ordering".into(), "pulp_created".into()));
            }
            q.push(("limit".into(), PAGE.to_string()));
            q.push(("offset".into(), offset.to_string()));
            async move { page(client, path, &q).await }
        })
        .buffer_unordered(PARALLEL);
    while let Some(rows) = pages.try_next().await? {
        job.cancelled()?;
        job.advance(rows.len() as u64);
        each(rows);
    }
    Ok(count)
}

/// Owner of an artifact: index in the list of content types (`u16::MAX`: none).
const NO_OWNER: u16 = u16::MAX;

struct Source {
    type_id: String,
    plugin: String,
    path: String,
    fields: String,
    ordered: bool,
}

fn sources(reg: &Registry) -> Vec<Source> {
    reg.types()
        .iter()
        .filter(|t| t.category == "content" && !t.generic && t.parent.is_none() && t.list_path.is_some())
        .filter_map(|t| {
            let props = &reg.schema(t.read_schema.as_deref()?)?.properties;
            let fields: Vec<&str> = ["artifact", "artifacts"].into_iter().filter(|f| props.contains_key(*f)).collect();
            (!fields.is_empty()).then(|| Source {
                type_id: t.id.clone(),
                plugin: t.plugin.clone(),
                path: t.list_path.clone().unwrap_or_default(),
                fields: format!("pulp_href,{}", fields.join(",")),
                ordered: t.ordering_fields().iter().any(|o| o == "pulp_created"),
            })
        })
        .collect()
}

/// Keeps the `TOP` largest items, largest first.
pub fn push_top(top: &mut Vec<TopItem>, item: TopItem) {
    if top.len() == TOP && top.last().is_some_and(|l| l.size >= item.size) {
        return;
    }
    let at = top.iter().position(|t| t.size < item.size).unwrap_or(top.len());
    top.insert(at, item);
    top.truncate(TOP);
}

/// Readable name of a content unit.
fn content_label(item: &Value) -> Option<String> {
    let s = |k: &str| item.get(k).and_then(Value::as_str).filter(|v| !v.is_empty()).map(str::to_owned);
    if let Some(v) = s("relative_path").or_else(|| s("location_href")).or_else(|| s("filename")) {
        return Some(v);
    }
    match (s("name"), s("version")) {
        (Some(n), Some(v)) => Some(format!("{n} {v}")),
        (Some(n), None) => Some(n),
        _ => s("digest").or_else(|| s("repo_id")),
    }
}

async fn scan(job: &StorageJob, client: &PulpClient, reg: &Registry) -> Result<Report, String> {
    let api = |rel: &str| client.api_path(rel);
    let sources = sources(reg);
    let steps = 5;

    // 1. Every artifact: id → (size, owner).
    let artifacts_path = api("artifacts/");
    let total = count(client, &artifacts_path, &[]).await?;
    job.set_step(1, steps, "Reading artifacts", total);
    let mut artifacts: HashMap<u128, (u64, u16)> = HashMap::with_capacity(total as usize);
    walk(job, client, &artifacts_path, &[("fields", "pulp_href,size")], true, |rows| {
        for a in rows {
            if let Some(id) = a.get("pulp_href").and_then(Value::as_str).and_then(href_id) {
                artifacts.insert(id, (a.get("size").and_then(Value::as_u64).unwrap_or(0), NO_OWNER));
            }
        }
    })
    .await?;

    // 2. Orphaned artifacts (no content uses them).
    let orphan_total = count(client, &artifacts_path, &[("orphaned_for", "0")]).await?;
    job.set_step(2, steps, "Reading orphaned artifacts", orphan_total);
    let mut orphans: HashSet<u128> = HashSet::with_capacity(orphan_total as usize);
    walk(job, client, &artifacts_path, &[("fields", "pulp_href"), ("orphaned_for", "0")], true, |rows| {
        orphans.extend(rows.iter().filter_map(|a| a.get("pulp_href").and_then(Value::as_str).and_then(href_id)));
    })
    .await?;

    // 3. Content units: who uses which artifact, and the largest units of each plugin.
    // A type the server fails to list is skipped (its files count as unattributed).
    let mut warnings = Vec::new();
    let mut counts = Vec::with_capacity(sources.len());
    for s in &sources {
        match count(client, &s.path, &[]).await {
            Ok(n) => counts.push(Some(n)),
            Err(e) => {
                warnings.push(format!("{}: {e}", s.type_id));
                counts.push(None);
            }
        }
    }
    job.set_step(3, steps, "Reading content", counts.iter().flatten().sum());
    let mut tops: HashMap<String, Vec<TopItem>> = HashMap::new();
    for (idx, s) in sources.iter().enumerate() {
        if counts[idx].is_none_or(|n| n == 0) {
            continue;
        }
        let owner = u16::try_from(idx).unwrap_or(NO_OWNER - 1);
        let mut attribute = |rows: Vec<Value>| {
            for c in rows {
                let mut size = 0;
                for href in artifact_refs(&c) {
                    if let Some(entry) = href_id(href).and_then(|id| artifacts.get_mut(&id)) {
                        size += entry.0;
                        if entry.1 == NO_OWNER {
                            entry.1 = owner;
                        }
                    }
                }
                if size > 0
                    && let Some(href) = c.get("pulp_href").and_then(Value::as_str)
                {
                    let item =
                        TopItem { href: href.to_owned(), type_id: s.type_id.clone(), label: String::new(), size };
                    push_top(tops.entry(s.plugin.clone()).or_default(), item);
                }
            }
        };
        // `fields=` keeps pages small; some plugins fail on it, then whole units are read.
        let first = walk(job, client, &s.path, &[("fields", s.fields.as_str())], s.ordered, &mut attribute).await;
        if let Err(e) = first {
            job.cancelled()?;
            tracing::debug!("{}: {e}, retrying without fields", s.type_id);
            if let Err(e) = walk(job, client, &s.path, &[], s.ordered, &mut attribute).await {
                job.cancelled()?;
                warnings.push(format!("{}: {e}", s.type_id));
            }
        }
    }

    // Totals by plugin and content type.
    let mut report = aggregate(&artifacts, &orphans, &sources);
    report.warnings = warnings;
    for p in &mut report.plugins {
        p.top = tops.remove(&p.plugin).unwrap_or_default();
    }
    drop(orphans);

    // 4. Names of the largest units.
    let labels: Vec<String> = report.plugins.iter().flat_map(|p| p.top.iter().map(|t| t.href.clone())).collect();
    job.set_step(4, steps, "Naming the largest items", labels.len() as u64);
    for p in &mut report.plugins {
        for t in &mut p.top {
            job.cancelled()?;
            t.label = match client.get(&t.href, &[]).await {
                Ok(item) => content_label(&item).unwrap_or_else(|| t.href.clone()),
                Err(_) => t.href.clone(),
            };
            job.advance(1);
        }
    }

    // 5. Repositories: files of their latest version (shared files count in each).
    if let Some(list) = reg.get("repositories").and_then(|t| t.list_path.clone()) {
        let mut repos: Vec<Value> = Vec::new();
        let n = walk(job, client, &list, &[("fields", "pulp_href,name,latest_version_href")], true, |rows| {
            repos.extend(rows)
        })
        .await?;
        job.set_step(5, steps, "Measuring repositories", n);
        let mut out = Vec::with_capacity(repos.len());
        for r in &repos {
            job.cancelled()?;
            let href = r.get("pulp_href").and_then(Value::as_str).unwrap_or_default().to_owned();
            let mut usage = Usage::default();
            if let Some(version) = r.get("latest_version_href").and_then(Value::as_str) {
                let mut ids = Vec::new();
                // The inner walk reports artifact rows; the step counts repositories.
                let before = job.lock().progress.done;
                let walked = walk(
                    job,
                    client,
                    &artifacts_path,
                    &[("fields", "pulp_href"), ("repository_version", version)],
                    true,
                    |rows| {
                        ids.extend(
                            rows.iter().filter_map(|a| a.get("pulp_href").and_then(Value::as_str).and_then(href_id)),
                        );
                    },
                )
                .await;
                if let Err(e) = walked {
                    job.cancelled()?;
                    report.warnings.push(format!("{href}: {e}"));
                }
                job.lock().progress.done = before;
                for id in ids {
                    usage.add(artifacts.get(&id).map_or(0, |a| a.0));
                }
            }
            job.advance(1);
            let plugin = reg.type_for_href(&href).map_or_else(|| "core".to_owned(), |t| t.plugin.clone());
            let name = r.get("name").and_then(Value::as_str).unwrap_or(&href).to_owned();
            out.push(RepoUsage { href, name, plugin, usage });
        }
        out.sort_by_key(|r| std::cmp::Reverse(r.usage.size));
        report.repositories_total = out.len() as u64;
        out.truncate(TOP_REPOS);
        report.repositories = out;
    }
    Ok(report)
}

fn aggregate(artifacts: &HashMap<u128, (u64, u16)>, orphans: &HashSet<u128>, sources: &[Source]) -> Report {
    let mut total = Usage::default();
    let mut orphaned = Usage::default();
    let mut unattributed = Usage::default();
    let mut by_type: Vec<Usage> = vec![Usage::default(); sources.len()];
    for (id, (size, owner)) in artifacts {
        total.add(*size);
        if let Some(u) = by_type.get_mut(usize::from(*owner)) {
            u.add(*size);
        } else if orphans.contains(id) {
            orphaned.add(*size);
        } else {
            unattributed.add(*size);
        }
    }
    let mut plugins: Vec<PluginUsage> = Vec::new();
    for (s, usage) in sources.iter().zip(by_type) {
        if usage.count == 0 {
            continue;
        }
        let p = match plugins.iter_mut().position(|p| p.plugin == s.plugin) {
            Some(i) => &mut plugins[i],
            None => {
                plugins.push(PluginUsage {
                    plugin: s.plugin.clone(),
                    usage: Usage::default(),
                    types: vec![],
                    top: vec![],
                });
                plugins.last_mut().expect("just pushed")
            }
        };
        p.usage.size += usage.size;
        p.usage.count += usage.count;
        p.types.push(TypeUsage { id: s.type_id.clone(), usage });
    }
    for p in &mut plugins {
        p.types.sort_by_key(|t| std::cmp::Reverse(t.usage.size));
    }
    plugins.sort_by_key(|p| std::cmp::Reverse(p.usage.size));
    Report {
        computed_at: Utc::now().to_rfc3339(),
        duration_secs: 0,
        total,
        orphaned,
        unattributed,
        plugins,
        repositories: vec![],
        repositories_total: 0,
        warnings: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_ids_and_artifact_references() {
        assert_eq!(
            href_id("/pulp/api/v3/artifacts/01a1d261-30d1-78f0-b4ac-3d5352f7c3f5/"),
            Some(0x01a1d26130d178f0b4ac3d5352f7c3f5)
        );
        assert_eq!(href_id("/pulp/api/v3/users/12/"), None);
        assert_eq!(artifact_refs(&json!({"artifact": "/a/"})), ["/a/"]);
        assert_eq!(artifact_refs(&json!({"artifacts": {"x": "/b/", "y": "/c/"}})), ["/b/", "/c/"]);
        assert!(artifact_refs(&json!({"artifact": null})).is_empty());
    }

    #[test]
    fn keeps_the_largest_items() {
        let mut top = Vec::new();
        for (i, size) in [5u64, 1, 9, 7, 3].into_iter().enumerate() {
            push_top(&mut top, TopItem { href: i.to_string(), type_id: String::new(), label: String::new(), size });
        }
        assert_eq!(top.iter().map(|t| t.size).collect::<Vec<_>>(), [9, 7, 5]);
    }

    #[test]
    fn attributes_artifacts_to_types_orphans_or_other() {
        let sources = vec![
            Source {
                type_id: "content/rpm/packages".into(),
                plugin: "rpm".into(),
                path: String::new(),
                fields: String::new(),
                ordered: true,
            },
            Source {
                type_id: "content/file/files".into(),
                plugin: "file".into(),
                path: String::new(),
                fields: String::new(),
                ordered: true,
            },
        ];
        let artifacts = HashMap::from([(1, (100, 0)), (2, (50, 1)), (3, (7, NO_OWNER)), (4, (3, NO_OWNER))]);
        let r = aggregate(&artifacts, &HashSet::from([3]), &sources);
        assert_eq!(r.total, Usage { size: 160, count: 4 });
        assert_eq!(r.orphaned, Usage { size: 7, count: 1 });
        assert_eq!(r.unattributed, Usage { size: 3, count: 1 });
        assert_eq!(
            r.plugins.iter().map(|p| (p.plugin.as_str(), p.usage.size)).collect::<Vec<_>>(),
            [("rpm", 100), ("file", 50)]
        );
    }
}
