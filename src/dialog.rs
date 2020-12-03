use unicode_width::UnicodeWidthStr;

use crate::terminal::{Dimensions, Terminal};

/// TUI dialog.
#[derive(PartialEq, Eq)]
pub struct Dialog {
    /// Dialog text content.
    pub text: String,
}

impl Dialog {
    /// Create a new dialog.
    pub fn new<S: Into<String>>(text: S) -> Self {
        Self { text: text.into() }
    }

    /// Render the dialog to the terminal.
    pub fn render(&self, dimensions: Dimensions) {
        let text_width = self.text.width() + 4;

        let origin_column = (dimensions.columns as usize - text_width) / 2;
        let origin_line = (dimensions.lines as usize - 5) / 2;

        // Write the top of the dialog box.
        Terminal::goto(origin_column, origin_line);
        Terminal::write(format!("┌{}┐", "─".repeat(text_width - 2)));

        // Write the dialog text.
        Terminal::goto(origin_column, origin_line + 1);
        Terminal::write(format!("│ {} │", self.text));

        // Write the bottom of the dialog box.
        Terminal::goto(origin_column, origin_line + 2);
        Terminal::write(format!("└{}┘", "─".repeat(text_width - 2)));

        // Put cursor below the last character in the text.
        Terminal::goto(origin_column + text_width - 3, origin_line + 1);
    }
}
