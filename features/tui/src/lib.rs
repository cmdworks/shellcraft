//! Full-screen setup and plugin manager. `state` is the logic (tested with key events), `draw` the
//! ratatui views (tested with a `TestBackend`), `edit` the config text edits. Cold path only.

mod draw;
pub mod edit;
pub mod state;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
pub use state::{App, Key, Outcome, Plan, PluginInfo, Snapshot};

/// Run the manager on the terminal until the user applies or cancels.
pub fn run(snapshot: Snapshot) -> Result<Outcome> {
    let mut app = App::new(snapshot);
    let mut term = ratatui::init();
    let result = (|| -> Result<()> {
        while app.outcome.is_none() {
            term.draw(|f| draw::draw(f, &app))?;
            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                let key = match k.code {
                    KeyCode::Up => Key::Up,
                    KeyCode::Down => Key::Down,
                    KeyCode::Left => Key::Left,
                    KeyCode::Right => Key::Right,
                    KeyCode::Enter => Key::Enter,
                    KeyCode::Esc => Key::Esc,
                    KeyCode::Backspace => Key::Backspace,
                    KeyCode::Tab => Key::Char('\t'),
                    KeyCode::Char(c) => Key::Char(c),
                    _ => continue,
                };
                app.handle(key);
            }
        }
        Ok(())
    })();
    ratatui::restore();
    result?;
    Ok(app.outcome.unwrap_or(Outcome::Cancelled))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn screen(app: &App) -> String {
        let mut t = Terminal::new(TestBackend::new(70, 14)).expect("terminal");
        t.draw(|f| draw::draw(f, app)).expect("draw");
        let b = t.backend().buffer().clone();
        (0..b.area.height)
            .map(|y| {
                (0..b.area.width)
                    .map(|x| b[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn every_screen_draws_its_content() {
        let snap = crate::state::tests_snapshot("[theme]\nname = \"nord\"\n");
        let mut app = App::new(snap);
        assert!(screen(&app).contains("Nothing is written until you apply"));
        app.keys("\n");
        assert!(screen(&app).contains("[x] Built-in prompt"));
        app.handle(Key::Right);
        let s = screen(&app);
        assert!(
            s.contains("todos")
                && s.contains("available")
                && s.contains("notes")
                && !s.contains("docker"),
            "{s}"
        );
        app.keys(" ");
        assert!(screen(&app).contains("+ todos"));
        app.handle(Key::Right);
        assert!(screen(&app).contains("nord") && screen(&app).contains("preview of the prompt"));
        app.handle(Key::Right);
        let s = screen(&app);
        assert!(s.contains("+ install plugin todos"), "{s}");
    }
}
