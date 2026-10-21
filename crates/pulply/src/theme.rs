//! Colors used across the UI, loaded from skins (R14.1): built-in `dark` (default) and
//! `light`, or `<config dir>/skins/<name>.toml`.

use std::path::Path;
use std::str::FromStr;

use ratatui::style::{Color, Modifier, Style};

const BUILTIN: &[(&str, &str)] =
    &[("dark", include_str!("../skins/dark.toml")), ("light", include_str!("../skins/light.toml"))];

pub const DEFAULT_SKIN: &str = "dark";

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub muted: Color,
    pub accent: Color,
    pub border: Color,
    pub border_focus: Color,
    pub header_bg: Color,
    pub header_fg: Color,
    pub selection_bg: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    pub key: Color,
}

impl Default for Theme {
    fn default() -> Self {
        // Base for skins: every color is overridden by the built-in dark skin.
        let base = Self {
            muted: Color::DarkGray,
            accent: Color::Cyan,
            border: Color::DarkGray,
            border_focus: Color::Cyan,
            header_bg: Color::Blue,
            header_fg: Color::White,
            selection_bg: Color::Blue,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            info: Color::LightBlue,
            key: Color::Magenta,
        };
        base.apply(BUILTIN[0].1, None, 0).unwrap_or_else(|_| unreachable!("built-in dark skin is valid"))
    }
}

impl Theme {
    /// Skin `name`: `<dir>/skins/<name>.toml` if present (overrides a built-in of the
    /// same name), else a built-in skin.
    pub fn load(name: &str, dir: Option<&Path>) -> Result<Self, String> {
        Self::named(name, dir, 0)
    }

    fn named(name: &str, dir: Option<&Path>, depth: u8) -> Result<Self, String> {
        if depth > 4 {
            return Err(format!("skin '{name}': inherit chain too deep"));
        }
        let file = dir.map(|d| d.join("skins").join(format!("{name}.toml"))).filter(|p| p.exists());
        let text = match &file {
            Some(p) => std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
            None => BUILTIN
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, t)| (*t).to_owned())
                .ok_or_else(|| format!("unknown skin '{name}'"))?,
        };
        Self::default().apply(&text, dir, depth).map_err(|e| format!("skin '{name}': {e}"))
    }

    /// Applies a skin file on top of `self` (or of its `inherit` skin).
    fn apply(mut self, text: &str, dir: Option<&Path>, depth: u8) -> Result<Self, String> {
        let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
        if let Some(parent) = table.get("inherit") {
            let parent = parent.as_str().ok_or("inherit: expected a skin name")?;
            self = Self::named(parent, dir, depth + 1)?;
        }
        for (key, value) in &table {
            if key == "inherit" {
                continue;
            }
            let text = value.as_str().ok_or_else(|| format!("{key}: expected a color string"))?;
            let color = Color::from_str(text).map_err(|_| format!("{key}: invalid color '{text}'"))?;
            let slot = match key.as_str() {
                "muted" => &mut self.muted,
                "accent" => &mut self.accent,
                "border" => &mut self.border,
                "border_focus" => &mut self.border_focus,
                "header_bg" => &mut self.header_bg,
                "header_fg" => &mut self.header_fg,
                "selection_bg" => &mut self.selection_bg,
                "success" => &mut self.success,
                "warning" => &mut self.warning,
                "error" => &mut self.error,
                "info" => &mut self.info,
                "key" => &mut self.key,
                other => return Err(format!("unknown color '{other}'")),
            };
            *slot = color;
        }
        Ok(self)
    }
}

impl Theme {
    pub fn muted(&self) -> Style {
        Style::new().fg(self.muted)
    }
    pub fn accent(&self) -> Style {
        Style::new().fg(self.accent)
    }
    pub fn selected(&self) -> Style {
        Style::new().bg(self.selection_bg).add_modifier(Modifier::BOLD)
    }
    pub fn header(&self) -> Style {
        Style::new().bg(self.header_bg).fg(self.header_fg)
    }
    pub fn key(&self) -> Style {
        Style::new().fg(self.key).add_modifier(Modifier::BOLD)
    }
    pub fn error(&self) -> Style {
        Style::new().fg(self.error)
    }
    pub fn warning(&self) -> Style {
        Style::new().fg(self.warning)
    }
    pub fn success(&self) -> Style {
        Style::new().fg(self.success)
    }

    /// Color for a Pulp task state.
    pub fn task_state(&self, state: &str) -> Style {
        let color = match state {
            "completed" => self.success,
            "failed" => self.error,
            "running" => self.info,
            "waiting" => self.warning,
            _ => self.muted,
        };
        Style::new().fg(color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_skins_load_and_differ() {
        let dark = Theme::load("dark", None).unwrap();
        let light = Theme::load("light", None).unwrap();
        assert_eq!(dark, Theme::default());
        assert_eq!(dark.header_bg, Color::Rgb(0x1e, 0x3c, 0x5a));
        assert_ne!(dark, light);
        assert!(Theme::load("nope", None).unwrap_err().contains("unknown skin"));
    }

    #[test]
    fn user_skin_inherits_and_overrides() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("skins")).unwrap();
        let write = |name: &str, text: &str| std::fs::write(dir.path().join("skins").join(name), text).unwrap();
        write("prod.toml", "inherit = \"light\"\nheader_bg = \"red\"\n");
        let prod = Theme::load("prod", Some(dir.path())).unwrap();
        let light = Theme::load("light", None).unwrap();
        assert_eq!(prod.header_bg, Color::Red);
        assert_eq!(prod.accent, light.accent, "unset colors come from the inherited skin");

        write("bad.toml", "acent = \"red\"\n");
        assert!(Theme::load("bad", Some(dir.path())).unwrap_err().contains("unknown color 'acent'"));
        write("loop.toml", "inherit = \"loop\"\n");
        assert!(Theme::load("loop", Some(dir.path())).unwrap_err().contains("too deep"));
    }
}
