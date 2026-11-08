//! `hidapi`-based background capture loop.

use crate::event::SpaceMouseEvent;
use crate::report::parse_report;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

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

/// Default wait between (re-)scans while no compatible device is open.
const DEFAULT_RESCAN_INTERVAL: Duration = Duration::from_secs(2);

/// Lower bound for a configured rescan interval, to avoid churning on device
/// enumeration.
const MIN_RESCAN_INTERVAL: Duration = Duration::from_millis(100);

/// `hidapi` blocking-read timeout while a device is open. Chosen short
/// enough that a stop or LED request is noticed promptly, long enough to
/// avoid busy-looping the background thread when the device is idle.
const READ_TIMEOUT_MS: i32 = 200;

/// HID input reports are small. This includes generous headroom.
const READ_BUF_LEN: usize = 64;

/// Request sent from a [`SpaceMouseLoop`] handle to its background thread.
enum Command {
  /// Set whether the device's LED is lit.
  SetLed(bool),
  /// Stop the loop.
  Stop,
}

/// Cross-platform SpaceMouse HID capture loop.
///
/// Handle to a loop running on a background thread, which continuously
/// (re-)discovers a compatible device, reads its raw HID reports, and
/// forwards the resulting [`SpaceMouseEvent`]s to the callback given to
/// [`SpaceMouseLoop::new`]. The loop stops when the handle is dropped.
/// Constructing one is a safe no-op on a machine without a compatible device
/// connected. A device that is plugged in later, or unplugged and replugged,
/// is picked up automatically.
pub struct SpaceMouseLoop {
  /// Whether a compatible device is currently open. Kept outside the
  /// thread so [`SpaceMouseLoop::is_connected`] doesn't have to
  /// synchronize with it via the callback.
  connected: Arc<AtomicBool>,
  /// Sends requests into the background thread - see
  /// [`SpaceMouseLoop::set_led`] and [`Drop::drop`].
  command_sender: mpsc::Sender<Command>,
  /// The background thread, joined on drop. Always `Some` until
  /// [`Drop::drop`] runs.
  thread: Option<thread::JoinHandle<()>>,
}

impl SpaceMouseLoop {
  /// Start capturing in a new background thread.
  ///
  /// `on_event` is invoked on the background thread, *not* the caller's
  /// thread, for every [`SpaceMouseEvent`]. Callers that need events on a
  /// specific thread are responsible for the hop themselves.
  ///
  /// Waits between scans (while no compatible device is open) using the
  /// default scan interval. See [`SpaceMouseLoop::with_rescan_interval`]
  /// for a configurable scan period.
  pub fn new<F>(on_event: F) -> Self
  where
    F: Fn(SpaceMouseEvent) + Send + 'static,
  {
    Self::with_rescan_interval(DEFAULT_RESCAN_INTERVAL, on_event)
  }

  /// Like [`SpaceMouseLoop::new`], but waits `rescan_interval` between scans
  /// while no compatible device is open.
  ///
  /// Shorter intervals pick up a newly connected device sooner, at the cost
  /// of enumerating the system's HID devices more often. Values below 100 ms
  /// are raised to 100 ms.
  pub fn with_rescan_interval<F>(rescan_interval: Duration, on_event: F) -> Self
  where
    F: Fn(SpaceMouseEvent) + Send + 'static,
  {
    let rescan_interval = rescan_interval.max(MIN_RESCAN_INTERVAL);
    let connected = Arc::new(AtomicBool::new(false));
    let (command_sender, commands) = mpsc::channel();
    let thread_connected = Arc::clone(&connected);
    let thread = thread::spawn(move || {
      run(&thread_connected, &commands, rescan_interval, &on_event);
    });
    Self {
      connected,
      command_sender,
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
    let _ = self.command_sender.send(Command::SetLed(enabled));
  }
}

impl Drop for SpaceMouseLoop {
  fn drop(&mut self) {
    // Send only fails if the background thread has already exited.
    let _ = self.command_sender.send(Command::Stop);
    if let Some(t) = self.thread.take() {
      // Only fails if the background thread itself panicked; there is no
      // sensible way to surface that from a `Drop` impl, so it's dropped
      // here rather than propagated.
      let _ = t.join();
    }
  }
}

/// Outcome of draining the pending commands.
struct Drained {
  /// Whether the desired LED state changed.
  led_changed: bool,
  /// Whether a stop was requested.
  stop: bool,
}

/// Apply all pending commands without blocking.
fn drain_commands(
  commands: &mpsc::Receiver<Command>,
  desired_led: &mut bool,
) -> Drained {
  let mut drained = Drained {
    led_changed: false,
    stop: false,
  };
  loop {
    match commands.try_recv() {
      Ok(Command::SetLed(enabled)) => {
        if enabled != *desired_led {
          *desired_led = enabled;
          drained.led_changed = true;
        }
      }
      Ok(Command::Stop) | Err(mpsc::TryRecvError::Disconnected) => {
        drained.stop = true;
        break;
      }
      Err(mpsc::TryRecvError::Empty) => break,
    }
  }
  drained
}

/// Wait up to `timeout` for a stop request, applying LED commands that
/// arrive in the meantime. Returns whether a stop was requested.
fn wait_for_stop(
  commands: &mpsc::Receiver<Command>,
  timeout: Duration,
  desired_led: &mut bool,
) -> bool {
  let deadline = Instant::now() + timeout;
  loop {
    let remaining = deadline.saturating_duration_since(Instant::now());
    match commands.recv_timeout(remaining) {
      Ok(Command::SetLed(enabled)) => *desired_led = enabled,
      Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
        return true;
      }
      Err(mpsc::RecvTimeoutError::Timeout) => return false,
    }
  }
}

/// Body of the background capture thread.
///
/// Repeatedly looks for a compatible device, reads from it until it's lost
/// (or a stop is requested), then goes back to looking for one - so a
/// hot-unplugged/replugged device recovers on its own without recreating
/// [`SpaceMouseLoop`].
fn run<F>(
  connected: &AtomicBool,
  commands: &mpsc::Receiver<Command>,
  rescan_interval: Duration,
  on_event: &F,
) where
  F: Fn(SpaceMouseEvent),
{
  // LED state is connection-bound
  let mut desired_led = true;

  loop {
    // Track any LED command(s) that arrive even if no device is connected.
    // This ensures that when a device is connected, its LED is set to the
    // correct state.
    if drain_commands(commands, &mut desired_led).stop {
      return;
    }

    let api = match hidapi::HidApi::new() {
      Ok(api) => api,
      Err(_) => {
        if wait_for_stop(commands, rescan_interval, &mut desired_led) {
          return;
        }
        continue;
      }
    };

    let Some(info) = find_device(&api) else {
      if wait_for_stop(commands, rescan_interval, &mut desired_led) {
        return;
      }
      continue;
    };

    let device = match info.open_device(&api) {
      Ok(d) => d,
      Err(_) => {
        if wait_for_stop(commands, rescan_interval, &mut desired_led) {
          return;
        }
        continue;
      }
    };

    connected.store(true, Ordering::Relaxed);
    on_event(SpaceMouseEvent::Connected);
    write_led(&device, desired_led);

    let mut buf = [0u8; READ_BUF_LEN];
    loop {
      // Apply any pending LED change before honoring a stop request, so a
      // command sent just before shutdown still reaches the device.
      let drained = drain_commands(commands, &mut desired_led);
      if drained.led_changed {
        write_led(&device, desired_led);
      }
      if drained.stop {
        connected.store(false, Ordering::Relaxed);
        on_event(SpaceMouseEvent::Disconnected);
        return;
      }
      match device.read_timeout(&mut buf, READ_TIMEOUT_MS) {
        // Timed out without any data - just loop around to re-check commands.
        Ok(0) => continue,
        Ok(len) => {
          if let Some(event) = parse_report(&buf[..len]) {
            on_event(event);
          }
        }
        // The device is gone (unplugged, I/O error, ...): stop reading and
        // go back to scanning for a (possibly different) device.
        Err(_) => break,
      }
    }

    connected.store(false, Ordering::Relaxed);
    on_event(SpaceMouseEvent::Disconnected);
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
