//! The Workbench frame around every screen: the mode rail on the left, the top bar
//! with the project switcher, search and live counters, the tab strip over a session,
//! and the Home page. Design and Architecture have their places on the rail; their pages
//! live in `design` and `architecture`.

use super::*;
use std::cell::Cell;

/// How long the shell's small transitions take: quick enough to feel immediate, long
/// enough that a change reads as a change rather than a jump.
const QUICK: u64 = 160;

/// The track behind a row of connected choices, such as the sidebar's filter or a
/// setting's options: a shallow well a step darker than what it sits on. Every
/// segmented control uses it, so they all read the same.
pub(crate) fn seg_track() -> Div {
    div()
        .flex()
        .items_center()
        .p(px(3.))
        .gap(px(2.))
        .rounded(px(9.))
        .bg(rgba(if is_dark() { 0x00000047 } else { 0x0000000d }))
}

/// The raised pill under the chosen choice in a `seg_track`, which fades in when it is
/// chosen. A child of the choice, which must be `relative` and add it first.
pub(crate) fn seg_pill(key: impl Into<SharedString>) -> AnyElement {
    chosen_fill_of(
        key,
        7.,
        div()
            // A touch more lift through glass, where the desktop behind thins it.
            .bg(rgba(match (is_dark(), glass_on()) {
                (true, false) => 0xffffff17,
                (true, true) => 0xffffff21,
                (false, false) => 0xffffffff,
                (false, true) => 0xffffffd9,
            }))
            .shadow_sm(),
    )
}

/// One choice in a `seg_track`: the chosen one on its pill in full text colour, the
/// rest quieter until hovered.
pub(crate) fn seg_choice(
    id: impl Into<ElementId>,
    key: impl Into<SharedString>,
    on: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .h(px(26.))
        .px(px(10.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(5.))
        .rounded(px(7.))
        .cursor_pointer()
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .when(on, |el| el.child(seg_pill(key)).text_color(rgb(c_text())))
        .when(!on, |el| {
            el.text_color(rgb(c_secondary()))
                .hover(|el| el.text_color(rgb(c_text())))
                .active(|el| el.opacity(0.7))
        })
}

thread_local! {
    /// Where the open tab sits this frame, so the rail can leave a hole for it.
    static TAB_CUT: Cell<Option<Bounds<Pixels>>> = const { Cell::new(None) };
    /// The layer the open tab is cut from, set while the rail is built.
    static TAB_SHEET: Cell<Option<Rgba>> = const { Cell::new(None) };
    /// Where the rail's islands end, above the gutter, as the rail last painted it.
    static RAIL_FLOOR: Cell<Option<Pixels>> = const { Cell::new(None) };
}

/// The fill under a chosen item, such as a segment, that fades in when the item is
/// chosen, with its own look. A child of the item, which must be `relative` and add it
/// first, under its content.
pub(crate) fn chosen_fill_of(key: impl Into<SharedString>, radius: f32, look: Div) -> AnyElement {
    look.absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .right_0()
        .rounded(px(radius))
        .animated(
            ElementId::Name(key.into()),
            Animation::new(Duration::from_millis(QUICK)).with_easing(ease_out_quint()),
            |el, d| el.opacity(d),
        )
}

/// The gap between the window's islands, in the frame's grey.
pub(crate) const GUTTER: f32 = 8.;
/// Radius of an island's rounded corners.
pub(crate) const ISLAND_R: f32 = 14.;

thread_local! {
    /// Where each fused strip's open tab sits this frame, and how far its light has grown
    /// out since it opened (see `accent::opening`), keyed by strip.
    static STRIP_CUTS: std::cell::RefCell<HashMap<&'static str, (Bounds<Pixels>, f32)>> =
        std::cell::RefCell::new(HashMap::new());
}

/// One rounded corner of an island: the corner of a square outside a quarter circle, in
/// the frame's grey, so the island reads as rounded over whatever it holds. The island's
/// own fill is rounded too, so the two never overlap in glass mode.
pub(crate) fn frame_corner(top: bool, left: bool, under: Rgba) -> AnyElement {
    // The frame as the difference over the island's own square fill (see
    // `settings::frame`), so along the curve the two blend instead of overlapping or
    // leaving a gap. The island must not round this corner itself.
    let fill = Hsla::from(settings::over(settings::frame(), under));
    let r = ISLAND_R;
    div()
        .absolute()
        .size(px(r))
        .map(|el| if top { el.top_0() } else { el.bottom_0() })
        .map(|el| if left { el.left_0() } else { el.right_0() })
        .child(
            canvas(
                |_, _, _| (),
                move |b, _, window, _| {
                    let k = 0.5523;
                    // Drawn as the top-left corner, then mirrored into place.
                    let p = |x: f32, y: f32| {
                        let x = if left { x } else { r - x };
                        let y = if top { y } else { r - y };
                        point(b.origin.x + px(x), b.origin.y + px(y))
                    };
                    let mut f = PathBuilder::fill();
                    f.move_to(p(0., 0.));
                    f.line_to(p(r, 0.));
                    f.cubic_bezier_to(p(0., r), p(r - k * r, 0.), p(0., r - k * r));
                    f.close();
                    if let Ok(path) = f.build() {
                        window.paint_path(path, fill);
                    }
                },
            )
            .size_full(),
        )
        .into_any_element()
}

/// An island on the frame: `content` on a fill with the given corners rounded, as
/// top-left, top-right, bottom-right and bottom-left. The fill is square; each rounded
/// corner is the frame painted over it (see `frame_corner`). A transparent fill leaves
/// the layers and their corners to the content, as on pages that paint their own list
/// beside the page.
pub(crate) fn island(content: impl IntoElement, fill: Rgba, round: [bool; 4]) -> Div {
    let [tl, tr, br, bl] = round;
    // A transparent fill leaves the corners to the content too, which knows what lies
    // under each; see `frame_corner`.
    let own = fill.a > 0.0;
    // The content's fill takes the content's pattern; any other fill is a side panel's.
    let area = if fill == settings::glass_fill(c_canvas()) {
        crate::pattern::Area::Content
    } else {
        crate::pattern::Area::Panels
    };
    div()
        .relative()
        .flex()
        .flex_col()
        .bg(fill)
        .when(own, |el| el.children(crate::pattern::surface(area)))
        .child(content)
        .when(own && tl, |el| el.child(frame_corner(true, true, fill)))
        .when(own && tr, |el| el.child(frame_corner(true, false, fill)))
        .when(own && br, |el| el.child(frame_corner(false, false, fill)))
        .when(own && bl, |el| el.child(frame_corner(false, true, fill)))
}

/// A strip of the frame's grey, such as the gutter between two islands.
pub(crate) fn gutter() -> Div {
    div()
        .flex_none()
        .bg(settings::frame())
        .relative()
        .children(crate::pattern::chrome())
}

thread_local! {
    /// Whether the pointer was over each hover-washed element when it last painted, and
    /// since when, keyed by id.
    static HOVERS: std::cell::RefCell<HashMap<ElementId, (bool, std::time::Instant)>> =
        std::cell::RefCell::new(HashMap::new());
}

/// How long a hover wash takes to ease in or out.
const HOVER_MS: f32 = 120.;

/// Give an element a hover fill that eases in and out instead of snapping: `color`
/// washes up under its content while the pointer is over it. Call it before adding the
/// element's content, so the wash sits beneath it, and keep `on_hover` free for it. The
/// element must be `relative`.
///
/// Whether the pointer is over it is read from the pointer itself as the wash paints,
/// so an element that went away while hovered never comes back lit.
pub(crate) fn hover_wash(
    el: Stateful<Div>,
    id: impl Into<ElementId>,
    color: Rgba,
    radius: f32,
) -> Stateful<Div> {
    let id: ElementId = id.into();
    let color = Hsla::from(color);
    let view = std::rc::Rc::new(Cell::new(None));
    let drawn_in = view.clone();
    el.on_hover(move |_, window, cx| redraw_view(view.get(), window, cx))
        .child(
            canvas(
                move |_, window, _| drawn_in.set(Some(window.current_view())),
                move |b, _, window, _| {
                    let over = b.contains(&window.mouse_position());
                    let now = std::time::Instant::now();
                    let since = HOVERS.with_borrow_mut(|h| {
                        // First seen: already settled, so nothing fades on arrival.
                        let seen = h
                            .entry(id.clone())
                            .or_insert((over, now - Duration::from_secs(1)));
                        if seen.0 != over {
                            *seen = (over, now);
                        }
                        seen.1.elapsed().as_secs_f32() * 1000.
                    });
                    let t = if anim_on() {
                        (since / HOVER_MS).min(1.)
                    } else {
                        1.
                    };
                    if t < 1. {
                        window.request_animation_frame();
                    }
                    let t = 1. - (1. - t).powi(3);
                    let strength = if over { t } else { 1. - t };
                    if strength > 0. && color.a > 0. {
                        window
                            .paint_quad(fill(b, color.opacity(strength)).corner_radii(px(radius)));
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
}

thread_local! {
    /// Whether the pointer is over each hover group that reveals actions, and since when.
    static GROUPS: std::cell::RefCell<HashMap<ElementId, (bool, std::time::Instant)>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Draw `view` again, the view a hovered element was drawn in, rather than the whole
/// window: the window's parts are cached views (see `panes`), and only the hovered one
/// changed. Before the element is first drawn, the window.
fn redraw_view(view: Option<EntityId>, window: &mut Window, cx: &mut App) {
    match view {
        Some(view) => cx.notify(view),
        None => window.refresh(),
    }
}
