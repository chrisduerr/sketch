use log::debug;
use vte::Perform;

use crate::terminal::event::{Event, MouseEvent};
use crate::terminal::Terminal;

impl Perform for Terminal {
    fn print(&mut self, c: char) {
        debug!("print {}", c);

        self.publish(Event::KeyboardEvent(c));
    }

    fn execute(&mut self, byte: u8) {
        debug!("execute {}", byte);

        match byte {
                3 | 4 => self.terminated = true,
            _ => (),
        }
    }

    fn hook(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, action: char) {
        debug!("hook {:?} {:?} {} {}", params, intermediates, ignore, action);
    }

    fn put(&mut self, byte: u8) {
        debug!("put {}", byte);
    }

    fn unhook(&mut self) {
        debug!("unhook");
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        debug!("osc {:?} {}", params, bell_terminated);
    }

    fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, action: char) {
        debug!("csi {:?} {:?} {} {}", params, intermediates, ignore, action);

        let event = match (action, intermediates) {
            ('M', [b'<']) | ('m', [b'<']) => MouseEvent::new(params, action),
            _ => None,
        };

        if let Some(event) = event {
            self.publish(event);
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        debug!("esc {:?} {} {}", intermediates, ignore, byte);
    }
}
