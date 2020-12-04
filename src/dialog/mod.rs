use unicode_width::UnicodeWidthStr;

use crate::terminal::{Dimensions, Terminal, TerminalMode, CursorShape};

/// Message prompt of the brush character picker dialog.
const BRUSH_CHARACTER_DIALOG_PROMPT: &str = "Pick a brush character: ";

pub trait Dialog {
    fn lines(&self) -> &[String];

    /// Position of the top left dialog corner.
    #[inline]
    fn origin(&self, dimensions: Dimensions) -> (usize, usize) {
        let lines = self.lines();
        let max_width = lines.iter().map(|line| line.width() + 4).max().unwrap_or(0);

        let column = (dimensions.columns as usize - max_width) / 2;
        let line = (dimensions.lines as usize - 5) / 2;
        (column, line)
    }

    /// Render the dialog to the terminal.
    fn render(&self, dimensions: Dimensions) {
        let (column, mut line) = self.origin(dimensions);
        let lines = self.lines();

        let max_width = lines.iter().map(|line| line.width() + 4).max().unwrap_or(0);

        // Write the top of the dialog box.
        Terminal::goto(column, line);
        Terminal::write(format!("┌{}┐", "─".repeat(max_width - 2)));
        line += 1;

        // Write the dialog text.
        for text in lines {
            Terminal::goto(column, line);
            Terminal::write(format!("│ {} │", text));
            line += 1;
        }

        // Write the bottom of the dialog box.
        Terminal::goto(column, line);
        Terminal::write(format!("└{}┘", "─".repeat(max_width - 2)));
    }
}

/// Dialog for picking a new brush glyph.
#[derive(PartialEq, Eq)]
pub struct BrushCharacterDialog {
    text: Vec<String>,
}

impl BrushCharacterDialog {
    /// Create a new brush character dialog.
    ///
    /// The brush character `glyph` will be rendered at the end of the prompt to indicate to the
    /// user what the active glyph for the brush is.
    pub fn new(glyph: char) -> Self {
        Self {
            text: vec![format!("{}{}", BRUSH_CHARACTER_DIALOG_PROMPT, glyph)],
        }
    }

    /// Custom renderer which moves the cursor below the selected character.
    pub fn render(&self, terminal: &mut Terminal) {
        // Render the dialog itself using the trait impl.
        Dialog::render(self, terminal.dimensions);

        // Show the terminal cursor.
        terminal.set_mode(TerminalMode::ShowCursor, true);
        Terminal::set_cursor_shape(CursorShape::Underline);

        // Put cursor below the selected character.
        let (column, line) = self.origin(terminal.dimensions);
        Terminal::goto(column + BRUSH_CHARACTER_DIALOG_PROMPT.len() + 2, line + 1);
    }
}

impl Dialog for BrushCharacterDialog {
    fn lines(&self) -> &[String] {
        &self.text
    }
}
