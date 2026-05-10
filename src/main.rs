mod app;
mod config;
mod ui;

use app::App;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::time::Duration;

fn main() -> io::Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run_app(&mut terminal);

    terminal::disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut app = App::new();

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => app.should_quit = true,
                        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                        KeyCode::Char('g') | KeyCode::Home => app.go_top(),
                        KeyCode::Char('G') | KeyCode::End => app.go_bottom(),
                        KeyCode::PageUp => app.page_up(),
                        KeyCode::PageDown => app.page_down(),
                        KeyCode::Enter => app.enter_directory(),
                        KeyCode::Esc | KeyCode::Backspace => app.go_back(),
                        KeyCode::Char(' ') | KeyCode::Char('i') => app.toggle_ignore(),
                        KeyCode::Char('p') => app.toggle_syncable_only(),
                        KeyCode::Char('s') => app.sync(),
                        KeyCode::Char('a') => app.toggle_show_all(),
                        KeyCode::Char('r') => app.refresh(),
                        _ => {}
                    }
                }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
