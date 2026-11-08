//! Shell tabs in the terminal panel: real PTYs rendered from a VT100 screen.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static FONT_SIZE: AtomicU32 = AtomicU32::new(12);
static SCROLL_SPEED: AtomicU32 = AtomicU32::new(1);
static RIGHT_CLICK_PASTE: AtomicBool = AtomicBool::new(false);

/// Apply the Terminal settings (size in points, wheel multiplier, right-click paste).
pub(crate) fn configure(size: u32, scroll: u32, right_paste: bool) {
    FONT_SIZE.store(size.clamp(9, 24), Ordering::Relaxed);
    SCROLL_SPEED.store(scroll.max(1), Ordering::Relaxed);
    RIGHT_CLICK_PASTE.store(right_paste, Ordering::Relaxed);
}
