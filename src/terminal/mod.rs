use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};
use std::os::unix::io::AsRawFd;
use std::ptr;

use libc::{self, SIGCONT, SIGHUP, SIGINT, SIGTERM, SIGTSTP, SIG_DFL};
use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};
use signal_hook_mio::v0_7::Signals;
use vte::Parser;

use crate::terminal::event::EventHandler;

pub mod event;
mod parser;

/// Mio token for reading from STDIN.
const STDIN_TOKEN: Token = Token(0);
/// Mio token for signal handling.
const SIGNAL_TOKEN: Token = Token(1);

/// Terminal emulation state.
///
/// This is used to make sure the terminal can reset itself properly after the application is
/// closed.
pub struct Terminal<'a> {
    /// Callbacks for all terminal events.
    event_handler: &'a mut dyn EventHandler,

    /// Terminal attributes for reset after we're done.
    original_termios: libc::termios,
    /// Terminal modes for reset after we're done.
    modes: TerminalModes,

    /// Shared state to allow for termination from the parser.
    terminated: bool,

    // NOTE: This is necessary since `signal-hook` does not yet have a way to remove a signal
    // handler which allows adding a handler for the same signal in the future:
    //
    // https://github.com/vorner/signal-hook/issues/30
    //
    /// SIGTSTP signal handler action.
    signal_handler: Option<libc::sighandler_t>,
}

impl<'a> Terminal<'a> {
    pub fn new(event_handler: &'a mut dyn EventHandler) -> Self {
        Terminal {
            modes: TerminalModes::default(),
            original_termios: setup_tty(),
            signal_handler: None,
            terminated: false,
            event_handler,
        }
    }

    /// Run the terminal event loop.
    ///
    /// This will block until the application is terminated. The `EventHandler` registered to this
    /// terminal will be called whenever a new event is received.
    pub fn run(&mut self) -> io::Result<()> {
        // Setup terminal escape sequence parser.
        let mut parser = Parser::new();

        // Setup mio.
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(1024);

        // Register STDIN read interest.
        let mut stdin = io::stdin();
        poll.registry().register(&mut SourceFd(&0), STDIN_TOKEN, Interest::READABLE)?;

        // Register signal handlers.
        let mut signals = Signals::new(&[SIGINT, SIGHUP, SIGTERM, SIGTSTP, SIGCONT])?;
        poll.registry().register(&mut signals, SIGNAL_TOKEN, Interest::READABLE)?;

        // Reserve buffer for reading from STDIN.
        let mut buf = [0; u16::max_value() as usize];

        'event_loop: while !self.terminated {
            // Stop if we run into a polling error we cannot handle ourselves.
            if let Err(err) = poll.poll(&mut events, None) {
                if err.kind() != io::ErrorKind::Interrupted {
                    return Err(err);
                }
            }

            for event in &events {
                match event.token() {
                    STDIN_TOKEN => {
                        // Pass STDIN to parser.
                        let read = stdin.read(&mut buf)?;
                        for byte in &buf[..read] {
                            parser.advance(self, *byte);
                        }
                    },
                    SIGNAL_TOKEN => {
                        // Handle shutdown if a signal requested it.
                        if self.handle_signals(&mut signals).is_err() {
                            break 'event_loop;
                        }
                    },
                    _ => unreachable!(),
                }
            }
        }

        Ok(())
    }

    /// Handle POSIX signals.
    ///
    /// This function will raise an [`io::ErrorKind::Interrupted`] error if the signal requested an
    /// application shutdown.
    fn handle_signals(&mut self, signals: &mut Signals) -> io::Result<()> {
        for signal in signals.pending() {
            match signal {
                SIGINT | SIGHUP | SIGTERM => return Err(io::ErrorKind::Interrupted.into()),
                SIGCONT => {
                    // Restore the terminal state.
                    self.restore_modes();
                    self.original_termios = setup_tty();

                    // Restore the SIGTSTP signal handler.
                    if let Some(sigaction) = self.signal_handler.take() {
                        unsafe {
                            libc::signal(SIGTSTP, sigaction);
                        }
                    }

                    // Request application state update.
                    self.event_handler.redraw();
                },
                SIGTSTP => {
                    // Clear terminal state.
                    self.reset();

                    // Remove SIGTSTP handler and self-request another suspension.
                    unsafe {
                        self.signal_handler = Some(libc::signal(SIGTSTP, SIG_DFL));
                        libc::raise(SIGTSTP);
                    }
                },
                _ => unreachable!(),
            }
        }

        Ok(())
    }

    /// Set a terminal mode.
    pub fn set_mode(&mut self, mode: TerminalMode, enabled: bool) {
        Self::set_mode_raw(mode, enabled);
        self.modes.insert(mode, enabled);
    }

    /// Set the color for the following characters.
    // pub fn set_color(foreground: Color, background: Color) {
    //     match foreground {
    //         Color::Named(color) => Self::write(format!("\x1b[3{}m", color as u8)),
    //         Color::Rgb(Rgb { r, g, b }) => Self::write(format!("\x1b[38:2:{}:{}:{}m", r, g, b)),
    //     }

    //     match background {
    //         Color::Named(color) => Self::write(format!("\x1b[4{}m", color as u8)),
    //         Color::Rgb(Rgb { r, g, b }) => Self::write(format!("\x1b[48:2:{}:{}:{}m", r, g, b)),
    //     }
    // }

    /// Decrease intensity for the following characters.
    pub fn set_dim() {
        Self::write("\x1b[2m");
    }

    /// Reset all text attributes (color/dim/bold/...) to the default.
    pub fn reset_sgr() {
        Self::write("\x1b[0m");
    }

    /// Write some text at the current cursor location.
    pub fn write<T: Into<String>>(text: T) {
        let mut stdout = io::stdout();
        let _ = stdout.write(text.into().as_bytes());
        let _ = stdout.flush();
    }

    /// Move the cursor to a specific point in the grid.
    ///
    /// The indexing for both column and line is 1-based.
    pub fn goto(column: usize, line: usize) {
        Self::write(format!("\x1b[{};{}H", line, column));
    }

    /// Reset all terminal modifications.
    fn reset(&self) {
        Self::reset_modes();
        reset_tty(self.original_termios);
    }

    /// Restore terminal modes from internal state.
    fn restore_modes(&mut self) {
        // Set all modes based on the last internal state.
        for (mode, value) in self.modes.iter() {
            Self::set_mode_raw(*mode, *value);
        }
    }

    /// Reset all terminal modes to the default.
    fn reset_modes() {
        for (mode, value) in TerminalModes::default().iter() {
            Self::set_mode_raw(*mode, *value);
        }
    }

    /// Set a terminal mode without any persistence.
    #[inline]
    fn set_mode_raw(mode: TerminalMode, value: bool) {
        if value {
            Self::write(format!("\x1b[?{}h", mode as u16));
        } else {
            Self::write(format!("\x1b[?{}l", mode as u16));
        }
    }
}

impl<'a> Drop for Terminal<'a> {
    fn drop(&mut self) {
        self.reset();
    }
}

/// Terminal modes.
#[derive(Copy, Clone, Hash, PartialEq, Eq)]
pub enum TerminalMode {
    LineWrap = 7,
    ShowCursor = 25,
    SgrMouse = 1006,
    MouseMotion = 1003,
    AltScreen = 1049,
}

/// Track active terminal modes.
pub struct TerminalModes(HashMap<TerminalMode, bool>);

impl Default for TerminalModes {
    fn default() -> Self {
        // Fill the modes with what should be the defaults for every terminal.
        let mut modes = HashMap::new();
        modes.insert(TerminalMode::LineWrap, true);
        modes.insert(TerminalMode::ShowCursor, true);
        modes.insert(TerminalMode::SgrMouse, false);
        modes.insert(TerminalMode::MouseMotion, false);
        modes.insert(TerminalMode::AltScreen, false);

        Self(modes)
    }
}

impl Deref for TerminalModes {
    type Target = HashMap<TerminalMode, bool>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TerminalModes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// /// Terminal color.
// #[derive(Copy, Clone)]
// pub enum Color {
//     Named(NamedColor),
//     Rgb(Rgb),
// }

// impl Default for Color {
//     fn default() -> Self {
//         Self::Named(NamedColor::Default)
//     }
// }

// /// CTerm color.
// #[derive(Copy, Clone)]
// pub enum NamedColor {
//     Black = 0,
//     Red,
//     Green,
//     Yellow,
//     Blue,
//     Magenta,
//     Cyan,
//     White,
//     Default = 9,
// }

// /// RGB color.
// #[derive(Copy, Clone)]
// pub struct Rgb {
//     r: u8,
//     g: u8,
//     b: u8,
// }

/// Enable raw terminal input handling.
fn setup_tty() -> libc::termios {
    unsafe {
        let mut previous_termios = MaybeUninit::uninit();
        change_terminal_attributes(|termios| {
            ptr::write(&mut previous_termios, MaybeUninit::new(*termios));
            termios.c_lflag &= !(libc::ECHO | libc::ICANON);
        });
        previous_termios.assume_init()
    }
}

/// Disable raw terminal input handling.
fn reset_tty(nonraw_termios: libc::termios) {
    change_terminal_attributes(|termios| *termios = nonraw_termios);
}

/// Change the tty properties.
fn change_terminal_attributes(change_attributes: impl FnOnce(&mut libc::termios)) {
    let mut termios = MaybeUninit::uninit();
    let stdout_fd = io::stdout().as_raw_fd();

    unsafe {
        libc::tcgetattr(stdout_fd, termios.as_mut_ptr());
        let mut termios = termios.assume_init();
        change_attributes(&mut termios);
        libc::tcsetattr(stdout_fd, 0, &termios);
    }
}
