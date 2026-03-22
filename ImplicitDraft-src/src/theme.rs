use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub heading1: Style,
    pub heading2: Style,
    pub heading3: Style,
    pub heading4: Style,
    pub heading5: Style,
    pub heading6: Style,
    pub bold: Style,
    pub italic: Style,
    pub code: Style,
    pub blockquote: Style,
    pub link: Style,
    pub rule: Style,
    pub list_marker: Style,
}

impl Theme {
    pub fn source_hints_default() -> Self {
        Self {
            heading1: Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
            heading2: Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
            heading3: Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
            heading4: Style::default()
                .fg(Color::LightYellow)
                .add_modifier(Modifier::BOLD),
            heading5: Style::default()
                .fg(Color::LightMagenta)
                .add_modifier(Modifier::BOLD),
            heading6: Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
            bold: Style::default().add_modifier(Modifier::BOLD),
            italic: Style::default().add_modifier(Modifier::ITALIC),
            code: Style::default().fg(Color::Yellow).bg(Color::DarkGray),
            blockquote: Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::ITALIC),
            link: Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::UNDERLINED),
            rule: Style::default().fg(Color::DarkGray),
            list_marker: Style::default().fg(Color::LightGreen),
        }
    }

    pub fn heading(&self, level: usize) -> Style {
        match level {
            1 => self.heading1,
            2 => self.heading2,
            3 => self.heading3,
            4 => self.heading4,
            5 => self.heading5,
            _ => self.heading6,
        }
    }
}
