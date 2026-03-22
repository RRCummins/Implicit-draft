use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Paragraph},
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
}
