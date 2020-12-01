use vte::{Params, Perform};

use crate::terminal::event::MouseEvent;
use crate::terminal::Terminal;

impl Perform for Terminal {
    fn print(&mut self, c: char) {
        self.event_handler.keyboard_input(c);
    }

    fn execute(&mut self, byte: u8) {
        // Handle ^D.
        if byte == 4 {
            self.terminated = true;
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        match (action, intermediates) {
            // Handle mouse events.
            ('M', [b'<']) | ('m', [b'<']) => {
                let params: Vec<u16> = params.into_iter().flatten().copied().collect();
                if params.len() >= 3 {
                    let event = MouseEvent::new(params[0], params[1], params[2], action);
                    self.event_handler.mouse_input(event);
                }
            },
            _ => (),
        }
    }
}
