//! Waiting without polling, and how much of Squad Code the person can see.
//!
//! - [`Inbox`] takes over a channel's receiving end, so the UI wakes when something
//!   arrives instead of checking the channel on a timer.
//! - [`shown`] says whether the window can be seen, so periodic work can slow down or
//!   stop while Squad Code is hidden, minimized or covered.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static SHOWN: AtomicBool = AtomicBool::new(true);

/// Whether any part of the main window is on screen: not hidden, minimized, covered or
/// on a sleeping display.
pub fn shown() -> bool {
    SHOWN.load(Ordering::Relaxed)
}

/// `front` while the window is on screen, `hidden` otherwise.
pub fn pace(front: Duration, hidden: Duration) -> Duration {
    if shown() {
        front
    } else {
        hidden
    }
}
