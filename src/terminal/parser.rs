use vte::{Params, Perform};

use crate::terminal::event::MouseEvent;
use crate::terminal::Terminal;

impl Perform for Terminal {
    fn print(&mut self, c: char) {
        self.handle_event(|handler, terminal| handler.keyboard_input(terminal, c));
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            // Handle Ctrl+D.
            4 => self.handle_event(|handler, terminal| handler.shutdown(terminal)),
            b => self.handle_event(|handler, terminal| handler.keyboard_input(terminal, b as char)),
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        match (action, intermediates) {
            // Handle mouse events.
            ('M', [b'<']) | ('m', [b'<']) => {
                let params: Vec<u16> = params.into_iter().flatten().copied().collect();
                if params.len() >= 3 {
                    let event = MouseEvent::new(params[0], params[1], params[2], action);
                    self.handle_event(|handler, terminal| handler.mouse_input(terminal, event));
                }
            },
            ('I', _) => {
                self.handle_event(|handler, terminal| handler.focus_changed(terminal, true));
            },
            ('O', _) => {
                self.handle_event(|handler, terminal| handler.focus_changed(terminal, false));
            },
            ('~', _) => match params.into_iter().next() {
                Some([200]) => {
                    self.handle_event(|handler, _| handler.set_bracketed_paste_state(true))
                },
                Some([201]) => {
                    self.handle_event(|handler, _| handler.set_bracketed_paste_state(false))
                },
                _ => (),
            },
            _ => (),
        }
    }
}
