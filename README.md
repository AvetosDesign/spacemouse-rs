# spacemouse

Provides cross-platform (Windows/macOS/Linux) raw HID capture for
3Dconnexion SpaceMouse and compatible 3D-mouse devices. HID reports are read
directly via `hidapi` rather than depending on 3Dconnexion's proprietary
driver (3DxWare).

This crate deliberately knows nothing about sensitivity, inversion,
calibration or dead-zones — it reports the device's raw, unscaled axis
readings and leaves interpretation to the caller.

Built for [LibrePCB](https://librepcb.org)'s SpaceMouse support, but usable by
any application.

## Usage

The crate offers two layers: `SpaceMouse` gives synchronous access to a device
(scan for and open it, read events, set its LED), and `SpaceMouseLoop` runs it
on a background thread and delivers events to a callback, reconnecting
automatically. See the `examples` directory:

    cargo run --example print_events
    cargo run --example print_events_sync

## Why `hidapi`

This crate depends on the `hidapi` crate for cross-platform HID access
(`hidraw` on Linux, `IOHIDManager` on macOS, `hid.dll` on Windows).

On Linux, the pure-Rust `hidraw` backend of `hidapi` is used (the
`linux-native-basic-udev` feature), so building and running this crate needs
neither `libudev` nor `libusb`; no `-dev` packages have to be installed. On
macOS and Windows, `hidapi` vendors a small amount of C source code (the
underlying `hidapi` C library) and compiles it as part of `cargo build`, via
the `cc` crate.

This crate is offered under the terms of the MIT license.

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

