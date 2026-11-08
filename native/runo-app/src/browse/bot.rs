//! The bot's side of the job's browser: what Squad's MCP tools do. The MCP server keeps
//! one connection to the job's browser for the length of the turn, so it hears the
//! page's errors and failed requests between calls and can say what each step changed.
//!
//! A page is read as an outline of text lines (see `page.js`) rather than a picture,
//! and after a step only the lines that changed come back: a few hundred tokens where a
//! screenshot costs a thousand or more.

use super::cdp::{self, Cdp};
use super::host;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

const PAGE_JS: &str = include_str!("page.js");

/// The most of an outline sent at once, in characters (about 1,500 tokens).
pub const MAX_OUTLINE: usize = 6000;
/// The most of a "what changed" reply.
const MAX_CHANGES: usize = 2500;
/// Errors kept between calls, and told at most per reply.
const MAX_ERRORS: usize = 12;

/// What the page did between calls.
#[derive(Default)]
struct Log {
    errors: Vec<String>,
    /// The main document's last HTTP status and URL.
    status: Option<(u16, String)>,
    /// Requests by id, to name the ones that fail.
    requests: HashMap<String, String>,
}

impl Log {
    fn error(&mut self, e: String) {
        if !self.errors.contains(&e) {
            self.errors.push(e);
        }
        if self.errors.len() > MAX_ERRORS * 2 {
            self.errors.drain(..MAX_ERRORS);
        }
    }
}

/// A page's outline as `page.js` reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    pub title: String,
    pub url: String,
    pub height: u64,
    pub scrolled: u64,
    pub dialog: String,
    pub lines: Vec<String>,
}

impl Outline {
    fn from(v: &Value) -> Outline {
        Outline {
            title: v["title"].as_str().unwrap_or_default().to_string(),
            url: v["url"].as_str().unwrap_or_default().to_string(),
            height: v["height"].as_u64().unwrap_or(0),
            scrolled: v["scrolled"].as_u64().unwrap_or(0),
            dialog: v["dialog"].as_str().unwrap_or_default().to_string(),
            lines: v["lines"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|l| l.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// The top of a reply: where the page is.
    pub fn head(&self) -> String {
        let mut out = format!(
            "Page: {}\nURL: {}",
            if self.title.is_empty() {
                "(no title)"
            } else {
                &self.title
            },
            self.url
        );
        let screens = self.height.div_ceil(host::HEIGHT as u64);
        if screens > 1 {
            out.push_str(&format!(
                "\n{screens} screens tall, scrolled to {}px",
                self.scrolled
            ));
        }
        if !self.dialog.is_empty() {
            out.push_str(&format!("\nOpen dialog: {}", self.dialog));
        }
        out
    }

    /// The lines, the ones with `find` in them when it's set, cut at `max` characters.
    pub fn body(&self, find: &str, max: usize) -> String {
        let find = find.trim().to_lowercase();
        let lines: Vec<&String> = self
            .lines
            .iter()
            .filter(|l| find.is_empty() || l.to_lowercase().contains(&find))
            .collect();
        if lines.is_empty() {
            return if find.is_empty() {
                "(the page is empty)".into()
            } else {
                format!("Nothing on the page mentions \"{find}\".")
            };
        }
        cut(&lines, max)
    }
}

/// Lines joined until `max` characters, then how many were left out.
fn cut(lines: &[&String], max: usize) -> String {
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        if out.len() + l.len() + 1 > max {
            out.push_str(&format!(
                "… {} more lines. Pass find to browser_look to search them.",
                lines.len() - i
            ));
            return out;
        }
        out.push_str(l);
        out.push('\n');
    }
    out.trim_end().to_string()
}

/// What changed between two outlines of the same page: the new lines, and how many went.
pub fn changes(before: &[String], after: &[String]) -> String {
    let old: HashSet<&String> = before.iter().collect();
    let new: HashSet<&String> = after.iter().collect();
    let added: Vec<&String> = after.iter().filter(|l| !old.contains(l)).collect();
    let gone = before.iter().filter(|l| !new.contains(l)).count();
    if added.is_empty() && gone == 0 {
        return "Nothing on the page changed.".into();
    }
    let mut out = String::from("What changed on the page");
    if gone > 0 {
        out.push_str(&format!(" ({gone} lines went away)"));
    }
    if added.is_empty() {
        out.push('.');
    } else {
        out.push_str(":\n");
        out.push_str(&cut(&added, MAX_CHANGES));
    }
    out
}

/// A key a bot can press: its CDP key, code, key code and the text it types.
pub fn key_of(name: &str) -> Option<(&'static str, &'static str, u32, &'static str)> {
    let n = name
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '_', '-'], "");
    Some(match n.as_str() {
        "enter" | "return" => ("Enter", "Enter", 13, "\r"),
        "tab" => ("Tab", "Tab", 9, ""),
        "escape" | "esc" => ("Escape", "Escape", 27, ""),
        "backspace" => ("Backspace", "Backspace", 8, ""),
        "delete" => ("Delete", "Delete", 46, ""),
        "arrowup" | "up" => ("ArrowUp", "ArrowUp", 38, ""),
        "arrowdown" | "down" => ("ArrowDown", "ArrowDown", 40, ""),
        "arrowleft" | "left" => ("ArrowLeft", "ArrowLeft", 37, ""),
        "arrowright" | "right" => ("ArrowRight", "ArrowRight", 39, ""),
        "home" => ("Home", "Home", 36, ""),
        "end" => ("End", "End", 35, ""),
        "pageup" => ("PageUp", "PageUp", 33, ""),
        "pagedown" => ("PageDown", "PageDown", 34, ""),
        "space" => (" ", "Space", 32, " "),
        _ => return None,
    })
}

/// The URL a bot gave, with `localhost:3000` and `:5173` read as local addresses.
pub fn url_of(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.starts_with("file://") || raw.starts_with("about:") || raw.starts_with("data:") {
        return Some(raw.to_string());
    }
    crate::browser::normalize_url(raw)
}

pub struct Driver {
    dir: String,
    cdp: Arc<Cdp>,
    session: String,
    log: Arc<Mutex<Log>>,
    last: Option<Outline>,
}

impl Driver {
    /// Connects to the job's browser in `dir`, starting it when it isn't running.
    pub fn connect(dir: &str) -> Result<Driver, String> {
        let ep = host::ensure(dir)?;
        let log: Arc<Mutex<Log>> = Arc::default();
        let me: Arc<Mutex<Weak<Cdp>>> = Arc::new(Mutex::new(Weak::new()));
        let (l, m) = (log.clone(), me.clone());
        let listener = Box::new(move |method: &str, p: &Value, session: &str| {
            let mut log = l.lock().unwrap_or_else(|e| e.into_inner());
            // Failed requests are told once, from the network events below.
            if method == "Log.entryAdded" && p["entry"]["source"] == "network" {
                return;
            }
            if let Some(e) = crate::browser_engine::console_error(method, p) {
                log.error(e);
                return;
            }
            match method {
                "Network.requestWillBeSent" => {
                    if log.requests.len() > 2000 {
                        log.requests.clear();
                    }
                    let id = p["requestId"].as_str().unwrap_or_default().to_string();
                    let url = p["request"]["url"].as_str().unwrap_or_default().to_string();
                    log.requests.insert(id, url);
                }
                "Network.responseReceived" => {
                    let status = p["response"]["status"].as_u64().unwrap_or(0) as u16;
                    let url = p["response"]["url"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    let kind = p["type"].as_str().unwrap_or_default();
                    if kind == "Document" {
                        log.status = Some((status, url.clone()));
                    }
                    if status >= 400 && kind != "Document" && !url.ends_with("/favicon.ico") {
                        log.error(format!("{status} from {url}"));
                    }
                }
                "Network.loadingFailed" => {
                    let why = p["errorText"].as_str().unwrap_or("failed");
                    if !p["canceled"].as_bool().unwrap_or(false) && why != "net::ERR_ABORTED" {
                        let id = p["requestId"].as_str().unwrap_or_default();
                        let url = log.requests.get(id).cloned().unwrap_or_default();
                        log.error(format!("{why} loading {url}"));
                    }
                }
                // A bot can't see alert() and confirm(): say what they said and accept.
                "Page.javascriptDialogOpening" => {
                    let kind = p["type"].as_str().unwrap_or("alert");
                    let text = p["message"].as_str().unwrap_or_default();
                    log.error(format!(
                        "The page showed a {kind}: \"{text}\" (Squad said OK)"
                    ));
                    drop(log);
                    if let Some(cdp) = m.lock().ok().and_then(|w| w.upgrade()) {
                        cdp.send(
                            "Page.handleJavaScriptDialog",
                            json!({"accept": true}),
                            session,
                        );
                    }
                }
                _ => {}
            }
        });
        let cdp = Cdp::connect(ep.port, &ep.path, listener)?;
        if let Ok(mut w) = me.lock() {
            *w = Arc::downgrade(&cdp);
        }
        let target = cdp::the_page(&cdp)?;
        let session = cdp::attach(&cdp, &target)?;
        for method in [
            "Page.enable",
            "Runtime.enable",
            "Log.enable",
            "Network.enable",
        ] {
            cdp.call(method, json!({}), &session)?;
        }
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
        Ok(Driver {
            dir: dir.to_string(),
            cdp,
            session,
            log,
            last: None,
        })
    }

    fn alive(&self) -> bool {
        self.cdp.alive()
    }

    fn eval(&self, js: &str) -> Result<Value, String> {
        let r = self.cdp.call(
            "Runtime.evaluate",
            json!({"expression": js, "returnByValue": true, "awaitPromise": true}),
            &self.session,
        )?;
        if let Some(ex) = r.get("exceptionDetails") {
            let text = ex["exception"]["description"]
                .as_str()
                .or_else(|| ex["text"].as_str())
                .unwrap_or("it threw");
            let first = text.lines().next().unwrap_or(text);
            return Err(first.trim_start_matches("Error: ").to_string());
        }
        Ok(r["result"]["value"].clone())
    }

    /// Runs one of `page.js`'s calls with `args`.
    fn page(&self, op: &str, args: Value) -> Result<Value, String> {
        self.eval(&format!(
            "(() => {{ const A = {args}; const OP = {op:?};\n{PAGE_JS}\n}})()"
        ))
    }

    /// Waits for the page to finish loading after a step, up to a few seconds.
    fn settle(&self) {
        std::thread::sleep(Duration::from_millis(250));
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(8) {
            if self
                .eval("document.readyState")
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                == Some("complete".into())
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(Duration::from_millis(150));
    }

    fn outline(&self) -> Result<Outline, String> {
        self.page("outline", json!({})).map(|v| Outline::from(&v))
    }

    fn errors(&self) -> String {
        let mut log = self.log.lock().unwrap_or_else(|e| e.into_inner());
        if log.errors.is_empty() {
            return String::new();
        }
        let all = std::mem::take(&mut log.errors);
        let mut out = String::from("\n\nErrors from the page:");
        for e in all.iter().take(MAX_ERRORS) {
            let e: String = e.chars().take(300).collect();
            out.push_str(&format!("\n- {e}"));
        }
        if all.len() > MAX_ERRORS {
            out.push_str(&format!("\n- … and {} more", all.len() - MAX_ERRORS));
        }
        out
    }

    /// The reply after a step: the whole outline on a new page, else what changed.
    fn report(&mut self, done: &str) -> Result<String, String> {
        let now = self.outline()?;
        let mut out = format!("{done}\n\n{}\n\n", now.head());
        match &self.last {
            Some(before) if before.url == now.url => {
                out.push_str(&changes(&before.lines, &now.lines))
            }
            _ => out.push_str(&now.body("", MAX_OUTLINE)),
        }
        out.push_str(&self.errors());
        self.last = Some(now);
        Ok(out)
    }

    pub fn open(&mut self, url: &str) -> Result<String, String> {
        let url = url_of(url).ok_or_else(|| format!("\"{url}\" isn't an address."))?;
        {
            // What the last page logged is no news about this one.
            let mut log = self.log.lock().unwrap_or_else(|e| e.into_inner());
            log.status = None;
            log.errors.clear();
        }
        let r = self.cdp.call_for(
            "Page.navigate",
            json!({"url": url}),
            &self.session,
            Duration::from_secs(30),
        )?;
        if let Some(why) = r["errorText"].as_str().filter(|s| !s.is_empty()) {
            let hint = if why.contains("CONNECTION_REFUSED") {
                " Nothing is listening there: start the server first (in the background), then open it again."
            } else {
                ""
            };
            return Err(format!("Couldn't load {url}: {why}.{hint}"));
        }
        self.settle();
        self.last = None;
        let status = self
            .log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status
            .clone();
        let done = match status {
            Some((code, _)) if code >= 400 => {
                format!("Opened {url}, but the server answered {code}.")
            }
            _ => format!("Opened {url}."),
        };
        self.report(&done)
    }

    pub fn look(&mut self, find: &str, wait_for: &str, max: usize) -> Result<String, String> {
        if !wait_for.trim().is_empty() {
            let want = wait_for.trim().to_lowercase();
            let start = Instant::now();
            loop {
                let o = self.outline()?;
                if o.lines.iter().any(|l| l.to_lowercase().contains(&want)) {
                    break;
                }
                if start.elapsed() > Duration::from_secs(15) {
                    return Err(format!(
                        "\"{wait_for}\" didn't show up within 15 seconds.\n\n{}\n\n{}{}",
                        o.head(),
                        o.body("", max),
                        self.errors()
                    ));
                }
                std::thread::sleep(Duration::from_millis(300));
            }
        }
        let now = self.outline()?;
        let out = format!(
            "{}\n\n{}{}",
            now.head(),
            now.body(find, max.clamp(500, 40_000)),
            self.errors()
        );
        self.last = Some(now);
        Ok(out)
    }

    fn mouse(&self, kind: &str, x: f64, y: f64, clicks: u32) -> Result<(), String> {
        self.cdp
            .call(
                "Input.dispatchMouseEvent",
                json!({"type": kind, "x": x, "y": y, "button": if kind == "mouseMoved" { "none" } else { "left" },
                    "buttons": if kind == "mousePressed" { 1 } else { 0 }, "clickCount": clicks}),
                &self.session,
            )
            .map(|_| ())
    }

    /// Clicks the middle of an element, the way a person would.
    fn click_at(&self, target: &Value) -> Result<Value, String> {
        let aim = self.page("aim", target.clone())?;
        let (x, y) = (
            aim["x"].as_f64().unwrap_or(0.),
            aim["y"].as_f64().unwrap_or(0.),
        );
        self.mouse("mouseMoved", x, y, 0)?;
        self.mouse("mousePressed", x, y, 1)?;
        self.mouse("mouseReleased", x, y, 1)?;
        Ok(aim)
    }

    pub fn click(&mut self, target: &Value) -> Result<String, String> {
        let aim = self.click_at(target)?;
        self.settle();
        let mut done = format!("Clicked {}.", short(aim["what"].as_str().unwrap_or("it")));
        if let Some(over) = aim["covered"].as_str().filter(|s| !s.is_empty()) {
            done.push_str(&format!(" Something was on top of it: {over}."));
        }
        self.report(&done)
    }

    fn key(&self, name: &str) -> Result<(), String> {
        let (key, code, vk, text) =
            key_of(name).ok_or_else(|| format!("Squad doesn't know the key \"{name}\"."))?;
        let mut down = json!({"type": if text.is_empty() { "rawKeyDown" } else { "keyDown" },
            "key": key, "code": code, "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk});
        if !text.is_empty() {
            down["text"] = json!(text);
            down["unmodifiedText"] = json!(text);
        }
        self.cdp
            .call("Input.dispatchKeyEvent", down, &self.session)?;
        self.cdp.call(
            "Input.dispatchKeyEvent",
            json!({"type": "keyUp", "key": key, "code": code, "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk}),
            &self.session,
        )?;
        Ok(())
    }

    pub fn press(&mut self, name: &str) -> Result<String, String> {
        self.key(name)?;
        self.settle();
        self.report(&format!("Pressed {name}."))
    }

    pub fn type_in(&mut self, target: &Value, text: &str, submit: bool) -> Result<String, String> {
        let probe = self.page("ready", target.clone())?;
        let done = if probe["tag"] == "SELECT" {
            let mut args = target.clone();
            args["value"] = json!(text);
            let r = self.page("choose", args)?;
            format!(
                "Chose \"{text}\" in {}.",
                short(r["what"].as_str().unwrap_or("it"))
            )
        } else {
            // Clicked first, as a person would, then whatever is there replaced.
            self.click_at(target)?;
            self.page("ready", target.clone())?;
            if text.is_empty() {
                self.key("Backspace")?;
            } else {
                self.cdp
                    .call("Input.insertText", json!({"text": text}), &self.session)?;
            }
            format!(
                "Typed into {}.",
                short(probe["what"].as_str().unwrap_or("it"))
            )
        };
        if submit {
            self.key("Enter")?;
        }
        self.settle();
        self.report(&done)
    }

    pub fn back(&mut self) -> Result<String, String> {
        self.eval("history.back()")?;
        self.settle();
        self.report("Went back.")
    }

    pub fn eval_js(&mut self, js: &str) -> Result<String, String> {
        let v = self.eval(js)?;
        let text = match &v {
            Value::String(s) => s.clone(),
            Value::Null => "(nothing)".into(),
            other => serde_json::to_string_pretty(other).unwrap_or_default(),
        };
        let mut text: String = text.chars().take(4000).collect();
        text.push_str(&self.errors());
        Ok(text)
    }

    /// A small JPEG of the page: what's on screen, or the whole page when `full`.
    pub fn screenshot(&mut self, full: bool) -> Result<(String, String), String> {
        let mut params = json!({"format": "jpeg", "quality": 70});
        if full {
            let o = self.outline()?;
            params["captureBeyondViewport"] = json!(true);
            params["clip"] = json!({"x": 0, "y": 0, "width": host::WIDTH,
                "height": o.height.clamp(1, 6000), "scale": 1});
        }
        let r = self.cdp.call_for(
            "Page.captureScreenshot",
            params,
            &self.session,
            Duration::from_secs(30),
        )?;
        let data = r["data"].as_str().ok_or("the browser sent no picture")?;
        let small = shrink(data, 800).unwrap_or_else(|| data.to_string());
        let o = self.outline()?;
        Ok((small, o.head()))
    }
}

/// An outline line without its value and state: `[2] textbox "Name"`.
fn short(line: &str) -> &str {
    // After the quoted name, which may hold anything.
    let from = line
        .find('"')
        .and_then(|open| line[open + 1..].find("\" ").map(|c| open + 1 + c + 1))
        .unwrap_or(0);
    let end = [" = ", " (", " → "]
        .iter()
        .filter_map(|m| line[from..].find(m).map(|i| from + i))
        .min()
        .unwrap_or(line.len());
    &line[..end]
}

/// A base64 JPEG scaled down to at most `width` pixels across, again as base64 JPEG.
pub fn shrink(data: &str, width: u32) -> Option<String> {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD;
    let img = image::load_from_memory(&b64.decode(data).ok()?).ok()?;
    let img = if img.width() > width {
        let h = (img.height() as u64 * width as u64 / img.width() as u64).max(1) as u32;
        img.resize_exact(width, h, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 60)
        .encode_image(&img.to_rgb8())
        .ok()?;
    Some(b64.encode(out))
}

/// The turn's connection, kept between calls.
static DRIVER: Mutex<Option<Driver>> = Mutex::new(None);

/// Runs `f` with the job's browser, connecting (again) when needed, and once more on a
/// fresh connection when the old one had gone.
pub fn with<T>(dir: &str, f: impl Fn(&mut Driver) -> Result<T, String>) -> Result<T, String> {
    let mut slot = DRIVER.lock().unwrap_or_else(|e| e.into_inner());
    for attempt in 0..2 {
        let fresh = !matches!(&*slot, Some(d) if d.dir == dir && d.alive());
        if fresh {
            *slot = Some(Driver::connect(dir)?);
        }
        let d = slot.as_mut().expect("connected above");
        host::touch(dir);
        match f(d) {
            Ok(v) => return Ok(v),
            Err(e) if attempt == 0 && !fresh && gone(&e, d) => {
                *slot = None;
            }
            Err(e) => return Err(e),
        }
    }
    Err("the browser went away".into())
}

/// Whether a failure means the page or browser went, so a new connection may help.
fn gone(e: &str, d: &Driver) -> bool {
    !d.alive()
        || e.contains("closed")
        || e.contains("No session")
        || e.contains("No target")
        || e.contains("Session with given id not found")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn changes_list_only_new_lines() {
        let before = lines(&["# Shop", "[1] button \"Add\"", "Cart: 0"]);
        let after = lines(&["# Shop", "[1] button \"Add\"", "Cart: 1"]);
        assert_eq!(
            changes(&before, &after),
            "What changed on the page (1 lines went away):\nCart: 1"
        );
        assert_eq!(changes(&before, &before), "Nothing on the page changed.");
    }

    #[test]
    fn outlines_are_cut_and_searched() {
        let o = Outline {
            title: "Shop".into(),
            url: "http://localhost:3000/".into(),
            height: 2400,
            lines: lines(&["# Shop", "[1] link \"Cart\" → /cart", "Free shipping"]),
            ..Outline::default()
        };
        assert!(o.head().contains("3 screens tall"));
        assert_eq!(o.body("cart", 1000), "[1] link \"Cart\" → /cart");
        assert!(o.body("", 20).starts_with("# Shop\n… 2 more lines"));
        assert!(o
            .body("refund", 1000)
            .contains("Nothing on the page mentions"));
    }

    #[test]
    fn steps_name_elements_shortly() {
        assert_eq!(
            short("[2] textbox \"Your name\" = \"Anish\""),
            "[2] textbox \"Your name\""
        );
        assert_eq!(short("[1] link \"About\" → /about"), "[1] link \"About\"");
        assert_eq!(short("[4] button \"Go\""), "[4] button \"Go\"");
        assert_eq!(
            short("[5] button \"Cart (2)\" (disabled)"),
            "[5] button \"Cart (2)\""
        );
    }

    #[test]
    fn keys_and_urls() {
        assert_eq!(key_of("Enter").unwrap().0, "Enter");
        assert_eq!(key_of("arrow down").unwrap().0, "ArrowDown");
        assert!(key_of("hyper").is_none());
        assert_eq!(url_of("localhost:5173").unwrap(), "http://localhost:5173");
        assert_eq!(url_of(":3000").unwrap(), "http://localhost:3000");
        assert_eq!(url_of("about:blank").unwrap(), "about:blank");
        assert!(url_of("not a url").is_none());
    }
}
