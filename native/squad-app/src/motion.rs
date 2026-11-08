//! Whether motion in the app should be calmed.

use super::*;

/// Whether motion should be calmed: the system asks for less (Accessibility > Display >
/// Reduce motion).
pub(crate) fn calm(cx: &App) -> bool {
    cx.reduce_motion() || system_reduce_motion()
}

#[cfg(target_os = "macos")]
fn system_reduce_motion() -> bool {
    objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

#[cfg(not(target_os = "macos"))]
fn system_reduce_motion() -> bool {
    false
}
