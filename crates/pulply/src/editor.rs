//! Edit a resource as YAML in `$VISUAL`/`$EDITOR` (R7.5): only writable fields are
//! offered, changed ones are PATCHed; invalid YAML or a server refusal reopens the
//! editor with the error as a comment on top.

use std::path::PathBuf;

use pulply_api::{Method, PulpClient};
use ratatui::DefaultTerminal;
use rust_i18n::t;
use serde_json::{Map, Value};

pub enum EditOutcome {
    Cancelled,
    Unchanged,
    /// Server response to the PATCH (resource or `{"task": …}`).
    Applied(Value),
}

/// Writable fields of `resource` as a YAML document.
pub fn editable(resource: &Value, writable: &[String]) -> Map<String, Value> {
    writable.iter().filter_map(|k| resource.get(k).map(|v| (k.clone(), v.clone()))).collect()
}

/// PATCH body: changed or added keys. Removed keys are left untouched; unknown keys are an error.
pub fn diff(
    original: &Map<String, Value>,
    edited: &Map<String, Value>,
    writable: &[String],
) -> Result<Map<String, Value>, String> {
    if let Some(unknown) = edited.keys().find(|k| !writable.contains(k)) {
        return Err(t!("editor.unknown_field", field = unknown).into_owned());
    }
    Ok(edited.iter().filter(|(k, v)| original.get(*k) != Some(*v)).map(|(k, v)| (k.clone(), v.clone())).collect())
}

/// Parses the edited file; `Ok(None)` when it is empty (comments ignored), i.e. cancelled.
pub fn parse(text: &str) -> Result<Option<Map<String, Value>>, String> {
    let content: String = text.lines().filter(|l| !l.trim_start().starts_with('#')).collect::<Vec<_>>().join("\n");
    if content.trim().is_empty() {
        return Ok(None);
    }
    match serde_norway::from_str::<Value>(&content).map_err(|e| e.to_string())? {
        Value::Object(map) => Ok(Some(map)),
        _ => Err(t!("editor.not_mapping").into_owned()),
    }
}

fn editor_command() -> (String, Vec<String>) {
    let raw = std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR")).unwrap_or_else(|_| "vi".into());
    let mut parts = raw.split_whitespace().map(str::to_owned);
    let program = parts.next().unwrap_or_else(|| "vi".into());
    (program, parts.collect())
}

fn header(href: &str, error: Option<&str>) -> String {
    let mut h = format!("# pulply: {href}\n# {}\n", t!("editor.help"));
    if let Some(e) = error {
        for line in e.lines() {
            h.push_str(&format!("# {}: {line}\n", t!("editor.error")));
        }
    }
    h
}

/// Runs the edit loop. The caller must not read terminal events meanwhile.
pub async fn edit(
    terminal: &mut DefaultTerminal,
    client: &PulpClient,
    href: &str,
    writable: &[String],
) -> Result<EditOutcome, String> {
    let resource = client.get(href, &[]).await.map_err(|e| e.to_string())?;
    let original = editable(&resource, writable);
    let mut body = serde_norway::to_string(&Value::Object(original.clone())).map_err(|e| e.to_string())?;
    let path: PathBuf = std::env::temp_dir().join(format!("pulply-edit-{}.yaml", std::process::id()));
    let mut error: Option<String> = None;
    let outcome = loop {
        std::fs::write(&path, format!("{}{body}", header(href, error.as_deref()))).map_err(|e| e.to_string())?;
        let (program, args) = editor_command();
        ratatui::restore();
        let status = tokio::process::Command::new(&program).args(&args).arg(&path).status().await;
        // Re-entering the alternate screen (`?1049h`) clears it; a fresh Terminal redraws fully.
        *terminal = ratatui::init();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => break Err(t!("editor.exit", program = program, code = s).into_owned()),
            Err(e) => break Err(format!("{program}: {e}")),
        }
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        body = text.lines().filter(|l| !l.starts_with("# ")).collect::<Vec<_>>().join("\n");
        let edited = match parse(&text) {
            Ok(Some(map)) => map,
            Ok(None) => break Ok(EditOutcome::Cancelled),
            Err(e) => {
                error = Some(e);
                continue;
            }
        };
        let patch = match diff(&original, &edited, writable) {
            Ok(p) if p.is_empty() => break Ok(EditOutcome::Unchanged),
            Ok(p) => p,
            Err(e) => {
                error = Some(e);
                continue;
            }
        };
        match client.mutate(Method::PATCH, href, Some(&Value::Object(patch))).await {
            Ok(v) => break Ok(EditOutcome::Applied(v)),
            Err(e) => error = Some(e.to_string()),
        }
    };
    let _ = std::fs::remove_file(&path);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn writable() -> Vec<String> {
        ["name", "description", "retain_repo_versions", "pulp_labels"].map(String::from).to_vec()
    }

    #[test]
    fn only_writable_fields_are_offered() {
        let resource = json!({"name": "iso", "pulp_href": "/x/", "description": null, "versions_href": "/x/v/"});
        let map = editable(&resource, &writable());
        assert_eq!(Value::Object(map), json!({"name": "iso", "description": null}));
    }

    #[test]
    fn diff_keeps_changes_and_rejects_unknown_fields() {
        let original = editable(&json!({"name": "iso", "description": "a", "pulp_labels": {"k": "v"}}), &writable());
        let edited = parse("# comment\nname: iso\ndescription: b\npulp_labels:\n  k: v\nretain_repo_versions: 2\n")
            .unwrap()
            .unwrap();
        let patch = diff(&original, &edited, &writable()).unwrap();
        assert_eq!(Value::Object(patch), json!({"description": "b", "retain_repo_versions": 2}));
        let bad = parse("pulp_href: /y/").unwrap().unwrap();
        assert!(diff(&original, &bad, &writable()).is_err());
    }

    #[test]
    fn empty_file_cancels_and_invalid_yaml_errors() {
        assert!(matches!(parse("# only comments\n\n"), Ok(None)));
        assert!(parse("name: [unclosed").is_err());
        assert!(parse("- a list").is_err(), "top level must be a mapping");
    }
}
