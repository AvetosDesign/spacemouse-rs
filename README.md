# spacemouse

Provides a cross-platform (Windows/macOS/Linux) raw HID capture backend for
3Dconnexion SpaceMouse and compatible 3D-mouse devices. HID reports are read
directly via `hidapi` rather than depending on 3Dconnexion's proprietary
driver (3DxWare).

This crate deliberately knows nothing about sensitivity, inversion,
calibration or dead-zones — it reports the device's raw, unscaled axis
readings and leaves interpretation to the caller.

Originally developed as part of [LibrePCB](https://librepcb.org)'s SpaceMouse
support; extracted into this standalone crate so it can be reused outside
that project. LibrePCB itself consumes this crate through a small FFI layer
that lives in its own `rust-core` crate rather than here, so this crate has
no C/C++ bindings of its own.

## Why `hidapi`

This crate depends on the `hidapi` crate for cross-platform HID access
(`hidraw` on Linux, `IOHIDManager` on macOS, `hid.dll` on Windows).

On Linux, the pure-Rust `hidraw` backend of `hidapi` is used (the
`linux-native-basic-udev` feature), so building and running this crate needs
neither `libudev` nor `libusb`; no `-dev` packages have to be installed. On
macOS and Windows, `hidapi` vendors a small amount of C source code (the
underlying `hidapi` C library) and compiles it as part of `cargo build`, via
the `cc` crate.

Licensing is not a concern: the C `hidapi` library is dual/triple licensed
(GPLv2-or-later / a 3-clause BSD variant / the original permissive "HIDAPI"
license, at the user's choice) and the `hidapi` Rust crate itself is MIT —
both are unambiguously compatible with this crate's MIT license.

## Features

* `fail-on-warnings`: Turn compiler warnings into errors (used on CI).

## Build

    cargo build

## Check

    cargo clippy

## Test

    cargo test

## Build Documentation

    cargo doc --no-deps --document-private-items --open

