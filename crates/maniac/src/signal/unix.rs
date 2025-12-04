//! Unix-specific signal handling.
//!
//! This module provides Unix-specific signal types and registration.
//!
//! # Example
//!
//! ```no_run
//! use maniac::signal::unix::{signal, SignalKind};
//!
//! #[maniac::main]
//! async fn main() -> std::io::Result<()> {
//!     let mut sighup = signal(SignalKind::hangup())?;
//!     sighup.recv().await;
//!     println!("Received SIGHUP");
//!     Ok(())
//! }
//! ```

use std::io;
use std::mem::MaybeUninit;
use std::sync::Once;

use super::Signal;
use super::registry::registry;

/// Unix-specific signal kind.
///
/// This provides access to all Unix signals, not just the cross-platform ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SignalKind(i32);

impl SignalKind {
    /// Creates a `SignalKind` from a raw signal number.
    pub const fn from_raw(signum: i32) -> Self {
        SignalKind(signum)
    }

    /// Returns the raw signal number.
    pub const fn as_raw(self) -> i32 {
        self.0
    }

    /// Represents the SIGALRM signal.
    pub const fn alarm() -> Self {
        SignalKind(libc::SIGALRM)
    }

    /// Represents the SIGCHLD signal.
    ///
    /// Sent when a child process terminates, stops, or continues.
    pub const fn child() -> Self {
        SignalKind(libc::SIGCHLD)
    }

    /// Represents the SIGHUP signal.
    ///
    /// Sent when the controlling terminal is closed.
    pub const fn hangup() -> Self {
        SignalKind(libc::SIGHUP)
    }

    /// Represents the SIGINT signal.
    ///
    /// Sent when the user presses Ctrl+C.
    pub const fn interrupt() -> Self {
        SignalKind(libc::SIGINT)
    }

    /// Represents the SIGIO signal.
    #[cfg(not(target_os = "haiku"))]
    pub const fn io() -> Self {
        SignalKind(libc::SIGIO)
    }

    /// Represents the SIGPIPE signal.
    ///
    /// Sent when writing to a pipe with no readers.
    pub const fn pipe() -> Self {
        SignalKind(libc::SIGPIPE)
    }

    /// Represents the SIGQUIT signal.
    ///
    /// Sent when the user presses Ctrl+\.
    pub const fn quit() -> Self {
        SignalKind(libc::SIGQUIT)
    }

    /// Represents the SIGTERM signal.
    ///
    /// The default signal sent by the `kill` command.
    pub const fn terminate() -> Self {
        SignalKind(libc::SIGTERM)
    }

    /// Represents the SIGUSR1 signal.
    pub const fn user_defined1() -> Self {
        SignalKind(libc::SIGUSR1)
    }

    /// Represents the SIGUSR2 signal.
    pub const fn user_defined2() -> Self {
        SignalKind(libc::SIGUSR2)
    }

    /// Represents the SIGWINCH signal.
    ///
    /// Sent when the terminal window size changes.
    pub const fn window_change() -> Self {
        SignalKind(libc::SIGWINCH)
    }

    /// Represents the SIGCONT signal.
    ///
    /// Sent to resume a stopped process.
    pub const fn cont() -> Self {
        SignalKind(libc::SIGCONT)
    }

    /// Represents the SIGTSTP signal.
    ///
    /// Sent when the user presses Ctrl+Z.
    pub const fn terminal_stop() -> Self {
        SignalKind(libc::SIGTSTP)
    }

    /// Represents the SIGTTIN signal.
    pub const fn tty_input() -> Self {
        SignalKind(libc::SIGTTIN)
    }

    /// Represents the SIGTTOU signal.
    pub const fn tty_output() -> Self {
        SignalKind(libc::SIGTTOU)
    }

    /// Represents the SIGURG signal.
    pub const fn urgent() -> Self {
        SignalKind(libc::SIGURG)
    }
}

impl From<SignalKind> for super::SignalKind {
    fn from(kind: SignalKind) -> Self {
        super::SignalKind::from_raw(kind.as_raw())
    }
}

impl From<super::SignalKind> for SignalKind {
    fn from(kind: super::SignalKind) -> Self {
        SignalKind::from_raw(kind.as_raw())
    }
}

/// Creates a new signal listener for the specified Unix signal.
///
/// # Errors
///
/// Returns an error if the signal handler cannot be registered.
///
/// # Examples
///
/// ```no_run
/// use maniac::signal::unix::{signal, SignalKind};
///
/// #[maniac::main]
/// async fn main() -> std::io::Result<()> {
///     let mut sig = signal(SignalKind::hangup())?;
///     sig.recv().await;
///     println!("Received SIGHUP");
///     Ok(())
/// }
/// ```
pub fn signal(kind: SignalKind) -> io::Result<Signal> {
    Signal::new(kind.into())
}

/// Global initialization for signal handling.
static INIT: Once = Once::new();

/// Registered signals bitmap.
static mut REGISTERED_SIGNALS: u64 = 0;

/// Signal handler that notifies the registry.
extern "C" fn signal_handler(signum: libc::c_int) {
    registry().notify(signum);
}

/// Register a signal handler with the OS.
pub(crate) fn register_signal(signum: i32) -> io::Result<()> {
    // Validate signal number
    if signum < 1 || signum >= 64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid signal number: {}", signum),
        ));
    }

    // Check if this signal cannot be caught
    if signum == libc::SIGKILL || signum == libc::SIGSTOP {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("signal {} cannot be caught", signum),
        ));
    }

    let mask = 1u64 << signum;

    // Check if already registered
    // SAFETY: We're using atomic-like access patterns with the Once guard
    let already_registered = unsafe { REGISTERED_SIGNALS & mask != 0 };
    if already_registered {
        return Ok(());
    }

    // Register the signal handler
    let mut new_action: libc::sigaction = unsafe { MaybeUninit::zeroed().assume_init() };
    new_action.sa_sigaction = signal_handler as *const () as usize;
    new_action.sa_flags = libc::SA_RESTART | libc::SA_SIGINFO;

    // Block all signals during handler execution
    unsafe {
        libc::sigemptyset(&mut new_action.sa_mask);
    }

    let result = unsafe { libc::sigaction(signum, &new_action, std::ptr::null_mut()) };

    if result == -1 {
        return Err(io::Error::last_os_error());
    }

    // Mark as registered
    unsafe {
        REGISTERED_SIGNALS |= mask;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_kind_values() {
        assert_eq!(SignalKind::interrupt().as_raw(), libc::SIGINT);
        assert_eq!(SignalKind::terminate().as_raw(), libc::SIGTERM);
        assert_eq!(SignalKind::hangup().as_raw(), libc::SIGHUP);
        assert_eq!(SignalKind::quit().as_raw(), libc::SIGQUIT);
        assert_eq!(SignalKind::user_defined1().as_raw(), libc::SIGUSR1);
        assert_eq!(SignalKind::user_defined2().as_raw(), libc::SIGUSR2);
    }

    #[test]
    fn test_signal_kind_conversion() {
        let unix_kind = SignalKind::terminate();
        let generic_kind: super::super::SignalKind = unix_kind.into();
        let back: SignalKind = generic_kind.into();
        assert_eq!(unix_kind, back);
    }
}
