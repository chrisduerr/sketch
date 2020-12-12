use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::terminal::{Color, CursorShape, Terminal, TerminalMode};

pub mod colorpicker;

/// Message prompt of the brush character picker dialog.
const BRUSH_CHARACTER_DIALOG_PROMPT: &str = "Pick a brush character: ";

pub trait Dialog {
    fn lines(&self) -> Vec<String>;

    /// Foreground and background for the box drawing characters.
    fn box_color(&self) -> (Color, Color) {
        (Color::default(), Color::default())
    }

    /// Render the dialog to the terminal.
    fn render(&self, terminal: &mut Terminal) {
        let lines = self.lines();

        let max_width = lines.iter().map(|line| line.width()).max().unwrap_or(0) + 4;
        let column = (terminal.dimensions.columns as usize - max_width) / 2;
        let mut line = (terminal.dimensions.lines as usize - 5) / 2;

        // Setup the colored box drawing characters.
        let box_color = self.box_color();
        Terminal::set_color(box_color.0, box_color.1);

        // Write the top of the dialog box.
        Terminal::goto(column, line);
        Terminal::write(format!("┌{}┐", "─".repeat(max_width - 2)));
        line += 1;

        // Write the dialog text.
        for text in &lines {
            Terminal::goto(column, line);

            // Write a colored box drawing character.
            Terminal::set_color(box_color.0, box_color.1);
            Terminal::write("│");

            // Write the text itself without colors.
            Terminal::set_color(Color::default(), Color::default());
            let padding = " ".repeat(max_width - text.width() - 4);
            Terminal::write(format!(" {}{} ", text, padding));

            // Write a colored box drawing character.
            Terminal::set_color(box_color.0, box_color.1);
            Terminal::write("│");

            line += 1;
        }

        // Write the bottom of the dialog box.
        Terminal::goto(column, line);
        Terminal::write(format!("└{}┘", "─".repeat(max_width - 2)));

        // Show the terminal cursor.
        terminal.set_mode(TerminalMode::ShowCursor, true);
        Terminal::set_cursor_shape(CursorShape::Underline);

        // Always put the cursor at the last cell in the first line.
        let line_len = lines.get(0).map(|line| line.width()).unwrap_or_default();
        Terminal::goto(column + line_len + 1, line - lines.len());
    }
}

/// Dialog for picking a new brush glyph.
#[derive(PartialEq, Eq)]
pub struct BrushCharacterDialog {
    glyph: char,
}

impl BrushCharacterDialog {
    /// Create a new brush character dialog.
    ///
    /// The brush character `glyph` will be rendered at the end of the prompt to indicate to the
    /// user what the active glyph for the brush is.
    pub fn new(glyph: char) -> Self {
        Self { glyph }
    }

    /// Process a keystroke.
    pub fn keyboard_input(&mut self, terminal: &mut Terminal, glyph: char) {
        // Only accept renderable non-whitespace glyphs.
        if glyph.width().unwrap_or_default() == 0 || glyph.is_whitespace() {
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
}
