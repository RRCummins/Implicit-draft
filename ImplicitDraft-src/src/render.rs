use ratatui::{
    Frame,
    layout::{Constraint, Direction, Flex, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::{
    app::{App, DialogView, EditorSplitView, OverlayView, SelectionRange, ViewModel},
    buffer::SearchMatch,
    gitdiff::LineChange,
    picker::PickerEntry,
    settings::ConfigPane,
    sidebar::SidebarRow,
    theme::Theme,
};

#[derive(Clone, Copy)]
struct WelcomeMeta {
    selected_row: Option<usize>,
    search_active: bool,
    tick: u64,
}

struct WelcomeView<'a> {
    logo: &'a [String],
    version: &'a str,
    badge: Option<&'a str>,
    shortcuts: &'a [(String, String)],
    recents: &'a [(String, String)],
    meta: WelcomeMeta,
}

struct EditorView {
    title: String,
    line_numbers: bool,
    wrap: bool,
    git_change_markers: Vec<Option<LineChange>>,
    lines: Vec<Line<'static>>,
    search_matches: Vec<SearchMatch>,
    search_current: Option<usize>,
    selection: Option<SelectionRange>,
    cursor: Option<(usize, usize)>,
    scroll: (usize, usize),
    sidebar_rows: Vec<SidebarRow>,
    sidebar_selected_row: Option<usize>,
    sidebar_width: u16,
    sidebar_focused: bool,
    sidebar_root: String,
    split: Option<Box<EditorSplitView>>,
    dialog: Option<DialogView>,
}

struct EditorPanelView {
    title: String,
    lines: Vec<Line<'static>>,
    focused: bool,
    line_numbers: bool,
    wrap: bool,
    git_change_markers: Option<Vec<Option<LineChange>>>,
    search_matches: Vec<SearchMatch>,
    search_current: Option<usize>,
    selection: Option<SelectionRange>,
    cursor: Option<(usize, usize)>,
    scroll: (usize, usize),
}

struct ConfigView {
    themes: Vec<String>,
    selected_theme: usize,
    applied_theme: String,
    active_pane: ConfigPane,
    options: Vec<(String, String)>,
    selected_option: usize,
    preview_theme: Theme,
    preview_lines: Vec<Line<'static>>,
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
            title,
            line_numbers,
            wrap,
            git_change_markers,
            lines,
            search_matches,
            search_current,
            selection,
            cursor,
            scroll,
            sidebar_rows,
            sidebar_selected_row,
            sidebar_width,
            sidebar_focused,
            sidebar_root,
            split,
            dialog,
        } => draw_editor(
            frame,
            buffer_area,
            EditorView {
                title,
                line_numbers,
                wrap,
                git_change_markers,
                lines,
                search_matches,
                search_current,
                selection,
                cursor,
                scroll,
                sidebar_rows,
                sidebar_selected_row,
                sidebar_width,
                sidebar_focused,
                sidebar_root,
                split,
                dialog,
            },
            theme,
        ),
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
        ViewModel::Config {
            themes,
            selected_theme,
            applied_theme,
            active_pane,
            options,
            selected_option,
            preview_theme,
            preview_lines,
        } => draw_config(
            frame,
            buffer_area,
            ConfigView {
                themes,
                selected_theme,
                applied_theme,
                active_pane,
                options,
                selected_option,
                preview_theme: *preview_theme,
                preview_lines,
            },
            theme,
        ),
        ViewModel::Welcome {
            logo,
            version,
            badge,
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
                badge: badge.as_deref(),
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

fn draw_editor(frame: &mut Frame, area: Rect, editor: EditorView, theme: Theme) {
    let editor_area = if editor.sidebar_width > 0 {
        let [sidebar_area, editor_area] =
            Layout::horizontal([Constraint::Length(editor.sidebar_width), Constraint::Min(1)])
                .areas(area);

        draw_sidebar(
            frame,
            sidebar_area,
            &editor.sidebar_root,
            &editor.sidebar_rows,
            editor.sidebar_selected_row,
            editor.sidebar_focused,
            theme,
        );

        editor_area
    } else {
        area
    };

    if let Some(split) = editor.split {
        let [primary_area, secondary_area] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(editor_area);
        draw_editor_panel(
            frame,
            primary_area,
            EditorPanelView {
                title: editor.title,
                lines: editor.lines,
                focused: !split.focused,
                line_numbers: editor.line_numbers,
                wrap: editor.wrap,
                git_change_markers: Some(editor.git_change_markers),
                search_matches: editor.search_matches,
                search_current: editor.search_current,
                selection: editor.selection,
                cursor: editor.cursor,
                scroll: editor.scroll,
            },
            theme,
        );
        draw_editor_panel(
            frame,
            secondary_area,
            EditorPanelView {
                title: split.title,
                lines: split.lines,
                focused: split.focused,
                line_numbers: split.line_numbers,
                wrap: split.wrap,
                git_change_markers: split.git_change_markers,
                search_matches: split.search_matches,
                search_current: split.search_current,
                selection: split.selection,
                cursor: split.cursor,
                scroll: split.scroll,
            },
            theme,
        );
    } else {
        draw_editor_panel(
            frame,
            editor_area,
            EditorPanelView {
                title: editor.title,
                lines: editor.lines,
                focused: true,
                line_numbers: editor.line_numbers,
                wrap: editor.wrap,
                git_change_markers: Some(editor.git_change_markers),
                search_matches: editor.search_matches,
                search_current: editor.search_current,
                selection: editor.selection,
                cursor: editor.cursor,
                scroll: editor.scroll,
            },
            theme,
        );
    }

    if let Some(dialog) = editor.dialog {
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

fn draw_editor_panel(frame: &mut Frame, area: Rect, panel: EditorPanelView, theme: Theme) {
    let mut lines = panel.lines;
    highlight_selection(&mut lines, panel.selection, theme);
    highlight_search_matches(
        &mut lines,
        &panel.search_matches,
        panel.search_current,
        theme,
    );
    if let Some((_, row)) = panel.cursor.filter(|(_, row)| *row < lines.len()) {
        lines[row].style = lines[row].style.patch(theme.cursor);
    }

    let [title_area, editor_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    let title_style = if panel.focused {
        theme.selection.patch(theme.ui_chrome)
    } else {
        theme.ui_chrome
    };
    frame.render_widget(
        Paragraph::new(Line::styled(format!(" {} ", panel.title), title_style))
            .style(theme.background),
        title_area,
    );

    let show_git_change_gutter = panel
        .git_change_markers
        .as_ref()
        .is_some_and(|markers| markers.iter().any(Option::is_some));
    let editor_area = if show_git_change_gutter {
        let [gutter_area, editor_area] =
            Layout::horizontal([Constraint::Length(2), Constraint::Min(1)]).areas(editor_area);

        let gutter_lines = (0..lines.len())
            .map(|row| {
                match panel
                    .git_change_markers
                    .as_ref()
                    .and_then(|markers| markers.get(row))
                    .copied()
                    .flatten()
                {
                    Some(LineChange::Added) => Line::styled("▏ ".to_owned(), theme.git_added),
                    Some(LineChange::Deleted) => Line::styled("▔ ".to_owned(), theme.git_deleted),
                    Some(LineChange::Modified) => Line::styled("▏ ".to_owned(), theme.git_modified),
                    None => Line::styled("  ".to_owned(), theme.background),
                }
            })
            .collect::<Vec<_>>();
        let gutter = Paragraph::new(gutter_lines)
            .style(theme.background)
            .scroll((panel.scroll.0 as u16, 0));
        frame.render_widget(gutter, gutter_area);
        editor_area
    } else {
        editor_area
    };

    let editor_area = if panel.line_numbers {
        let gutter_width = line_number_gutter_width(lines.len());
        let [gutter_area, editor_area] =
            Layout::horizontal([Constraint::Length(gutter_width), Constraint::Min(1)])
                .areas(editor_area);

        let gutter_lines = (1..=lines.len())
            .map(|number| {
                Line::styled(
                    format!("{number:>width$} ", width = gutter_width as usize - 1),
                    theme.ui_chrome,
                )
            })
            .collect::<Vec<_>>();
        let gutter = Paragraph::new(gutter_lines)
            .style(theme.background)
            .scroll((panel.scroll.0 as u16, 0));
        frame.render_widget(gutter, gutter_area);
        editor_area
    } else {
        editor_area
    };

    let editor_widget = Paragraph::new(lines)
        .block(Block::default())
        .style(theme.background);
    let editor_widget = if panel.wrap {
        editor_widget
            .wrap(Wrap { trim: false })
            .scroll((panel.scroll.0 as u16, 0))
    } else {
        editor_widget.scroll((panel.scroll.0 as u16, panel.scroll.1 as u16))
    };
    frame.render_widget(editor_widget, editor_area);

    if let Some((column, row)) = panel.cursor {
        frame.set_cursor_position((editor_area.x + column as u16, editor_area.y + row as u16));
    }
}

fn draw_sidebar(
    frame: &mut Frame,
    area: Rect,
    root: &str,
    rows: &[SidebarRow],
    selected_row: Option<usize>,
    focused: bool,
    theme: Theme,
) {
    let lines = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let selected = Some(index) == selected_row;
            let style = if selected {
                theme.selection.patch(theme.ui_chrome)
            } else {
                theme.background
            };
            let marker_style = row.marker.map(|marker| {
                let base = match marker {
                    'A' => theme.git_added,
                    'M' => theme.git_modified,
                    '?' => theme.git_untracked,
                    _ => theme.ui_chrome,
                };
                if selected {
                    base.patch(theme.selection)
                } else {
                    base
                }
            });
            sidebar_line(&row.label, row.marker, style, marker_style, area.width)
        })
        .collect::<Vec<_>>();

    let title = if focused {
        format!(
            " Files * {} ",
            fit_status_line(root, area.width.saturating_sub(12) as usize)
        )
    } else {
        format!(
            " Files {} ",
            fit_status_line(root, area.width.saturating_sub(10) as usize)
        )
    };

    let widget = Paragraph::new(lines).style(theme.background).block(
        Block::default()
            .title(title)
            .title_style(theme.ui_chrome)
            .borders(Borders::ALL)
            .border_style(theme.ui_chrome),
    );
    frame.render_widget(widget, area);
}

fn sidebar_line(
    label: &str,
    marker: Option<char>,
    label_style: ratatui::style::Style,
    marker_style: Option<ratatui::style::Style>,
    width: u16,
) -> Line<'static> {
    let inner_width = width.saturating_sub(2) as usize;
    if inner_width == 0 {
        return Line::styled(String::new(), label_style);
    }

    let label_width = label.chars().count();
    let marker_width = usize::from(marker.is_some());
    let gap = inner_width.saturating_sub(label_width + marker_width);

    let mut spans = vec![Span::styled(label.to_owned(), label_style)];
    if gap > 0 {
        spans.push(Span::styled(" ".repeat(gap), label_style));
    }
    if let Some(marker) = marker {
        spans.push(Span::styled(
            marker.to_string(),
            marker_style.unwrap_or(label_style),
        ));
    }

    Line::from(spans)
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

fn draw_config(frame: &mut Frame, area: Rect, config: ConfigView, theme: Theme) {
    let [main_area, hint_area] =
        Layout::vertical([Constraint::Min(8), Constraint::Length(2)]).areas(area);
    let [left_area, preview_area] =
        Layout::horizontal([Constraint::Length(32), Constraint::Min(20)]).areas(main_area);
    let [theme_area, options_area] =
        Layout::vertical([Constraint::Min(8), Constraint::Length(9)]).areas(left_area);

    let theme_lines = config
        .themes
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let prefix = if *name == config.applied_theme {
                "> "
            } else {
                "  "
            };
            let style = if config.active_pane == ConfigPane::Theme && index == config.selected_theme
            {
                theme.selection.patch(theme.ui_chrome)
            } else if *name == config.applied_theme {
                theme.ui_chrome
            } else {
                theme.background
            };
            Line::styled(format!("{prefix}{name}"), style)
        })
        .collect::<Vec<_>>();
    let theme_widget = Paragraph::new(theme_lines).style(theme.background).block(
        Block::default()
            .title(if config.active_pane == ConfigPane::Theme {
                " Theme * "
            } else {
                " Theme "
            })
            .title_style(theme.ui_chrome)
            .borders(Borders::ALL)
            .border_style(theme.ui_chrome),
    );
    frame.render_widget(theme_widget, theme_area);

    let option_lines = config
        .options
        .iter()
        .enumerate()
        .map(|(index, (label, value))| {
            let style =
                if config.active_pane == ConfigPane::Options && index == config.selected_option {
                    theme.selection.patch(theme.ui_chrome)
                } else {
                    theme.background
                };
            Line::styled(format!("{label:<13} [{value}]"), style)
        })
        .collect::<Vec<_>>();
    let options_widget = Paragraph::new(option_lines).style(theme.background).block(
        Block::default()
            .title(if config.active_pane == ConfigPane::Options {
                " Options * "
            } else {
                " Options "
            })
            .title_style(theme.ui_chrome)
            .borders(Borders::ALL)
            .border_style(theme.ui_chrome),
    );
    frame.render_widget(options_widget, options_area);

    let preview_widget = Paragraph::new(config.preview_lines)
        .style(config.preview_theme.background)
        .block(
            Block::default()
                .title(" Preview ")
                .title_style(config.preview_theme.ui_chrome)
                .borders(Borders::ALL)
                .border_style(config.preview_theme.ui_chrome),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(preview_widget, preview_area);

    let hint = Paragraph::new(vec![Line::raw(
        "Tab switch pane   Enter apply   Ctrl+, close   S save   Esc cancel",
    )])
    .style(theme.background.patch(theme.ui_chrome))
    .block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(theme.ui_chrome),
    );
    frame.render_widget(hint, hint_area);
}

fn draw_welcome(frame: &mut Frame, area: Rect, welcome: WelcomeView<'_>, theme: Theme) {
    let [hero_area, body_area, hint_area] = Layout::vertical([
        Constraint::Length(8),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .areas(area);
    let [logo_area, title_area] =
        Layout::horizontal([Constraint::Length(36), Constraint::Min(20)]).areas(hero_area);
    let [shortcuts_area, recents_area] = two_column(body_area);

    // Slide the logo in from the right over ~1.4 seconds (17 ticks × 80ms).
    // offset starts at LOGO_WIDTH and decreases by 2 per tick until 0.
    const LOGO_WIDTH: usize = 34;
    let tick = welcome.meta.tick as usize;
    let offset = (LOGO_WIDTH)
        .saturating_sub(tick.saturating_mul(2))
        .min(LOGO_WIDTH);

    let logo_lines: Vec<Line> = welcome
        .logo
        .iter()
        .map(|row| {
            let visible: String = row.chars().skip(offset).collect();
            let spaces = " ".repeat(offset);
            Line::raw(format!("{spaces}{visible}"))
        })
        .collect();

    let logo_widget = Paragraph::new(logo_lines).style(theme.background).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.ui_chrome),
    );
    frame.render_widget(logo_widget, logo_area);

    let mut title_lines = vec![Line::raw("implicit"), Line::raw(welcome.version.to_owned())];
    if let Some(badge) = welcome.badge {
        title_lines.push(Line::styled(badge.to_owned(), theme.ui_chrome));
    }
    title_lines.extend([
        Line::raw("a markdown editor for the terminal"),
        Line::raw(""),
        Line::raw("Press O to open a file"),
        Line::raw("Press N for a new untitled buffer"),
        Line::raw("Press C for settings"),
    ]);
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
        "O open picker   N new buffer   C settings   Enter open recent   / search files   Q quit"
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

fn line_number_gutter_width(line_count: usize) -> u16 {
    (line_count.max(1).to_string().len() as u16) + 2
}

fn highlight_search_matches(
    lines: &mut [Line<'static>],
    search_matches: &[SearchMatch],
    current: Option<usize>,
    theme: Theme,
) {
    for (index, search_match) in search_matches.iter().copied().enumerate() {
        let Some(line) = lines.get_mut(search_match.row) else {
            continue;
        };

        let style = if Some(index) == current {
            theme.selection.patch(theme.ui_chrome)
        } else {
            theme.selection
        };
        style_line_range(line, search_match.col, search_match.len, style);
    }
}

fn highlight_selection(
    lines: &mut [Line<'static>],
    selection: Option<SelectionRange>,
    theme: Theme,
) {
    let Some(selection) = selection else {
        return;
    };

    let ((start_row, start_col), (end_row, end_col)) = (selection.start, selection.end);
    for row in start_row..=end_row {
        let Some(line) = lines.get_mut(row) else {
            continue;
        };

        let start = if row == start_row { start_col } else { 0 };
        let end = if row == end_row {
            end_col
        } else {
            line.width()
        };
        let len = end.saturating_sub(start);
        style_line_range(line, start, len.max(1), theme.selection);
    }
}

fn style_line_range(
    line: &mut Line<'static>,
    start: usize,
    len: usize,
    style: ratatui::style::Style,
) {
    if len == 0 {
        return;
    }

    let end = start.saturating_add(len);
    let mut spans = Vec::new();
    let mut offset = 0usize;

    for span in line.spans.drain(..) {
        let content = span.content.as_ref();
        let span_len = content.chars().count();
        let span_start = offset;
        let span_end = offset + span_len;

        if span_end <= start || span_start >= end {
            spans.push(span);
            offset = span_end;
            continue;
        }

        let prefix_len = start.saturating_sub(span_start).min(span_len);
        let highlight_start = prefix_len;
        let highlight_end = end.saturating_sub(span_start).min(span_len);

        if prefix_len > 0 {
            spans.push(Span::styled(
                content.chars().take(prefix_len).collect::<String>(),
                span.style,
            ));
        }

        if highlight_end > highlight_start {
            spans.push(Span::styled(
                content
                    .chars()
                    .skip(highlight_start)
                    .take(highlight_end - highlight_start)
                    .collect::<String>(),
                span.style.patch(style),
            ));
        }

        if highlight_end < span_len {
            spans.push(Span::styled(
                content.chars().skip(highlight_end).collect::<String>(),
                span.style,
            ));
        }

        offset = span_end;
    }

    line.spans = spans;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Style;

    #[test]
    fn keeps_short_status_lines_intact() {
        assert_eq!(fit_status_line("hello", 8), "hello   ");
    }

    #[test]
    fn truncates_long_status_lines_from_the_left() {
        assert_eq!(fit_status_line("abcdef", 4), "…def");
    }

    #[test]
    fn highlights_search_match_inside_line_spans() {
        let theme = Theme::source_hints_default();
        let mut line = Line::from(vec![
            Span::styled(String::from("alpha "), Style::default()),
            Span::styled(String::from("beta"), theme.code_keyword),
        ]);

        style_line_range(&mut line, 6, 4, theme.selection);

        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[1].content.as_ref(), "beta");
        assert_eq!(
            line.spans[1].style,
            theme.code_keyword.patch(theme.selection)
        );
    }

    #[test]
    fn current_search_match_uses_emphasized_style() {
        let theme = Theme::source_hints_default();
        let mut lines = vec![Line::from(String::from("alpha beta"))];

        highlight_search_matches(
            &mut lines,
            &[SearchMatch {
                row: 0,
                col: 6,
                len: 4,
            }],
            Some(0),
            theme,
        );

        assert!(
            lines[0]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == "beta"
                    && span.style
                        == Style::default().patch(theme.selection.patch(theme.ui_chrome)))
        );
    }

    #[test]
    fn selection_highlight_styles_selected_range() {
        let theme = Theme::source_hints_default();
        let mut lines = vec![Line::from(String::from("alpha beta"))];

        highlight_selection(
            &mut lines,
            Some(SelectionRange {
                start: (0, 6),
                end: (0, 10),
            }),
            theme,
        );

        assert!(
            lines[0]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == "beta"
                    && span.style == Style::default().patch(theme.selection))
        );
    }
}
