use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::os::unix::io::AsRawFd;
use std::ptr;

use libc::{cfmakeraw, tcgetattr, tcsetattr, termios};
use vte::Parser;

pub mod event;
mod parser;

use crate::terminal::event::Event;

pub struct Terminal {
    original_termios: termios,
    event: Option<Event>,
    terminated: bool,
}

impl Terminal {
    pub fn new() -> Self {
        let original_termios = enable_raw_mode();

        Terminal::write("\x1b[?1049h\x1b[?1006h\x1b[?1003h");
        Terminal::goto(0, 0);

        Terminal { event: None, terminated: false, original_termios }
    }

    pub fn run(&mut self, mut event_handler: impl FnMut(Event)) {
        let mut parser = Parser::new();

        for byte in io::stdin().bytes() {
            match byte {
                Ok(byte) => parser.advance(self, byte),
                Err(_) => break,
            }

            if self.terminated {
                break;
            }

            if let Some(event) = self.event.take() {
                event_handler(event);
            }
        }
    }

    pub fn write<T: AsRef<str>>(text: T) {
        let mut stdout = io::stdout();
        let _ = stdout.write(text.as_ref().as_bytes());
        let _ = stdout.flush();
    }

    // TODO: Move to a separate module?
    pub fn goto(column: usize, line: usize) {
        Self::write(format!("\x1b[{};{}H", line, column));
    }

    fn publish<E: Into<Event>>(&mut self, event: E) {
        self.event = Some(event.into());
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        Self::write("\x1b[?1003l\x1b[?1006l\x1b[?1049l");

        disable_raw_mode(self.original_termios);
    }
}

fn enable_raw_mode() -> termios {
    unsafe {
        let mut previous_termios = MaybeUninit::uninit();
        change_terminal_attributes(|termios| {
            ptr::write(&mut previous_termios, MaybeUninit::new(*termios));
            cfmakeraw(termios);
        });
        previous_termios.assume_init()
    }
}

fn disable_raw_mode(nonraw_termios: termios) {
    change_terminal_attributes(|termios| *termios = nonraw_termios);
}

fn change_terminal_attributes(change_attributes: impl FnOnce(&mut termios)) {
    let mut termios = MaybeUninit::uninit();
    let stdout_fd = io::stdout().as_raw_fd();

    unsafe {
        tcgetattr(stdout_fd, termios.as_mut_ptr());
        let mut termios = termios.assume_init();
        change_attributes(&mut termios);
        tcsetattr(stdout_fd, 0, &termios);
    }
}
