use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Terminal UI to observe and manage Pulp 3 servers.
#[derive(Debug, Parser)]
#[command(name = "pulply", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub mode: Option<Mode>,

    /// Context to connect to (from config.toml or a pulp-cli profile).
    #[arg(long, env = "PULPLY_CONTEXT", global = true)]
    pub context: Option<String>,

    /// Path to the pulply config file [default: ~/.config/pulply/config.toml].
    #[arg(long, env = "PULPLY_CONFIG", global = true)]
    pub config: Option<PathBuf>,

    /// Disable every modifying request (POST/PUT/PATCH/DELETE).
    #[arg(long, global = true)]
    pub readonly: bool,

    /// Start view, as typed in command mode (e.g. "repo rpm", "task").
    #[arg(short = 'c', long = "command")]
    pub command: Option<String>,

    /// Refresh interval in seconds (minimum 1).
    #[arg(long)]
    pub refresh: Option<u64>,

    /// Log level written to the log file.
    #[arg(short = 'l', long = "log-level", default_value = "info", global = true,
          value_parser = ["error", "warn", "info", "debug", "trace"])]
    pub log_level: String,
}

#[derive(Debug, Subcommand)]
pub enum Mode {
    /// Serve the web interface instead of the terminal UI.
    Serve(ServeArgs),
}

#[derive(Debug, Args)]
pub struct ServeArgs {
    /// Address to listen on. Outside loopback, the printed token is required to log in.
    #[arg(long, default_value = "127.0.0.1:8484", env = "PULPLY_LISTEN")]
    pub listen: SocketAddr,

    /// Access token of the web interface [default: random at each start].
    #[arg(long, env = "PULPLY_WEB_TOKEN", hide_env_values = true)]
    pub token: Option<String>,

    /// Open the interface in the default browser.
    #[arg(long)]
    pub open: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serve_accepts_global_flags_after_the_subcommand() {
        let cli =
            Cli::try_parse_from(["pulply", "serve", "--readonly", "--context", "lab", "--listen", "0.0.0.0:9000"])
                .unwrap();
        assert!(cli.readonly);
        assert_eq!(cli.context.as_deref(), Some("lab"));
        let Some(Mode::Serve(args)) = cli.mode else { panic!("serve expected") };
        assert_eq!(args.listen.to_string(), "0.0.0.0:9000");
        assert!(Cli::try_parse_from(["pulply"]).unwrap().mode.is_none());
    }
}
