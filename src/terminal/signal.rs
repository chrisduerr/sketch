use std::io::{self, Write};
use std::ptr::addr_of_mut;

use mio::unix::pipe::{self, Receiver, Sender};

/// Pipe for sending signals from the handler to mio.
static mut SIGNALS: Option<Sender> = None;

/// Setup a channel for mio to check for new signals.
pub fn mio_receiver() -> io::Result<Receiver> {
    let rx = unsafe {
        let (tx, rx) = pipe::new()?;
        SIGNALS = Some(tx);
        rx
    };

    Ok(rx)
}

/// Add a new signal to the signal handler.
pub fn register(signal: libc::c_int) -> io::Result<()> {
    unsafe {
        let result = libc::signal(signal, handler as libc::sighandler_t);
        if result == libc::SIG_ERR {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(())
}

/// Remove a signal handler, falling back to `SIG_DFL`.
pub fn unregister(signal: libc::c_int) -> io::Result<()> {
    unsafe {
        let result = libc::signal(signal, libc::SIG_DFL);
        if result == libc::SIG_ERR {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(())
}

/// POSIX signal handler for [`libc::signal`].
unsafe extern "C" fn handler(signal: libc::c_int) {
    if let Some(Some(signals)) = addr_of_mut!(SIGNALS).as_mut() {
        let _ = signals.write_all(&signal.to_ne_bytes());
    }
}
