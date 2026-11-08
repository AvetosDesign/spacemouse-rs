//! Error type of the synchronous API.

use std::fmt;

/// Errors reported by [`SpaceMouse`](crate::SpaceMouse).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpaceMouseError {
  /// The HID subsystem could not be initialized. Carries `hidapi`'s message.
  Init(String),
  /// No device is open. See [`connect()`](crate::SpaceMouse::connect).
  NotConnected,
  /// The device was lost and has been closed. [`connect()`] can be called
  /// again to scan for it.
  ///
  /// [`connect()`]: crate::SpaceMouse::connect
  DeviceLost,
}

impl fmt::Display for SpaceMouseError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Init(msg) => {
        write!(f, "failed to initialize the HID subsystem: {msg}")
      }
      Self::NotConnected => f.write_str("no SpaceMouse is connected"),
      Self::DeviceLost => f.write_str("the SpaceMouse was disconnected"),
    }
  }
}

impl std::error::Error for SpaceMouseError {}
