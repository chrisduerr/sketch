mod terminal;

use std::cmp::max;
use std::convert::TryFrom;
use std::fmt::{self, Display, Formatter};
use std::io;

use unicode_width::UnicodeWidthChar;

use crate::terminal::event::{EventHandler, Modifiers, MouseButton, MouseEvent};
use crate::terminal::{Dimensions, Terminal, TerminalMode};

fn main() -> io::Result<()> {
    Sketch::new().run()
}

/// Sketch application state.
#[derive(Default)]
struct Sketch {
    content: Vec<Vec<char>>,
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
        self.resize(terminal.dimensions());

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
        // Store character in the grid state.
        if persist {
            let Point { column, line } = self.brush.position;
            if let Some(cell) =
                self.content.get_mut(line - 1).and_then(|line| line.get_mut(column - 1))
            {
                *cell = c;
            }
        }

        // Write to the terminal.
        self.brush.position.column += 1;
        Terminal::write(c);
    }

    /// Write the cursor's content at its current location.
    fn write_cursor(&mut self, mode: CursorWriteMode) {
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
                Some(first_occupied) if target_line > 0 => first_occupied + skip,
                _ => continue,
            };

            // Move the cursor to the target for the first occupied cell.
            let first_column = origin_column + first_occupied as isize;
            self.goto(first_column as usize, target_line as usize);

            for column in first_occupied..self.brush.template[line].len() {
                let target_column = origin_column + column as isize;

                // Stop once we've reached the end of the current line.
                if !self.brush.template[line][column] {
                    break;
                }

                match mode {
                    CursorWriteMode::WriteVolatile => self.write(self.brush.glyph, false),
                    CursorWriteMode::Write => self.write(self.brush.glyph, true),
                    CursorWriteMode::Erase => self.write(' ', true),
                    CursorWriteMode::Reset => {
                        let c = self
                            .content
                            .get(target_line as usize - 1)
                            .and_then(|line| line.get(target_column as usize - 1));

                        match c {
                            Some('\0') => self.write(' ', false),
                            Some(&c) => self.write(c, false),
                            _ => (),
                        }
                    },
                }
            }
        }

        // Restore cursor position.
        self.goto(cursor_position.column, cursor_position.line);
    }
}

impl EventHandler for Sketch {
    fn keyboard_input(&mut self, glyph: char) {
        self.write(glyph, true);
    }

    fn mouse_input(&mut self, event: MouseEvent) {
        // TODO: Lock stdin in here?

        self.write_cursor(CursorWriteMode::Reset);

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
    fn resize(&mut self, dimensions: Dimensions) {
        let Dimensions { columns, lines } = dimensions;
        let (columns, lines) = (columns as usize, lines as usize);

        // Add/remove lines.
        self.content.resize(lines, vec!['\0'; columns]);

        // Resize columns of each line.
        for line in &mut self.content {
            line.resize(columns, '\0');
        }

        // Force redraw to make sure user is up to date.
        self.redraw();
    }

    fn redraw(&mut self) {
        print!("\x1b[H{}", self);
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
            for c in line {
                // TODO: Handle fullwidth characters.
                match c.width() {
                    None | Some(0) => text.push(' '),
                    _ => text.push(*c),
                }
            }
            text.push('\n');
        }

        write!(f, "{}", text)
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

/// Modes for writing text using the mouse cursor.
enum CursorWriteMode {
    /// Write the cursor without storing the result.
    WriteVolatile,
    /// Write the cursor.
    Write,
    /// Write the cursor as whitespace.
    Erase,
    /// Reset the cursor to the grid's content.
    Reset,
}

/// Coordinate in the terminal grid.
#[derive(Default, Copy, Clone)]
struct Point {
    column: usize,
    line: usize,
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
