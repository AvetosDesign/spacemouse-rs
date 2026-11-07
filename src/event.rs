//! Events reported by a 3D mouse.

/// An event reported by the SpaceMouse capture loop.
///
/// Translation and rotation events carry the raw, unscaled values of a single
/// HID report as absolute readings, not deltas. No sensitivity scaling,
/// dead-zone handling, or calibration is applied (see the crate-level docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpaceMouseEvent {
  /// A compatible device was opened.
  Connected,
  /// The device was lost, or the capture loop is stopping.
  Disconnected,
  /// Translation reading.
  Translation {
    /// Left(-)/right(+)
    x: i16,
    /// Away(-)/towards(+) the user
    y: i16,
    /// Down(-)/up(+)
    z: i16,
  },
  /// Rotation reading.
  Rotation {
    /// Tilt (pitch)
    x: i16,
    /// Tilt (roll)
    y: i16,
    /// Twist (yaw)
    z: i16,
  },
}
