//! Prints the events of a connected SpaceMouse until Enter is pressed.
//!
//! Uses the synchronous `SpaceMouse` API: the main thread polls for the
//! device, reads its events, and rescans after the device is lost. A helper
//! thread waits for Enter and raises the quit flag.

use spacemouse::{SpaceMouse, SpaceMouseError};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), SpaceMouseError> {
  let quit = Arc::new(AtomicBool::new(false));
  let quit_flag = Arc::clone(&quit);
  thread::spawn(move || {
    let _ = io::stdin().read_line(&mut String::new());
    quit_flag.store(true, Ordering::Relaxed);
  });

  println!("Listening for SpaceMouse events. Press Enter to quit.");
  let mut mouse = SpaceMouse::new()?;
  while !quit.load(Ordering::Relaxed) {
    if !mouse.connect() {
      thread::sleep(Duration::from_secs(1));
      continue;
    }
    println!("Connected.");
    while !quit.load(Ordering::Relaxed) {
      match mouse.read(Duration::from_millis(200)) {
        Ok(Some(event)) => println!("{event:?}"),
        Ok(None) => {}
        Err(error) => {
          println!("{error}");
          break;
        }
      }
    }
  }
  Ok(())
}
