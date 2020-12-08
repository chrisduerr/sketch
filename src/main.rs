mod dialog;
mod terminal;

use std::cmp::max;
use std::convert::TryFrom;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::mem;

use unicode_width::UnicodeWidthChar;

use crate::dialog::colorpicker::{ColorPosition, ColorpickerDialog};
use crate::dialog::{BrushCharacterDialog, Dialog};
use crate::terminal::event::{ButtonState, EventHandler, MouseButton, MouseEvent};
use crate::terminal::{Color, CursorShape, Dimensions, Terminal, TerminalMode};

/// Help text for the last line.
const KEYBINDING_HELP: &str =
    "[^T] Brush glyph  [^F] Foreground  [^B] Background  [Wheel] Brush size  [^C] Quit";

fn main() -> io::Result<()> {
    // Launch the application.
    Sketch::new().run()
}

/// Sketch application state.
#[derive(Default)]
struct Sketch {
    content: Vec<Vec<Cell>>,
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
        let foreground = self.brush.foreground_color;
        let background = self.brush.background_color;
        if persist {
            // Replace the glyph itself.
            let line = &mut self.content[line - 1];
            line[column - 1] = Cell::new(c, foreground, background);

            // Reset the following character when writing fullwidth characters.
            if width == 2 {
                line[column].clear();
            }

            // Replace previous fullwidth character if we're writing inside its spacer.
            if column >= 2 && line[column - 2].c.width() == Some(2) {
                line[column - 2].clear();
            }
        }

        // Set the correct colors for the terminal write.
        Terminal::set_color(foreground, background);

        // Write to the terminal.
        self.brush.position.column += width;
        Terminal::write(c);
    }

    /// Write the cursor's content at its current location.
    fn write_cursor(&mut self, mode: CursorWriteMode) {
        let last_line = self.content.len() as isize;
        let cursor_position = self.brush.position;

        // Find the top left corner of the cursor.
        let brush_width = self.brush.template[0].len();
        let brush_height = self.brush.template.len();
        let origin_column = cursor_position.column as isize - brush_width as isize / 2;
        let origin_line = cursor_position.line as isize - brush_height as isize / 2;

        // Write the cursor characters.
        for line in 0..brush_height {
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
            for column in (first_occupied..brush_width).step_by(step_size) {
                // Stop once we've reached the end of the current line.
                if !self.brush.template[line][column] {
                    break;
                }

                match mode {
                    CursorWriteMode::WriteVolatile => self.write(self.brush.glyph, false),
                    CursorWriteMode::Write => self.write(self.brush.glyph, true),
                    CursorWriteMode::Erase => {
                        // Overwrite characters with default background set.
                        let background = mem::take(&mut self.brush.background_color);
                        for _ in 0..step_size {
                            self.write(' ', true);
                        }
                        self.brush.background_color = background;
                    },
                }
            }
        }

        // Restore cursor position.
        self.goto(cursor_position.column, cursor_position.line);
    }

    // Preview the sketching brush using the dim colors.
    fn preview_brush(&mut self) {
        Terminal::set_dim();
        self.write_cursor(CursorWriteMode::WriteVolatile);
        Terminal::reset_sgr();
    }

    /// Close all dialogs and go back to sketching mode.
    fn close_dialog(&mut self, terminal: &mut Terminal) {
        self.mode = SketchMode::Sketching;

        // Hide the terminal cursor.
        terminal.set_mode(TerminalMode::ShowCursor, false);

        // Redraw everything.
        self.redraw(terminal);

        self.preview_brush();
    }

    /// Emulate backspace to delete the last character.
    fn backspace(&mut self) {
        // Move cursor to the previous cell.
        let Point { column, line } = self.brush.position;
        let column = max(column.saturating_sub(1), 1);
        self.goto(column, line);

        // Overwrite cell without moving cursor.
        self.write(' ', true);
        self.goto(column, line);
    }

    /// Open the dialog for color selection.
    fn open_color_dialog(&mut self, terminal: &mut Terminal, color_position: ColorPosition) {
        let dialog = ColorpickerDialog::new(color_position);
        dialog.render(terminal);

        self.mode = SketchMode::ColorpickerDialog(dialog);
    }

    /// Open the dialog for brush character selection.
    fn open_brush_character_dialog(&mut self, terminal: &mut Terminal) {
        let dialog = BrushCharacterDialog::new(self.brush.glyph);
        dialog.render(terminal);

        self.mode = SketchMode::BrushCharacterDialog(dialog);
    }

    /// Render the keybinding help message.
    fn render_help(&mut self) {
        // Skip drawing if the last line has any content in it.
        let last_line = self.content.len() - 1;
        if self.content[last_line].iter().find(|cell| **cell != Cell::default()).is_some() {
            return;
        }

        // Write the help message into the last line.
        Terminal::set_color(Color::default(), Color::default());
        Terminal::goto(0, last_line + 1);
        Terminal::write(KEYBINDING_HELP);
    }
}

impl EventHandler for Sketch {
    fn keyboard_input(&mut self, terminal: &mut Terminal, glyph: char) {
        // Hide mouse brush while typing.
        self.redraw(terminal);

        match &mut self.mode {
            SketchMode::BrushCharacterDialog(dialog) => match glyph {
                '\n' => {
                    self.brush.glyph = dialog.glyph();
                    self.close_dialog(terminal);
                },
                glyph => dialog.keyboard_input(terminal, glyph),
            },
            SketchMode::ColorpickerDialog(dialog) => match glyph {
                // Reset to default color on ^E.
                '\x05' => {
                    self.brush.set_color(dialog.color_position(), dialog.color());
                    self.close_dialog(terminal);
                },
                '\n' => {
                    self.brush.set_color(dialog.color_position(), dialog.color());
                    self.close_dialog(terminal);
                },
                glyph => dialog.keyboard_input(terminal, glyph),
            },
            SketchMode::Sketching => match glyph {
                // Open background colorpicker dialog on ^B.
                '\x02' => self.open_color_dialog(terminal, ColorPosition::Background),
                // Open foreground colorpicker dialog on ^F.
                '\x06' => self.open_color_dialog(terminal, ColorPosition::Foreground),
                // Open brush character dialog on ^B.
                '\x14' => self.open_brush_character_dialog(terminal),
                // Delete last character on backspace.
                '\x7f' => self.backspace(),
                glyph if glyph.width().unwrap_or_default() > 0 => {
                    // Show IBeam cursor while typing.
                    terminal.set_mode(TerminalMode::ShowCursor, true);
                    Terminal::set_cursor_shape(CursorShape::IBeam);

                    self.write(glyph, true);
                },
                _ => (),
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

        self.redraw(terminal);

        self.goto(event.column, event.line);

        match event.button {
            MouseButton::Left => self.write_cursor(CursorWriteMode::Write),
            MouseButton::Right => self.write_cursor(CursorWriteMode::Erase),
            MouseButton::Index(4) => {
                self.brush.size += 1;
                self.brush.template = Brush::create_template(self.brush.size);
            },
            MouseButton::Index(5) => {
                self.brush.size = max(1, self.brush.size - 1);
                self.brush.template = Brush::create_template(self.brush.size);
            },
            _ => (),
        }

        self.preview_brush();
    }

    /// Resize the internal terminal state.
    ///
    /// This will discard all content that was written outside the terminal dimensions with no way
    /// to recover it.
    fn resize(&mut self, terminal: &mut Terminal, dimensions: Dimensions) {
        let Dimensions { columns, lines } = dimensions;
        let (columns, lines) = (columns as usize, lines as usize);

        // Add/remove lines.
        self.content.resize(lines, vec![Cell::default(); columns]);

        // Resize columns of each line.
        for line in &mut self.content {
            line.resize(columns, Cell::default());
        }

        // Force redraw to make sure user is up to date.
        self.redraw(terminal);
    }

    fn redraw(&mut self, terminal: &mut Terminal) {
        let Point { column, line } = self.brush.position;

        // Re-print the entire stored buffer.
        Terminal::goto(1, 1);
        Terminal::write(self.to_string());

        // Redraw dialogs.
        match &mut self.mode {
            SketchMode::BrushCharacterDialog(dialog) => dialog.render(terminal),
            SketchMode::ColorpickerDialog(dialog) => dialog.render(terminal),
            SketchMode::Sketching => (),
        }

        self.render_help();

        // Restore cursor position.
        Terminal::goto(column, line);
    }
}

impl Display for Sketch {
    /// Render the entire grid to the formatter.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.content.is_empty() {
            return Ok(());
        }

        let mut text = String::new();

        // Store colors to reduce the number of writes when nothing changes.
        let mut foreground = Color::default();
        let mut background = Color::default();
        Terminal::set_color(foreground, background);

        for line in &self.content {
            let mut column = 0;
            while column < line.len() {
                let cell = line[column];

                // Restore the cell's colors
                if cell.foreground != foreground {
                    text.push_str(&cell.foreground.escape(true));
                    foreground = cell.foreground;
                }
                if cell.background != background {
                    text.push_str(&cell.background.escape(false));
                    background = cell.background;
                }

                // Render empty cells as whitespace.
                let width = cell.c.width();
                match width {
                    Some(1) | Some(2) => text.push(cell.c),
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

/// Content of a cell in the grid.
#[derive(Copy, Clone, Default, PartialEq, Eq)]
struct Cell {
    c: char,
    foreground: Color,
    background: Color,
}

impl Cell {
    fn new(c: char, foreground: Color, background: Color) -> Self {
        Self { c, foreground, background }
    }

    /// Reset the cell to the default content.
    fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Drawing brush.
struct Brush {
    template: Vec<Vec<bool>>,
    foreground_color: Color,
    background_color: Color,
    position: Point,
    glyph: char,
    size: u8,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            foreground_color: Color::default(),
            background_color: Color::default(),
            template: Self::create_template(1),
            position: Default::default(),
            glyph: '+',
            size: 1,
        }
    }
}

impl Brush {
    /// Update the brushe's colors.
    fn set_color(&mut self, position: ColorPosition, color: Color) {
        match position {
            ColorPosition::Foreground => self.foreground_color = color,
            ColorPosition::Background => self.background_color = color,
        }
    }

    /// Create a new brush template.
    ///
    /// The brush will always be hexagon shaped, the resulting template is a matrix that stores
    /// `true` for every cell that contains a brush glyph and `false` for all empty cells.
    ///
    /// A brush with size 6 might look like this (`+`: `true`, `-`: `false`):
    ///
    /// ```
    /// --++++++--
    /// -++++++++-
    /// ++++++++++
    /// -++++++++-
    /// --++++++--
    /// ```
    fn create_template(size: u8) -> Vec<Vec<bool>> {
        // Special case the default 1x1 cursor.
        if size == 1 {
            return vec![vec![true]];
        }

        let size = size as usize;

        let width = size + (size / 2 - 1) * 2;
        let height = size - 1;

        // Initialize an empty cursor.
        let mut cursor = vec![vec![false; width]; height];

        let mid_point = (size - 1) as f32 / 2.;
        let mut num_occupied = size;
        for line in 0..height {
            // Set all occupied bits in the current line.
            for column in 0..num_occupied {
                let column = (width - num_occupied) / 2 + column;
                cursor[line][column] = true;
            }

            // Increment/Decrement based on current line in hexagon.
            if line as f32 + 1. < mid_point {
                num_occupied += 2;
            } else if line as f32 + 1. > mid_point {
                num_occupied -= 2;
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
    BrushCharacterDialog(BrushCharacterDialog),
    /// Colorpicker dialog.
    ColorpickerDialog(ColorpickerDialog),
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
        Self { column: 1, line: 1 }
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
            vec![true, true],
        ]);

        let cursor = Brush::create_template(3);
        assert_eq!(cursor, vec![
            vec![true, true, true],
            vec![true, true, true],
        ]);

        let cursor = Brush::create_template(6);
        assert_eq!(cursor, vec![
            vec![false, false, true, true, true, true, true, true, false, false],
            vec![false, true,  true, true, true, true, true, true, true,  false],
            vec![true,  true,  true, true, true, true, true, true, true,  true ],
            vec![false, true,  true, true, true, true, true, true, true,  false],
            vec![false, false, true, true, true, true, true, true, false, false],
        ]);
    }
}
