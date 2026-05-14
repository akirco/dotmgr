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
use std::io::{self, Write};
use std::process::Command;
use std::time::Duration;

#[tokio::main]
async fn main() -> io::Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run_app(&mut terminal).await;

    terminal::disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut app = App::new();

    loop {
        app.tick = app.tick.wrapping_add(1);
        terminal.draw(|f| ui::draw(f, &app))?;

        loop {
            match app.rx.try_recv() {
                Ok(event) => app.handle_background_event(event),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
            }
        }

        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if app.pending {
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
                KeyCode::Enter => {
                    if let Some(path) = app.enter_directory() {
                        open_editor(&mut app, &mut *terminal, &path)?;
                    }
                }
                KeyCode::Esc | KeyCode::Backspace => app.go_back(),
                KeyCode::Tab => app.toggle_browse_mode(),
                KeyCode::Char(' ') | KeyCode::Char('i') => app.toggle_ignore(),
                KeyCode::Char('p') => app.toggle_syncable_only(),
                KeyCode::Char('S') => app.request_confirm("sync_all"),
                KeyCode::Char('s') if !shift => app.sync_selected(),
                KeyCode::Char('D') => app.request_confirm("deploy_all"),
                KeyCode::Char('v') => diff_file(&mut app, &mut *terminal)?,
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

fn diff_file(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> io::Result<()> {
    let entry = match app.entries.get(app.selected) {
        Some(e) if !e.is_dir => e,
        _ => return Ok(()),
    };

    let rel = app.relative_path(&entry.path);
    let home_path = app.home_dir.join(&rel);
    let sync_path = app.config.sync_dir.join(&rel);

    if !home_path.exists() || !sync_path.exists() {
        app.status = "Both copies must exist to diff".into();
        return Ok(());
    }

    let (left, right) = match app.browse_mode {
        app::BrowseMode::Home => (home_path, sync_path),
        app::BrowseMode::Sync => (sync_path, home_path),
    };

    terminal::disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    Command::new("delta")
        .arg("--side-by-side")
        .arg(&left)
        .arg(&right)
        .status()?;

    let mut stdout = io::stdout();
    write!(stdout, "\nPress Enter to continue...")?;
    stdout.flush()?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;

    terminal::enable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;

    Ok(())
}

fn open_editor(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    path: &std::path::Path,
) -> io::Result<()> {
    let editor = app
        .config
        .editor
        .clone()
        .or_else(|| std::env::var("EDITOR").ok())
        .or_else(|| std::env::var("VISUAL").ok())
        .unwrap_or_else(|| "vim".to_string());

    terminal::disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    let status = Command::new(&editor).arg(path).status()?;

    terminal::enable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;
    app.refresh();

    if !status.success() {
        app.status = format!("{}: exit code {}", editor, status.code().unwrap_or(-1));
    }

    Ok(())
}
