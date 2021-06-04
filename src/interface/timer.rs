// Author: Nicholas Renner
//
// Timer functions for Rust interface. 
#![allow(dead_code)]

use std::thread;
pub use std::time::Instant as RustInstant;
pub use std::time::Duration as RustDuration;

// Create a new timer
pub fn starttimer() -> RustInstant {
    RustInstant::now()
}

// Return time since timer was started
pub fn readtimer(now: RustInstant) -> RustDuration {
    now.elapsed()
}

// Sleep function to sleep for x milliseconds
pub fn sleep_ms(dur: RustDuration) {
    thread::sleep(dur);
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  pub fn naptime() {
      let starttime = starttimer();
      let onesec = RustDuration::new(1, 0);
      sleep_ms(onesec);
      println!("{:?}", readtimer(starttime));
  }
}
