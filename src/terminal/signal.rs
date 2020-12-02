use std::io;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;

use mio::{Registry, Token, Waker};

/// Maximum signal channel size.
///
/// Any signal received after this bound has been reached will be silently dropped, while `mio`
/// will still be woken up to allow processing the remaining signals.
const CHANNEL_SIZE: usize = 512;

// TODO: Using a single atomic flag for each signal would be more reliable.
//
/// Buffer storing all unprocessed signals.
static mut SIGNALS: Option<SyncSender<libc::c_int>> = None;

/// Waker to notify mio about new signals which require processing.
static mut WAKER: Option<Arc<Waker>> = None;

/// Setup a channel for mio to check for new signals.
pub fn mio_receiver(registry: &Registry, token: Token) -> io::Result<Receiver<libc::c_int>> {
    let rx = unsafe {
        let waker = Waker::new(registry, token)?;
        WAKER = Some(Arc::new(waker));

        let (tx, rx) = mpsc::sync_channel(CHANNEL_SIZE);
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
    if let Some(waker) = &WAKER {
        let _ = waker.wake();
    }

    if let Some(signals) = &SIGNALS {
        let _ = signals.try_send(signal);
    }
}
