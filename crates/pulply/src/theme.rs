use ratatui::style::{Color, Modifier, Style};

/// Colors used across the UI. Single built-in palette until skins land (M4).
#[derive(Debug, Clone)]
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
        Self {
            muted: Color::DarkGray,
            accent: Color::Cyan,
            border: Color::DarkGray,
            border_focus: Color::Cyan,
            header_bg: Color::Rgb(30, 60, 90),
            header_fg: Color::White,
            selection_bg: Color::Rgb(40, 70, 110),
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            info: Color::LightBlue,
            key: Color::Magenta,
        }
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
