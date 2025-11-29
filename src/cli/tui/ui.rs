use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    style::{Color, Modifier, Style},
};

use super::app::{AppMode, AppScreen, FocusedPanel, TuiApp};

pub fn render(app: &TuiApp, frame: &mut Frame) {
    let size = frame.area();

    // Create main layout
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(10),    // Main content
            Constraint::Length(3),  // Query input
            Constraint::Length(1),  // Status bar
        ])
        .split(size);

    // Render header
    render_header(app, frame, main_chunks[0]);

    // Render content based on current screen
    match app.screen {
        AppScreen::ConnectionManager => {
            render_connection_manager(app, frame, main_chunks[1]);
        }
        AppScreen::DatabaseExplorer => {
            render_database_explorer(app, frame, main_chunks[1]);
        }
    }

    // Render query input
    render_query_input(app, frame, main_chunks[2]);

    // Render status bar
    render_status_bar(app, frame, main_chunks[3]);

    // Render help popup if active
    if app.show_help {
        render_help_popup(app, frame, size);
    }
}

fn render_header(app: &TuiApp, frame: &mut Frame, area: Rect) {
    let (title, info) = match app.screen {
        AppScreen::ConnectionManager => {
            ("KeraDB", format!("{} saved connections", app.connections.len()))
        }
        AppScreen::DatabaseExplorer => {
            let db_name = app.db_path.as_ref()
                .and_then(|p| std::path::Path::new(p).file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            ("KeraDB", format!("{} │ {} collections", db_name, app.collections.len()))
        }
    };

    let mode_str = match app.mode {
        AppMode::Normal => "NORMAL",
        AppMode::Insert => "INSERT",
        AppMode::Command => "COMMAND",
    };

    let header_text = format!(" {} │ {} │ {}", title, info, mode_str);

    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" KeraDB TUI "));

    frame.render_widget(header, area);
}

fn render_connection_manager(app: &TuiApp, frame: &mut Frame, area: Rect) {
    // Split into connections list and info/results panel
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),  // Connections
            Constraint::Percentage(60),  // Results/Info
        ])
        .split(area);

    render_connections_list(app, frame, chunks[0]);
    render_results(app, frame, chunks[1]);
}

fn render_database_explorer(app: &TuiApp, frame: &mut Frame, area: Rect) {
    // Split into collections sidebar and results
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),  // Collections
            Constraint::Percentage(75),  // Results
        ])
        .split(area);

    render_collections(app, frame, chunks[0]);
    render_results(app, frame, chunks[1]);
}

fn render_connections_list(app: &TuiApp, frame: &mut Frame, area: Rect) {
    let is_focused = app.focused == FocusedPanel::Connections;
    let border_color = if is_focused { Color::Yellow } else { Color::Gray };

    let items: Vec<ListItem> = if app.connections.is_empty() {
        vec![
            ListItem::new("  No saved connections").style(Style::default().fg(Color::DarkGray)),
            ListItem::new(""),
            ListItem::new("  Press [n] to create new").style(Style::default().fg(Color::DarkGray)),
            ListItem::new("  Press [o] to open existing").style(Style::default().fg(Color::DarkGray)),
        ]
    } else {
        app.connections
            .iter()
            .enumerate()
            .map(|(i, conn)| {
                let is_selected = i == app.selected_connection;
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                // Format connection info
                let name = &conn.name;
                let stats = format!("{} cols, {} docs", conn.collections_count, conn.total_documents);
                let time = conn.format_last_accessed();
                
                let content = if is_selected {
                    format!("▶ {} ({}) - {}", name, stats, time)
                } else {
                    format!("  {} ({}) - {}", name, stats, time)
                };

                ListItem::new(content).style(style)
            })
            .collect()
    };

    let title = if is_focused { 
        "[ Databases ]" 
    } else { 
        " Databases " 
    };

    let list = List::new(items)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(title));

    frame.render_widget(list, area);
}

fn render_collections(app: &TuiApp, frame: &mut Frame, area: Rect) {
    let is_focused = app.focused == FocusedPanel::Collections;
    let border_color = if is_focused { Color::Yellow } else { Color::Gray };

    let items: Vec<ListItem> = if app.collections.is_empty() {
        vec![
            ListItem::new("  No collections").style(Style::default().fg(Color::DarkGray)),
            ListItem::new(""),
            ListItem::new("  Use 'insert' to").style(Style::default().fg(Color::DarkGray)),
            ListItem::new("  create one").style(Style::default().fg(Color::DarkGray)),
        ]
    } else {
        app.collections
            .iter()
            .enumerate()
            .map(|(i, (name, count))| {
                let is_selected = i == app.selected_collection;
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let prefix = if is_selected { "▶ " } else { "  " };
                let content = format!("{}{} ({})", prefix, name, count);
                ListItem::new(content).style(style)
            })
            .collect()
    };

    let title = if is_focused { "[ Collections ]" } else { " Collections " };

    let list = List::new(items)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(title));

    frame.render_widget(list, area);
}

fn render_results(app: &TuiApp, frame: &mut Frame, area: Rect) {
    let is_focused = app.focused == FocusedPanel::Results;
    let border_color = if is_focused { Color::Yellow } else { Color::Gray };

    let results_text: Vec<Line> = app.results
        .iter()
        .map(|line| {
            // Colorize different types of lines
            if line.starts_with("→ ") {
                Line::from(Span::styled(line.as_str(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)))
            } else if line.starts_with("✓") {
                Line::from(Span::styled(line.as_str(), Style::default().fg(Color::Green)))
            } else if line.starts_with("✗") || line.contains("Error") {
                Line::from(Span::styled(line.as_str(), Style::default().fg(Color::Red)))
            } else if line.starts_with("╔") || line.starts_with("║") || line.starts_with("╚") {
                Line::from(Span::styled(line.as_str(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
            } else if line.starts_with("  [") {
                Line::from(Span::styled(line.as_str(), Style::default().fg(Color::Yellow)))
            } else if line.contains("\"_id\"") || line.contains("\"id\"") {
                Line::from(Span::styled(line.as_str(), Style::default().fg(Color::Cyan)))
            } else if line.starts_with("  •") {
                Line::from(Span::styled(line.as_str(), Style::default().fg(Color::Blue)))
            } else {
                Line::from(Span::raw(line.as_str()))
            }
        })
        .collect();

    let title = match app.screen {
        AppScreen::ConnectionManager => if is_focused { "[ Info ]" } else { " Info " },
        AppScreen::DatabaseExplorer => if is_focused { "[ Results ]" } else { " Results " },
    };

    let results = Paragraph::new(results_text)
        .scroll((app.results_scroll as u16, 0))
        .wrap(Wrap { trim: false })
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(title));

    frame.render_widget(results, area);

    // Render scrollbar if there's content
    if app.results.len() > (area.height as usize - 2) {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"));
        
        let mut scrollbar_state = ScrollbarState::new(app.results.len())
            .position(app.results_scroll);

        let scrollbar_area = Rect {
            x: area.x + area.width - 1,
            y: area.y + 1,
            width: 1,
            height: area.height.saturating_sub(2),
        };

        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }
}

fn render_query_input(app: &TuiApp, frame: &mut Frame, area: Rect) {
    let is_focused = app.focused == FocusedPanel::Query;
    let is_insert = app.mode == AppMode::Insert;
    let is_command = app.mode == AppMode::Command;
    
    let border_color = if is_insert {
        Color::Green
    } else if is_command {
        Color::Yellow
    } else if is_focused {
        Color::Cyan
    } else {
        Color::Gray
    };

    let title = match app.screen {
        AppScreen::ConnectionManager => {
            if is_insert { " Input (type path) " } 
            else if is_command { " Command " }
            else { " Input " }
        }
        AppScreen::DatabaseExplorer => {
            if is_insert { " Query (INSERT) " } 
            else if is_command { " Command " }
            else { " Query " }
        }
    };

    let prefix = if is_command { ":" } else { "> " };
    let display_text = format!("{}{}", prefix, app.input);

    let input = Paragraph::new(display_text.as_str())
        .style(Style::default().fg(Color::White))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(title));

    frame.render_widget(input, area);

    // Show cursor in insert/command mode
    if is_insert || is_command {
        let cursor_x = area.x + prefix.len() as u16 + app.cursor_position as u16 + 1;
        let cursor_y = area.y + 1;
        frame.set_cursor_position(Position { x: cursor_x, y: cursor_y });
    }
}

fn render_status_bar(app: &TuiApp, frame: &mut Frame, area: Rect) {
    let mode_style = match app.mode {
        AppMode::Normal => Style::default().fg(Color::White).bg(Color::Blue),
        AppMode::Insert => Style::default().fg(Color::White).bg(Color::Green),
        AppMode::Command => Style::default().fg(Color::Black).bg(Color::Yellow),
    };

    let mode_text = match app.mode {
        AppMode::Normal => " NORMAL ",
        AppMode::Insert => " INSERT ",
        AppMode::Command => " COMMAND ",
    };

    let screen_indicator = match app.screen {
        AppScreen::ConnectionManager => " 🏠 ",
        AppScreen::DatabaseExplorer => " 📁 ",
    };

    let status_line = Line::from(vec![
        Span::styled(mode_text, mode_style.add_modifier(Modifier::BOLD)),
        Span::styled(screen_indicator, Style::default().fg(Color::Cyan)),
        Span::raw("│ "),
        Span::styled(&app.status_message, Style::default().fg(Color::Gray)),
    ]);

    let status_bar = Paragraph::new(status_line)
        .style(Style::default().bg(Color::DarkGray));

    frame.render_widget(status_bar, area);
}

fn render_help_popup(app: &TuiApp, frame: &mut Frame, area: Rect) {
    let popup_width = 50;
    let popup_height = 28;
    
    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width.min(area.width),
        height: popup_height.min(area.height),
    };

    // Clear the popup area
    frame.render_widget(Clear, popup_area);

    let help_string = app.get_help_text();
    let help_text: Vec<Line> = help_string
        .lines()
        .map(|line| {
            if line.contains("═") || line.starts_with("CONNECTION") || 
               line.starts_with("DOCUMENTS") || line.starts_with("VECTORS") ||
               line.starts_with("NAVIGATION") || line.starts_with("HOME") {
                Line::from(Span::styled(line, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))
            } else {
                Line::from(Span::raw(line))
            }
        })
        .collect();

    let help = Paragraph::new(help_text)
        .wrap(Wrap { trim: false })
        .block(Block::default()
            .title(" Help (? to close) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black)));

    frame.render_widget(help, popup_area);
}
