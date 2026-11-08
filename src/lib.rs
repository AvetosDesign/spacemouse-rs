//! spacemouse
//!
//! Cross-platform (Windows/macOS/Linux) raw HID capture for 3Dconnexion
//! SpaceMouse (and compatible) 3D-mouse devices. See `README.md` for
//! more information.
//!
//! The crate offers two layers:
//!
//! * [`SpaceMouse`] gives synchronous access to a device: scan for and open
//!   it, read events, and set its LED. The caller drives it.
//! * [`SpaceMouseLoop`] runs a [`SpaceMouse`] on a background thread and
//!   delivers [`SpaceMouseEvent`]s to a callback, reconnecting automatically.
//!
//! This crate deliberately knows nothing about sensitivity, inversion,
//! calibration or dead-zones. It always reports the device's raw,
//! unscaled axis readings. Data manipulation (scaling, filtering, mapping
//! to application actions, etc.) is left entirely to the caller.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

mod error;
mod event;
mod report;
mod spacemouse;
mod spacemouse_loop;

// Re-Exports
pub use error::SpaceMouseError;
pub use event::SpaceMouseEvent;
pub use spacemouse::SpaceMouse;
pub use spacemouse_loop::SpaceMouseLoop;
