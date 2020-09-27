mod terminal;

use crate::terminal::Terminal;
use crate::terminal::event::{Event, MouseButton};

fn main() {
    env_logger::init();

    let mut terminal = Terminal::new();
    terminal.run(|event| {
        match event {
            Event::MouseEvent(mouse_event) => {
                Terminal::goto(mouse_event.column, mouse_event.line);

                match mouse_event.button {
                    MouseButton::Left => Terminal::write("+"),
                    MouseButton::Right => Terminal::write(" "),
                    _ => (),
                }
            },
            Event::KeyboardEvent(c) => {
                Terminal::write(c.to_string());
            },
        }
    });
}
