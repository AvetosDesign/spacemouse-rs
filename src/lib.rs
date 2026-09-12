// Claude AI assisted in the writing of this file.

//! spacemouse
//!
//! Cross-platform (Windows/macOS/Linux) raw HID capture for 3Dconnexion
//! SpaceMouse (and compatible) 3D-mouse devices. See `README.md` for
//! more information.
//!
//! This crate deliberately knows nothing about sensitivity, inversion,
//! calibration or dead-zones. It always reports the device's raw,
//! unscaled axis readings. Data manipulation (scaling, filtering, mapping
//! to application actions, etc.) is left entirely to the caller, so the
//! crate's surface stays small and easy to wrap with any FFI or binding
//! layer a consumer needs.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

pub mod device;
mod hid;
mod report;

// Re-Exports
pub use device::SpaceMouseMotion;
pub use hid::SpaceMouseBackend;
