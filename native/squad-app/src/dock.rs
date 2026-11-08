//! The count on Squad Code's Dock icon, the red bubble other apps show: everything that needs
//! the person. The corner mark counts only the assistant's own
//! unread answers.

use std::sync::Mutex;

/// What the Dock shows now, so only a change reaches AppKit; `None` until the first call.
static SHOWN: Mutex<Option<Option<String>>> = Mutex::new(None);

/// Set the count again, when the Dock icon comes back from the menu bar.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn reapply() {
    let shown = SHOWN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(label) = shown.as_ref() {
        platform::set(label.as_deref());
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    pub(super) fn set(label: Option<&str>) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let tile = NSApplication::sharedApplication(mtm).dockTile();
        let label = label.map(NSString::from_str);
        tile.setBadgeLabel(label.as_deref());
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    pub(super) fn set(_: Option<&str>) {}
}
