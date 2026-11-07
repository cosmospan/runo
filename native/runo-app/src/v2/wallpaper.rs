//! The window's background image, picked under Settings > Appearance: one of a few
//! that ship with the app, or a picture of the person's own.
//!
//! The image lies under the whole window. The page and the sidebar thin over it as
//! far as the image's "Show through" slider says, down to the theme's floor, the
//! same floor that keeps text readable over a busy desktop with window
//! transparency on (see `theme::amount`). A veil in the theme's ground colour dims
//! the picture on top of that, and a blur softens it.
//!
//! Pictures of your own are copied into `backgrounds/` in the data folder when
//! picked, so moving or deleting the original doesn't take the background away.
//! Blurring a photo is too slow to do every frame, so a blurred copy is made once,
//! off the main thread, and kept in `backgrounds/cache/` under a name made from the
//! picture and the blur, which also keeps the image cache from showing a stale one.
//! The built-in images are drawn by `paint_builtin` the first time they're needed,
//! so the app ships no image files for them.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Mutex;

use gpui_kit::*;

use super::theme;
use super::ui::{c, ca};
use super::Shell;

/// A background image that ships with the app: a soft wash of colour, drawn as a
/// vertical gradient with glows laid over it.
pub struct Builtin {
    pub id: &'static str,
    pub label: &'static str,
    /// The gradient, top to bottom.
    top: u32,
    bottom: u32,
    /// Glows: centre (0 to 1 across and down), radius (a share of the width) and
    /// colour.
    glows: &'static [(f32, f32, f32, u32)],
}

pub const BUILTINS: [Builtin; 6] = [
    Builtin {
        id: "aurora",
        label: "Aurora",
        top: 0x0B1026,
        bottom: 0x070A14,
        glows: &[
            (0.18, 0.30, 0.34, 0x22C7B0),
            (0.74, 0.22, 0.40, 0x7C4DFF),
            (0.55, 0.92, 0.46, 0x1D3A8A),
        ],
    },
    Builtin {
        id: "dusk",
        label: "Dusk",
        top: 0x2A1740,
        bottom: 0x14101F,
        glows: &[
            (0.82, 0.18, 0.40, 0xFF7E5F),
            (0.20, 0.82, 0.50, 0x6A3093),
            (0.52, 0.48, 0.28, 0xF4A261),
        ],
    },
    Builtin {
        id: "lagoon",
        label: "Lagoon",
        top: 0x031F30,
        bottom: 0x06374D,
        glows: &[
            (0.28, 0.72, 0.44, 0x02A9D9),
            (0.82, 0.30, 0.36, 0x1FAF6E),
            (0.08, 0.08, 0.30, 0x0B132B),
        ],
    },
    Builtin {
        id: "ember",
        label: "Ember",
        top: 0x1A0B0B,
        bottom: 0x120707,
        glows: &[
            (0.70, 0.78, 0.46, 0xE8590C),
            (0.24, 0.34, 0.34, 0x9F1D1D),
            (0.92, 0.10, 0.24, 0xF2B32A),
        ],
    },
    Builtin {
        id: "meadow",
        label: "Meadow",
        top: 0xF4F8EE,
        bottom: 0xE4EFDA,
        glows: &[
            (0.18, 0.20, 0.40, 0xBFE5A8),
            (0.82, 0.72, 0.46, 0x9FD3C0),
            (0.60, 0.08, 0.30, 0xFBE08A),
        ],
    },
    Builtin {
        id: "blush",
        label: "Blush",
        top: 0xFFF6F6,
        bottom: 0xF6F0FF,
        glows: &[
            (0.14, 0.82, 0.46, 0xF8C6E0),
            (0.86, 0.24, 0.40, 0xC4CEFC),
            (0.50, 0.50, 0.28, 0xFDEFC0),
        ],
    },
];

/// Where a background image comes from, as `AppSettings::v2_bg_image` names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Builtin(&'static Builtin),
    Own(PathBuf),
}

impl PartialEq for Builtin {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Builtin {}
impl std::fmt::Debug for Builtin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id)
    }
}

impl Source {
    /// The source a setting names; none for an empty one or an unknown built-in.
    pub fn parse(setting: &str) -> Option<Source> {
        let setting = setting.trim();
        if setting.is_empty() {
            return None;
        }
        if let Some(id) = setting.strip_prefix("builtin:") {
            return BUILTINS.iter().find(|b| b.id == id).map(Source::Builtin);
        }
        Some(Source::Own(PathBuf::from(setting)))
    }

    /// How the setting writes this source.
    pub fn setting(&self) -> String {
        match self {
            Source::Builtin(b) => format!("builtin:{}", b.id),
            Source::Own(p) => p.to_string_lossy().into_owned(),
        }
    }

    /// The full-size picture, drawing a built-in one first if it isn't on disk yet.
    fn original(&self) -> Result<PathBuf, String> {
        match self {
            Source::Builtin(b) => {
                let path = dir().join(format!("builtin-{}.jpg", b.id));
                if !path.exists() {
                    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
                    write_jpeg(&paint_builtin(b, 1920, 1200), &path)?;
                }
                Ok(path)
            }
            Source::Own(p) if p.is_file() => Ok(p.clone()),
            Source::Own(_) => Err("That picture is no longer there.".into()),
        }
    }
}

/// How the picture meets the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fit {
    /// Covers the window, cropping what doesn't fit.
    Fill,
    /// The whole picture, with the theme's ground in the bars beside it.
    Contain,
    /// Stretched to the window's shape.
    Stretch,
    /// At its own size, in the middle.
    Centre,
}

impl Fit {
    pub const ALL: [Fit; 4] = [Fit::Fill, Fit::Contain, Fit::Stretch, Fit::Centre];

    pub fn id(self) -> &'static str {
        match self {
            Fit::Fill => "fill",
            Fit::Contain => "fit",
            Fit::Stretch => "stretch",
            Fit::Centre => "centre",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Fit::Fill => "Fill",
            Fit::Contain => "Fit",
            Fit::Stretch => "Stretch",
            Fit::Centre => "Centre",
        }
    }

    pub fn parse(id: &str) -> Fit {
        Fit::ALL
            .into_iter()
            .find(|f| f.id() == id)
            .unwrap_or(Fit::Fill)
    }

    fn object_fit(self) -> ObjectFit {
        match self {
            Fit::Fill => ObjectFit::Cover,
            Fit::Contain => ObjectFit::Contain,
            Fit::Stretch => ObjectFit::Fill,
            Fit::Centre => ObjectFit::None,
        }
    }
}

/// The picture on screen now, blurred as asked, once it's ready.
static SHOWN: Mutex<Option<PathBuf>> = Mutex::new(None);
static FIT: AtomicU8 = AtomicU8::new(0);
static DIM: AtomicU8 = AtomicU8::new(15);
/// Counts the pictures asked for, so a slow one finishing late doesn't replace the
/// one picked after it.
static ASKED: AtomicU64 = AtomicU64::new(0);
/// The previews under Settings are being made.
static PREVIEWING: AtomicBool = AtomicBool::new(false);

/// Where background images are kept.
pub fn dir() -> PathBuf {
    crate::store::data_dir().join("backgrounds")
}

fn cache_dir() -> PathBuf {
    dir().join("cache")
}

/// Sets how the image meets the window and how far it's dimmed (0 to 100). Both
/// take effect on the next frame, with no new copy to make.
pub fn set_look(fit: Fit, dim: u8) {
    FIT.store(
        Fit::ALL.iter().position(|f| *f == fit).unwrap_or(0) as u8,
        Ordering::Relaxed,
    );
    DIM.store(dim.min(100), Ordering::Relaxed);
}

pub fn fit() -> Fit {
    Fit::ALL[FIT.load(Ordering::Relaxed) as usize % Fit::ALL.len()]
}

/// The layer under the whole window while an image is set: the theme's ground,
/// opaque, then the picture once it's ready, then the veil that dims it.
pub fn layer() -> Option<AnyElement> {
    if !theme::image_on() {
        return None;
    }
    let t = theme::theme();
    let ground = t.bg & 0xff_ffff;
    let shown = SHOWN.lock().ok().and_then(|s| s.clone());
    // At most 85% of the way into the ground, so the picture never quite goes.
    let veil = DIM.load(Ordering::Relaxed).min(100) as f32 / 100. * 0.85;
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .overflow_hidden()
            .bg(c(ground))
            .children(shown.map(|path| {
                img(path)
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .object_fit(fit().object_fit())
            }))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(ca(ground, veil)),
            )
            .into_any_element(),
    )
}

/// What a picture needs before it can be shown: where it is and how blurred.
struct Want {
    source: Source,
    blur: u8,
    asked: u64,
}

/// Puts the image the settings name under the window: at once when its copy is
/// ready, else once it's made, off the main thread. A picture that can't be opened
/// is taken off again, with a note saying so.
pub fn refresh(cx: &mut Context<Shell>) {
    let st = cx.global::<crate::settings::AppSettings>();
    let source = Source::parse(&st.v2_bg_image);
    let (blur, dim, show, fit) = (
        st.v2_bg_blur.min(100),
        st.v2_bg_dim,
        st.v2_bg_show,
        Fit::parse(&st.v2_bg_fit),
    );
    set_look(fit, dim);
    let asked = ASKED.fetch_add(1, Ordering::Relaxed) + 1;
    let Some(source) = source else {
        theme::set_image(false, show);
        set_shown(None);
        cx.notify();
        return;
    };
    theme::set_image(true, show);
    let want = Want {
        source,
        blur,
        asked,
    };
    if let Some(ready) = ready(&want) {
        set_shown(Some(ready));
        cx.notify();
        return;
    }
    let job = cx.background_executor().spawn(async move {
        let made = make(&want);
        (want.asked, made)
    });
    cx.spawn(async move |this, cx| {
        let (asked, made) = job.await;
        if ASKED.load(Ordering::Relaxed) != asked {
            return;
        }
        let _ = this.update(cx, |this, cx| {
            match made {
                Ok(path) => set_shown(Some(path)),
                Err(why) => {
                    theme::set_image(false, 0);
                    set_shown(None);
                    cx.update_global::<crate::settings::AppSettings, _>(|st, _| {
                        st.v2_bg_image.clear();
                        st.save();
                    });
                    this.say(format!("The background image couldn't be shown. {why}"));
                }
            }
            cx.notify();
        });
    })
    .detach();
}

fn set_shown(path: Option<PathBuf>) {
    if let Ok(mut s) = SHOWN.lock() {
        *s = path;
    }
}

/// The copy to show for `want`, if there's nothing to make: an unblurred picture
/// small enough to show as it is, or a copy made before.
fn ready(want: &Want) -> Option<PathBuf> {
    if let Source::Builtin(b) = &want.source {
        if want.blur == 0 {
            let path = dir().join(format!("builtin-{}.jpg", b.id));
            return path.exists().then_some(path);
        }
    }
    let original = match &want.source {
        Source::Own(p) => p.clone(),
        Source::Builtin(b) => dir().join(format!("builtin-{}.jpg", b.id)),
    };
    if want.blur == 0 && is_small(&original) {
        return Some(original);
    }
    let copy = cache_dir().join(copy_name(&original, want.blur)?);
    copy.exists().then_some(copy)
}

/// The longest side a copy is made at: about the width of a large window. A blur
/// hides detail anyway, so a blurred copy is made smaller, which is far quicker.
const SHARP_SIDE: u32 = 3200;
const BLURRED_SIDE: u32 = 1280;

/// A PNG or JPEG small enough to show as it is. Anything else gets a JPEG copy, so
/// the window only ever reads the two formats every platform's image cache knows.
fn is_small(path: &Path) -> bool {
    let plain = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ["png", "jpg", "jpeg"].contains(&e.to_ascii_lowercase().as_str()));
    plain && image::image_dimensions(path).is_ok_and(|(w, h)| w.max(h) <= SHARP_SIDE)
}

/// The copy's file name: the picture (where it is, when it changed, how big it is)
/// and the blur, hashed.
fn copy_name(original: &Path, blur: u8) -> Option<String> {
    let meta = std::fs::metadata(original).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    original.hash(&mut h);
    meta.len().hash(&mut h);
    meta.modified().ok().hash(&mut h);
    blur.hash(&mut h);
    Some(format!("{:016x}.jpg", h.finish()))
}

/// Makes the copy `want` needs. Slow: off the main thread only.
fn make(want: &Want) -> Result<PathBuf, String> {
    let original = want.source.original()?;
    if let Some(ready) = ready(want) {
        return Ok(ready);
    }
    let name = copy_name(&original, want.blur).ok_or("It couldn't be read.")?;
    let picture = open(&original)?;
    let side = if want.blur == 0 {
        SHARP_SIDE
    } else {
        BLURRED_SIDE
    };
    let mut picture = if picture.width().max(picture.height()) > side {
        picture.resize(side, side, image::imageops::FilterType::Triangle)
    } else {
        picture
    };
    if want.blur > 0 {
        // The slider's far end blurs about as much as a frosted window does.
        let long = picture.width().max(picture.height()) as f32;
        let sigma = want.blur as f32 / 100. * 28. * long / BLURRED_SIDE as f32;
        picture = picture.fast_blur(sigma.max(0.5));
    }
    std::fs::create_dir_all(cache_dir()).map_err(|e| e.to_string())?;
    let copy = cache_dir().join(name);
    write_jpeg(&picture, &copy)?;
    Ok(copy)
}

/// Opens a picture the right way up (phones save photos sideways with a note to
/// turn them).
fn open(path: &Path) -> Result<image::DynamicImage, String> {
    use image::ImageDecoder;
    let unreadable = |_| "It isn't a picture Squad Code can open (PNG, JPEG or WebP).".to_string();
    let mut decoder = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(unreadable)?;
    let orientation = decoder.orientation().ok();
    let mut picture = image::DynamicImage::from_decoder(decoder).map_err(unreadable)?;
    if let Some(o) = orientation {
        picture.apply_orientation(o);
    }
    Ok(picture)
}

fn write_jpeg(picture: &image::DynamicImage, path: &Path) -> Result<(), String> {
    // Written beside, then moved in: the window never reads half a file.
    let part = path.with_extension("part");
    let file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut out = std::io::BufWriter::new(file);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
        .encode_image(&picture.to_rgb8())
        .map_err(|e| e.to_string())?;
    drop(out);
    std::fs::rename(&part, path).map_err(|e| e.to_string())
}

/// Copies a picture the person picked into the data folder, so it stays when the
/// original moves, and returns where it went. Checks it opens first. Slow: off the
/// main thread only.
pub fn add(from: &Path) -> Result<PathBuf, String> {
    let picture = open(from)?;
    if picture.width() < 2 || picture.height() < 2 {
        return Err("It's too small to use.".into());
    }
    let bytes = std::fs::read(from).map_err(|e| e.to_string())?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    let ext = from
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| ["png", "jpg", "jpeg", "webp"].contains(&e.as_str()))
        .unwrap_or_else(|| "img".into());
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let to = dir().join(format!("own-{:016x}.{ext}", h.finish()));
    if !to.exists() {
        std::fs::write(&to, &bytes).map_err(|e| e.to_string())?;
    }
    // Its preview, while the picture is open anyway.
    let _ = write_jpeg(&preview_of(&picture), &preview_path(&to));
    Ok(to)
}

/// Pictures of the person's own, newest first.
pub fn mine() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir()) else {
        return Vec::new();
    };
    let mut own: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("own-"))
        .filter(|e| e.path().extension().is_some_and(|x| x != "part"))
        .map(|e| {
            let when = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (when, e.path())
        })
        .collect();
    own.sort_by_key(|e| std::cmp::Reverse(e.0));
    own.into_iter().map(|(_, p)| p).collect()
}

/// Forgets a picture of the person's own, with its preview and copies.
pub fn remove(path: &Path) {
    if !path.starts_with(dir()) {
        return;
    }
    let _ = std::fs::remove_file(preview_path(path));
    for blur in 0..=100u8 {
        if let Some(name) = copy_name(path, blur) {
            let _ = std::fs::remove_file(cache_dir().join(name));
        }
    }
    let _ = std::fs::remove_file(path);
}

/// The small copy the Settings tiles show, so a large photo isn't opened at full
/// size just for a preview.
fn preview_path(original: &Path) -> PathBuf {
    let stem = original
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    cache_dir().join(format!("preview-{stem}.jpg"))
}

fn preview_of(picture: &image::DynamicImage) -> image::DynamicImage {
    picture.resize_to_fill(320, 200, image::imageops::FilterType::Triangle)
}

/// The preview for a source, if it's been made.
pub fn preview(source: &Source) -> Option<PathBuf> {
    let original = match source {
        Source::Builtin(b) => dir().join(format!("builtin-{}.jpg", b.id)),
        Source::Own(p) => p.clone(),
    };
    let path = preview_path(&original);
    path.exists().then_some(path)
}

/// Makes the previews Settings is missing (the built-in pictures the first time,
/// and any of your own whose preview went), then redraws. Once at a time.
pub fn make_previews(cx: &mut Context<Shell>) {
    let missing = BUILTINS
        .iter()
        .map(Source::Builtin)
        .chain(mine().into_iter().map(Source::Own))
        .any(|s| preview(&s).is_none());
    if !missing || PREVIEWING.swap(true, Ordering::Relaxed) {
        return;
    }
    let job = cx.background_executor().spawn(async move {
        let _ = std::fs::create_dir_all(cache_dir());
        for source in BUILTINS
            .iter()
            .map(Source::Builtin)
            .chain(mine().into_iter().map(Source::Own))
        {
            if preview(&source).is_some() {
                continue;
            }
            let Ok(original) = source.original() else {
                continue;
            };
            if let Ok(picture) = open(&original) {
                let _ = write_jpeg(&preview_of(&picture), &preview_path(&original));
            }
        }
    });
    cx.spawn(async move |this, cx| {
        job.await;
        PREVIEWING.store(false, Ordering::Relaxed);
        let _ = this.update(cx, |_, cx| cx.notify());
    })
    .detach();
}

/// Draws a built-in picture at `w` by `h`: its gradient, its glows laid over in
/// turn, and a faint grain so the gradient doesn't band once it's a JPEG.
pub fn paint_builtin(b: &Builtin, w: u32, h: u32) -> image::DynamicImage {
    let split = |c: u32| {
        [
            ((c >> 16) & 0xff) as f32,
            ((c >> 8) & 0xff) as f32,
            (c & 0xff) as f32,
        ]
    };
    let (top, bottom) = (split(b.top), split(b.bottom));
    let glows: Vec<(f32, f32, f32, [f32; 3])> = b
        .glows
        .iter()
        .map(|&(x, y, r, c)| (x * w as f32, y * h as f32, r * w as f32, split(c)))
        .collect();
    let mut out = image::RgbImage::new(w, h);
    for (x, y, px) in out.enumerate_pixels_mut() {
        let (fx, fy) = (x as f32, y as f32);
        let t = fy / (h.max(2) - 1) as f32;
        let mut rgb = [0.; 3];
        for i in 0..3 {
            rgb[i] = top[i] + (bottom[i] - top[i]) * t;
        }
        for &(gx, gy, r, col) in &glows {
            let d2 = (fx - gx).powi(2) + (fy - gy).powi(2);
            let k = (-d2 / (2. * r * r)).exp() * 0.85;
            for i in 0..3 {
                rgb[i] += (col[i] - rgb[i]) * k;
            }
        }
        // A small, fixed grain from the pixel's place.
        let mut n = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (y as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        n ^= n >> 31;
        let grain = (n & 0xff) as f32 / 255. * 3. - 1.5;
        *px = image::Rgb(rgb.map(|v| (v + grain).round().clamp(0., 255.) as u8));
    }
    image::DynamicImage::ImageRgb8(out)
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that brings in gpui's own `test` attribute.
    use super::{paint_builtin, Fit, Source, BUILTINS};
    use std::path::PathBuf;

    #[test]
    fn settings_round_trip() {
        assert_eq!(Source::parse(""), None);
        assert_eq!(Source::parse("builtin:nope"), None);
        for b in &BUILTINS {
            let s = Source::Builtin(b);
            assert_eq!(Source::parse(&s.setting()), Some(s));
        }
        let own = Source::Own(PathBuf::from("/tmp/a picture.png"));
        assert_eq!(Source::parse(&own.setting()), Some(own));
        for f in Fit::ALL {
            assert_eq!(Fit::parse(f.id()), f);
        }
        assert_eq!(Fit::parse("tile"), Fit::Fill);
    }

    #[test]
    fn builtins_are_drawn_with_their_colours() {
        for b in &BUILTINS {
            let p = paint_builtin(b, 64, 40).to_rgb8();
            assert_eq!(p.dimensions(), (64, 40));
            // Not one flat colour: the glows show.
            let first = p.get_pixel(0, 0);
            assert!(p
                .pixels()
                .any(|q| q.0.iter().zip(first.0).any(|(a, b)| a.abs_diff(b) > 20)));
        }
    }
}
