use bitflags::bitflags;

use crate::terminal::{Dimensions, Terminal};

pub trait EventHandler {
    /// Mouse cursor clicks and motion.
    fn mouse_input(&mut self, _terminal: &mut Terminal, _event: MouseEvent) {}

    /// Keyboard characters.
    fn keyboard_input(&mut self, _terminal: &mut Terminal, _glyph: char) {}

    /// Terminal columns/lines have changed.
    fn resize(&mut self, _terminal: &mut Terminal, _dimensions: Dimensions) {}

    /// Redraw request.
    ///
    /// Since some actions can clear the terminal, like recovering from a suspension, this event is
    /// emitted whenever the application state should be rendered again.
    fn redraw(&mut self, _terminal: &mut Terminal) {}
}

/// Dummy event handler implementation.
impl EventHandler for () {}

/// Mouse cursor event.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    pub button_state: ButtonState,
    pub modifiers: Modifiers,
    pub button: MouseButton,
    pub column: usize,
    pub line: usize,
}

impl MouseEvent {
    /// Create a new mouse event from CSI parameters.
    pub fn new(button: u16, column: u16, line: u16, action: char) -> Self {
        let button_state = if button == 35 {
            ButtonState::Up
        } else if button & 0b10_0000 == 0b10_0000 {
            ButtonState::Down
        } else if action == 'M' {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        };

        let modifiers = Modifiers::from_bits_truncate(button as u8);

        let button = match button & 0b11_0000_11 {
            0 => MouseButton::Left,
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            3 => MouseButton::None,
            index if index & 0b1_0000_00 == 0b1_0000_00 => {
                MouseButton::Index(4 + (index as usize & 0b11))
            },
            index if index & 0b10_0000_00 == 0b10_0000_00 => {
                MouseButton::Index(8 + (index as usize & 0b11))
            },
            _ => unreachable!(),
        };

        MouseEvent { button_state, button, modifiers, column: column as usize, line: line as usize }
    }
}

/// Mouse cursor button state.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ButtonState {
    /// Button was just pressed.
    Pressed,
    /// Cursor motion while a button is held down.
    Down,
    /// Cursor was just released.
    Released,
    /// Cursor motion without any buttons held down.
    Up,
}

/// Mouse buttons.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    /// No mouse button pressed.
    ///
    /// This is emitted in an event when the mouse cursor has moved without any active button
    /// presses.
    None,
    /// Mouse buttons beyond Left/Middle/Right.
    ///
    /// Buttons 4 and 5 are used for the scrollwheel. Buttons 7-11 are used for mice with macro
    /// buttons.
    Index(usize),
}

bitflags! {
    /// Mouse event keyboard modifiers.
    pub struct Modifiers: u8 {
        const SHIFT   = 0b001_00;
        const ALT     = 0b010_00;
        const CONTROL = 0b100_00;
    }
}
