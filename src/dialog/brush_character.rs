use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::dialog::Dialog;
use crate::terminal::Terminal;

/// Message prompt of the brush character picker dialog.
const BRUSH_CHARACTER_DIALOG_PROMPT: &str = "Pick a brush character: ";

/// Dialog for picking a new brush glyph.
#[derive(PartialEq, Eq)]
pub struct BrushCharacterDialog {
    glyph: char,
}

impl BrushCharacterDialog {
    /// Create a new brush character dialog.
    ///
    /// The brush character `glyph` will be rendered at the end of the prompt to
    /// indicate to the user what the active glyph for the brush is.
    pub fn new(glyph: char) -> Self {
        Self { glyph }
    }

    /// Process a keystroke.
    pub fn keyboard_input(&mut self, terminal: &mut Terminal, glyph: char) {
        // Only accept renderable glyphs.
        if glyph.width().unwrap_or_default() == 0 {
            return;
        }

        // Switch to the new glyph.
        self.glyph = glyph;

        // Update the dialog.
        self.render(terminal);
    }

    /// The selected brush glyph.
    pub fn glyph(&self) -> char {
        self.glyph
    }
}

impl Dialog for BrushCharacterDialog {
    fn lines(&self) -> Vec<String> {
        vec![format!("{}{}", BRUSH_CHARACTER_DIALOG_PROMPT, self.glyph)]
    }

    fn cursor_position(&self, lines: &[String]) -> Option<(usize, usize)> {
        Some((lines.first().map(|line| line.width()).unwrap_or_default() - 1, 0))
    }
}
