use std::path::PathBuf;

use clap::Parser;

/// Terminal UI to observe and manage Pulp 3 servers.
#[derive(Debug, Parser)]
#[command(name = "pulply", version, about)]
pub struct Cli {
    /// Context to connect to (from config.toml or a pulp-cli profile).
    #[arg(long, env = "PULPLY_CONTEXT")]
    pub context: Option<String>,

    /// Path to the pulply config file [default: ~/.config/pulply/config.toml].
    #[arg(long, env = "PULPLY_CONFIG")]
    pub config: Option<PathBuf>,

    /// Disable every modifying request (POST/PUT/PATCH/DELETE).
    #[arg(long)]
    pub readonly: bool,

    /// Start view, as typed in command mode (e.g. "repo rpm", "task").
    #[arg(short = 'c', long = "command")]
    pub command: Option<String>,

    /// Refresh interval in seconds (minimum 1).
    #[arg(long)]
    pub refresh: Option<u64>,

    /// Log level written to the log file.
    #[arg(short = 'l', long = "log-level", default_value = "info",
          value_parser = ["error", "warn", "info", "debug", "trace"])]
    pub log_level: String,
}
