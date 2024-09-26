mod cli;
mod dialog;
mod terminal;

use std::cmp::{max, min};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt::{self, Display, Formatter};
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::{fs, io, mem};

use clap::Parser;
use unicode_width::UnicodeWidthChar;

use crate::cli::Options;
use crate::dialog::brush_character::BrushCharacterDialog;
use crate::dialog::colorpicker::{ColorPosition, ColorpickerDialog};
use crate::dialog::help::HelpDialog;
use crate::dialog::save::SaveDialog;
use crate::dialog::Dialog;
use crate::terminal::event::{ButtonState, EventHandler, Modifiers, MouseButton, MouseEvent};
use crate::terminal::{Color, CursorShape, Dimensions, Terminal, TerminalMode, TextStyle};

/// Help dialog binding information.
const HELP: &str = "[CTRL + ?] Help";

fn main() -> io::Result<()> {
    // Launch the application.
    Sketch::new().run()
}

/// Sketch application state.
struct Sketch {
    /// Content of the terminal grid.
    content: Grid,

    /// CLI config.
    options: Options,

    /// Current application mode.
    mode: SketchMode,

    /// Mouse cursor brush used for drawing.
    brush: Brush,

    /// Text cursor position.
    text_cursor: Option<Point>,

    /// Current change revision for undo/redo tracking.
    revision: usize,

    /// Highest revision available for redo.
    max_revision: usize,

    /// Whether the Sketch was successfully saved to a file.
    persisted: bool,

    /// Whether there's currently text being pasted.
    pasting: bool,
}

impl Sketch {
    /// Setup the Sketch application state.
    fn new() -> Self {
        Self {
            options: Options::parse(),
            max_revision: Default::default(),
            text_cursor: Default::default(),
            persisted: Default::default(),
            revision: Default::default(),
            content: Default::default(),
            pasting: Default::default(),
            brush: Default::default(),
            mode: Default::default(),
        }
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
        terminal.set_mode(TerminalMode::FocusInOut, true);
        terminal.set_mode(TerminalMode::BracketedPaste, true);
        Terminal::goto(0, 0);

        // Resize internal buffer to fit terminal dimensions.
        let dimensions = terminal.dimensions();
        self.resize(&mut terminal, dimensions);

        // Run the terminal event loop.
        terminal.set_event_handler(Box::new(self));
        terminal.run()
    }

    /// Clear the entire screen, going back to an empty canvas.
    fn clear(&mut self, terminal: &mut Terminal) {
        // Reset storage.
        for line in self.content.iter_mut() {
            for cell in line {
                cell.clear(self.revision);
            }
        }

        // Clear terminal.
        Terminal::clear();

        // Redraw cursor template and help message.
        self.redraw(terminal);
        self.preview_brush();

        // Increment undo history.
        self.bump_revision();
    }

    /// Write character at the specified position.
    ///
    /// The `persist` flag determines if the write operation will be committed
    /// to Sketch's application state. This is used to clear things from the
    /// grid which are not part of the sketch (like the cursor preview).
    fn write(&mut self, at: Point, c: char, persist: bool) -> Point {
        self.write_many(at, c, 1, persist)
    }

    /// Write the same character multiple times.
    ///
    /// This is a version of [`write`] optimized to repeat the same character
    /// many times.
    fn write_many(&mut self, at: Point, c: char, count: usize, persist: bool) -> Point {
        if count == 0 {
            return at;
        }

        // Verify that the glyph is a printable character.
        let width = match c.width() {
            Some(width) if width > 0 => width,
            _ => return at,
        };

        let Point { column, line } = at;

        // Verify the first cell write is within the grid.
        if self.content.len() < line || self.content[line - 1].len() + 1 < column + width {
            return at;
        }

        // Store character in the grid state.
        let foreground = self.brush.foreground;
        let background = self.brush.background;
        if persist {
            let line = &mut self.content[line - 1];
            let max = min(column + (count - 1) * width, line.len());
            for column in (column..=max).step_by(width) {
                // Replace the glyph itself.
                let cell = Cell::new(c, foreground, background, self.brush.style);
                line[column - 1].replace(cell, self.revision);

                // Reset the following character when writing fullwidth characters.
                if width == 2 {
                    line[column].clear(self.revision);
                }

                // Replace previous fullwidth character if we're writing inside its spacer.
                if column >= 2 && line[column - 2].c.width() == Some(2) {
                    line[column - 2].clear(self.revision);
                }
            }
        }

        // Set the text style.
        Terminal::set_style(self.brush.style);

        // Set the correct colors for the terminal write.
        Terminal::set_color(foreground, background);

        // Write to the terminal.
        Terminal::goto(column, line);
        Terminal::write(c);

        // Use the terminal escape to repeat the character.
        if count > 1 {
            Terminal::repeat(count - 1);
        }

        Point { column: column + width * count, line }
    }

    /// Write the brush's content at its current location.
    fn write_brush(&mut self, mode: WriteMode) {
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
            let skip = usize::try_from(-origin_column + 1).unwrap_or_default();
            let first_occupied = self.brush.template[line].iter().skip(skip).position(|b| *b);

            // Skip this line if there is no occupied cell within the grid.
            let first_occupied = match first_occupied {
                Some(first_occupied) if target_line > 0 && target_line <= last_line => {
                    first_occupied + skip
                },
                _ => continue,
            };

            // Get write target start location.
            let write_location = Point {
                column: (origin_column + first_occupied as isize) as usize,
                line: target_line as usize,
            };

            // Get the last non-empty cell in the brush.
            let last_occupied = self.brush.template[line].iter().rposition(|occ| *occ).unwrap_or(0);

            // Ignore every second cell for fullwidth brushes.
            let width = self.brush.glyph.width().unwrap_or(1);
            let columns = (last_occupied + width - first_occupied) / width;

            match mode {
                WriteMode::WriteVolatile => {
                    self.write_many(write_location, self.brush.glyph, columns, false);
                },
                WriteMode::Write => {
                    self.write_many(write_location, self.brush.glyph, columns, true);
                },
                WriteMode::Erase => {
                    // Overwrite characters with default background set.
                    let background = mem::take(&mut self.brush.background);
                    self.write_many(write_location, ' ', columns * width, true);
                    self.brush.background = background;
                },
            }
        }

        // Increment undo history.
        if mode != WriteMode::WriteVolatile {
            self.bump_revision();
        }
    }

    // Preview the brush using dim colors.
    fn preview_brush(&mut self) {
        Terminal::set_dim();
        self.write_brush(WriteMode::WriteVolatile);
        Terminal::reset_sgr();
    }

    /// Write a box.
    fn write_box(&mut self, mut start: Point, mut end: Point, mode: WriteMode) {
        // Erasing line drawing mode does not exist.
        if mode == WriteMode::Erase {
            return;
        }
        let persistent = mode == WriteMode::Write;

        // Ensure start is always at the top left corner of the box.
        if start.column > end.column {
            mem::swap(&mut start.column, &mut end.column);
        }
        if start.line > end.line {
            mem::swap(&mut start.line, &mut end.line);
        }

        // Write box drawing characters for first and last line.
        if start.column == end.column && start.line == end.line {
            // Single cell box.
            self.write(start, '┼', persistent);
        } else if start.column == end.column {
            // Vertical line.
            self.write(start, '┬', persistent);
            let point = Point { column: start.column, line: end.line };
            self.write(point, '┴', persistent);
        } else if start.line == end.line {
            // Horizontal line.
            let mut point = self.write(start, '├', persistent);
            if end.column - start.column > 1 {
                point = self.write_many(point, '─', end.column - start.column - 1, persistent);
            }
            self.write(point, '┤', persistent);
        } else {
            // Full box.
            let mut point = self.write(start, '┌', persistent);
            point = self.write_many(point, '─', end.column - start.column - 1, persistent);
            self.write(point, '┐', persistent);

            let mut point = Point { column: start.column, line: end.line };
            point = self.write(point, '└', persistent);
            point = self.write_many(point, '─', end.column - start.column - 1, persistent);
            self.write(point, '┘', persistent);
        };

        // Draw the sides of the box.
        for line in (start.line..end.line).skip(1) {
            // Write left border.
            let point = Point { column: start.column, line };
            self.write(point, '│', persistent);

            // Write right border.
            if end.column != start.column {
                let point = Point { column: end.column, line };
                self.write(point, '│', persistent);
            }
        }

        // Increment undo history.
        if mode != WriteMode::WriteVolatile {
            self.bump_revision();
        }
    }

    /// Preview the box using dim colors.
    fn preview_box(&mut self, start: Point, end: Point) {
        Terminal::set_dim();
        self.write_box(start, end, WriteMode::WriteVolatile);
        Terminal::reset_sgr();
    }

    /// Write a one-dimensional line.
    fn write_line(&mut self, start: Point, end: Point, mode: WriteMode) {
        // Erasing line drawing mode does not exist.
        if mode == WriteMode::Erase {
            return;
        }
        let persistent = mode == WriteMode::Write;

        // Check the brush travel in X and Y direction.
        let min_column = min(start.column, end.column);
        let column_delta = max(start.column, end.column) - min_column;
        let min_line = min(start.line, end.line);
        let max_line = max(start.line, end.line);
        let line_delta = max_line - min_line;

        // Write the line.
        if column_delta >= line_delta * 2 {
            let count = (column_delta + 1) / self.brush.glyph.width().unwrap_or(1);
            let point = Point { column: min_column, line: start.line };
            self.write_many(point, self.brush.glyph, count, persistent);
        } else {
            for line in min_line..=max_line {
                let point = Point { column: start.column, line };
                self.write(point, self.brush.glyph, persistent);
            }
        }

        // Increment undo history.
        if mode != WriteMode::WriteVolatile {
            self.bump_revision();
        }
    }

    /// Preview the line using dim colors.
    fn preview_line(&mut self, start: Point, end: Point) {
        Terminal::set_dim();
        self.write_line(start, end, WriteMode::WriteVolatile);
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
    fn backspace(&mut self, terminal: &mut Terminal) {
        // Ignore backspace in the first column.
        let text_cursor = self.text_cursor.get_or_insert(self.brush.position);
        if text_cursor.column <= 1 {
            return;
        }

        // Move cursor to the previous cell.
        text_cursor.column -= 1;

        // Overwrite cell with whitespace.
        let point = *text_cursor;
        self.write(point, ' ', true);

        // Move terminal cursor to new location.
        Terminal::goto(point.column, point.line);

        // Ensure IBeam cursor is visible.
        terminal.set_mode(TerminalMode::ShowCursor, true);
        Terminal::set_cursor_shape(CursorShape::IBeam);

        self.bump_revision();
    }

    /// Open the dialog for color selection.
    fn open_color_dialog(&mut self, terminal: &mut Terminal, color_position: ColorPosition) {
        let dialog =
            ColorpickerDialog::new(color_position, self.brush.foreground, self.brush.background);
        dialog.render(terminal);

        self.mode = SketchMode::ColorpickerDialog(dialog);
    }

    /// Open the dialog for brush character selection.
    fn open_brush_character_dialog(&mut self, terminal: &mut Terminal) {
        let dialog = BrushCharacterDialog::new(self.brush.glyph);
        dialog.render(terminal);

        self.mode = SketchMode::BrushCharacterDialog(dialog);
    }

    /// Open the dialog for picking the save path.
    fn open_save_dialog(&mut self, terminal: &mut Terminal, error: bool) {
        let path = match &self.options.output {
            Some(path) => path.to_string_lossy().into(),
            None => String::new(),
        };
        self.mode = SketchMode::SaveDialog(SaveDialog::new(path, error));

        // Redraw the entire terminal to clear previous dialogs.
        self.redraw(terminal);
    }

    /// Open the dialog for showing keybarding and usage information.
    fn open_help_dialog(&mut self, terminal: &mut Terminal) {
        let dialog = HelpDialog::new();
        dialog.render(terminal);

        self.mode = SketchMode::HelpDialog(dialog);
    }

    /// Render the help dialog message.
    fn render_help(&mut self) {
        // Skip drawing if the first line has any content in it.
        if !self.content[0].iter().all(Cell::is_empty) {
            return;
        }

        // Write the help message into the last line.
        Terminal::reset_sgr();
        Terminal::goto(0, 0);
        Terminal::write(HELP);
    }

    /// Set the grid's revision to a certain point in history.
    fn set_revision(&mut self, terminal: &mut Terminal, revision: usize) {
        // Only allow increasing to revisions that actually exist.
        if revision > self.max_revision {
            return;
        }

        // Set grid state revision.
        for line in self.content.iter_mut() {
            for cell in line {
                cell.set_revision(self.revision, revision);
            }
        }
        self.revision = revision;

        // Render changes.
        self.redraw(terminal);
    }

    /// Increment the current revision.
    fn bump_revision(&mut self) {
        // Ignore revision changes during bracketed paste.
        if self.pasting {
            return;
        }

        // Clear redo history.
        self.clear_history(self.revision);

        // Bump the current revision.
        self.revision += 1;
        self.max_revision = self.revision;
    }

    /// Drop all revisions after `revision`.
    fn clear_history(&mut self, revision: usize) {
        // Remove redo history from all cells.
        for line in self.content.iter_mut() {
            for cell in line {
                cell.clear_history(revision);
            }
        }

        // Limit redo history to new revision.
        self.max_revision = revision;
    }

    /// Toggle through text styles.
    fn toggle_text_style(&mut self) {
        // Switch to the next style.
        let new_bits = (self.brush.style.bits() + 1) % (TextStyle::all().bits() + 1);
        self.brush.style = TextStyle::from_bits(new_bits).unwrap();

        // Print a helpful little message.
        Terminal::reset_sgr();
        Terminal::goto(0, usize::MAX);
        Terminal::write(format!("Changed text style to \x1b[32m{}", self.brush.style.name()));
    }
}

impl EventHandler for Sketch {
    fn keyboard_input(&mut self, terminal: &mut Terminal, glyph: char) {
        // Hide mouse brush while typing.
        self.redraw(terminal);

        match &mut self.mode {
            // Allow closing dialogs with Escape.
            SketchMode::BrushCharacterDialog(_)
            | SketchMode::ColorpickerDialog(_)
            | SketchMode::SaveDialog(_)
            | SketchMode::HelpDialog(_)
                if glyph == '\x1b' =>
            {
                self.close_dialog(terminal);
            },
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
                    self.brush.set_color(dialog.color_position(), Color::default());
                    self.close_dialog(terminal);
                },
                '\n' => {
                    self.brush.set_color(dialog.color_position(), dialog.color());
                    self.close_dialog(terminal);
                },
                glyph => dialog.keyboard_input(terminal, glyph),
            },
            SketchMode::SaveDialog(dialog) => match glyph {
                '\n' => {
                    // Check if a path was submitted.
                    let path = match dialog.path() {
                        Some(path) => path,
                        None => {
                            terminal.shutdown();
                            return;
                        },
                    };

                    // Attempt to persist the path.
                    match self.content.persist(&path) {
                        Ok(()) => {
                            self.persisted = true;
                            terminal.shutdown();
                        },
                        Err(_) => dialog.mark_failed(terminal),
                    }
                },
                glyph => {
                    let redraw_required = dialog.keyboard_input(terminal, glyph);
                    if redraw_required {
                        self.redraw(terminal);
                    }
                },
            },
            SketchMode::HelpDialog(_) if glyph == '\n' => self.close_dialog(terminal),
            // Cancel box/line drawing on escape.
            SketchMode::LineDrawing(..) if glyph == '\x1b' => self.mode = SketchMode::Sketching,
            _ => match glyph {
                // Open background colorpicker dialog on ^B.
                '\x02' => self.open_color_dialog(terminal, ColorPosition::Background),
                // Open foreground colorpicker dialog on ^F.
                '\x06' => self.open_color_dialog(terminal, ColorPosition::Foreground),
                // Toggle through text styles on ^S.
                '\x13' => self.toggle_text_style(),
                // Open brush character dialog on ^T.
                '\x14' => self.open_brush_character_dialog(terminal),
                // Open help dialog on ^?.
                '\x1f' => self.open_help_dialog(terminal),
                // Delete last character on backspace.
                '\x7f' => self.backspace(terminal),
                // Clear the screen.
                '\x0c' => self.clear(terminal),
                // Undo last action.
                '\x15' => self.set_revision(terminal, self.revision.saturating_sub(1)),
                // Redo last undone action.
                '\x12' => self.set_revision(terminal, self.revision + 1),
                // Go to the next line.
                '\n' => {
                    // Ignore enter without previous text input.
                    let text_cursor = match &mut self.text_cursor {
                        Some(text_cursor) => text_cursor,
                        None => return,
                    };

                    // Move text cursor to next line.
                    text_cursor.column = self.brush.position.column;
                    text_cursor.line += 1;
                    Terminal::goto(text_cursor.column, text_cursor.line);
                },
                // Write the character to the screen.
                glyph if glyph.width().unwrap_or_default() > 0 => {
                    // Show IBeam cursor while typing.
                    terminal.set_mode(TerminalMode::ShowCursor, true);
                    Terminal::set_cursor_shape(CursorShape::IBeam);

                    // Write character at text cursor location.
                    let text_cursor = *self.text_cursor.get_or_insert(self.brush.position);
                    self.text_cursor = Some(self.write(text_cursor, glyph, true));
                    self.bump_revision();
                },
                _ => (),
            },
        }
    }

    fn mouse_input(&mut self, terminal: &mut Terminal, event: MouseEvent) {
        // Always keep track of cursor on position change.
        self.brush.position = Point { column: event.column, line: event.line };
        self.text_cursor = None;

        // Ignore mouse events while dialogs are open.
        if let SketchMode::SaveDialog(_)
        | SketchMode::HelpDialog(_)
        | SketchMode::BrushCharacterDialog(_)
        | SketchMode::ColorpickerDialog(_) = self.mode
        {
            return;
        }

        // Hide terminal cursor while using the mouse.
        terminal.set_mode(TerminalMode::ShowCursor, false);

        self.redraw(terminal);

        match (event, &self.mode) {
            // Start line drawing mode.
            (
                MouseEvent {
                    button: MouseButton::Left,
                    button_state: ButtonState::Pressed,
                    modifiers: Modifiers::CONTROL,
                    ..
                },
                SketchMode::Sketching,
            ) => {
                let point = Point { column: event.column, line: event.line };
                self.mode = SketchMode::LineDrawing(point, false);
            },
            // Preview the line drawing box.
            (
                MouseEvent { button_state: ButtonState::Up, .. },
                SketchMode::LineDrawing(start_point, false),
            ) => {
                let end_point = Point { column: event.column, line: event.line };
                let start_point = *start_point;
                self.preview_box(start_point, end_point);
            },
            // Draw the box once line drawing mode is finished.
            (
                MouseEvent {
                    button: MouseButton::Left, button_state: ButtonState::Pressed, ..
                },
                SketchMode::LineDrawing(start_point, false),
            ) => {
                let end_point = Point { column: event.column, line: event.line };
                let start_point = *start_point;
                self.write_box(start_point, end_point, WriteMode::Write);
                self.mode = SketchMode::Sketching;
            },
            // Preview the line drawing line.
            (
                MouseEvent { button: MouseButton::Left, button_state: ButtonState::Down, .. },
                SketchMode::LineDrawing(start_point, _),
            ) => {
                // Preview the line.
                let end_point = Point { column: event.column, line: event.line };
                let start_point = *start_point;
                self.preview_line(start_point, end_point);

                // Prevent box drawing since the cursor has moved.
                self.mode = SketchMode::LineDrawing(start_point, true);
            },
            // Stop line drawing once the mouse was released after moving.
            (
                MouseEvent {
                    button: MouseButton::Left, button_state: ButtonState::Released, ..
                },
                SketchMode::LineDrawing(start_point, true),
            ) => {
                let end_point = Point { column: event.column, line: event.line };
                let start_point = *start_point;
                self.write_line(start_point, end_point, WriteMode::Write);
                self.mode = SketchMode::Sketching;
            },
            // Write brush with left mouse button pressed.
            (MouseEvent { button: MouseButton::Left, button_state, .. }, SketchMode::Sketching)
                if button_state == ButtonState::Down || button_state == ButtonState::Pressed =>
            {
                self.write_brush(WriteMode::Write)
            },
            // Erase brush with right mouse button pressed.
            (
                MouseEvent { button: MouseButton::Right, button_state, .. },
                SketchMode::Sketching,
            ) if button_state == ButtonState::Down || button_state == ButtonState::Pressed => {
                self.write_brush(WriteMode::Erase)
            },
            // Increase brush size.
            (MouseEvent { button: MouseButton::Index(4), .. }, SketchMode::Sketching) => {
                self.brush.size = self.brush.size.saturating_add(1);
                self.brush.template = Brush::create_template(self.brush.size);
            },
            // Decrease brush size.
            (MouseEvent { button: MouseButton::Index(5), .. }, SketchMode::Sketching) => {
                self.brush.size = max(1, self.brush.size - 1);
                self.brush.template = Brush::create_template(self.brush.size);
            },
            _ => (),
        }

        // Preview cursor only while sketching.
        if self.mode == SketchMode::Sketching {
            // Draw brush at size 1 for line drawing preview.
            if event.modifiers.contains(Modifiers::CONTROL) && event.button != MouseButton::Right {
                let original_size = mem::replace(&mut self.brush.size, 1);
                self.brush.template = Brush::create_template(self.brush.size);

                self.preview_brush();

                self.brush.size = original_size;
                self.brush.template = Brush::create_template(self.brush.size);
            } else {
                self.preview_brush();
            }
        }
    }

    /// Resize the internal terminal state.
    ///
    /// This will discard all content that was written outside the terminal
    /// dimensions with no way to recover it.
    fn resize(&mut self, terminal: &mut Terminal, dimensions: Dimensions) {
        let Dimensions { columns, lines } = dimensions;
        let (columns, lines) = (columns as usize, lines as usize);

        // Add/remove lines.
        self.content.resize(lines, vec![Cell::default(); columns]);

        // Resize columns of each line.
        for line in self.content.iter_mut() {
            line.resize(columns, Cell::default());
        }

        // Force redraw to make sure user is up to date.
        self.redraw(terminal);
    }

    /// Redraw the entire UI.
    fn redraw(&mut self, terminal: &mut Terminal) {
        // Re-print the entire stored buffer.
        Terminal::goto(1, 1);
        Terminal::write(self.content.to_string());

        self.render_help();

        // Restore text cursor.
        if let Some(text_cursor) = self.text_cursor {
            Terminal::goto(text_cursor.column, text_cursor.line);
        }

        // Redraw dialogs.
        match &mut self.mode {
            SketchMode::BrushCharacterDialog(dialog) => dialog.render(terminal),
            SketchMode::ColorpickerDialog(dialog) => dialog.render(terminal),
            SketchMode::SaveDialog(dialog) => dialog.render(terminal),
            SketchMode::HelpDialog(dialog) => dialog.render(terminal),
            _ => (),
        }
    }

    fn focus_changed(&mut self, terminal: &mut Terminal, focus: bool) {
        // Hide mouse brush while unfocused.
        if !focus {
            self.redraw(terminal);
        }
    }

    fn shutdown(&mut self, terminal: &mut Terminal) {
        // If another dialog is open, close it.
        match self.mode {
            SketchMode::BrushCharacterDialog(_)
            | SketchMode::ColorpickerDialog(_)
            | SketchMode::HelpDialog(_) => self.close_dialog(terminal),
            _ => (),
        }

        match &self.options.output {
            Some(path) => match self.content.persist(path) {
                Ok(()) => {
                    self.persisted = true;
                    terminal.shutdown();
                },
                Err(_) => self.open_save_dialog(terminal, true),
            },
            None => self.open_save_dialog(terminal, false),
        }
    }

    fn set_bracketed_paste_state(&mut self, active: bool) {
        self.pasting = active;

        // Create a revision once bracketed paste is done.
        if !self.pasting {
            self.bump_revision();
        }
    }
}

impl Drop for Sketch {
    fn drop(&mut self) {
        // Write Sketch to STDOUT if it wasn't saved to a file.
        if !self.persisted {
            print!("{}", self.content.trimmed_text());
        }
    }
}

/// Sketch content grid.
#[derive(Default)]
struct Grid(Vec<Vec<Cell>>);

impl Grid {
    /// Get a trimmed version of the sketch.
    ///
    /// This will remove all empty lines from the top and bottom of the sketch.
    fn trimmed_text(&self) -> String {
        let mut text = self.to_string();

        // Find the first non-empty line.
        let start_offset = text
            .chars()
            .enumerate()
            .take_while(|&(_, c)| c.is_whitespace())
            .fold(0, |acc, (i, c)| if c == '\n' { i + 1 } else { acc });

        // Remove empty lines above or below the sketch.
        text = text[start_offset..].trim_end().to_owned();
        text.push('\n');

        text
    }

    /// Try to write the Sketch to a file.
    fn persist(&self, path: &Path) -> io::Result<()> {
        let text = self.trimmed_text();
        fs::write(path, text)
    }
}

impl Display for Grid {
    /// Render the entire grid to the formatter.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return Ok(());
        }

        let mut text = String::new();

        // Store colors/styles to reduce number of writes.
        let mut foreground = Color::default();
        let mut background = Color::default();
        Terminal::set_color(foreground, background);
        let mut style = TextStyle::empty();
        Terminal::set_style(style);

        for line in &self.0 {
            let mut column = 0;
            while column < line.len() {
                let cell = &line[column];

                // Set the cell's colors
                if cell.foreground != foreground {
                    text.push_str(&cell.foreground.escape(true));
                    foreground = cell.foreground;
                }
                if cell.background != background {
                    text.push_str(&cell.background.escape(false));
                    background = cell.background;
                }

                // Set the cell's text style.
                if cell.style != style {
                    text.push_str(cell.style.escape());
                    style = cell.style;
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

impl Deref for Grid {
    type Target = Vec<Vec<Cell>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Grid {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Content of a cell in the grid.
#[derive(Debug, Clone, Default)]
struct Cell {
    // Cell contents.
    c: char,
    foreground: Color,
    background: Color,
    style: TextStyle,

    /// Versioned cell change history.
    history: HashMap<usize, Cell>,
}

impl Cell {
    fn new(c: char, foreground: Color, background: Color, style: TextStyle) -> Self {
        Self { c, style, foreground, background, history: HashMap::new() }
    }

    /// Reset the cell to the default content.
    fn clear(&mut self, revision: usize) {
        self.replace(Self::default(), revision);
    }

    /// Replace the cell with a new cell.
    ///
    /// This should be used over replacing the cell directly, since it correctly
    /// keeps track of the cell's history for undoing changes in the future.
    fn replace(&mut self, mut cell: Self, revision: usize) {
        // Replace the current cell.
        cell.history = mem::take(&mut self.history);
        let old_cell = mem::replace(self, cell);

        // Update history if this was the first update for the revision.
        self.history.entry(revision).or_insert(old_cell);
    }

    /// Set the active revision of the cell.
    fn set_revision(&mut self, current_revision: usize, new_revision: usize) {
        // Find a change matching the revision.
        let mut cell = match self.history.remove(&new_revision) {
            Some(cell) => cell,
            None => return,
        };

        // Swap old revision with current cell.
        cell.history = mem::take(&mut self.history);
        let old_cell = mem::replace(self, cell);
        self.history.insert(current_revision, old_cell);
    }

    /// Drop all revisions after `revision`.
    fn clear_history(&mut self, revision: usize) {
        self.history.retain(|rev, _| *rev <= revision);
    }

    /// Check if cell has any visible content.
    fn is_empty(&self) -> bool {
        (self.c.is_whitespace() || self.c == '\0') && self.background == Color::default()
    }
}

/// Drawing brush.
struct Brush {
    template: Vec<Vec<bool>>,
    foreground: Color,
    background: Color,
    style: TextStyle,
    position: Point,
    glyph: char,
    size: u8,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            template: Self::create_template(1),
            glyph: '+',
            size: 1,
            foreground: Default::default(),
            background: Default::default(),
            position: Default::default(),
            style: Default::default(),
        }
    }
}

impl Brush {
    /// Update the brushe's colors.
    fn set_color(&mut self, position: ColorPosition, color: Color) {
        match position {
            ColorPosition::Foreground => self.foreground = color,
            ColorPosition::Background => self.background = color,
        }
    }

    /// Create a new brush template.
    ///
    /// The brush will always be hexagon shaped, the resulting template is a
    /// matrix that stores `true` for every cell that contains a brush glyph
    /// and `false` for all empty cells.
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
        for (i, line) in cursor.iter_mut().enumerate().take(height) {
            // Set all occupied bits in the current line.
            for column in 0..num_occupied {
                let column = (width - num_occupied) / 2 + column;
                line[column] = true;
            }

            // Increment/Decrement based on current line in hexagon.
            if i as f32 + 1. < mid_point {
                num_occupied += 2;
            } else if i as f32 + 1. > mid_point {
                num_occupied -= 2;
            }
        }

        cursor
    }
}

/// Current application state.
#[derive(Default, PartialEq, Eq)]
enum SketchMode {
    /// Default drawing mode.
    #[default]
    Sketching,
    /// Line/Box drawing mode.
    LineDrawing(Point, bool),
    /// Brush character dialog prompt.
    BrushCharacterDialog(BrushCharacterDialog),
    /// Colorpicker dialog.
    ColorpickerDialog(ColorpickerDialog),
    /// Save dialog.
    SaveDialog(SaveDialog),
    /// Help dialog.
    HelpDialog(HelpDialog),
}

/// Modes for writing text to the grid.
#[derive(Debug, PartialEq, Eq)]
enum WriteMode {
    /// Write to the terminal without storing the result.
    WriteVolatile,
    /// Write to the terminal and internal state.
    Write,
    /// Write whitespace to erase content from terminal and internal state.
    Erase,
}

/// Coordinate in the terminal grid.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
