use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Paragraph},
};

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App) {
    let [buffer_area, status_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .areas(frame.area());

    frame.render_widget(Block::default(), buffer_area);

    let status = Line::from(format!(
        " {} | phase 0 scaffold | ctrl+q quit ",
        app.buffer_name()
    ));
    let status_bar =
        Paragraph::new(status).style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_widget(status_bar, status_area);
}
