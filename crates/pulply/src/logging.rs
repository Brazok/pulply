use std::path::PathBuf;

use color_eyre::eyre::{Context, Result};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::EnvFilter;

/// `$XDG_STATE_HOME/pulply` (Linux), the local data dir elsewhere.
pub fn log_dir() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "pulply")?;
    Some(dirs.state_dir().unwrap_or(dirs.data_local_dir()).to_path_buf())
}

/// Daily-rotated `pulply.<date>.log`, 7 files kept. The terminal belongs to the TUI,
/// so nothing is ever written to stdout/stderr.
pub fn init(level: &str) -> Result<Option<WorkerGuard>> {
    let Some(dir) = log_dir() else { return Ok(None) };
    std::fs::create_dir_all(&dir).wrap_err_with(|| format!("creating {}", dir.display()))?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("pulply")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&dir)
        .wrap_err("creating log file")?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    // Dependencies stay at warn: hyper/reqwest debug output may include headers.
    let filter = EnvFilter::new(format!("warn,pulply={level},pulply_api={level}"));
    tracing_subscriber::fmt().with_env_filter(filter).with_writer(writer).with_ansi(false).init();
    Ok(Some(guard))
}
