//! The person's side of a job's browser: the page the bot uses, streamed into Squad as
//! it changes (DevTools screencast frames, decoded off the UI thread), with clicks,
//! scrolling and keys sent back. It attaches to the same page as the bot, so both see
//! and touch one page.

use super::cdp::{self, Cdp};
use gpui_kit::RenderImage;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Connecting,
    Live,
    Closed(String),
}

#[derive(Default)]
struct Shared {
    state: Option<State>,
    frame: Option<Arc<RenderImage>>,
    /// The CSS size of the page the frame shows.
    css: (f32, f32),
    url: String,
    loading: bool,
    session: String,
    cdp: Option<Arc<Cdp>>,
}

pub struct Viewer {
    shared: Arc<Mutex<Shared>>,
    /// Bumped on every new frame or page change, so the UI knows to redraw.
    pub version: Arc<AtomicU64>,
}

/// A point in the view (from its top-left) as page CSS pixels, when it's on the page.
/// The page is fitted into `area` keeping its shape, and centred.
pub fn to_page(local: (f32, f32), area: (f32, f32), css: (f32, f32)) -> Option<(f64, f64)> {
    let (pw, ph) = css;
    if pw <= 0. || ph <= 0. || area.0 <= 0. || area.1 <= 0. {
        return None;
    }
    let k = (area.0 / pw).min(area.1 / ph);
    let (ox, oy) = ((area.0 - pw * k) / 2., (area.1 - ph * k) / 2.);
    let (x, y) = ((local.0 - ox) / k, (local.1 - oy) / k);
    (x >= 0. && y >= 0. && x < pw && y < ph).then_some((x as f64, y as f64))
}

impl Viewer {
    /// Starts watching the browser at `port` (`path` is its `/devtools/browser/…`), and
    /// opens `url` there first when given.
    pub fn connect(port: u16, path: String, url: Option<String>) -> Viewer {
        let shared: Arc<Mutex<Shared>> = Arc::new(Mutex::new(Shared {
            state: Some(State::Connecting),
            ..Shared::default()
        }));
        let version = Arc::new(AtomicU64::new(0));
        let (s, v) = (shared.clone(), version.clone());
        std::thread::Builder::new()
            .name("squad-browser-view".into())
            .spawn(move || {
                if let Err(why) = watch(port, &path, url, &s, &v) {
                    let mut sh = s.lock().unwrap_or_else(|e| e.into_inner());
                    sh.state = Some(State::Closed(why));
                    v.fetch_add(1, Ordering::Relaxed);
                }
            })
            .ok();
        Viewer { shared, version }
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn state(&self) -> State {
        let sh = self.locked();
        match (&sh.state, &sh.cdp) {
            (Some(State::Live), Some(c)) if !c.alive() => {
                State::Closed("The browser closed.".into())
            }
            (Some(s), _) => s.clone(),
            (None, _) => State::Connecting,
        }
    }

    pub fn frame(&self) -> Option<(Arc<RenderImage>, (f32, f32))> {
        let sh = self.locked();
        sh.frame.clone().map(|f| (f, sh.css))
    }

    pub fn css(&self) -> (f32, f32) {
        self.locked().css
    }

    pub fn url(&self) -> String {
        self.locked().url.clone()
    }

    pub fn loading(&self) -> bool {
        self.locked().loading
    }

    fn send(&self, method: &str, params: Value) {
        let (cdp, session) = {
            let sh = self.locked();
            (sh.cdp.clone(), sh.session.clone())
        };
        if let Some(cdp) = cdp.filter(|_| !session.is_empty()) {
            cdp.send(method, params, &session);
        }
    }

    pub fn navigate(&self, url: &str) {
        self.send("Page.navigate", json!({"url": url}));
    }

    pub fn reload(&self) {
        self.send("Page.reload", json!({}));
    }

    pub fn history(&self, back: bool) {
        let js = if back {
            "history.back()"
        } else {
            "history.forward()"
        };
        self.send("Runtime.evaluate", json!({"expression": js}));
    }

    /// `kind` is mouseMoved, mousePressed or mouseReleased.
    pub fn mouse(
        &self,
        kind: &str,
        (x, y): (f64, f64),
        button: &str,
        clicks: usize,
        modifiers: u8,
    ) {
        let buttons = match (kind, button) {
            ("mousePressed", "left") => 1,
            ("mousePressed", "right") => 2,
            ("mousePressed", "middle") => 4,
            _ => 0,
        };
        self.send(
            "Input.dispatchMouseEvent",
            json!({"type": kind, "x": x, "y": y, "button": button, "buttons": buttons,
                "clickCount": clicks, "modifiers": modifiers}),
        );
    }

    pub fn wheel(&self, (x, y): (f64, f64), dx: f64, dy: f64) {
        self.send(
            "Input.dispatchMouseEvent",
            json!({"type": "mouseWheel", "x": x, "y": y, "deltaX": dx, "deltaY": dy}),
        );
    }

    pub fn insert_text(&self, text: &str) {
        self.send("Input.insertText", json!({"text": text}));
    }

    /// A key with no text of its own (Enter, arrows, Backspace…), down and up.
    pub fn key(&self, k: &crate::browser_engine::KeySpec, modifiers: u8, commands: &[&str]) {
        let mut down = json!({
            "type": if k.text.is_empty() { "rawKeyDown" } else { "keyDown" },
            "key": k.key, "code": k.code, "windowsVirtualKeyCode": k.vk,
            "nativeVirtualKeyCode": k.vk, "modifiers": modifiers,
        });
        if !k.text.is_empty() {
            down["text"] = json!(k.text);
            down["unmodifiedText"] = json!(k.text);
        }
        if !commands.is_empty() {
            down["commands"] = json!(commands);
        } else if !k.command.is_empty() {
            down["commands"] = json!([k.command]);
        }
        self.send("Input.dispatchKeyEvent", down);
        self.send(
            "Input.dispatchKeyEvent",
            json!({"type": "keyUp", "key": k.key, "code": k.code,
                "windowsVirtualKeyCode": k.vk, "nativeVirtualKeyCode": k.vk, "modifiers": modifiers}),
        );
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        let (cdp, session) = {
            let sh = self.locked();
            (sh.cdp.clone(), sh.session.clone())
        };
        if let Some(cdp) = cdp {
            if !session.is_empty() {
                cdp.send("Page.stopScreencast", json!({}), &session);
            }
            cdp.close();
        }
    }
}

/// Connects, attaches to the shared page and starts the stream. Events keep arriving
/// on the connection's reader thread after this returns.
fn watch(
    port: u16,
    path: &str,
    url: Option<String>,
    shared: &Arc<Mutex<Shared>>,
    version: &Arc<AtomicU64>,
) -> Result<(), String> {
    let me: Arc<Mutex<Weak<Cdp>>> = Arc::new(Mutex::new(Weak::new()));
    let (s, v, m) = (shared.clone(), version.clone(), me.clone());
    let listener = Box::new(move |method: &str, p: &Value, session: &str| {
        let lock = || s.lock().unwrap_or_else(|e| e.into_inner());
        let mine = lock().session.clone();
        let ours = !mine.is_empty() && mine == session;
        match method {
            "Page.screencastFrame" if ours => {
                if let Some(cdp) = m.lock().ok().and_then(|w| w.upgrade()) {
                    cdp.send(
                        "Page.screencastFrameAck",
                        json!({"sessionId": p["sessionId"]}),
                        session,
                    );
                }
                let Some(image) = p["data"]
                    .as_str()
                    .and_then(|d| crate::browser_engine::decode_image(d, image::ImageFormat::Jpeg))
                else {
                    return;
                };
                let css = (
                    p["metadata"]["deviceWidth"].as_f64().unwrap_or(0.) as f32,
                    p["metadata"]["deviceHeight"].as_f64().unwrap_or(0.) as f32,
                );
                let mut sh = lock();
                sh.frame = Some(image);
                sh.css = css;
                drop(sh);
                v.fetch_add(1, Ordering::Relaxed);
            }
            "Page.frameNavigated" if ours && p["frame"]["parentId"].is_null() => {
                lock().url = p["frame"]["url"].as_str().unwrap_or_default().to_string();
                v.fetch_add(1, Ordering::Relaxed);
            }
            "Page.navigatedWithinDocument" if ours => {
                lock().url = p["url"].as_str().unwrap_or_default().to_string();
                v.fetch_add(1, Ordering::Relaxed);
            }
            "Page.frameStartedLoading" if ours => {
                lock().loading = true;
                v.fetch_add(1, Ordering::Relaxed);
            }
            "Page.loadEventFired" | "Page.frameStoppedLoading" if ours => {
                lock().loading = false;
                v.fetch_add(1, Ordering::Relaxed);
            }
            "Target.detachedFromTarget" if !mine.is_empty() && p["sessionId"] == mine.as_str() => {
                lock().state = Some(State::Closed("The page closed.".into()));
                v.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    });
    let cdp = Cdp::connect(port, path, listener)?;
    if let Ok(mut w) = me.lock() {
        *w = Arc::downgrade(&cdp);
    }
    let target = cdp::the_page(&cdp)?;
    let info = cdp.call("Target.getTargetInfo", json!({"targetId": target}), "")?;
    let session = cdp::attach(&cdp, &target)?;
    {
        let mut sh = shared.lock().unwrap_or_else(|e| e.into_inner());
        sh.session = session.clone();
        sh.url = info["targetInfo"]["url"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        sh.cdp = Some(cdp.clone());
    }
    cdp.call("Page.enable", json!({}), &session)?;
    cdp.send(
        "Page.addScriptToEvaluateOnNewDocument",
        json!({"source": crate::browser_engine::IN_PLACE_JS}),
        &session,
    );
    cdp.send(
        "Emulation.setFocusEmulationEnabled",
        json!({"enabled": true}),
        &session,
    );
    cdp::fit(&cdp, &session);
    if let Some(url) = url {
        cdp.send("Page.navigate", json!({"url": url}), &session);
    }
    cdp.call(
        "Page.startScreencast",
        json!({"format": "jpeg", "quality": 75,
            "maxWidth": super::host::WIDTH * 2, "maxHeight": super::host::HEIGHT * 2}),
        &session,
    )?;
    shared.lock().unwrap_or_else(|e| e.into_inner()).state = Some(State::Live);
    version.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::to_page;

    #[test]
    fn points_land_on_the_fitted_page() {
        // A 1280×800 page in a 640×800 area: half size, 200 px above and below.
        assert_eq!(
            to_page((0., 200.), (640., 800.), (1280., 800.)),
            Some((0., 0.))
        );
        assert_eq!(
            to_page((320., 400.), (640., 800.), (1280., 800.)),
            Some((640., 400.))
        );
        assert_eq!(to_page((320., 100.), (640., 800.), (1280., 800.)), None);
        assert_eq!(to_page((1., 1.), (640., 800.), (0., 0.)), None);
    }
}

/// The bot and a viewer on one real page. Needs a Chromium-based browser and a free
/// port, so it runs only when asked: `cargo test -p runo shared_page -- --ignored`.
#[cfg(all(test, unix))]
mod live {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait(what: &str, ok: impl Fn() -> bool) {
        let start = Instant::now();
        while !ok() {
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "timed out: {what}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    #[ignore]
    fn shared_page() {
        let home = std::env::temp_dir().join(format!("squad-view-{}", std::process::id()));
        std::env::set_var("SQUADCODE_HOME", &home);
        let dir = home.join("job");
        std::fs::create_dir_all(&dir).unwrap();
        let page = dir.join("index.html");
        std::fs::write(
            &page,
            "<title>T</title><input id=q placeholder=Search><p id=o></p>\
             <script>q.oninput=()=>o.textContent='typed '+q.value</script>",
        )
        .unwrap();
        let d = dir.to_str().unwrap();
        let url = format!("file://{}", page.display());
        let opened = crate::browse::bot::with(d, |b| b.open(&url)).unwrap();
        assert!(opened.contains("[1] textbox \"Search\""), "{opened}");
        let e = crate::browse::host::ensure(d).unwrap();
        let v = Viewer::connect(e.port, e.path.clone(), None);
        wait("live", || v.state() == State::Live);
        wait("a frame", || v.frame().is_some());
        assert_eq!(v.css(), (1280., 800.));
        assert!(v.url().ends_with("index.html"));
        // The person clicks the field and types; the bot sees it.
        let at = crate::browse::bot::with(d, |b| b.eval_js(
            "JSON.stringify((r => [r.x + r.width / 2, r.y + r.height / 2])(q.getBoundingClientRect()))",
        ))
        .unwrap();
        let xy: Vec<f64> = serde_json::from_str(at.trim()).unwrap();
        v.mouse("mousePressed", (xy[0], xy[1]), "left", 1, 0);
        v.mouse("mouseReleased", (xy[0], xy[1]), "left", 1, 0);
        std::thread::sleep(Duration::from_millis(200));
        v.insert_text("hi");
        wait("the bot sees it", || {
            crate::browse::bot::with(d, |b| b.look("typed", "", 1000))
                .is_ok_and(|t| t.contains("typed hi"))
        });
        drop(v);
        crate::browse::host::close(d);
        let _ = std::fs::remove_dir_all(&home);
    }
}
