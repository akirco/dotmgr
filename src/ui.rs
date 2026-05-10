use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{App, IgnoreStatus, format_size},
    config::APP_NAME,
};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_list(f, app, chunks[1]);
    draw_footer(f, chunks[2]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .title(APP_NAME)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Blue))
        .title_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        );

    let inner_area = outer_block.inner(area);
    f.render_widget(outer_block, area);

    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Min(0)])
        .split(inner_area);

    let mut spans = vec![Span::styled(
        app.display_path(),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )];

    if app.current_dir != app.home_dir && app.is_ignored(&app.current_dir) {
        spans.push(Span::styled(
            "  ⚠ INHERITED IGNORE",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let left = Paragraph::new(Line::from(spans));
    f.render_widget(left, h_chunks[0]);

    let sync_text = format!("sync → {} ", app.config.sync_dir.display());
    let right = Paragraph::new(Line::from(Span::styled(
        sync_text,
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Right);
    f.render_widget(right, h_chunks[1]);
}

fn draw_list(f: &mut Frame, app: &App, area: Rect) {
    let top_title = list_title(app);

    let mut block = Block::default()
        .borders(Borders::ALL)
        .title_top(top_title)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Blue));

    if !app.status.is_empty() {
        let style = if app.status.contains("FAILED") || app.status.contains("errors") {
            Style::default().fg(Color::Red)
        } else if app.status.contains("Already up to date") {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else if app.status.contains("cleaned") {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Cyan)
        };
        let bottom_title =
            Line::from(Span::styled(format!(" {} ", app.status), style)).right_aligned();
        block = block.title_bottom(bottom_title);
    }

    if app.entries.is_empty() {
        let msg = if app.show_syncable_only {
            "  (no syncable items — all entries are ignored)"
        } else {
            "  (empty directory)"
        };
        let empty = Paragraph::new(msg)
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = app
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let status = app.ignore_status(&entry.path);
            let is_selected = i == app.selected;

            let (check_char, check_style) = match status {
                IgnoreStatus::NotIgnored => (" ", Style::default().fg(Color::Green)),
                IgnoreStatus::DirectlyIgnored => (
                    "✗",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                IgnoreStatus::InheritedIgnored => ("~", Style::default().fg(Color::Yellow)),
            };

            let name_style = match status {
                IgnoreStatus::NotIgnored if entry.is_dir => Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
                IgnoreStatus::NotIgnored => Style::default().fg(Color::White),
                IgnoreStatus::DirectlyIgnored => Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::CROSSED_OUT),
                IgnoreStatus::InheritedIgnored => Style::default().fg(Color::DarkGray),
            };

            let dir_indicator = if entry.is_dir { "/" } else { "" };
            let symlink_indicator = if entry.is_symlink { " →" } else { "" };

            let size_str = if !entry.is_dir && entry.size > 0 {
                format!("  {}", format_size(entry.size))
            } else {
                String::new()
            };

            let tag_span = match status {
                IgnoreStatus::DirectlyIgnored => {
                    Some(Span::styled("  IGN", Style::default().fg(Color::Red)))
                }
                IgnoreStatus::InheritedIgnored => {
                    Some(Span::styled("  INH", Style::default().fg(Color::Yellow)))
                }
                IgnoreStatus::NotIgnored => {
                    if app.show_syncable_only {
                        Some(Span::styled("  ✓", Style::default().fg(Color::Green)))
                    } else {
                        None
                    }
                }
            };

            let mut line_spans = vec![
                Span::styled(format!("[{}]", check_char), check_style),
                Span::styled(" ", Style::default()),
                Span::styled(
                    format!("{}{}{}", entry.name, dir_indicator, symlink_indicator),
                    name_style,
                ),
                Span::styled(size_str, Style::default().fg(Color::DarkGray)),
            ];

            if let Some(tag) = tag_span {
                line_spans.push(tag);
            }

            let mut item = ListItem::new(Line::from(line_spans));

            if is_selected {
                item = item.style(Style::default().bg(Color::DarkGray));
            }

            item
        })
        .collect();

    let list = List::new(items).block(block);

    let mut state = ListState::default();
    state.select(Some(app.selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn list_title(app: &App) -> String {
    let mode = if app.show_all { "ALL" } else { "DOT" };

    if app.show_syncable_only {
        format!(
            " Pending [{}] {}/{} shown │ tracked:{} ignored:{} ",
            mode,
            app.entries.len(),
            app.full_count,
            app.full_tracked,
            app.full_ignored,
        )
    } else {
        format!(
            " Files [{}] {} items │ tracked:{} ignored:{} ",
            mode, app.full_count, app.full_tracked, app.full_ignored,
        )
    }
}

fn draw_footer(f: &mut Frame, area: Rect) {
    let help = Paragraph::new(
        Line::from(vec![
            Span::styled(" ↑/k↓/j", Style::default().fg(Color::Cyan)),
            Span::raw(" Nav │"),
            Span::styled(" Enter", Style::default().fg(Color::Cyan)),
            Span::raw(" Open │"),
            Span::styled(" Esc", Style::default().fg(Color::Cyan)),
            Span::raw(" Back │"),
            Span::styled(" Space/i", Style::default().fg(Color::Cyan)),
            Span::raw(" Ignore │"),
            Span::styled(" p", Style::default().fg(Color::Magenta)),
            Span::raw(" Pending │"),
            Span::styled(" s", Style::default().fg(Color::Green)),
            Span::raw(" Sync │"),
            Span::styled(" a", Style::default().fg(Color::Cyan)),
            Span::raw(" All/Dot │"),
            Span::styled(" r", Style::default().fg(Color::Cyan)),
            Span::raw(" Refresh │"),
            Span::styled(" q", Style::default().fg(Color::Red)),
            Span::raw(" Quit"),
        ])
        .centered(),
    );
    f.render_widget(help, area);
}
