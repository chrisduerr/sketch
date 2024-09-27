use std::iter;

use vte::{Params, ParamsIter, Perform};

use crate::terminal::{Color, NamedColor, Rgb};
use crate::{Point, Sketch, TextStyle};

/// Parser for importing existing sketches.
pub struct SketchParser<'a> {
    sketch: &'a mut Sketch,
    origin: Point,
    point: Point,
}

impl<'a> SketchParser<'a> {
    pub fn new(sketch: &'a mut Sketch, origin: Point) -> Self {
        Self { sketch, origin, point: origin }
    }
}

impl<'a> Perform for SketchParser<'a> {
    fn print(&mut self, c: char) {
        self.point = self.sketch.write(self.point, c, true);
    }

    fn execute(&mut self, byte: u8) {
        if byte == b'\n' {
            self.point.column = self.origin.column;
            self.point.line += 1;
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        has_ignored_intermediates: bool,
        action: char,
    ) {
        if has_ignored_intermediates || intermediates.len() > 2 {
            return;
        }

        if action == 'm' && intermediates.is_empty() {
            if params.is_empty() {
                self.sketch.brush.style = TextStyle::empty();
                self.sketch.brush.foreground = Color::default();
                self.sketch.brush.background = Color::default();
            } else {
                handle_sgr(self.sketch, &mut params.into_iter());
            }
        }
    }
}

/// Parse SGR modes and update the brush accordingly.
///
/// Based on Alacritty's VTE crate ansi module.
fn handle_sgr(sketch: &mut Sketch, params: &mut ParamsIter<'_>) {
    while let Some(param) = params.next() {
        match param {
            [0] => {
                sketch.brush.style = TextStyle::empty();
                sketch.brush.foreground = Color::default();
                sketch.brush.background = Color::default();
            },
            [1] => sketch.brush.style.insert(TextStyle::BOLD),
            [3] => sketch.brush.style.insert(TextStyle::ITALICS),
            [21] => sketch.brush.style.remove(TextStyle::BOLD),
            [23] => sketch.brush.style.remove(TextStyle::ITALICS),
            [30] => sketch.brush.foreground = Color::Named(NamedColor::Black),
            [31] => sketch.brush.foreground = Color::Named(NamedColor::Red),
            [32] => sketch.brush.foreground = Color::Named(NamedColor::Green),
            [33] => sketch.brush.foreground = Color::Named(NamedColor::Yellow),
            [34] => sketch.brush.foreground = Color::Named(NamedColor::Blue),
            [35] => sketch.brush.foreground = Color::Named(NamedColor::Magenta),
            [36] => sketch.brush.foreground = Color::Named(NamedColor::Cyan),
            [37] => sketch.brush.foreground = Color::Named(NamedColor::White),
            [38] => {
                let mut iter = params.map(|param| param[0]);
                if let Some(color) = parse_sgr_color(&mut iter) {
                    sketch.brush.foreground = color;
                }
            },
            [38, params @ ..] => {
                if let Some(color) = handle_colon_rgb(params) {
                    sketch.brush.foreground = color;
                }
            },
            [39] => sketch.brush.foreground = Color::Named(NamedColor::Default),
            [40] => sketch.brush.background = Color::Named(NamedColor::Black),
            [41] => sketch.brush.background = Color::Named(NamedColor::Red),
            [42] => sketch.brush.background = Color::Named(NamedColor::Green),
            [43] => sketch.brush.background = Color::Named(NamedColor::Yellow),
            [44] => sketch.brush.background = Color::Named(NamedColor::Blue),
            [45] => sketch.brush.background = Color::Named(NamedColor::Magenta),
            [46] => sketch.brush.background = Color::Named(NamedColor::Cyan),
            [47] => sketch.brush.background = Color::Named(NamedColor::White),
            [48] => {
                let mut iter = params.map(|param| param[0]);
                if let Some(color) = parse_sgr_color(&mut iter) {
                    sketch.brush.background = color;
                }
            },
            [48, params @ ..] => {
                if let Some(color) = handle_colon_rgb(params) {
                    sketch.brush.background = color;
                }
            },
            [49] => sketch.brush.background = Color::Named(NamedColor::Default),
            _ => (),
        }
    }
}

/// Handle colon separated rgb color escape sequence.
///
/// Based on Alacritty's VTE crate ansi module.
fn handle_colon_rgb(params: &[u16]) -> Option<Color> {
    let rgb_start = if params.len() > 4 { 2 } else { 1 };
    let rgb_iter = params[rgb_start..].iter().copied();
    let mut iter = iter::once(params[0]).chain(rgb_iter);

    parse_sgr_color(&mut iter)
}

/// Parse a color specifier from list of attributes.
///
/// Based on Alacritty's VTE crate ansi module.
fn parse_sgr_color(params: &mut dyn Iterator<Item = u16>) -> Option<Color> {
    match params.next() {
        Some(2) => Some(Color::Rgb(Rgb {
            r: u8::try_from(params.next()?).ok()?,
            g: u8::try_from(params.next()?).ok()?,
            b: u8::try_from(params.next()?).ok()?,
        })),
        Some(5) => Some(Color::Indexed(u8::try_from(params.next()?).ok()?)),
        _ => None,
    }
}
