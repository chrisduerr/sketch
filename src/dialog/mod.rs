use unicode_width::UnicodeWidthStr;

use crate::terminal::{Color, CursorShape, Terminal, TerminalMode};

pub mod brush_character;
pub mod colorpicker;
pub mod save;

pub trait Dialog {
    fn lines(&self) -> Vec<String>;

    /// Foreground and background for the box drawing characters.
    fn box_color(&self) -> (Color, Color) {
        (Color::default(), Color::default())
    }

    /// Cursor position relative to the top left corner of the dialog content.
    ///
    /// If this is `None`, the cursor will not be visible in the dialog.
    fn cursor_position(&self, _lines: &[String]) -> Option<(usize, usize)> {
        None
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

        let (cursor_column, cursor_line) = match self.cursor_position(&lines) {
            Some(position) => position,
            None => {
                terminal.set_mode(TerminalMode::ShowCursor, false);
                return;
            },
        };

        // Show the terminal cursor.
        terminal.set_mode(TerminalMode::ShowCursor, true);
        Terminal::set_cursor_shape(CursorShape::Underline);

        // Always put the cursor at the last cell in the first line.
        Terminal::goto(column + 2 + cursor_column, line - lines.len() + cursor_line);
    }
}
