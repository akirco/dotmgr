use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, List, ListItem, ListState, Padding, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
};

use crate::{
    app::{App, BrowseMode, IgnoreStatus, format_size},
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
    draw_footer(f, app, chunks[2]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .title(APP_NAME)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Blue));

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

    if app.current_dir != *app.base_dir() && app.is_ignored(&app.current_dir) {
        spans.push(Span::styled(
            "  ⚠ INHERITED IGNORE",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }

    if app.show_syncable_only {
        spans.push(Span::styled(
            "  [PENDING]",
            Style::default()
                .fg(Color::Magenta)
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
    let bottom_title = if app.status.is_empty() {
        None
    } else {
        let style = if app.status.contains("FAILED") || app.status.contains("errors") {
            Style::default().fg(Color::Red)
        } else if app.status.contains("(y/N)") {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else if app.status.contains("Already up to date") {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else if app.status.contains("overwritten") {
            Style::default().fg(Color::Yellow)
        } else if app.status.contains("deployed") {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if app.status.contains("cleaned") {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Cyan)
        };

        Some(Line::from(Span::styled(format!(" {} ", app.status), style)))
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Blue))
        .title_top(list_title(app));

    if let Some(title) = bottom_title {
        block = block.title_bottom(title);
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

            let mirror_span = if !entry.mirror_exists {
                match app.browse_mode {
                    BrowseMode::Home => {
                        Some(Span::styled("  ⊘", Style::default().fg(Color::Yellow)))
                    }
                    BrowseMode::Sync => Some(Span::styled(
                        "  ⚡",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    )),
                }
            } else {
                None
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
            if let Some(mir) = mirror_span {
                line_spans.push(mir);
            }

            let mut item = ListItem::new(Line::from(line_spans));

            if is_selected {
                item = item.style(Style::default().bg(Color::DarkGray));
            }

            item
        })
        .collect();

    let list = List::new(items).block(block);

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected));
    f.render_stateful_widget(list, area, &mut list_state);

    let visible_rows = area.height.saturating_sub(2) as usize;
    if app.entries.len() > visible_rows {
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("╮"))
            .end_symbol(Some("╯"))
            .thumb_style(Style::default().fg(Color::Blue));

        let mut scrollbar_state = ScrollbarState::default();
        scrollbar_state = scrollbar_state
            .content_length(app.entries.len())
            .position(app.selected);
        f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

fn list_title(app: &App) -> String {
    let mode = if app.show_all { "ALL" } else { "DOT" };
    let view = match app.browse_mode {
        BrowseMode::Home => "HOME",
        BrowseMode::Sync => "SYNC",
    };

    if app.show_syncable_only {
        format!(
            " {} [{}] {}/{} shown │ tracked:{} ignored:{} ",
            view,
            mode,
            app.entries.len(),
            app.full_count,
            app.full_tracked,
            app.full_ignored,
        )
    } else {
        format!(
            " {} [{}] {} items │ tracked:{} ignored:{} ",
            view, mode, app.full_count, app.full_tracked, app.full_ignored,
        )
    }
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    if app.awaiting_confirm.is_some() {
        let help = Paragraph::new(Line::from(vec![
            Span::styled(
                " y ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Confirm │ "),
            Span::styled(
                " Esc/other ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Cancel"),
        ]))
        .block(Block::default().padding(Padding::left(1)));
        f.render_widget(help, area);
        return;
    }

    let help = Paragraph::new(
        Line::from(vec![
            Span::styled(
                "Tab",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Swap │ "),
            Span::styled("↑↓", Style::default().fg(Color::Cyan)),
            Span::raw(" Nav │ "),
            Span::styled("s/S", Style::default().fg(Color::Green)),
            Span::raw(" Sync │ "),
            Span::styled("d/D", Style::default().fg(Color::Yellow)),
            Span::raw(" Deploy │ "),
            Span::styled("a", Style::default().fg(Color::Cyan)),
            Span::raw(" ShowAll │ "),
            Span::styled("q", Style::default().fg(Color::Red)),
            Span::raw(" Quit"),
        ])
        .centered(),
    );
    f.render_widget(help, area);
}
