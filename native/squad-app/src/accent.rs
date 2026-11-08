//! The accent's glow strip: a selected row or an open tab has the edge it leads with lit
//! in the accent (the left edge of a row or the rail's tab, the top of a tab), brightest
//! at the middle and fading out before the corners. When it opens the light grows out
//! from the middle, and while the pointer is over it the light brightens, easing in and
//! out. With Animations off it snaps.

use super::*;
use std::cell::RefCell;
use std::time::Instant;

thread_local! {
    /// Whether the pointer was over each bracket's tab or row when it last painted, and
    /// since when, keyed by the bracket.
    static HOVERS: RefCell<HashMap<SharedString, (bool, Instant)>> =
        RefCell::new(HashMap::new());
}
