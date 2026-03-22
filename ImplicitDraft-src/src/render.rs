use ratatui::{
    Frame,
    layout::{Constraint, Direction, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [buffer_area, status_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .areas(frame.area());

    app.sync_viewport(buffer_area.height as usize, buffer_area.width as usize);

    let lines = app
        .visible_lines(buffer_area.height as usize, buffer_area.width as usize)
        .into_iter()
        .map(Line::raw)
        .collect::<Vec<_>>();
    let editor = Paragraph::new(lines).block(Block::default());
    frame.render_widget(editor, buffer_area);

    if let Some((column, row)) = app.cursor_screen_position() {
        frame.set_cursor_position((buffer_area.x + column as u16, buffer_area.y + row as u16));
    }

    let status = Line::from(app.status_line());
    let status_bar =
        Paragraph::new(status).style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_widget(status_bar, status_area);

    if let Some(lines) = app.quit_dialog_lines() {
        let dialog_area = centered_rect(frame.area(), 52, 7);
        frame.render_widget(Clear, dialog_area);

        let dialog = Paragraph::new(lines.into_iter().map(Line::raw).collect::<Vec<_>>())
            .block(
                Block::default()
                    .title(" Unsaved Changes ")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(dialog, dialog_area);
    }
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let [vertical] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(area);

    let [horizontal] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(vertical);

    horizontal
}
