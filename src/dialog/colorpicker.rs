use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use crate::dialog::Dialog;
use crate::terminal::{Color, Rgb, Terminal};

/// Message prompt of the colorpicker dialog.
const COLORPICKER_DIALOG_PROMPT: &str = "Pick a color: ";
/// Help text of the colorpicker dialog.
const COLORPICKER_DIALOG_HELP: &str = "[^R] RGB   [^T] CTerm";

/// Dialog for selecting RGB or CTerm colors.
#[derive(PartialEq, Eq)]
pub struct ColorpickerDialog {
    color_position: ColorPosition,
    mode: ColorpickerMode,
}

impl ColorpickerDialog {
    pub fn new(color_position: ColorPosition) -> Self {
        Self { mode: ColorpickerMode::default(), color_position }
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

    pub fn color_position(&self) -> ColorPosition {
        self.color_position
    }

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
            ColorPosition::Foreground => (self.color(), Color::default()),
            ColorPosition::Background => (Color::default(), self.color()),
        }
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
            Self::Rgb(color) => Rgb::from_str(color).map(|rgb| Color::Rgb(rgb)).unwrap_or_default(),
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
            Self::Rgb(color) => write!(f, "#{: <1}", color),
            Self::CTerm(color) => write!(f, "{: >7}", color),
        }
    }
}
