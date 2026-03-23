use std::path::PathBuf;

use crate::recents::RecentFile;

// "implicit" rendered as a 2-row pixel-font using half-block characters.
// Each letter is 3–5 terminal columns wide with 1-column gaps between letters.
// ▀ = top pixel only, ▄ = bottom pixel only, █ = both pixels, space = neither.
//
//  i     m       p    l    i    c    i    t
pub const BRAILLE_LOGO: &[&str] = &[
    " ▀  █▀█▀█ █▀▄ █    ▀  █▀   ▀  ▀█▀",
    " █  █   █ █▀  █▄▄  █  █▄   █   █ ",
];

pub const SHORTCUTS: &[(&str, &str)] = &[
    ("O", "Open file picker"),
    ("N", "New untitled buffer"),
    ("Enter", "Open highlighted recent file"),
    ("/", "Search files (next)"),
    ("?", "Show controls"),
    ("Q", "Quit"),
];

#[derive(Debug, Default)]
pub struct WelcomeState {
    recents: Vec<RecentFile>,
    selected: usize,
}

impl WelcomeState {
    pub fn new(recents: Vec<RecentFile>) -> Self {
        Self {
            selected: 0,
            recents,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.recents.len() {
            self.selected += 1;
        }
    }

    pub fn selected_index(&self) -> Option<usize> {
        (!self.recents.is_empty()).then_some(self.selected)
    }

    pub fn recents(&self) -> &[RecentFile] {
        &self.recents
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.recents
            .get(self.selected)
            .map(|entry| entry.path().to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recents::RecentFile;

    #[test]
    fn keeps_selection_in_bounds() {
        let mut welcome = WelcomeState::new(vec![
            RecentFile::new_for_test(PathBuf::from("/tmp/a.md"), 1),
            RecentFile::new_for_test(PathBuf::from("/tmp/b.md"), 2),
        ]);

        welcome.move_down();
        welcome.move_down();
        welcome.move_up();

        assert_eq!(welcome.selected_index(), Some(0));
    }
}
