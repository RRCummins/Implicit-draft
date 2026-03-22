use ratatui::{
    Frame,
    layout::{Constraint, Direction, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::{
    app::{App, ViewModel},
    picker::PickerEntry,
};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [buffer_area, status_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .areas(frame.area());

    app.sync_viewport(buffer_area.height as usize, buffer_area.width as usize);

    match app.current_view(buffer_area.height as usize, buffer_area.width as usize) {
        ViewModel::Editor {
            lines,
            cursor,
            dialog,
        } => draw_editor(frame, buffer_area, lines, cursor, dialog),
        ViewModel::Picker {
            cwd,
            filter,
            query,
            entries,
            selected_row,
            metadata,
        } => draw_picker(
            frame,
            buffer_area,
            &format!(" {} ({filter}) /{} ", cwd, query),
            &entries,
            selected_row,
            metadata,
        ),
        ViewModel::Welcome {
            logo,
            shortcuts,
            recents,
            selected_row,
            search_active,
        } => draw_welcome(
            frame,
            buffer_area,
            &logo,
            &shortcuts,
            &recents,
            selected_row,
            search_active,
        ),
    }

    let status = Line::from(app.status_line());
    let status_bar =
        Paragraph::new(status).style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_widget(status_bar, status_area);
}

fn draw_editor(
    frame: &mut Frame,
    area: Rect,
    lines: Vec<String>,
    cursor: Option<(usize, usize)>,
    dialog: Option<[String; 3]>,
) {
    let editor = Paragraph::new(lines.into_iter().map(Line::raw).collect::<Vec<_>>())
        .block(Block::default());
    frame.render_widget(editor, area);

    if let Some((column, row)) = cursor {
        frame.set_cursor_position((area.x + column as u16, area.y + row as u16));
    }

    if let Some(lines) = dialog {
        let dialog_area = centered_rect(area, 52, 7);
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

fn draw_picker(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    entries: &[PickerEntry],
    selected_row: Option<usize>,
    metadata: [String; 4],
) {
    let [list_area, meta_area] = two_column(area);

    let list_lines = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let mut style = Style::default();
            if Some(index) == selected_row {
                style = style.add_modifier(Modifier::REVERSED);
            }
            Line::styled(entry.label().to_owned(), style)
        })
        .collect::<Vec<_>>();

    let list =
        Paragraph::new(list_lines).block(Block::default().title(title).borders(Borders::ALL));
    frame.render_widget(list, list_area);

    let meta = Paragraph::new(metadata.into_iter().map(Line::raw).collect::<Vec<_>>())
        .block(Block::default().title(" Selection ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(meta, meta_area);
}

fn draw_welcome(
    frame: &mut Frame,
    area: Rect,
    logo: &[String],
    shortcuts: &[(String, String)],
    recents: &[(String, String)],
    selected_row: Option<usize>,
    search_active: bool,
) {
    let [hero_area, body_area, hint_area] = Layout::vertical([
        Constraint::Length(8),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .areas(area);
    let [logo_area, title_area] =
        Layout::horizontal([Constraint::Length(24), Constraint::Min(20)]).areas(hero_area);
    let [shortcuts_area, recents_area] = two_column(body_area);

    let logo_widget = Paragraph::new(logo.iter().cloned().map(Line::raw).collect::<Vec<_>>())
        .block(
            Block::default()
                .title(" Braille Logo ")
                .borders(Borders::ALL),
        );
    frame.render_widget(logo_widget, logo_area);

    let title_lines = vec![
        Line::raw("implicit"),
        Line::raw("a markdown editor for the terminal"),
        Line::raw(""),
        Line::raw("Press O to open a file"),
        Line::raw("Press N for a new untitled buffer"),
    ];
    let title = Paragraph::new(title_lines)
        .block(Block::default().title(" Home ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(title, title_area);

    let shortcut_lines = shortcuts
        .iter()
        .map(|(label, value)| Line::raw(format!("{label:<8} {value}")))
        .collect::<Vec<_>>();
    let shortcut_widget = Paragraph::new(shortcut_lines)
        .block(Block::default().title(" Shortcuts ").borders(Borders::ALL));
    frame.render_widget(shortcut_widget, shortcuts_area);

    let recent_lines = if recents.is_empty() {
        vec![Line::raw("No recent files yet.")]
    } else {
        recents
            .iter()
            .enumerate()
            .map(|(index, (path, age))| {
                let mut style = Style::default();
                if Some(index) == selected_row {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                Line::styled(format!("{path}  {age}"), style)
            })
            .collect::<Vec<_>>()
    };
    let recents_widget = Paragraph::new(recent_lines)
        .block(
            Block::default()
                .title(" Recent Files ")
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(recents_widget, recents_area);

    let hint = Paragraph::new(vec![Line::raw(if search_active {
        "/ search active in picker"
    } else {
        "O open picker   N new buffer   Enter open recent   / search files   Q quit"
    })])
    .block(Block::default().borders(Borders::TOP));
    frame.render_widget(hint, hint_area);
}

fn two_column(area: Rect) -> [Rect; 2] {
    Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(area)
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
