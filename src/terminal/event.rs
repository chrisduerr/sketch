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
    /// Since some actions can clear the terminal, like recovering from a
    /// suspension, this event is emitted whenever the application state
    /// should be rendered again.
    fn redraw(&mut self, _terminal: &mut Terminal) {}

    /// Terminal focus has changed.
    fn focus_changed(&mut self, _terminal: &mut Terminal, _focus: bool) {}

    /// Shutdown request.
    ///
    /// By default this will terminate the terminal event loop by calling
    /// [`Terminal::shutdown`].
    fn shutdown(&mut self, terminal: &mut Terminal) {
        terminal.shutdown();
    }
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
        let sgr_event = SgrEvent::from_bits_truncate(button as u8);

        let button_state = if sgr_event.contains(SgrEvent::NONE) {
            ButtonState::Up
        } else if sgr_event.contains(SgrEvent::DOWN) {
            ButtonState::Down
        } else if action == 'm' {
            ButtonState::Released
        } else {
            ButtonState::Pressed
        };

        let modifiers = Modifiers::from_bits_truncate(button as u8);

        let button = match button as u8 & SgrEvent::BUTTONS.bits() {
            0 => MouseButton::Left,
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            3 => MouseButton::None,
            index if sgr_event.contains(SgrEvent::EXTENDED1) => {
                MouseButton::Index(4 + (index & SgrEvent::NONE.bits()))
            },
            index if sgr_event.contains(SgrEvent::EXTENDED2) => {
                MouseButton::Index(8 + (index & SgrEvent::NONE.bits()))
            },
            _ => unreachable!(),
        };

        MouseEvent { button_state, button, modifiers, column: column as usize, line: line as usize }
    }
}

bitflags! {
    /// Bitflag information of SGR mouse events.
    pub struct SgrEvent: u8 {
        // Mouse buttons.
        const LEFT      = 0b0000_0000;
        const MIDDLE    = 0b0000_0001;
        const RIGHT     = 0b0000_0010;
        const NONE      = 0b0000_0011;

        // Modifiers.
        const SHIFT     = 0b0000_0100;
        const ALT       = 0b0000_1000;
        const CONTROL   = 0b0001_0000;

        // Button state.
        const DOWN      = 0b0010_0000;

        // Extended mouse buttons.
        const EXTENDED1 = 0b0100_0000;
        const EXTENDED2 = 0b1000_0000;
        const BUTTONS   = 0b1100_0011;
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
    /// This is emitted in an event when the mouse cursor has moved without any
    /// active button presses.
    None,
    /// Mouse buttons beyond Left/Middle/Right.
    ///
    /// Buttons 4 and 5 are used for the scrollwheel. Buttons 7-11 are used for
    /// mice with macro buttons.
    Index(u8),
}

bitflags! {
    /// Mouse event keyboard modifiers.
    #[derive(Copy, Clone, PartialEq, Eq, Debug)]
    pub struct Modifiers: u8 {
        const SHIFT   = 0b0000_0100;
        const ALT     = 0b0000_1000;
        const CONTROL = 0b0001_0000;
    }
}
