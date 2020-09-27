#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Event {
    MouseEvent(MouseEvent),
    KeyboardEvent(char),
}

impl From<MouseEvent> for Event {
    fn from(mouse_event: MouseEvent) -> Event {
        Event::MouseEvent(mouse_event)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    pub button_state: ButtonState,
    pub button: MouseButton,
    pub column: usize,
    pub line: usize,
}

impl MouseEvent {
    pub fn new(params: &[i64], action: char) -> Option<Self> {
        if params.len() < 3 {
            return None;
        }

        let button_state = if params[0] == 35 {
            ButtonState::Up
        } else if params[0] & 32 == 32 {
            ButtonState::Down
        } else if action == 'M' {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        };

        // TODO: This drops modifiers and all buttons beyond RMB
        let button = match params[0] & 0b11 {
            0 => MouseButton::Left,
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            3 => MouseButton::None,
            _ => unreachable!(),
        };

        Some(MouseEvent { button_state, button, column: params[1] as usize, line: params[2] as usize })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ButtonState {
    Pressed,
    Down,
    Released,
    Up,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    None,
    Index(usize),
}
