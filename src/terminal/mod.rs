use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::mem::{self, MaybeUninit};
use std::ops::{Deref, DerefMut};
use std::os::unix::io::AsRawFd;
use std::ptr;
use std::str::{self, FromStr};

use libc::{self, SIGCONT, SIGHUP, SIGINT, SIGTERM, SIGTSTP, SIGWINCH};
use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};
use vte::{Parser, Perform};

use crate::terminal::event::EventHandler;

pub mod event;
mod parser;
mod signal;

/// Mio token for reading from STDIN.
const STDIN_TOKEN: Token = Token(0);
/// Mio token for signal handling.
const SIGNAL_TOKEN: Token = Token(1);

/// Terminal emulation state.
///
/// This is used to make sure the terminal can reset itself properly after the
/// application is closed.
pub struct Terminal {
    /// Terminal dimensions in columns/lines.
    pub dimensions: Dimensions,

    /// Callbacks for all terminal events.
    event_handler: Box<dyn EventHandler>,

    /// Terminal attributes for reset after we're done.
    original_termios: libc::termios,
    /// Terminal modes for reset after we're done.
    modes: TerminalModes,

    /// Shared state to allow for termination from the parser.
    terminated: bool,
}

impl Terminal {
    pub fn new() -> Self {
        Terminal {
            modes: TerminalModes::default(),
            dimensions: Self::tty_dimensions(),
            original_termios: setup_tty(),
            event_handler: Box::new(()),
            terminated: false,
        }
    }

    /// Set the handler for terminal events.
    ///
    /// It is necessary to call this before [`run`] is called to make sure that
    /// events like mouse and keyboard input can be reacted upon.
    pub fn set_event_handler(&mut self, event_handler: Box<dyn EventHandler>) {
        self.event_handler = event_handler;
    }

    /// Run the terminal event loop.
    ///
    /// This will block until the application is terminated. The `EventHandler`
    /// registered to this terminal will be called whenever a new event is
    /// received.
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
        let mut signal_receiver = signal::mio_receiver()?;
        poll.registry().register(&mut signal_receiver, SIGNAL_TOKEN, Interest::READABLE)?;
        signal::register(SIGWINCH)?;
        signal::register(SIGTSTP)?;
        signal::register(SIGCONT)?;
        signal::register(SIGTERM)?;
        signal::register(SIGINT)?;
        signal::register(SIGHUP)?;

        // Reserve buffer for reading from STDIN.
        let mut buf = [0; u16::MAX as usize];

        while !self.terminated {
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

                        if buf[..read] == [b'\x1b'] {
                            // Treat a single ESC read as a key press.
                            self.print('\x1b');
                        } else {
                            // Pass read bytes to VT parser.
                            for byte in &buf[..read] {
                                parser.advance(self, *byte);
                            }
                        }
                    },
                    SIGNAL_TOKEN => {
                        let mut signal = [0; 4];
                        while signal_receiver.read_exact(&mut signal).is_ok() {
                            let signal = unsafe { mem::transmute::<[u8; 4], libc::c_int>(signal) };
                            self.handle_signal(signal)?;
                        }
                    },
                    _ => unreachable!(),
                }
            }
        }

        Ok(())
    }

    /// Shutdown the terminal event handler.
    pub fn shutdown(&mut self) {
        self.terminated = true;
    }

    /// Handle a POSIX signal.
    ///
    /// # Errors
    ///
    /// This function will raise an [`io::ErrorKind::Interrupted`] error if the
    /// signal requested an application shutdown.
    fn handle_signal(&mut self, signal: libc::c_int) -> io::Result<()> {
        match signal {
            // Try to tear everything down nicely when the controlling terminal died.
            SIGHUP => return Err(io::ErrorKind::BrokenPipe.into()),
            // Allow application to handle SIGINT/SIGTERM shutdown requests.
            SIGINT | SIGTERM => {
                self.handle_event(|handler, terminal| handler.shutdown(terminal));
            },
            // Handle terminal resize.
            SIGWINCH => self.update_size(),
            SIGCONT => {
                // Restore the terminal state.
                self.restore_modes();
                self.original_termios = setup_tty();

                // Restore the SIGTSTP signal handler.
                signal::register(SIGTSTP)?;

                // Check for potential dimension changes.
                //
                // This is necessary since `SIGWINCH` is not sent when the application is in
                // the background.
                self.update_size();

                // Request application state update.
                self.handle_event(|handler, terminal| handler.redraw(terminal));
            },
            SIGTSTP => {
                // Clear terminal state.
                self.reset();

                // Remove SIGTSTP handler and self-request another suspension.
                signal::unregister(SIGTSTP)?;
                unsafe {
                    let result = libc::raise(SIGTSTP);
                    if result != 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
            },
            _ => unreachable!(),
        }

        Ok(())
    }

    pub fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    /// Set a terminal mode.
    pub fn set_mode(&mut self, mode: TerminalMode, enabled: bool) {
        Self::set_mode_raw(mode, enabled);
        self.modes.insert(mode, enabled);
    }

    /// Set the color for all following characters.
    pub fn set_color(foreground: Color, background: Color) {
        Self::write(foreground.escape(true));
        Self::write(background.escape(false));
    }

    /// Clear the terminal screen.
    pub fn clear() {
        Self::write("\x1b[2J");
    }

    /// Decrease intensity for the following characters.
    pub fn set_dim() {
        Self::write("\x1b[2m");
    }

    /// Reset all text attributes (color/dim/bold/...) to the default.
    pub fn reset_sgr() {
        Self::write("\x1b[0m");
    }

    /// Set the terminal cursor shape.
    pub fn set_cursor_shape(cursor_shape: CursorShape) {
        Self::write(format!("\x1b[{} q", cursor_shape as u8));
    }

    /// Write some text at the current cursor location.
    pub fn write<T: Into<String>>(text: T) {
        let mut stdout = io::stdout();
        let _ = stdout.write(text.into().as_bytes());
        let _ = stdout.flush();
    }

    /// Repeat the last character `count` times.
    pub fn repeat(count: usize) {
        Self::write(format!("\x1b[{}b", count));
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
        Self::set_cursor_shape(CursorShape::Default);
        reset_tty(self.original_termios);
    }

    /// Restore terminal modes from internal state.
    fn restore_modes(&mut self) {
        // Set all modes based on the last internal state.
        for (mode, value) in self.modes.iter() {
            Self::set_mode_raw(*mode, *value);
        }
    }

    /// Check if the terminal dimensions have changed.
    fn update_size(&mut self) {
        // Skip resize that do not change columns/lines.
        let dimensions = Self::tty_dimensions();
        if dimensions != self.dimensions {
            self.dimensions = dimensions;

            // Notify event handler about the change.
            self.handle_event(|handler, terminal| handler.resize(terminal, dimensions));
        }
    }

    /// Dispatch an event with a reference to the terminal attached.
    fn handle_event<F: FnMut(&mut dyn EventHandler, &mut Terminal)>(&mut self, mut f: F) {
        let mut event_handler = mem::replace(&mut self.event_handler, Box::new(()));
        f(event_handler.as_mut(), self);
        self.event_handler = event_handler;
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

    /// Query terminal dimensions in columns and lines.
    fn tty_dimensions() -> Dimensions {
        unsafe {
            let mut winsize = MaybeUninit::<libc::winsize>::uninit();
            let result = libc::ioctl(0, libc::TIOCGWINSZ, winsize.as_mut_ptr());
            if result != -1 {
                return winsize.assume_init().into();
            }
        }

        Dimensions::default()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.reset();
    }
}

/// Terminal cursor shape.
pub enum CursorShape {
    Default = 0,
    // Block = 2,
    Underline = 4,
    IBeam = 6,
}

#[derive(Default, Copy, Clone, PartialEq, Eq)]
pub struct Dimensions {
    pub columns: u16,
    pub lines: u16,
}

impl From<libc::winsize> for Dimensions {
    fn from(winsize: libc::winsize) -> Self {
        Self { columns: winsize.ws_col, lines: winsize.ws_row }
    }
}

/// Terminal modes.
#[derive(Copy, Clone, Hash, PartialEq, Eq)]
pub enum TerminalMode {
    LineWrap = 7,
    ShowCursor = 25,
    SgrMouse = 1006,
    MouseMotion = 1003,
    FocusInOut = 1004,
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
        modes.insert(TerminalMode::FocusInOut, false);
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

/// Terminal color.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Color {
    Named(NamedColor),
    Indexed(u8),
    Rgb(Rgb),
}

impl Default for Color {
    fn default() -> Self {
        Self::Named(NamedColor::Default)
    }
}

impl Color {
    pub fn escape(&self, foreground: bool) -> String {
        match (self, foreground) {
            // Foreground:
            (Color::Named(color), true) => format!("\x1b[3{}m", *color as u8),
            (Color::Indexed(color), true) => format!("\x1b[38:5:{}m", color),
            (Color::Rgb(Rgb { r, g, b }), true) => format!("\x1b[38:2:{}:{}:{}m", r, g, b),
            // Background:
            (Color::Named(color), false) => format!("\x1b[4{}m", *color as u8),
            (Color::Indexed(color), false) => format!("\x1b[48:5:{}m", color),
            (Color::Rgb(Rgb { r, g, b }), false) => format!("\x1b[48:2:{}:{}:{}m", r, g, b),
        }
    }
}

/// CTerm color.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum NamedColor {
    // Black = 0,
    // Red = 1,
    // Green = 2,
    // Yellow = 3,
    // Blue = 4,
    // Magenta = 5,
    // Cyan = 6,
    // White = 7,
    Default = 9,
}

/// RGB color.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl FromStr for Rgb {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 6 {
            return Err(());
        }

        Ok(Rgb {
            r: u8::from_str_radix(&s[0..2], 16).map_err(|_| ())?,
            g: u8::from_str_radix(&s[2..4], 16).map_err(|_| ())?,
            b: u8::from_str_radix(&s[4..6], 16).map_err(|_| ())?,
        })
    }
}

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
