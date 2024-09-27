use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use crate::dialog::{Dialog, DialogLine};
use crate::terminal::{Color, NamedColor, Rgb, Terminal};

/// Message prompt of the colorpicker dialog.
const COLORPICKER_DIALOG_PROMPT: &str = "Pick a color: ";
/// Help text of the colorpicker dialog.
const COLORPICKER_DIALOG_HELP: &str = "[^R] RGB    [^T] CTerm    [^E] Default";

/// Dialog for selecting RGB or CTerm colors.
#[derive(PartialEq, Eq)]
pub struct ColorpickerDialog {
    color_position: ColorPosition,
    mode: ColorpickerMode,
    foreground: Color,
    background: Color,
}

impl ColorpickerDialog {
    pub fn new(color_position: ColorPosition, foreground: Color, background: Color) -> Self {
        let mode = match color_position {
            ColorPosition::Foreground => foreground.into(),
            ColorPosition::Background => background.into(),
        };

        Self { mode, color_position, foreground, background }
    }

    /// Process a keystroke.
    pub fn keyboard_input(&mut self, terminal: &mut Terminal, glyph: char) {
        match glyph {
            // Switch to RGB mode on ^R.
            '\x12' => self.mode = ColorpickerMode::Rgb(String::new()),
            // Switch to CTerm mode on ^T.
            '\x14' => self.mode = ColorpickerMode::CTerm(0),
            glyph => self.mode.keyboard_input(glyph),
        }

        // Update the dialog.
        self.render(terminal);
    }

    /// Color which is being changed.
    pub fn color_position(&self) -> ColorPosition {
        self.color_position
    }

    /// Selected color.
    pub fn color(&self) -> Color {
        self.mode.color()
    }
}

impl Dialog for ColorpickerDialog {
    fn lines(&self) -> Vec<String> {
        vec![
            format!("{}{}", COLORPICKER_DIALOG_PROMPT, self.mode),
            String::new(),
            COLORPICKER_DIALOG_HELP.to_string(),
        ]
    }

    fn box_color(&self) -> (Color, Color) {
        match self.color_position {
            ColorPosition::Foreground => (self.color(), self.background),
            ColorPosition::Background => (self.foreground, self.color()),
        }
    }

    fn cursor_position(&self, lines: &[DialogLine]) -> Option<(usize, usize)> {
        let mut line_len = lines.first().map(|line| line.width()).unwrap_or_default();

        // Move below 0 when the first digit hasn't been picked yet.
        if let ColorpickerMode::CTerm(0) = self.mode {
            line_len -= 1;
        }

        Some((line_len, 0))
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ColorPosition {
    Foreground,
    Background,
}

#[derive(PartialEq, Eq)]
enum ColorpickerMode {
    Rgb(String),
    CTerm(u8),
}

impl Default for ColorpickerMode {
    fn default() -> Self {
        Self::Rgb(String::new())
    }
}

impl ColorpickerMode {
    fn keyboard_input(&mut self, glyph: char) {
        match self {
            Self::CTerm(_) => self.cterm_input(glyph),
            Self::Rgb(_) => self.rgb_input(glyph),
        }
    }

    fn color(&self) -> Color {
        match self {
            Self::CTerm(color) => Color::Indexed(*color),
            Self::Rgb(color) => Rgb::from_str(color).map(Color::Rgb).unwrap_or_default(),
        }
    }

    fn cterm_input(&mut self, glyph: char) {
        let color = match self {
            Self::CTerm(color) => color,
            _ => return,
        };

        match glyph {
            '\x7f' => *color /= 10,
            glyph => {
                if let Ok(num) = u8::from_str(&glyph.to_string()) {
                    *color = color.saturating_mul(10).saturating_add(num);
                }
            },
        }
    }

    fn rgb_input(&mut self, glyph: char) {
        let color = match self {
            Self::Rgb(color) => color,
            _ => return,
        };

        match glyph {
            '\x7f' => color.truncate(color.len().saturating_sub(1)),
            glyph if color.len() < 6 => {
                if u8::from_str_radix(&glyph.to_string(), 16).is_ok() {
                    color.push(glyph);
                }
            },
            _ => (),
        }
    }
}

impl Display for ColorpickerMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rgb(color) => write!(f, "#{}", color),
            Self::CTerm(color) => write!(f, "{}", color),
        }
    }
}

impl From<Color> for ColorpickerMode {
    fn from(color: Color) -> Self {
        match color {
            Color::Named(NamedColor::Default) => Self::default(),
            Color::Named(color) => Self::CTerm(color as u8),
            Color::Indexed(index) => Self::CTerm(index),
            Color::Rgb(Rgb { r, g, b }) => Self::Rgb(format!("{:02x}{:02x}{:02x}", r, g, b)),
        }
    }
}
