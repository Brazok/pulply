use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use color_eyre::eyre::{Context, Result};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

/// Size at which `pulply.log` is rotated to `pulply.log.1`.
const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024;

/// `$XDG_STATE_HOME/pulply` (Linux), the local data dir elsewhere.
pub fn log_dir() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "pulply")?;
    Some(dirs.state_dir().unwrap_or(dirs.data_local_dir()).to_path_buf())
}

/// `pulply.log`, rotated to `pulply.log.1` past `MAX_LOG_SIZE`. The terminal belongs to
/// the TUI, so nothing is ever written to stdout/stderr.
pub fn init(level: &str) -> Result<Option<WorkerGuard>> {
    let Some(dir) = log_dir() else { return Ok(None) };
    std::fs::create_dir_all(&dir).wrap_err_with(|| format!("creating {}", dir.display()))?;
    remove_daily_logs(&dir);
    let file = SizeRotatingFile::open(dir.join("pulply.log"), MAX_LOG_SIZE).wrap_err("creating log file")?;
    let (writer, guard) = tracing_appender::non_blocking(file);
    // Dependencies stay at warn: hyper/reqwest debug output may include headers.
    let filter = EnvFilter::new(format!("warn,pulply={level},pulply_api={level}"));
    tracing_subscriber::fmt().with_env_filter(filter).with_writer(writer).with_ansi(false).init();
    Ok(Some(guard))
}

/// Removes `pulply.<date>.log` files left by the former daily rotation.
fn remove_daily_logs(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("pulply.") && name.ends_with(".log") && name != "pulply.log" {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Append-only file renamed to `<path>.1` (replacing it) once a write would exceed `max`,
/// so at most `2 * max` bytes are kept on disk.
struct SizeRotatingFile {
    path: PathBuf,
    max: u64,
    file: File,
    size: u64,
}

impl SizeRotatingFile {
    fn open(path: PathBuf, max: u64) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let size = file.metadata()?.len();
        Ok(Self { path, max, file, size })
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        let mut archive = self.path.clone().into_os_string();
        archive.push(".1");
        // Windows refuses to rename over an existing file.
        let _ = std::fs::remove_file(&archive);
        std::fs::rename(&self.path, &archive)?;
        self.file = OpenOptions::new().create(true).write(true).truncate(true).open(&self.path)?;
        self.size = 0;
        Ok(())
    }
}

impl Write for SizeRotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.size > 0 && self.size + buf.len() as u64 > self.max {
            self.rotate()?;
        }
        let n = self.file.write(buf)?;
        self.size += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_to_a_single_archive_past_max_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pulply.log");
        let mut file = SizeRotatingFile::open(path.clone(), 10).unwrap();
        file.write_all(b"first-").unwrap();
        file.write_all(b"second").unwrap();
        file.write_all(b"third-").unwrap();
        file.flush().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "third-");
        assert_eq!(std::fs::read_to_string(dir.path().join("pulply.log.1")).unwrap(), "second");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn resumes_size_of_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pulply.log");
        std::fs::write(&path, "12345678").unwrap();
        let mut file = SizeRotatingFile::open(path.clone(), 10).unwrap();
        file.write_all(b"abc").unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("pulply.log.1")).unwrap(), "12345678");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "abc");
    }

    #[test]
    fn removes_former_daily_logs_only() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["pulply.2026-10-20.log", "pulply.log", "pulply.log.1", "other.log"] {
            std::fs::write(dir.path().join(name), "").unwrap();
        }
        remove_daily_logs(dir.path());
        let mut left: Vec<String> =
            std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into()).collect();
        left.sort();
        assert_eq!(left, ["other.log", "pulply.log", "pulply.log.1"]);
    }
}
