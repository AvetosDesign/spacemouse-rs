//! `hidapi`-based background capture loop.

use crate::report::{parse_report, ReportAxes};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
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

/// Wait between (re-)scans while no compatible device is open.
const RESCAN_INTERVAL: Duration = Duration::from_millis(500);

/// `hidapi` blocking-read timeout while a device is open. Chosen short
/// enough that a `stop()` request is noticed promptly, long enough to avoid
/// busy-looping the background thread when the device is idle.
const READ_TIMEOUT_MS: i32 = 200;

/// HID input reports are small. This includes generous headroom.
const READ_BUF_LEN: usize = 64;

/// Raw, unscaled motion values reported by a 3D mouse.
///
/// Values are whatever the device's translation/rotation HID reports contain -
/// no sensitivity scaling, dead-zone handling, or calibration is applied here
/// (see the crate-level docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpaceMouseMotion {
  /// Translate left(-)/right(+)
  pub translation_x: i16,
  /// Translate away(-)/towards(+) the user
  pub translation_y: i16,
  /// Translate down(-)/up(+)
  pub translation_z: i16,
  /// Tilt (pitch)
  pub rotation_x: i16,
  /// Tilt (roll)
  pub rotation_y: i16,
  /// Twist (yaw)
  pub rotation_z: i16,
}

/// Cross-platform SpaceMouse HID capture loop.
///
/// Handle to a loop running on a background thread, which continuously
/// (re-)discovers a compatible device, reads its raw HID reports, and
/// forwards decoded motion / connection-state changes to the callbacks given
/// to [`SpaceMouseLoop::new`]. The loop stops when the handle is dropped.
/// Constructing one is a safe no-op on a machine without a compatible device
/// connected. A device that is plugged in later, or unplugged and replugged,
/// is picked up automatically.
pub struct SpaceMouseLoop {
  /// Signals the background thread to stop and return.
  stop: Arc<AtomicBool>,
  /// Whether a compatible device is currently open. Kept outside the
  /// thread so [`SpaceMouseLoop::is_connected`] doesn't have to
  /// synchronize with it via the callbacks.
  connected: Arc<AtomicBool>,
  /// Sends desired LED on/off state into the background thread - see
  /// [`SpaceMouseLoop::set_led`].
  led_sender: mpsc::Sender<bool>,
  /// The background thread, joined on drop. Always `Some` until
  /// [`Drop::drop`] runs.
  thread: Option<thread::JoinHandle<()>>,
}

impl SpaceMouseLoop {
  /// Start capturing in a new background thread.
  ///
  /// `on_motion` is invoked (on the background thread, *not* the caller's
  /// thread) with the full, merged six-axis state every time a report is
  /// decoded. `on_connected_changed` is invoked on the background thread
  /// whenever a compatible device is opened or lost. Callers that need
  /// these on a specific thread are responsible for the hop themselves.
  pub fn new<M, C>(on_motion: M, on_connected_changed: C) -> Self
  where
    M: Fn(SpaceMouseMotion) + Send + 'static,
    C: Fn(bool) + Send + 'static,
  {
    let stop = Arc::new(AtomicBool::new(false));
    let connected = Arc::new(AtomicBool::new(false));
    let (led_sender, led_receiver) = mpsc::channel();
    let thread_stop = Arc::clone(&stop);
    let thread_connected = Arc::clone(&connected);
    let thread = thread::spawn(move || {
      run(
        &thread_stop,
        &thread_connected,
        &led_receiver,
        &on_motion,
        &on_connected_changed,
      );
    });
    Self {
      stop,
      connected,
      led_sender,
      thread: Some(thread),
    }
  }

  /// Whether a compatible device is currently open.
  pub fn is_connected(&self) -> bool {
    self.connected.load(Ordering::Relaxed)
  }

  /// Set whether the device's LED is lit.
  ///
  /// Applied immediately if a device is currently open, and reapplied
  /// whenever a device is (re)opened. Devices without an LED and write
  /// failures are silently ignored.
  pub fn set_led(&self, enabled: bool) {
    // Send only fails once the background thread has exited, e.g. during drop.
    let _ = self.led_sender.send(enabled);
  }
}

impl Drop for SpaceMouseLoop {
  fn drop(&mut self) {
    self.stop.store(true, Ordering::Relaxed);
    if let Some(t) = self.thread.take() {
      // Only fails if the background thread itself panicked; there is no
      // sensible way to surface that from a `Drop` impl, so it's dropped
      // here rather than propagated.
      let _ = t.join();
    }
  }
}

/// Body of the background capture thread.
///
/// Repeatedly looks for a compatible device, reads from it until it's lost
/// (or `stop` is set), then goes back to looking for one - so a
/// hot-unplugged/replugged device recovers on its own without recreating
/// [`SpaceMouseLoop`].
fn run<M, C>(
  stop: &AtomicBool,
  connected: &AtomicBool,
  led_receiver: &mpsc::Receiver<bool>,
  on_motion: &M,
  on_connected_changed: &C,
) where
  M: Fn(SpaceMouseMotion),
  C: Fn(bool),
{
  // LED state is connection-bound
  let mut desired_led = true;

  while !stop.load(Ordering::Relaxed) {
    // Track any LED command(s) that arrive even if no device is connected.
    // This ensures that when a device is connected, its LED is set to the
    // correct state.
    while let Ok(enabled) = led_receiver.try_recv() {
      desired_led = enabled;
    }

    let api = match hidapi::HidApi::new() {
      Ok(api) => api,
      Err(_) => {
        thread::sleep(RESCAN_INTERVAL);
        continue;
      }
    };

    let Some(info) = find_device(&api) else {
      thread::sleep(RESCAN_INTERVAL);
      continue;
    };

    let device = match info.open_device(&api) {
      Ok(d) => d,
      Err(_) => {
        thread::sleep(RESCAN_INTERVAL);
        continue;
      }
    };

    connected.store(true, Ordering::Relaxed);
    on_connected_changed(true);
    write_led(&device, desired_led);

    let mut motion = SpaceMouseMotion::default();
    let mut buf = [0u8; READ_BUF_LEN];
    loop {
      // Drain (and apply) any pending LED command before honoring a `stop`
      // request below. Ordering matters: if this were done after the `stop`
      // check, a command sent just before shutdown could be left sitting in
      // the channel forever.
      let mut led_changed = false;
      while let Ok(enabled) = led_receiver.try_recv() {
        if enabled != desired_led {
          desired_led = enabled;
          led_changed = true;
        }
      }
      if led_changed {
        write_led(&device, desired_led);
      }
      if stop.load(Ordering::Relaxed) {
        connected.store(false, Ordering::Relaxed);
        on_connected_changed(false);
        return;
      }
      match device.read_timeout(&mut buf, READ_TIMEOUT_MS) {
        // Timed out without any data - just loop around to re-check `stop`.
        Ok(0) => continue,
        Ok(len) => {
          if let Some(axes) = parse_report(&buf[..len]) {
            match axes {
              ReportAxes::Translation(x, y, z) => {
                motion.translation_x = x;
                motion.translation_y = y;
                motion.translation_z = z;
              }
              ReportAxes::Rotation(x, y, z) => {
                motion.rotation_x = x;
                motion.rotation_y = y;
                motion.rotation_z = z;
              }
            }
            on_motion(motion);
          }
        }
        // The device is gone (unplugged, I/O error, ...): stop reading and
        // go back to scanning for a (possibly different) device.
        Err(_) => break,
      }
    }

    connected.store(false, Ordering::Relaxed);
    on_connected_changed(false);
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

/// Find the first currently-connected, compatible 3D-mouse HID interface.
fn find_device(api: &hidapi::HidApi) -> Option<&hidapi::DeviceInfo> {
  api.device_list().find(|d| {
    KNOWN_VENDOR_IDS.contains(&d.vendor_id())
      && d.usage_page() == USAGE_PAGE_GENERIC_DESKTOP
      && d.usage() == USAGE_ID_MULTI_AXIS_CONTROLLER
  })
}
