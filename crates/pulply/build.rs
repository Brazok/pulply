//! Builds the web interface (`web/dist`, embedded by `src/web/assets.rs`) when its sources
//! are newer than the last build. Best effort: without npm, or when the build fails, a
//! warning is printed and the binary serves a "not built" page instead of the interface.
//!
//! Skipped with `PULPLY_SKIP_WEB=1`, and in the published crate (which ships `web/dist`
//! without the sources).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

const SOURCES: &[&str] =
    &["src", "public", "index.html", "package.json", "package-lock.json", "vite.config.ts", "tsconfig.json"];

fn newest(path: &Path) -> Option<SystemTime> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.is_file() {
        return meta.modified().ok();
    }
    std::fs::read_dir(path).ok()?.flatten().filter_map(|e| newest(&e.path())).max()
}

fn npm() -> &'static str {
    if cfg!(windows) { "npm.cmd" } else { "npm" }
}

fn run(web: &Path, args: &[&str]) -> Result<(), String> {
    let status = Command::new(npm()).args(args).current_dir(web).status().map_err(|e| format!("npm: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("`npm {}` failed ({status})", args.join(" "))) }
}

fn main() {
    println!("cargo:rerun-if-env-changed=PULPLY_SKIP_WEB");
    let web = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo")).join("web");
    if !web.join("src").exists() {
        return; // Published crate: `web/dist` is shipped prebuilt.
    }
    for s in SOURCES {
        println!("cargo:rerun-if-changed=web/{s}");
    }
    if std::env::var_os("PULPLY_SKIP_WEB").is_some_and(|v| v != "0") {
        return;
    }
    let built = newest(&web.join("dist").join("index.html"));
    let sources = SOURCES.iter().filter_map(|s| newest(&web.join(s))).max();
    if built.is_some() && built >= sources {
        return;
    }
    let result = if web.join("node_modules").exists() { Ok(()) } else { run(&web, &["ci", "--no-audit", "--no-fund"]) }
        .and_then(|()| run(&web, &["run", "build"]));
    if let Err(e) = result {
        println!("cargo:warning=web interface not built ({e}); `pulply serve` will show a placeholder page");
    }
}
