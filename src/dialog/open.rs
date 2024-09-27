use std::path::{Path, PathBuf};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::dialog::{Dialog, DialogLine};
use crate::terminal::{Color, NamedColor, Terminal};

/// Message prompt of the open dialog.
const OPEN_DIALOG_PROMPT: &str = "Sketch path:";

/// Dialog for loading sketches.
#[derive(Default, PartialEq, Eq)]
pub struct OpenDialog {
    path: String,
    error: bool,
}

impl OpenDialog {
    /// Create a new import dialog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a keystroke.
    ///
    /// Returns `true` if the dialog shrunk and a full redraw is required.
    pub fn keyboard_input(&mut self, terminal: &mut Terminal, glyph: char) -> bool {
        // Only accept renderable glyphs.
        if glyph != '\x7f' && glyph.width().unwrap_or_default() == 0 {
            return false;
        }

        // Clear error when the path is changed.
        self.error = false;

        // Add the new glyph to the path.
        match glyph {
            '\x7f' => {
                let _ = self.path.pop();

                // Redraw everything if backspace caused dialog to shrink.
                if self.path.width() + 1 > OPEN_DIALOG_PROMPT.len() {
                    return true;
                }
            },
            c => self.path.push(c),
        }

        // Redraw just the dialog.
        self.render(terminal);
        false
    }

    /// The selected import path.
    pub fn path(&self) -> Option<PathBuf> {
        // Ignore paths that are empty or only whitespace.
        let path = self.path.trim();
        if path.is_empty() {
            return None;
        }

        // Handle home directory prefix.
        if let Some(stripped) = path.strip_prefix("~/") {
            // Ignore replacement without home dir, which conveniently causes an error.
            if let Some(mut path) = home::home_dir() {
                path.extend(Path::new(stripped));
                return Some(path);
            }
        }

        Some(PathBuf::from(path))
    }

    /// Indicate an error to the user.
    pub fn mark_failed(&mut self, terminal: &mut Terminal) {
        // Mark failure and update the dialog.
        self.error = true;
        self.render(terminal);
    }
}

impl Dialog for OpenDialog {
    fn lines(&self) -> Vec<String> {
        vec![OPEN_DIALOG_PROMPT.into(), self.path.clone()]
    }

    fn cursor_position(&self, lines: &[DialogLine]) -> Option<(usize, usize)> {
        Some((lines.get(1).map(|line| line.width()).unwrap_or_default(), 1))
    }

    fn box_color(&self) -> (Color, Color) {
        let fg = if self.error { Color::Named(NamedColor::Red) } else { Color::default() };
        (fg, Color::default())
    }
}
