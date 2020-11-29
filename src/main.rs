mod terminal;

use std::cmp::max;
use std::convert::TryFrom;
use std::fmt::{self, Display, Formatter};
use std::io;

use unicode_width::UnicodeWidthChar;

use crate::terminal::event::{EventHandler, Modifiers, MouseButton, MouseEvent};
use crate::terminal::{Terminal, TerminalMode};

fn main() -> io::Result<()> {
    Sketch::new().run()
}

/// Coordinate in the terminal grid.
#[derive(Default, Copy, Clone)]
struct Point {
    column: usize,
    line: usize,
}

/// Sketch application state.
struct Sketch {
    cursor_position: Point,
    content: Vec<Vec<char>>,

    cursor: Vec<Vec<char>>,
    cursor_size: u8,
}

impl Default for Sketch {
    fn default() -> Self {
        Self {
            cursor_position: Default::default(),
            content: Default::default(),
            cursor: Self::create_cursor(1, '+'),
            cursor_size: 1,
        }
    }
}

impl Sketch {
    /// Setup the Sketch application state.
    fn new() -> Self {
        Self::default()
    }

    /// Run the terminal event loop.
    fn run(&mut self) -> io::Result<()> {
        let mut terminal = Terminal::new(self);

        // Perform terminal setup for the TUI.
        terminal.set_mode(TerminalMode::ShowCursor, false);
        terminal.set_mode(TerminalMode::LineWrap, false);
        terminal.set_mode(TerminalMode::AltScreen, true);
        terminal.set_mode(TerminalMode::SgrMouse, true);
        terminal.set_mode(TerminalMode::MouseMotion, true);
        Terminal::goto(0, 0);

        // Run the terminal event loop.
        terminal.run()
    }

    /// Move terminal cursor.
    fn goto(&mut self, column: usize, line: usize) {
        self.cursor_position = Point { column, line };
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
            let Point { column, line } = self.cursor_position;
            *self.content.get_or_insert(line - 1).get_or_insert(column - 1) = c;
        }

        // Write to the terminal.
        self.cursor_position.column += 1;
        Terminal::write(c);
    }

    /// Write the cursor's content at its current location.
    fn write_cursor(&mut self, mode: CursorWriteMode) {
        let cursor_position = self.cursor_position;

        // Find the top left corner of the cursor.
        let origin_column = cursor_position.column as isize - (self.cursor_size as isize - 1);
        let origin_line = cursor_position.line as isize - (self.cursor_size as isize - 1);

        // Write the cursor characters.
        for line in 0..self.cursor.len() {
            let target_line = origin_line + line as isize;
            let skip = usize::try_from(origin_column * -1 + 1).unwrap_or_default();
            let first_occupied = self.cursor[line].iter().skip(skip).position(|c| *c != '\0');

            // Skip this line if there is no occupied cell within the grid.
            let first_occupied = match first_occupied {
                Some(first_occupied) if target_line > 0 => first_occupied + skip,
                _ => continue,
            };

            // Move the cursor to the target for the first occupied cell.
            let first_column = origin_column + first_occupied as isize;
            self.goto(first_column as usize, target_line as usize);

            for column in first_occupied..self.cursor[line].len() {
                let target_column = origin_column + column as isize;
                let c = self.cursor[line][column];

                // Stop once we've reached the end of the current line.
                if c == '\0' {
                    break;
                }

                match mode {
                    CursorWriteMode::WriteVolatile => self.write(c, false),
                    CursorWriteMode::Write => self.write(c, true),
                    CursorWriteMode::Erase => self.write(' ', true),
                    CursorWriteMode::Reset => {
                        let c = *self
                            .content
                            .get_or_insert(target_line as usize - 1)
                            .get_or_insert(target_column as usize - 1);

                        if c == '\0' {
                            self.write(' ', false);
                        } else {
                            self.write(c, false);
                        }
                    },
                }
            }
        }

        // Restore cursor position.
        self.goto(cursor_position.column, cursor_position.line);
    }

    /// Create a new cursor with the specified size.
    ///
    /// The cursor will always be diamond shaped, for a cursor of size 2 using the character `+`,
    /// the resulting vector would look like this:
    ///
    /// ```
    ///   +
    ///  +++
    /// +++++
    ///  +++
    ///   +
    /// ```
    fn create_cursor(size: u8, c: char) -> Vec<Vec<char>> {
        let width = size as usize * 2 - 1;
        let mut cursor = vec![vec!['\0'; width]; width];

        let mut num_chars = 1;
        for line in 0..width {
            let start = width / 2 - num_chars / 2;

            for column in start..(start + num_chars) {
                cursor[line][column] = c;
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

impl EventHandler for Sketch {
    fn keyboard_input(&mut self, event: char) {
        self.write(event, true);
    }

    fn mouse_input(&mut self, event: MouseEvent) {
        // TODO: Lock stdin in here?

        self.write_cursor(CursorWriteMode::Reset);

        self.goto(event.column, event.line);

        match event.button {
            MouseButton::Left => self.write_cursor(CursorWriteMode::Write),
            MouseButton::Right => self.write_cursor(CursorWriteMode::Erase),
            MouseButton::Index(4) if event.modifiers.contains(Modifiers::CONTROL) => {
                self.cursor_size += 1;
                self.cursor = Self::create_cursor(self.cursor_size, '+');
            },
            MouseButton::Index(5) if event.modifiers.contains(Modifiers::CONTROL) => {
                self.cursor_size = max(1, self.cursor_size - 1);
                self.cursor = Self::create_cursor(self.cursor_size, '+');
            },
            _ => (),
        }

        // Preview cursor using the dim colors.
        Terminal::set_dim();
        self.write_cursor(CursorWriteMode::WriteVolatile);
        Terminal::reset_sgr();
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

trait GetOrInsert<T> {
    fn get_or_insert(&mut self, index: usize) -> &mut T;
}

impl<T: Default> GetOrInsert<T> for Vec<T> {
    fn get_or_insert(&mut self, index: usize) -> &mut T {
        let len = self.len();
        if len <= index {
            for _ in len..=index {
                self.push(T::default());
            }
        }

        &mut self[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[rustfmt::skip]
    fn cursor() {
        let cursor = Sketch::create_cursor(1, '+');
        assert_eq!(cursor, vec![vec!['+']]);

        let cursor = Sketch::create_cursor(2, '+');
        assert_eq!(cursor, vec![
            vec!['\0', '+', '\0'],
            vec![ '+', '+',  '+'],
            vec!['\0', '+', '\0'],
        ]);

        let cursor = Sketch::create_cursor(3, 'x');
        assert_eq!(cursor, vec![
            vec!['\0', '\0', 'x', '\0', '\0'],
            vec!['\0',  'x', 'x',  'x', '\0'],
            vec![ 'x',  'x', 'x',  'x',  'x'],
            vec!['\0',  'x', 'x',  'x', '\0'],
            vec!['\0', '\0', 'x', '\0', '\0'],
        ]);
    }
}
