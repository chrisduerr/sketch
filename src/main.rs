mod dialog;
mod terminal;

use std::cmp::max;
use std::convert::TryFrom;
use std::fmt::{self, Display, Formatter};
use std::io;

use unicode_width::UnicodeWidthChar;

use crate::dialog::Dialog;
use crate::terminal::event::{ButtonState, EventHandler, Modifiers, MouseButton, MouseEvent};
use crate::terminal::{Dimensions, Terminal, TerminalMode, CursorShape};

fn main() -> io::Result<()> {
    Sketch::new().run()
}

/// Sketch application state.
#[derive(Default)]
struct Sketch {
    content: Vec<Vec<char>>,
    mode: SketchMode,
    brush: Brush,
}

impl Sketch {
    /// Setup the Sketch application state.
    fn new() -> Self {
        Self::default()
    }

    /// Run the terminal event loop.
    fn run(mut self) -> io::Result<()> {
        let mut terminal = Terminal::new();

        // Perform terminal setup for the TUI.
        terminal.set_mode(TerminalMode::ShowCursor, false);
        terminal.set_mode(TerminalMode::LineWrap, false);
        terminal.set_mode(TerminalMode::AltScreen, true);
        terminal.set_mode(TerminalMode::SgrMouse, true);
        terminal.set_mode(TerminalMode::MouseMotion, true);
        Terminal::goto(0, 0);

        // Resize internal buffer to fit terminal dimensions.
        let dimensions = terminal.dimensions();
        self.resize(&mut terminal, dimensions);

        // Run the terminal event loop.
        terminal.set_event_handler(Box::new(self));
        terminal.run()
    }

    /// Move terminal cursor.
    fn goto(&mut self, column: usize, line: usize) {
        self.brush.position = Point { column, line };
        Terminal::goto(column, line);
    }

    /// Write character at the current cursor position.
    ///
    /// The `persist` flag determines if the write operation will be committed to Sketch's
    /// application state. This is used to clear things from the grid which are not part of the
    /// sketch (like the cursor preview).
    fn write(&mut self, c: char, persist: bool) {
        // Verify that the glyph is a printable character.
        let width = match c.width() {
            Some(width) if width > 0 => width,
            _ => return,
        };

        let Point { column, line } = self.brush.position;

        // Verify the write is within the grid.
        if self.content.len() < line || self.content[line - 1].len() + 1 < column + width {
            return;
        }

        // Store character in the grid state.
        if persist {
            // Replace the glyph itself.
            let line = &mut self.content[line - 1];
            line[column - 1] = c;

            // Reset the following character when writing fullwidth characters.
            if width == 2 {
                line[column] = '\0';
            }

            // Replace previous fullwidth character if we're writing inside its spacer.
            if column >= 2 && line[column - 2].width() == Some(2) {
                line[column - 2] = '\0';
            }
        }

        // Write to the terminal.
        self.brush.position.column += width;
        Terminal::write(c);
    }

    /// Write the cursor's content at its current location.
    fn write_cursor(&mut self, mode: CursorWriteMode) {
        let last_line = self.content.len() as isize;
        let cursor_position = self.brush.position;

        // Find the top left corner of the cursor.
        let origin_column = cursor_position.column as isize - (self.brush.size as isize - 1);
        let origin_line = cursor_position.line as isize - (self.brush.size as isize - 1);

        // Write the cursor characters.
        for line in 0..self.brush.template.len() {
            let target_line = origin_line + line as isize;
            let skip = usize::try_from(origin_column * -1 + 1).unwrap_or_default();
            let first_occupied = self.brush.template[line].iter().skip(skip).position(|b| *b);

            // Skip this line if there is no occupied cell within the grid.
            let first_occupied = match first_occupied {
                Some(first_occupied) if target_line > 0 && target_line <= last_line => {
                    first_occupied + skip
                },
                _ => continue,
            };

            // Move the cursor to the target for the first occupied cell.
            let first_column = (origin_column + first_occupied as isize) as usize;
            self.goto(first_column, target_line as usize);

            // Ignore every second cell for fullwidth brushes.
            let step_size = self.brush.glyph.width().unwrap_or(1);
            for column in (first_occupied..self.brush.template[line].len()).step_by(step_size) {
                // Stop once we've reached the end of the current line.
                if !self.brush.template[line][column] {
                    break;
                }

                match mode {
                    CursorWriteMode::WriteVolatile => self.write(self.brush.glyph, false),
                    CursorWriteMode::Write => self.write(self.brush.glyph, true),
                    CursorWriteMode::Erase => {
                        for _ in 0..step_size {
                            self.write(' ', true);
                        }
                    },
                }
            }
        }

        // Restore cursor position.
        self.goto(cursor_position.column, cursor_position.line);
    }

    /// Redraw the current sketch.
    fn redraw_content(&self, dimensions: Dimensions) {
        let Point { column, line } = self.brush.position;

        print!("\x1b[H{}", self);

        // Redraw dialogs.
        if let SketchMode::BrushCharacterPrompt(dialog) = &self.mode {
            dialog.render(dimensions);
        }

        // Restore cursor position.
        Terminal::goto(column, line);
    }

    /// Open dialog for brush character selection.
    fn open_brush_character_dialog(&mut self, terminal: &mut Terminal) {
        let dialog = Dialog::new("Pick a brush character:  ");
        dialog.render(terminal.dimensions);

        // Show the terminal cursor.
        terminal.set_mode(TerminalMode::ShowCursor, true);
        Terminal::set_cursor_shape(CursorShape::Underline);

        self.mode = SketchMode::BrushCharacterPrompt(dialog);
    }

    /// Close all dialogs and go back to sketching mode.
    fn close_dialog(&mut self, terminal: &mut Terminal) {
        self.mode = SketchMode::Sketching;

        // Hide the terminal cursor.
        terminal.set_mode(TerminalMode::ShowCursor, false);

        // Redraw everything.
        self.redraw_content(terminal.dimensions);
    }

    /// Emulate backspace to delete the last character.
    fn backspace(&mut self) {
        // Move cursor to the previous cell.
        let Point { column, line } = self.brush.position;
        self.goto(column.saturating_sub(1), line);

        // Overwrite cell without moving cursor.
        self.write(' ', true);
        self.goto(column.saturating_sub(1), line);
    }
}

impl EventHandler for Sketch {
    fn keyboard_input(&mut self, terminal: &mut Terminal, glyph: char) {
        match &mut self.mode {
            SketchMode::BrushCharacterPrompt(dialog) => match glyph {
                '\n' => self.close_dialog(terminal),
                glyph if glyph.width().unwrap_or(0) > 0 && !glyph.is_whitespace() => {
                    dialog.text.truncate(dialog.text.len() - 1);
                    dialog.text.push(glyph);
                    dialog.render(terminal.dimensions);

                    self.brush.glyph = glyph;
                },
                _ => (),
            },
            SketchMode::Sketching => match glyph {
                '\x02' => self.open_brush_character_dialog(terminal),
                // Delete last character on backspace.
                '\x7f' => self.backspace(),
                glyph => {
                    // Hide mouse brush.
                    self.redraw_content(terminal.dimensions);

                    // Show IBeam cursor while typing.
                    terminal.set_mode(TerminalMode::ShowCursor, true);
                    Terminal::set_cursor_shape(CursorShape::IBeam);

                    self.write(glyph, true);
                }
            },
        }
    }

    fn mouse_input(&mut self, terminal: &mut Terminal, event: MouseEvent) {
        // Ignore mouse release events.
        if event.button_state == ButtonState::Released || self.mode != SketchMode::Sketching {
            return;
        }

        // Hide terminal cursor while using the mouse.
        terminal.set_mode(TerminalMode::ShowCursor, false);

        self.redraw_content(terminal.dimensions);

        self.goto(event.column, event.line);

        match event.button {
            MouseButton::Left => self.write_cursor(CursorWriteMode::Write),
            MouseButton::Right => self.write_cursor(CursorWriteMode::Erase),
            MouseButton::Index(4) if event.modifiers.contains(Modifiers::CONTROL) => {
                self.brush.size += 1;
                self.brush.template = Brush::create_template(self.brush.size);
            },
            MouseButton::Index(5) if event.modifiers.contains(Modifiers::CONTROL) => {
                self.brush.size = max(1, self.brush.size - 1);
                self.brush.template = Brush::create_template(self.brush.size);
            },
            _ => (),
        }

        // Preview cursor using the dim colors.
        Terminal::set_dim();
        self.write_cursor(CursorWriteMode::WriteVolatile);
        Terminal::reset_sgr();
    }

    /// Resize the internal terminal state.
    ///
    /// This will discard all content that was written outside the terminal dimensions with no way
    /// to recover it.
    fn resize(&mut self, terminal: &mut Terminal, dimensions: Dimensions) {
        let Dimensions { columns, lines } = dimensions;
        let (columns, lines) = (columns as usize, lines as usize);

        // Add/remove lines.
        self.content.resize(lines, vec!['\0'; columns]);

        // Resize columns of each line.
        for line in &mut self.content {
            line.resize(columns, '\0');
        }

        // Force redraw to make sure user is up to date.
        self.redraw_content(terminal.dimensions);
    }

    fn redraw(&mut self, terminal: &mut Terminal) {
        self.redraw_content(terminal.dimensions);
    }
}

impl Display for Sketch {
    /// Render the entire grid to the formatter.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.content.is_empty() {
            return Ok(());
        }

        let mut text = String::new();

        for line in &self.content {
            let mut column = 0;
            while column < line.len() {
                let c = line[column];

                // Render empty cells as whitespace.
                let width = c.width();
                match c.width() {
                    Some(1) | Some(2) => text.push(c),
                    _ => text.push(' '),
                }

                // Skip columns when dealing with fullwidth characters.
                column += width.filter(|w| *w != 0).unwrap_or(1);
            }
            text.push('\n');
        }

        write!(f, "{}", text.trim_end_matches('\n'))
    }
}

impl Drop for Sketch {
    /// Print the sketch to primary screen after quitting.
    fn drop(&mut self) {
        let text = self.to_string();

        // Find the first non-empty line.
        let start_offset = text
            .chars()
            .enumerate()
            .take_while(|&(_, c)| c.is_whitespace())
            .fold(0, |acc, (i, c)| if c == '\n' { i + 1 } else { acc });

        // Print sketch without empty lines above or below it.
        println!("{}", text[start_offset..].trim_end());
    }
}

/// Drawing brush.
struct Brush {
    template: Vec<Vec<bool>>,
    position: Point,
    glyph: char,
    size: u8,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            template: Self::create_template(1),
            position: Default::default(),
            glyph: '+',
            size: 1,
        }
    }
}

impl Brush {
    /// Create a new brush template.
    ///
    /// The brush will always be diamond shaped, the resulting template is a matrix that stores
    /// `true` for every cell that contains a brush glyph and `false` for all empty cells.
    ///
    /// A brush with size 3 might look like this (`+`: `true`, `-`: `false`):
    ///
    /// ```
    /// --+--
    /// -+++-
    /// +++++
    /// -+++-
    /// --+--
    /// ```
    fn create_template(size: u8) -> Vec<Vec<bool>> {
        let width = size as usize * 2 - 1;
        let mut cursor = vec![vec![false; width]; width];

        let mut num_chars = 1;
        for line in 0..width {
            let start = width / 2 - num_chars / 2;

            for column in start..(start + num_chars) {
                cursor[line][column] = true;
            }

            if line < width / 2 {
                num_chars += 2;
            } else {
                num_chars = num_chars.saturating_sub(2);
            }
        }

        cursor
    }
}

/// Current application state.
#[derive(PartialEq, Eq)]
enum SketchMode {
    /// Default drawing mode.
    Sketching,
    /// Brush character dialog prompt.
    BrushCharacterPrompt(Dialog),
}

impl Default for SketchMode {
    fn default() -> Self {
        SketchMode::Sketching
    }
}

/// Modes for writing text using the mouse cursor.
#[derive(Debug)]
enum CursorWriteMode {
    /// Write the cursor without storing the result.
    WriteVolatile,
    /// Write the cursor.
    Write,
    /// Write the cursor as whitespace.
    Erase,
}

/// Coordinate in the terminal grid.
#[derive(Copy, Clone)]
struct Point {
    column: usize,
    line: usize,
}

impl Default for Point {
    fn default() -> Self {
        Self {
            column: 1,
            line: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[rustfmt::skip]
    fn cursor() {
        let cursor = Brush::create_template(1);
        assert_eq!(cursor, vec![vec![true]]);

        let cursor = Brush::create_template(2);
        assert_eq!(cursor, vec![
            vec![false, true, false],
            vec![true,  true, true ],
            vec![false, true, false],
        ]);

        let cursor = Brush::create_template(3);
        assert_eq!(cursor, vec![
            vec![false, false, true, false, false],
            vec![false, true,  true, true,  false],
            vec![true,  true,  true, true,  true ],
            vec![false, true,  true, true,  false],
            vec![false, false, true, false, false],
        ]);
    }
}
