//! Windows-specific signal handling.
//!
//! This module provides Windows-specific console control event handling.
//!
//! # Console Control Events
//!
//! Windows provides console control events instead of Unix signals:
//!
//! - `CTRL_C_EVENT` - Ctrl+C was pressed
//! - `CTRL_BREAK_EVENT` - Ctrl+Break was pressed
//! - `CTRL_CLOSE_EVENT` - Console window is closing
//! - `CTRL_LOGOFF_EVENT` - User is logging off
//! - `CTRL_SHUTDOWN_EVENT` - System is shutting down
//!
//! # Example
//!
//! ```no_run
//! use maniac::signal::windows::{signal, SignalKind};
//!
//! #[maniac::main]
//! async fn main() -> std::io::Result<()> {
//!     let mut ctrl_break = signal(SignalKind::ctrl_break())?;
//!     ctrl_break.recv().await;
//!     println!("Received Ctrl+Break");
//!     Ok(())
//! }
//! ```

use std::io;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::System::Console::{
    CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT,
    SetConsoleCtrlHandler,
};

/// Windows BOOL type (i32)
type BOOL = i32;

use super::Signal;
use super::registry::registry;

/// Windows-specific signal kind.
///
/// This represents Windows console control events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SignalKind(i32);

impl SignalKind {
    /// Creates a `SignalKind` from a raw control event type.
    pub const fn from_raw(event: i32) -> Self {
        SignalKind(event)
    }

    /// Returns the raw control event type.
    pub const fn as_raw(self) -> i32 {
        self.0
    }

    /// Represents the CTRL_C_EVENT.
    ///
    /// Sent when the user presses Ctrl+C.
    pub const fn ctrl_c() -> Self {
        SignalKind(CTRL_C_EVENT as i32)
    }

    /// Represents the CTRL_BREAK_EVENT.
    ///
    /// Sent when the user presses Ctrl+Break.
    pub const fn ctrl_break() -> Self {
        SignalKind(CTRL_BREAK_EVENT as i32)
    }

    /// Represents the CTRL_CLOSE_EVENT.
    ///
    /// Sent when the console window is being closed.
    pub const fn ctrl_close() -> Self {
        SignalKind(CTRL_CLOSE_EVENT as i32)
    }

    /// Represents the CTRL_LOGOFF_EVENT.
    ///
    /// Sent when the user is logging off.
    pub const fn ctrl_logoff() -> Self {
        SignalKind(CTRL_LOGOFF_EVENT as i32)
    }

    /// Represents the CTRL_SHUTDOWN_EVENT.
    ///
    /// Sent when the system is shutting down.
    pub const fn ctrl_shutdown() -> Self {
        SignalKind(CTRL_SHUTDOWN_EVENT as i32)
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

/// Creates a new signal listener for the specified Windows control event.
///
/// # Errors
///
/// Returns an error if the handler cannot be registered.
///
/// # Examples
///
/// ```no_run
/// use maniac::signal::windows::{signal, SignalKind};
///
/// #[maniac::main]
/// async fn main() -> std::io::Result<()> {
///     let mut sig = signal(SignalKind::ctrl_break())?;
///     sig.recv().await;
///     println!("Received Ctrl+Break");
///     Ok(())
/// }
/// ```
pub fn signal(kind: SignalKind) -> io::Result<Signal> {
    Signal::new(kind.into())
}

/// Global initialization for console control handler.
static INIT: Once = Once::new();
static HANDLER_REGISTERED: AtomicBool = AtomicBool::new(false);

/// Console control handler callback.
///
/// This is called by Windows when a console control event occurs.
unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> BOOL {
    // Notify the registry
    registry().notify(ctrl_type as i32);

    // Return FALSE to allow the default handler to run for certain events
    // Return TRUE to indicate we handled it
    match ctrl_type {
        CTRL_C_EVENT | CTRL_BREAK_EVENT => {
            // We handle these, prevent default termination
            1 // TRUE
        }
        CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT => {
            // Allow default handler to run after we process
            // These events have a timeout, so we can't prevent them
            0 // FALSE
        }
        _ => 0, // FALSE
    }
}

/// Register the console control handler.
fn register_handler() -> io::Result<()> {
    if HANDLER_REGISTERED.load(Ordering::Acquire) {
        return Ok(());
    }

    INIT.call_once(|| {
        let result = unsafe { SetConsoleCtrlHandler(Some(console_ctrl_handler), 1) };
        if result != 0 {
            HANDLER_REGISTERED.store(true, Ordering::Release);
        }
    });

    if HANDLER_REGISTERED.load(Ordering::Acquire) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "failed to register console control handler",
        ))
    }
}

/// Register a signal handler with Windows.
pub(crate) fn register_signal(signum: i32) -> io::Result<()> {
    // Validate the event type
    let valid = matches!(
        signum as u32,
        CTRL_C_EVENT
            | CTRL_BREAK_EVENT
            | CTRL_CLOSE_EVENT
            | CTRL_LOGOFF_EVENT
            | CTRL_SHUTDOWN_EVENT
    );

    if !valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid control event type: {}", signum),
        ));
    }

    // Register the console control handler
    register_handler()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_kind_values() {
        assert_eq!(SignalKind::ctrl_c().as_raw(), CTRL_C_EVENT as i32);
        assert_eq!(SignalKind::ctrl_break().as_raw(), CTRL_BREAK_EVENT as i32);
        assert_eq!(SignalKind::ctrl_close().as_raw(), CTRL_CLOSE_EVENT as i32);
        assert_eq!(SignalKind::ctrl_logoff().as_raw(), CTRL_LOGOFF_EVENT as i32);
        assert_eq!(
            SignalKind::ctrl_shutdown().as_raw(),
            CTRL_SHUTDOWN_EVENT as i32
        );
    }

    #[test]
    fn test_signal_kind_conversion() {
        let win_kind = SignalKind::ctrl_c();
        let generic_kind: super::super::SignalKind = win_kind.into();
        let back: SignalKind = generic_kind.into();
        assert_eq!(win_kind, back);
    }
}
