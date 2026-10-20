mod app;
mod cli;
mod commands;
mod config;
mod format;
mod labels;
mod logging;
mod names;
mod session;
mod theme;
mod ui;
mod views;

use clap::Parser;
use color_eyre::eyre::Result;

rust_i18n::i18n!("locales", fallback = "en");

/// `config.language`, else the system locale, else English.
fn select_language(configured: Option<&str>) -> &'static str {
    let wanted = configured.map(str::to_owned).or_else(sys_locale::get_locale).unwrap_or_default().to_lowercase();
    if wanted.starts_with("fr") { "fr" } else { "en" }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = cli::Cli::parse();
    let _log_guard = logging::init(&cli.log_level)?;
    let config = config::Config::load(cli.config.as_deref())?;
    rust_i18n::set_locale(select_language(config.language.as_deref()));
    let context = config.select_context(cli.context.as_deref())?.name.clone();
    tracing::info!(context, "starting pulply {}", env!("CARGO_PKG_VERSION"));

    let opts = app::Options {
        context,
        readonly: cli.readonly,
        refresh: cli.refresh,
        start: cli.command.unwrap_or_else(|| config.start_view.clone()),
    };
    // ratatui::init() installs a panic hook that restores the terminal before color-eyre reports.
    let mut terminal = ratatui::init();
    let result = app::App::new(config, opts).run(&mut terminal).await;
    ratatui::restore();
    result
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    fn keys(yaml: &str) -> BTreeSet<String> {
        fn walk(prefix: &str, v: &serde_norway::Value, out: &mut BTreeSet<String>) {
            match v {
                serde_norway::Value::Mapping(m) => {
                    for (k, v) in m {
                        let k = k.as_str().unwrap_or_default();
                        if k != "_version" {
                            walk(&format!("{prefix}{k}."), v, out);
                        }
                    }
                }
                _ => {
                    out.insert(prefix.trim_end_matches('.').to_owned());
                }
            }
        }
        let mut out = BTreeSet::new();
        walk("", &serde_norway::from_str(yaml).unwrap(), &mut out);
        out
    }

    #[test]
    fn french_and_english_locales_have_the_same_keys() {
        let en = keys(include_str!("../locales/en.yml"));
        let fr = keys(include_str!("../locales/fr.yml"));
        assert_eq!(en.symmetric_difference(&fr).collect::<Vec<_>>(), Vec::<&String>::new());
    }

    #[test]
    fn language_selection() {
        assert_eq!(super::select_language(Some("fr_FR.UTF-8")), "fr");
        assert_eq!(super::select_language(Some("de")), "en");
    }
}
