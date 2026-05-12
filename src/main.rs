mod app;
mod config;
mod ui;
mod utils;

use app::App;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
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
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if app.awaiting_confirm.is_some() {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => app.confirm_action(),
                    _ => app.cancel_confirm(),
                }
                continue;
            }

            let shift = key.modifiers.contains(KeyModifiers::SHIFT);
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

            match key.code {
                KeyCode::Char('Q') | KeyCode::Char('q') => app.should_quit = true,
                KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                KeyCode::Char('g') | KeyCode::Home => app.go_top(),
                KeyCode::Char('G') | KeyCode::End => app.go_bottom(),
                KeyCode::PageUp => app.page_up(),
                KeyCode::PageDown => app.page_down(),
                KeyCode::Enter => app.enter_directory(),
                KeyCode::Esc | KeyCode::Backspace => app.go_back(),
                KeyCode::Tab => app.toggle_browse_mode(),
                KeyCode::Char(' ') | KeyCode::Char('i') => app.toggle_ignore(),
                KeyCode::Char('p') => app.toggle_syncable_only(),
                KeyCode::Char('S') => app.request_confirm("sync_all"),
                KeyCode::Char('s') if !shift => app.sync_selected(),
                KeyCode::Char('D') => app.request_confirm("deploy_all"),
                KeyCode::Char('d') if !shift => app.deploy_selected(),
                KeyCode::Char('a') if ctrl => app.ignore_all(),
                KeyCode::Char('a') if !ctrl => app.toggle_show_all(),
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
