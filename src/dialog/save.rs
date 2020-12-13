use std::path::PathBuf;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::dialog::Dialog;
use crate::terminal::Terminal;

/// Message prompt of the save dialog.
const SAVE_DIALOG_PROMPT: &str = "Output path (leave empty for stdout):";

/// Dialog for saving the sketch.
#[derive(Default, PartialEq, Eq)]
pub struct SaveDialog {
    path: String,
}

impl SaveDialog {
    /// Create a new save dialog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a keystroke.
    pub fn keyboard_input(&mut self, terminal: &mut Terminal, glyph: char) {
        // Only accept renderable glyphs.
        if glyph != '\x7f' && glyph.width().unwrap_or_default() == 0 {
            return;
        }

        // Add the new glyph to the path.
        match glyph {
            '\x7f' => {
                let _ = self.path.pop();
            },
            c => self.path.push(c),
        }

        // Update the dialog.
        self.render(terminal);
    }

    /// The selected save path.
    pub fn path(&self) -> Option<PathBuf> {
        let path = self.path.trim();
        if path.is_empty() {
            None
        } else {
            Some(PathBuf::from(path))
        }
    }
}

impl Dialog for SaveDialog {
    fn lines(&self) -> Vec<String> {
        vec![SAVE_DIALOG_PROMPT.into(), self.path.clone()]
    }

    fn cursor_position(&self, lines: &[String]) -> Option<(usize, usize)> {
        Some((lines.get(1).map(|line| line.width()).unwrap_or_default(), 1))
    }
}
