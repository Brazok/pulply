//! Chunked uploads (`uploads/` API) and form posts used to create content from them.

use std::path::{Path, PathBuf};

use reqwest::Method;
use reqwest::multipart::{Form, Part};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

use crate::client::PulpClient;
use crate::error::{Error, Result};
use crate::models::ResourceExt;

/// An upload whose chunks are all on the server, ready to be turned into content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uploaded {
    pub href: String,
    pub size: u64,
    pub sha256: String,
}

impl PulpClient {
    /// Creates an upload and sends `path` in `chunk_size` pieces (`Content-Range`),
    /// calling `progress(sent, total)` after each chunk. Refused in read-only mode.
    pub async fn upload_file(
        &self,
        path: &Path,
        chunk_size: usize,
        mut progress: impl FnMut(u64, u64) + Send,
    ) -> Result<Uploaded> {
        let io = |e: std::io::Error| Error::Config(format!("{}: {e}", path.display()));
        let mut file = tokio::fs::File::open(path).await.map_err(io)?;
        let total = file.metadata().await.map_err(io)?.len();
        let created = self.mutate(Method::POST, &self.api_path("uploads/"), Some(&json!({"size": total}))).await?;
        let href = created.href().ok_or_else(|| Error::Decode("upload without pulp_href".into()))?.to_owned();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; chunk_size.max(1)];
        let mut sent = 0u64;
        progress(0, total);
        while sent < total {
            let want = (total - sent).min(buf.len() as u64) as usize;
            file.read_exact(&mut buf[..want]).await.map_err(io)?;
            let chunk = &buf[..want];
            hasher.update(chunk);
            let range = format!("bytes {}-{}/{total}", sent, sent + want as u64 - 1);
            self.put_chunk(&href, &range, chunk.to_vec(), &name).await?;
            sent += want as u64;
            progress(sent, total);
        }
        Ok(Uploaded { href, size: total, sha256: hex::encode(hasher.finalize()) })
    }

    async fn put_chunk(&self, href: &str, range: &str, data: Vec<u8>, name: &str) -> Result<()> {
        if self.readonly() {
            return Err(Error::ReadOnly { method: "PUT", path: href.to_owned() });
        }
        let form = Form::new().part("file", Part::bytes(data).file_name(name.to_owned()));
        let resp =
            self.request(Method::PUT, &self.url(href)).header("Content-Range", range).multipart(form).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::from_response(status.as_u16(), href, &text));
        }
        Ok(())
    }

    /// `application/x-www-form-urlencoded` POST (content creation endpoints that take an
    /// `upload` do not accept JSON). Refused in read-only mode.
    pub async fn post_form(&self, path: &str, fields: &[(String, String)]) -> Result<Value> {
        if self.readonly() {
            return Err(Error::ReadOnly { method: "POST", path: path.to_owned() });
        }
        let resp = self.request(Method::POST, &self.url(path)).form(fields).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(Error::from_response(status.as_u16(), path, &text));
        }
        serde_json::from_str(&text).map_err(|e| Error::Decode(format!("{path}: {e}")))
    }
    /// `multipart/form-data` request for endpoints that refuse JSON or take files
    /// (`rpm/comps/`, content creation…). Scalars are sent as text, arrays as repeated
    /// parts, objects as JSON text; each `(field, path)` of `files` is attached as a file.
    /// Refused in read-only mode.
    pub async fn mutate_multipart(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        files: &[(String, PathBuf)],
    ) -> Result<Value> {
        if self.readonly() {
            return Err(Error::ReadOnly { method: method_name(&method), path: path.to_owned() });
        }
        let mut form = Form::new();
        for (name, value) in body.and_then(Value::as_object).into_iter().flatten() {
            for text in form_texts(value) {
                form = form.text(name.clone(), text);
            }
        }
        for (name, file) in files {
            let bytes = tokio::fs::read(file).await.map_err(|e| Error::Config(format!("{}: {e}", file.display())))?;
            let file_name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
            form = form.part(name.clone(), Part::bytes(bytes).file_name(file_name));
        }
        let resp = self.request(method, &self.url(path)).multipart(form).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(Error::from_response(status.as_u16(), path, &text));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).map_err(|e| Error::Decode(format!("{path}: {e}")))
    }
}

fn method_name(method: &Method) -> &'static str {
    match *method {
        Method::PUT => "PUT",
        Method::PATCH => "PATCH",
        Method::DELETE => "DELETE",
        _ => "POST",
    }
}

/// Text parts of one form field: nothing for null, one per item for arrays.
fn form_texts(value: &Value) -> Vec<String> {
    match value {
        Value::Null => Vec::new(),
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items.iter().flat_map(form_texts).collect(),
        other => vec![other.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_texts_flatten_arrays_and_encode_objects() {
        assert_eq!(form_texts(&json!("a")), ["a"]);
        assert_eq!(form_texts(&json!(["a", "b"])), ["a", "b"]);
        assert_eq!(form_texts(&json!(true)), ["true"]);
        assert_eq!(form_texts(&json!({"k": "v"})), [r#"{"k":"v"}"#]);
        assert!(form_texts(&Value::Null).is_empty());
    }
}
