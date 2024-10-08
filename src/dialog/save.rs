use std::path::{Path, PathBuf};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::dialog::{Dialog, DialogLine};
use crate::terminal::{Color, NamedColor, Terminal};

/// Message prompt of the save dialog.
const SAVE_DIALOG_SHUTDOWN_PROMPT: &str = "Output path (leave empty for stdout):";
const SAVE_DIALOG_PROMPT: &str = "Output path:";

/// Dialog for saving the sketch.
#[derive(PartialEq, Eq)]
pub struct SaveDialog {
    path: String,
    error: bool,
    shutdown: bool,
}

impl SaveDialog {
    /// Create a new save dialog.
    pub fn new(path: String, error: bool, shutdown: bool) -> Self {
        Self { path, error, shutdown }
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
                if self.path.width() + 1 > self.prompt().len() {
                    return true;
                }
            },
            c => self.path.push(c),
        }

        // Redraw just the dialog.
        self.render(terminal);
        false
    }

    /// The selected save path.
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

    /// Whether Sketch should terminate after successfully saving.
    pub fn shutdown_on_save(&self) -> bool {
        self.shutdown
    }

    /// Dialog prompt.
    fn prompt(&self) -> &str {
        if self.shutdown {
            SAVE_DIALOG_SHUTDOWN_PROMPT
        } else {
            SAVE_DIALOG_PROMPT
        }
    }
}

impl Dialog for SaveDialog {
    fn lines(&self) -> Vec<String> {
        vec![self.prompt().into(), self.path.clone()]
    }

    fn cursor_position(&self, lines: &[DialogLine]) -> Option<(usize, usize)> {
        Some((lines.get(1).map(|line| line.width()).unwrap_or_default(), 1))
    }

    fn box_color(&self) -> (Color, Color) {
        let fg = if self.error { Color::Named(NamedColor::Red) } else { Color::default() };
        (fg, Color::default())
    }
}
