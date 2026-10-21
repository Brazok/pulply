//! Chunked uploads (`uploads/` API) and form posts used to create content from them.

use std::path::Path;

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
}
