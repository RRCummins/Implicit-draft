use ratatui::{
    Frame,
    layout::{Constraint, Direction, Flex, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::{
    app::{App, DialogView, OverlayView, ViewModel},
    picker::PickerEntry,
    theme::Theme,
};

const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

#[derive(Clone, Copy)]
struct WelcomeMeta {
    selected_row: Option<usize>,
    search_active: bool,
    tick: u64,
}

struct WelcomeView<'a> {
    logo: &'a [String],
    version: &'a str,
    shortcuts: &'a [(String, String)],
    recents: &'a [(String, String)],
    meta: WelcomeMeta,
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let theme = app.theme();
    let [buffer_area, status_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .areas(frame.area());

    app.sync_viewport(buffer_area.height as usize, buffer_area.width as usize);

    match app.current_view(buffer_area.height as usize, buffer_area.width as usize) {
        ViewModel::Editor {
            lines,
            cursor,
            scroll,
            dialog,
        } => draw_editor(frame, buffer_area, lines, cursor, scroll, dialog, theme),
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
            theme,
        ),
        ViewModel::Welcome {
            logo,
            version,
            shortcuts,
            recents,
            selected_row,
            search_active,
            tick,
        } => draw_welcome(
            frame,
            buffer_area,
            WelcomeView {
                logo: &logo,
                version: &version,
                shortcuts: &shortcuts,
                recents: &recents,
                meta: WelcomeMeta {
                    selected_row,
                    search_active,
                    tick,
                },
            },
            theme,
        ),
    }

    let status = Line::from(fit_status_line(
        &app.status_line(),
        status_area.width as usize,
    ));
    let status_bar = Paragraph::new(status).style(theme.selection.patch(theme.ui_chrome));

    frame.render_widget(status_bar, status_area);

    if let Some(overlay) = app.overlay() {
        draw_overlay(frame, buffer_area, overlay, theme);
    }
}

fn fit_status_line(line: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let len = line.chars().count();
    if len <= width {
        return format!("{line:<width$}");
    }

    if width == 1 {
        return String::from("…");
    }

    let tail_len = width - 1;
    let tail = line
        .chars()
        .rev()
        .take(tail_len)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("…{tail}")
}

fn draw_editor(
    frame: &mut Frame,
    area: Rect,
    lines: Vec<Line<'static>>,
    cursor: Option<(usize, usize)>,
    scroll: (usize, usize),
    dialog: Option<DialogView>,
    theme: Theme,
) {
    let mut lines = lines;
    if let Some((_, row)) = cursor.filter(|(_, row)| *row < lines.len()) {
        lines[row].style = lines[row].style.patch(theme.cursor);
    }

    let editor = Paragraph::new(lines)
        .block(Block::default())
        .style(theme.background);
    let editor = editor.scroll((scroll.0 as u16, scroll.1 as u16));
    frame.render_widget(editor, area);

    if let Some((column, row)) = cursor {
        frame.set_cursor_position((area.x + column as u16, area.y + row as u16));
    }

    if let Some(dialog) = dialog {
        let dialog_area = centered_rect(area, 52, 7);
        frame.render_widget(Clear, dialog_area);

        let dialog = Paragraph::new(dialog.lines.into_iter().map(Line::raw).collect::<Vec<_>>())
            .style(theme.background)
            .block(
                Block::default()
                    .title(dialog.title)
                    .borders(Borders::ALL)
                    .border_style(theme.ui_chrome)
                    .title_style(theme.ui_chrome),
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
    theme: Theme,
) {
    let [list_area, meta_area] = two_column(area);

    let list_lines = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let mut style = theme.background;
            if Some(index) == selected_row {
                style = style.patch(theme.selection);
            }
            Line::styled(entry.label().to_owned(), style)
        })
        .collect::<Vec<_>>();

    let list = Paragraph::new(list_lines).style(theme.background).block(
        Block::default()
            .title(title)
            .title_style(theme.ui_chrome)
            .borders(Borders::ALL)
            .border_style(theme.ui_chrome),
    );
    frame.render_widget(list, list_area);

    let meta = Paragraph::new(metadata.into_iter().map(Line::raw).collect::<Vec<_>>())
        .style(theme.background)
        .block(
            Block::default()
                .title(" Selection ")
                .title_style(theme.ui_chrome)
                .borders(Borders::ALL)
                .border_style(theme.ui_chrome),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(meta, meta_area);
}

fn draw_welcome(frame: &mut Frame, area: Rect, welcome: WelcomeView<'_>, theme: Theme) {
    let [hero_area, body_area, hint_area] = Layout::vertical([
        Constraint::Length(8),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .areas(area);
    let [logo_area, title_area] =
        Layout::horizontal([Constraint::Length(24), Constraint::Min(20)]).areas(hero_area);
    let [shortcuts_area, recents_area] = two_column(body_area);

    let spinner_char = SPINNER[(welcome.meta.tick as usize) % SPINNER.len()];
    let mut logo_lines: Vec<Line> = welcome
        .logo
        .iter()
        .cloned()
        .map(Line::raw)
        .collect();
    logo_lines.push(Line::raw(""));
    logo_lines.push(Line::raw(format!("  {spinner_char}")));

    let logo_widget = Paragraph::new(logo_lines)
        .style(theme.background)
        .block(
            Block::default()
                .title(" implicit ")
                .title_style(theme.ui_chrome)
                .borders(Borders::ALL)
                .border_style(theme.ui_chrome),
        );
    frame.render_widget(logo_widget, logo_area);

    let title_lines = vec![
        Line::raw("implicit"),
        Line::raw(welcome.version.to_owned()),
        Line::raw("a markdown editor for the terminal"),
        Line::raw(""),
        Line::raw("Press O to open a file"),
        Line::raw("Press N for a new untitled buffer"),
    ];
    let title = Paragraph::new(title_lines)
        .style(theme.background)
        .block(
            Block::default()
                .title(" Home ")
                .title_style(theme.ui_chrome)
                .borders(Borders::ALL)
                .border_style(theme.ui_chrome),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(title, title_area);

    let shortcut_lines = welcome
        .shortcuts
        .iter()
        .map(|(label, value)| Line::raw(format!("{label:<8} {value}")))
        .collect::<Vec<_>>();
    let shortcut_widget = Paragraph::new(shortcut_lines)
        .style(theme.background)
        .block(
            Block::default()
                .title(" Shortcuts ")
                .title_style(theme.ui_chrome)
                .borders(Borders::ALL)
                .border_style(theme.ui_chrome),
        );
    frame.render_widget(shortcut_widget, shortcuts_area);

    let recent_lines = if welcome.recents.is_empty() {
        vec![Line::raw("No recent files yet.")]
    } else {
        welcome
            .recents
            .iter()
            .enumerate()
            .map(|(index, (path, age))| {
                let mut style = theme.background;
                if Some(index) == welcome.meta.selected_row {
                    style = style.patch(theme.selection);
                }
                Line::styled(format!("{path}  {age}"), style)
            })
            .collect::<Vec<_>>()
    };
    let recents_widget = Paragraph::new(recent_lines)
        .style(theme.background)
        .block(
            Block::default()
                .title(" Recent Files ")
                .title_style(theme.ui_chrome)
                .borders(Borders::ALL)
                .border_style(theme.ui_chrome),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(recents_widget, recents_area);

    let hint = Paragraph::new(vec![Line::raw(if welcome.meta.search_active {
        "/ search active in picker"
    } else {
        "O open picker   N new buffer   Enter open recent   / search files   Q quit"
    })])
    .style(theme.background.patch(theme.ui_chrome))
    .block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(theme.ui_chrome),
    );
    frame.render_widget(hint, hint_area);
}

fn draw_overlay(frame: &mut Frame, area: Rect, overlay: OverlayView, theme: Theme) {
    let height = (overlay.lines.len() as u16 + 2).max(6);
    let dialog_area = centered_rect(area, 60, height);
    frame.render_widget(Clear, dialog_area);

    let dialog = Paragraph::new(overlay.lines.into_iter().map(Line::raw).collect::<Vec<_>>())
        .style(theme.background)
        .block(
            Block::default()
                .title(overlay.title)
                .borders(Borders::ALL)
                .border_style(theme.ui_chrome)
                .title_style(theme.ui_chrome),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(dialog, dialog_area);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_short_status_lines_intact() {
        assert_eq!(fit_status_line("hello", 8), "hello   ");
    }

    #[test]
    fn truncates_long_status_lines_from_the_left() {
        assert_eq!(fit_status_line("abcdef", 4), "…def");
    }
}
