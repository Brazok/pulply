//! Shell plugins (R14.5): external commands run on the selected resource, either with
//! the TUI suspended (foreground) or detached (background).

use std::collections::HashMap;
use std::process::Stdio;

use ratatui::DefaultTerminal;
use rust_i18n::t;

use crate::custom::{PLUGIN_VARS, Plugin, substitute};

#[derive(Debug, Clone)]
pub struct PluginRun {
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    /// `PULPLY_HREF`, `PULPLY_NAME`… for scripts.
    pub env: Vec<(String, String)>,
    pub background: bool,
}

impl PluginRun {
    pub fn new(plugin: &Plugin, vars: &HashMap<&str, String>) -> Self {
        Self {
            name: plugin.name.clone(),
            program: substitute(&plugin.command, vars),
            args: plugin.args.iter().map(|a| substitute(a, vars)).collect(),
            env: PLUGIN_VARS
                .iter()
                .map(|v| (format!("PULPLY_{v}"), vars.get(v).cloned().unwrap_or_default()))
                .collect(),
            background: plugin.background,
        }
    }

    fn command(&self) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(&self.program);
        cmd.args(&self.args).envs(self.env.iter().map(|(k, v)| (k, v)));
        cmd
    }

    /// Runs with the terminal handed over, then waits for Enter so the output can be
    /// read. The caller must not read terminal events meanwhile.
    pub async fn run_foreground(&self, terminal: &mut DefaultTerminal) -> Result<(), String> {
        ratatui::restore();
        let status = self.command().status().await;
        let outcome = match &status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(t!("plugin.failed", name = self.name, status = s).into_owned()),
            Err(e) => Err(format!("{}: {e}", self.program)),
        };
        println!("\n{}", t!("plugin.press_enter"));
        let _ = tokio::task::spawn_blocking(|| std::io::stdin().read_line(&mut String::new())).await;
        // Re-entering the alternate screen clears it; a fresh Terminal redraws fully.
        *terminal = ratatui::init();
        outcome
    }

    /// Runs detached from the terminal (no input, output discarded).
    pub async fn run_background(self) -> Result<(), String> {
        let status = self
            .command()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|e| format!("{}: {e}", self.program))?;
        if status.success() { Ok(()) } else { Err(t!("plugin.failed", name = self.name, status = status).into_owned()) }
    }
}
