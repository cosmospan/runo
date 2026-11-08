//! The window that hangs under the menu bar glyph and holds the crew's panel
//! (`v2::tray`). It closes when it loses focus.

use super::Anchor;
use crate::*;
use std::cell::{Cell, RefCell};
use std::time::Instant;

thread_local! {
    static OPEN: RefCell<Option<AnyWindowHandle>> = const { RefCell::new(None) };
    /// When the panel last closed on losing focus, so the click on the glyph that took
    /// its focus doesn't open it again straight away.
    static CLOSED_AT: Cell<Option<Instant>> = const { Cell::new(None) };
}

pub(super) fn close(cx: &mut App) {
    if let Some(window) = OPEN.take() {
        let _ = window.update(cx, |_, window, _| window.remove_window());
    }
}

/// The panel lost focus: close it, and keep the click on the glyph that took its
/// focus from opening it again straight away.
pub(super) fn dismissed(window: &mut Window) {
    CLOSED_AT.set(Some(Instant::now()));
    OPEN.take();
    window.remove_window();
}

pub(super) fn toggle(anchor: Option<Anchor>, cx: &mut App) {
    if OPEN.with_borrow(Option::is_some) {
        close(cx);
        return;
    }
    if CLOSED_AT
        .get()
        .is_some_and(|at| at.elapsed() < Duration::from_millis(400))
    {
        return;
    }
    let (width, height) = (
        crate::v2::tray::WIDTH,
        crate::v2::tray::Tray::first_height(cx),
    );
    let origin = anchor
        .map(|a| point(px(a.x - width / 2.), px(a.y + 6.)))
        .unwrap_or_default();
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            origin,
            size(px(width), px(height)),
        ))),
        titlebar: None,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        focus: true,
        show: true,
        display_id: anchor
            .and_then(|a| a.display)
            .map(|id| DisplayId::new(id as u64)),
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    let opened = open_window(options, cx, |window, cx| {
        cx.new(|cx| crate::v2::tray::Tray::new(window, cx))
    })
    .map(|(h, _)| h);
    let Ok(handle) = opened else {
        return;
    };
    let _ = handle.update(cx, |_, window, cx| {
        // The component library's root paints an opaque background; the panel draws its
        // own rounded card.
        gpui_kit::base::Root::update(window, cx, |root, _, cx| {
            root.style().background = Some(gpui_kit::transparent_black().into());
            cx.notify();
        });
        super::platform::place(window, width, height);
    });
    OPEN.set(Some(handle));
}
