//! Synchronous access to a SpaceMouse device.

use crate::error::SpaceMouseError;
use crate::event::SpaceMouseEvent;
use crate::report::parse_report;
use std::time::Duration;

/// USB vendor ID used by current 3Dconnexion devices.
const VENDOR_ID_3DCONNEXION: u16 = 0x256f;

/// Legacy USB vendor ID used by early (Logitech-era) 3Dconnexion devices.
const VENDOR_ID_LOGITECH_3DCONNEXION: u16 = 0x046d;

/// All USB vendor IDs the device must match to be considered, in no
/// particular priority order.
const KNOWN_VENDOR_IDS: [u16; 2] =
  [VENDOR_ID_3DCONNEXION, VENDOR_ID_LOGITECH_3DCONNEXION];

/// HID usage page identifying "Generic Desktop Controls".
const USAGE_PAGE_GENERIC_DESKTOP: u16 = 0x01;

/// HID usage ID identifying a "Multi-axis Controller" within the Generic
/// Desktop Controls usage page.
const USAGE_ID_MULTI_AXIS_CONTROLLER: u16 = 0x08;

/// HID input reports are small. This includes generous headroom.
const READ_BUF_LEN: usize = 64;

/// Synchronous access to a SpaceMouse (3D mouse) HID device.
///
/// Owns the HID context and at most one open device. Nothing runs in the
/// background: the caller scans for a device with [`SpaceMouse::connect`],
/// reads events with [`SpaceMouse::read`], and sets the LED with
/// [`SpaceMouse::set_led`]. See [`SpaceMouseLoop`](crate::SpaceMouseLoop)
/// for a layer which does this on a background thread.
pub struct SpaceMouse {
  /// The open device, if any. Declared before `api` so it is dropped first.
  device: Option<hidapi::HidDevice>,
  /// The HID context used to scan for and open devices.
  api: hidapi::HidApi,
}

impl SpaceMouse {
  /// Initialize the HID subsystem.
  ///
  /// No device is opened yet, see [`SpaceMouse::connect`].
  pub fn new() -> Result<Self, SpaceMouseError> {
    let api = hidapi::HidApi::new()
      .map_err(|e| SpaceMouseError::Init(e.to_string()))?;
    Ok(Self { device: None, api })
  }

  /// Scan for a compatible device and open the first one found.
  ///
  /// Returns whether a device is open afterwards. Does nothing and returns
  /// `true` if a device is already open. Otherwise the system's HID devices
  /// are enumerated on every call, so callers polling for a device should
  /// pace their calls.
  pub fn connect(&mut self) -> bool {
    if self.device.is_none() {
      self.device = open_device(&mut self.api);
    }
    self.device.is_some()
  }

  /// Wait up to `timeout` for the next event from the open device.
  ///
  /// Returns `Ok(None)` if the timeout elapsed or the device sent a report
  /// this crate doesn't recognize. Only [`SpaceMouseEvent::Translation`] and
  /// [`SpaceMouseEvent::Rotation`] are returned. A lost device is reported
  /// as an error, and a new device through [`SpaceMouse::connect`].
  ///
  /// # Errors
  ///
  /// [`SpaceMouseError::NotConnected`] if no device is open, and
  /// [`SpaceMouseError::DeviceLost`] if reading failed, e.g. because the
  /// device was unplugged. In the latter case the device is closed, so the
  /// next [`SpaceMouse::connect`] scans again.
  pub fn read(
    &mut self,
    timeout: Duration,
  ) -> Result<Option<SpaceMouseEvent>, SpaceMouseError> {
    let device = self.device.as_ref().ok_or(SpaceMouseError::NotConnected)?;
    let timeout_ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
    let mut buf = [0u8; READ_BUF_LEN];
    match device.read_timeout(&mut buf, timeout_ms) {
      Ok(0) => Ok(None),
      Ok(len) => Ok(parse_report(&buf[..len])),
      Err(_) => {
        self.device = None;
        Err(SpaceMouseError::DeviceLost)
      }
    }
  }

  /// Set whether the device's LED is lit.
  ///
  /// Best-effort: devices without an LED and transient write failures are
  /// silently ignored.
  ///
  /// # Errors
  ///
  /// [`SpaceMouseError::NotConnected`] if no device is open.
  pub fn set_led(&self, enabled: bool) -> Result<(), SpaceMouseError> {
    let device = self.device.as_ref().ok_or(SpaceMouseError::NotConnected)?;
    write_led(device, enabled);
    Ok(())
  }
}

/// Write the HID output report that controls the device's LED.
///
/// All LED-capable devices across the SpaceNavigator/SpaceMouse/SpacePilot
/// product line utilize the same HID report format:
/// Report ID `0x04` + a single data byte (`0x01` on / `0x00` off)
/// Best-effort: quietly ignore devices without an LED and transient
/// write failures.
fn write_led(device: &hidapi::HidDevice, enabled: bool) {
  let _ = device.write(&[0x04, if enabled { 0x01 } else { 0x00 }]);
}

/// Refresh the device list and open the first compatible device, if any.
fn open_device(api: &mut hidapi::HidApi) -> Option<hidapi::HidDevice> {
  api.refresh_devices().ok()?;
  let api = &*api;
  find_device(api)?.open_device(api).ok()
}

/// Find the first currently-connected, compatible 3D-mouse HID interface.
fn find_device(api: &hidapi::HidApi) -> Option<&hidapi::DeviceInfo> {
  api.device_list().find(|d| {
    KNOWN_VENDOR_IDS.contains(&d.vendor_id())
      && d.usage_page() == USAGE_PAGE_GENERIC_DESKTOP
      && d.usage() == USAGE_ID_MULTI_AXIS_CONTROLLER
  })
}
