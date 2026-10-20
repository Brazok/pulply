use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::widgets::Paragraph;

fn main() -> color_eyre::Result<()> {
    // color-eyre first: ratatui::init() wraps the installed panic hook so the
    // terminal is restored before the report is printed.
    color_eyre::install()?;
    let mut terminal = ratatui::init();
    let result = (|| -> color_eyre::Result<()> {
        loop {
            terminal.draw(|f| f.render_widget(Paragraph::new("pulply — press q to quit"), f.area()))?;
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
                && key.code == KeyCode::Char('q')
            {
                return Ok(());
            }
        }
    })();
    ratatui::restore();
    result
}
