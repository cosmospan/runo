//! A headless Chromium driven over the DevTools protocol, for the Browser tab.
//!
//! Squad Code starts a Chromium-based browser already on the machine (the default browser when
//! it is one) with `--remote-debugging-pipe`, so CDP runs over two inherited file
//! descriptors (3 and 4) as NUL-separated JSON. Each Squad Code session gets its own page (target); the one on
//! screen streams JPEG frames that are decoded here, off the UI thread.
// Outside Linux only part of this is used, so the rest reads as unused there.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use gpui_kit::RenderImage;
use serde_json::{json, Value};
use std::collections::HashMap;
#[cfg(unix)]
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// Lock a mutex, taking its data even when a thread panicked while holding it, rather
/// than panicking in turn. The page map and reply table stay usable after a failed reader.
trait Locked<T> {
    fn locked(&self) -> MutexGuard<'_, T>;
}

impl<T> Locked<T> for Mutex<T> {
    fn locked(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// A Chromium-based browser Squad Code can drive: its macOS app (and the executable inside it,
/// which has the same name), its macOS bundle id, and its command names on Linux.
pub struct Known {
    pub app: &'static str,
    bundle: &'static str,
    commands: &'static [&'static str],
}

/// The Chromium-based browsers Squad Code knows, in the order they're tried when the default
/// browser isn't one of them.
const KNOWN: &[Known] = &[
    Known {
        app: "Google Chrome",
        bundle: "com.google.chrome",
        commands: &["google-chrome", "google-chrome-stable"],
    },
    Known {
        app: "Chromium",
        bundle: "org.chromium.chromium",
        commands: &["chromium", "chromium-browser"],
    },
    Known {
        app: "Microsoft Edge",
        bundle: "com.microsoft.edgemac",
        commands: &["microsoft-edge", "microsoft-edge-stable"],
    },
    Known {
        app: "Brave Browser",
        bundle: "com.brave.browser",
        commands: &["brave-browser", "brave"],
    },
    Known {
        app: "Arc",
        bundle: "company.thebrowser.browser",
        commands: &[],
    },
    Known {
        app: "Vivaldi",
        bundle: "com.vivaldi.vivaldi",
        commands: &["vivaldi", "vivaldi-stable"],
    },
    Known {
        app: "Opera",
        bundle: "com.operasoftware.opera",
        commands: &["opera"],
    },
    Known {
        app: "Dia",
        bundle: "company.thebrowser.dia",
        commands: &[],
    },
    Known {
        app: "Comet",
        bundle: "ai.perplexity.comet",
        commands: &[],
    },
    Known {
        app: "Google Chrome Canary",
        bundle: "com.google.chrome.canary",
        commands: &[],
    },
];

/// Where Chromium-based browsers install on Linux outside PATH's usual directories.
#[cfg(not(target_os = "macos"))]
const OTHER_PLACES: &[&str] = &[
    "/opt/google/chrome/chrome",
    "/opt/microsoft/msedge/msedge",
    "/opt/brave.com/brave/brave",
    "/opt/vivaldi/vivaldi",
    "/snap/bin/chromium",
];

/// The browser chosen in Settings: a path, or empty (or "auto") for the default browser.
static CHOICE: Mutex<String> = Mutex::new(String::new());

/// Remember the browser chosen in Settings. It's used the next time a browser starts.
pub fn set_choice(choice: &str) {
    if let Ok(mut c) = CHOICE.lock() {
        *c = choice.to_string();
    }
}

/// Where `k` would be: in /Applications or ~/Applications on macOS, under /usr/bin or on
/// PATH elsewhere.
fn places(k: &Known, home: Option<&Path>, path: &std::ffi::OsStr) -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        let inside = format!("{0}.app/Contents/MacOS/{0}", k.app);
        let mut out = vec![Path::new("/Applications").join(&inside)];
        out.extend(home.map(|h| h.join("Applications").join(&inside)));
        out
    } else {
        let mut dirs = vec![PathBuf::from("/usr/bin")];
        dirs.extend(std::env::split_paths(path));
        k.commands
            .iter()
            .flat_map(|c| dirs.iter().map(move |d| d.join(c)))
            .collect()
    }
}

/// The first known browser that exists, also looking under `~/Applications`.
pub fn pick_chromium(home: Option<&Path>, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    KNOWN.iter().find_map(|k| {
        places(k, home, std::ffi::OsStr::new(""))
            .into_iter()
            .find(|p| exists(p))
    })
}

/// The first known browser command found in a PATH-style list of directories.
pub fn search_path(path: &std::ffi::OsStr, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let dirs: Vec<PathBuf> = std::env::split_paths(path).collect();
    KNOWN
        .iter()
        .flat_map(|k| k.commands)
        .find_map(|name| dirs.iter().map(|d| d.join(name)).find(|p| exists(p)))
}

/// The known browser a default-browser id names: a macOS bundle id (`com.brave.Browser`)
/// or a Linux desktop file (`brave-browser.desktop`, or a Flatpak's `com.brave.Browser.desktop`).
pub fn known_for(id: &str) -> Option<&'static Known> {
    let id = id.trim().to_ascii_lowercase();
    let id = id.strip_suffix(".desktop").unwrap_or(&id);
    KNOWN
        .iter()
        .find(|k| k.bundle == id || k.commands.contains(&id))
}

/// The bundle id that opens https links, from macOS's Launch Services settings as JSON.
pub fn https_handler(launch_services: &Value) -> Option<String> {
    let handlers = launch_services["LSHandlers"].as_array()?;
    ["https", "http"].iter().find_map(|scheme| {
        handlers
            .iter()
            .find(|h| h["LSHandlerURLScheme"].as_str() == Some(scheme))
            .and_then(|h| h["LSHandlerRoleAll"].as_str())
            .map(str::to_string)
    })
}

/// The person's default browser, when it's one Squad Code can drive. Looked up once per run.
fn default_browser() -> Option<&'static Known> {
    static FOUND: std::sync::OnceLock<Option<&'static Known>> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| {
        let id = if cfg!(target_os = "macos") {
            let home = std::env::var_os("HOME").map(PathBuf::from)?;
            let plist = home.join(
                "Library/Preferences/com.apple.LaunchServices/com.apple.launchservices.secure.plist",
            );
            let out = Command::new("plutil")
                .args(["-convert", "json", "-o", "-"])
                .arg(plist)
                .stderr(Stdio::null())
                .output()
                .ok()?;
            https_handler(&serde_json::from_slice(&out.stdout).ok()?)?
        } else {
            let out = Command::new("xdg-settings")
                .args(["get", "default-web-browser"])
                .stderr(Stdio::null())
                .output()
                .ok()?;
            String::from_utf8_lossy(&out.stdout).to_string()
        };
        known_for(&id)
    })
}

/// The browser to drive: `CHROME_PATH` when set, then the one chosen in Settings, then the
/// default browser when it's Chromium-based, then the first known browser found.
pub fn find_chromium() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CHROME_PATH").map(PathBuf::from) {
        if path.exists() {
            return Some(path);
        }
    }
    let chosen = CHOICE.lock().map(|c| c.clone()).unwrap_or_default();
    if !chosen.is_empty() && chosen != "auto" && Path::new(&chosen).exists() {
        return Some(PathBuf::from(chosen));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path = std::env::var_os("PATH").unwrap_or_default();
    let default = default_browser().and_then(|k| {
        places(k, home.as_deref(), &path)
            .into_iter()
            .find(|p| p.exists())
    });
    let other = || {
        #[cfg(not(target_os = "macos"))]
        if let Some(p) = OTHER_PLACES.iter().map(PathBuf::from).find(|p| p.exists()) {
            return Some(p);
        }
        None
    };
    default
        .or_else(|| pick_chromium(home.as_deref(), Path::exists))
        .or_else(|| search_path(&path, Path::exists))
        .or_else(other)
}

/// Where downloads from the panel go: the person's Downloads folder.
#[cfg(unix)]
fn downloads_dir() -> Option<String> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let dir = home.join("Downloads");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.to_string_lossy().to_string())
}

/// What to say when no browser was found, for this system.
pub fn not_found_message() -> &'static str {
    if cfg!(target_os = "macos") {
        "No Chrome, Edge, Brave, Arc, Vivaldi, Opera or Chromium found in /Applications or \
         ~/Applications. Install one, or set CHROME_PATH to its executable."
    } else if cfg!(windows) {
        "The built-in browser needs macOS or Linux for now."
    } else {
        "No Chrome, Edge, Brave, Vivaldi, Opera or Chromium found in /usr/bin, /opt, /snap/bin \
         or on PATH. Install one (for example chromium), or set CHROME_PATH to its executable."
    }
}

/// The browser's name from its executable path, e.g. "Google Chrome".
pub fn browser_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Chromium".into())
}

/// One CDP message on the pipe: the JSON followed by a NUL byte.
pub fn frame_message(msg: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(msg).unwrap_or_default();
    bytes.push(0);
    bytes
}

/// Take every complete message off the front of `buf`, leaving a partial one behind.
#[cfg(unix)]
pub fn split_messages(buf: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    while let Some(end) = buf.iter().position(|b| *b == 0) {
        let rest = buf.split_off(end + 1);
        let mut msg = std::mem::replace(buf, rest);
        msg.pop();
        if !msg.is_empty() {
            out.push(msg);
        }
    }
    out
}

/// CDP's modifier bit mask.
pub fn modifier_bits(alt: bool, ctrl: bool, meta: bool, shift: bool) -> u8 {
    alt as u8 | (ctrl as u8) << 1 | (meta as u8) << 2 | (shift as u8) << 3
}

/// What a page shows now, kept current by the reader thread.
#[derive(Clone, Default)]
pub struct PageInfo {
    pub url: String,
    pub loading: bool,
    /// An `alert`, `confirm` or `prompt` waiting for an answer: (kind, message).
    pub dialog: Option<(String, String)>,
    /// Console errors and uncaught exceptions, oldest first.
    pub console: Vec<String>,
}

/// Console errors kept per page.
#[cfg(unix)]
const MAX_CONSOLE: usize = 50;

/// Run in every page before its own scripts: links and `window.open` that would open
/// a new tab or window load in the same page instead, since the panel shows one page.
pub(crate) const IN_PLACE_JS: &str = r#"(() => {
  const go = (url) => {
    if (!url) return;
    const href = new URL(String(url), location.href).href;
    if (href !== 'about:blank') location.href = href;
  };
  window.open = function (url) { go(url); return null; };
  const inPlace = (e) => {
    const t = e.target;
    const el = t && t.closest ? t.closest('a[target], area[target], form[target]') : null;
    if (el && el.target && el.target !== '_self') el.target = '_self';
  };
  document.addEventListener('click', inPlace, true);
  document.addEventListener('auxclick', inPlace, true);
  document.addEventListener('submit', inPlace, true);
})();"#;

/// A console error or uncaught exception from a CDP event, as one line of text.
pub fn console_error(method: &str, params: &Value) -> Option<String> {
    match method {
        "Runtime.consoleAPICalled" => {
            let kind = params["type"].as_str().unwrap_or_default();
            if kind != "error" && kind != "assert" {
                return None;
            }
            let args: Vec<String> = params["args"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|v| match v["value"].as_str() {
                            Some(s) => s.to_string(),
                            None if !v["value"].is_null() => v["value"].to_string(),
                            None => v["description"].as_str().unwrap_or_default().to_string(),
                        })
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            Some(format!("console.{kind}: {}", args.join(" ")))
        }
        "Runtime.exceptionThrown" => {
            let d = &params["exceptionDetails"];
            let text = d["exception"]["description"]
                .as_str()
                .or_else(|| d["text"].as_str())
                .unwrap_or("Uncaught exception");
            let place = d["url"]
                .as_str()
                .filter(|u| !u.is_empty())
                .map(|u| format!(" ({u}:{})", d["lineNumber"].as_u64().unwrap_or(0) + 1))
                .unwrap_or_default();
            Some(format!("{}{place}", text.lines().next().unwrap_or(text)))
        }
        "Log.entryAdded" => {
            let e = &params["entry"];
            if e["level"] != "error" {
                return None;
            }
            let url = e["url"].as_str().filter(|u| !u.is_empty());
            Some(match url {
                Some(url) => format!("{} ({url})", e["text"].as_str().unwrap_or_default()),
                None => e["text"].as_str().unwrap_or_default().to_string(),
            })
        }
        _ => None,
    }
}

struct Page {
    target: String,
    /// The flattened CDP session; empty until attached.
    session: String,
    info: PageInfo,
    viewport: (u32, u32, f32),
}

#[derive(Default)]
struct Shared {
    pages: HashMap<u64, Page>,
    replies: HashMap<u64, Sender<Value>>,
    dead: bool,
    /// New tabs or windows a page opened, by target, with the session of their opener.
    #[cfg(unix)]
    popups: HashMap<String, u64>,
    /// Downloads by guid: the file name, until they finish.
    #[cfg(unix)]
    downloads: HashMap<String, String>,
    /// What to tell the person about downloads, oldest first.
    notes: Vec<String>,
}

pub struct Engine {
    child: Child,
    writer: Mutex<std::fs::File>,
    next_id: Arc<AtomicU64>,
    shared: Arc<Mutex<Shared>>,
    /// Bumped on every page change.
    pub seq: Arc<AtomicU64>,
}

/// Duplicate `fd` above the standard descriptors, close-on-exec, and close the original.
#[cfg(unix)]
fn lift(fd: i32) -> std::io::Result<i32> {
    let high = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 10) };
    unsafe { libc::close(fd) };
    if high < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(high)
}

#[cfg(unix)]
fn pipe() -> std::io::Result<(i32, i32)> {
    let mut fds = [0i32; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok((lift(fds[0])?, lift(fds[1])?))
}

impl Engine {
    /// Start the browser headless with a Squad Code-only profile.
    #[cfg(unix)]
    pub fn start(browser: PathBuf, profile: &Path) -> Result<Engine, String> {
        use std::os::fd::FromRawFd;
        use std::os::unix::process::CommandExt;
        let _ = std::fs::create_dir_all(profile);
        let (to_child_r, to_child_w) = pipe().map_err(|e| e.to_string())?;
        let (from_child_r, from_child_w) = pipe().map_err(|e| e.to_string())?;
        let mut cmd = Command::new(&browser);
        cmd.args([
            "--headless=new",
            "--remote-debugging-pipe",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-networking",
            "--disable-sync",
            "--mute-audio",
            "--hide-scrollbars",
        ])
        .arg(format!("--user-data-dir={}", profile.display()));
        // Chromium won't start its sandbox as root (containers, some Linux setups).
        if unsafe { libc::geteuid() } == 0 {
            cmd.arg("--no-sandbox");
        }
        cmd.arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        unsafe {
            cmd.pre_exec(move || {
                // dup2 clears close-on-exec on the copies, so only 3 and 4 reach Chromium.
                if libc::dup2(to_child_r, 3) < 0 || libc::dup2(from_child_w, 4) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = cmd.spawn();
        unsafe {
            libc::close(to_child_r);
            libc::close(from_child_w);
        }
        let child = child.map_err(|e| format!("could not start {}: {e}", browser.display()))?;
        let writer = unsafe { std::fs::File::from_raw_fd(to_child_w) };
        let reader = unsafe { std::fs::File::from_raw_fd(from_child_r) };
        let engine = Engine {
            child,
            writer: Mutex::new(writer),
            next_id: Arc::new(AtomicU64::new(1)),
            shared: Arc::new(Mutex::new(Shared::default())),
            seq: Arc::new(AtomicU64::new(0)),
        };
        let (shared, seq, ids) = (
            engine.shared.clone(),
            engine.seq.clone(),
            engine.next_id.clone(),
        );
        let writer = engine
            .writer
            .lock()
            .unwrap()
            .try_clone()
            .map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("squadcode-cdp".into())
            .spawn(move || read_loop(reader, writer, shared, seq, ids))
            .map_err(|e| e.to_string())?;
        engine.send("Target.setDiscoverTargets", json!({"discover": true}), None);
        // Headless Chromium drops downloads unless told where to put them.
        if let Some(dir) = downloads_dir() {
            engine.send(
                "Browser.setDownloadBehavior",
                json!({"behavior": "allow", "downloadPath": dir, "eventsEnabled": true}),
                None,
            );
        }
        Ok(engine)
    }

    #[cfg(not(unix))]
    pub fn start(_browser: PathBuf, _profile: &Path) -> Result<Engine, String> {
        Err("The built-in browser needs macOS or Linux for now.".into())
    }

    pub fn alive(&self) -> bool {
        !self.shared.locked().dead
    }

    /// Fire a command; `session` targets a page, `None` the browser.
    fn send(&self, method: &str, params: Value, session: Option<&str>) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.write(id, method, params, session);
        id
    }

    fn write(&self, id: u64, method: &str, params: Value, session: Option<&str>) {
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session.filter(|s| !s.is_empty()) {
            msg["sessionId"] = json!(s);
        }
        if std::env::var_os("SQUADCODE_CDP_LOG").is_some() {
            eprintln!("cdp> {msg}");
        }
        let _ = self.writer.locked().write_all(&frame_message(&msg));
    }

    /// Send a command and wait for its result.
    fn call(&self, method: &str, params: Value, session: Option<&str>) -> Option<Value> {
        let (tx, rx) = channel();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.shared.locked().replies.insert(id, tx);
        self.write(id, method, params, session);
        rx.recv_timeout(Duration::from_secs(10)).ok()
    }

    fn session_of(&self, key: u64) -> Option<String> {
        let shared = self.shared.locked();
        shared
            .pages
            .get(&key)
            .map(|p| p.session.clone())
            .filter(|s| !s.is_empty())
    }

    pub fn has_page(&self, key: u64) -> bool {
        self.shared.locked().pages.contains_key(&key)
    }

    /// Open `url` in the session's page, creating the page the first time.
    pub fn open(self: &Arc<Self>, key: u64, url: &str, viewport: (u32, u32, f32)) {
        if let Some(session) = self.session_of(key) {
            self.send("Page.navigate", json!({"url": url}), Some(&session));
            return;
        }
        if self.has_page(key) {
            return;
        }
        self.shared.locked().pages.insert(
            key,
            Page {
                target: String::new(),
                session: String::new(),
                info: PageInfo {
                    url: url.to_string(),
                    loading: true,
                    ..PageInfo::default()
                },
                viewport,
            },
        );
        let (this, url) = (self.clone(), url.to_string());
        std::thread::spawn(move || this.attach(key, &url));
    }

    /// Create and attach the page, then load `url` and start streaming if it is on screen.
    fn attach(&self, key: u64, url: &str) {
        let Some(target) = self
            .call("Target.createTarget", json!({"url": "about:blank"}), None)
            .and_then(|r| r["targetId"].as_str().map(str::to_string))
        else {
            return;
        };
        let Some(session) = self
            .call(
                "Target.attachToTarget",
                json!({"targetId": target, "flatten": true}),
                None,
            )
            .and_then(|r| r["sessionId"].as_str().map(str::to_string))
        else {
            return;
        };
        let viewport = {
            let mut shared = self.shared.locked();
            let Some(page) = shared.pages.get_mut(&key) else {
                return;
            };
            page.target = target;
            page.session = session.clone();
            page.viewport
        };
        let s = Some(session.as_str());
        self.send("Page.enable", json!({}), s);
        self.send("Runtime.enable", json!({}), s);
        self.send("Log.enable", json!({}), s);
        self.send(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({"source": IN_PLACE_JS}),
            s,
        );
        // Pages behave as focused (caret, :focus, focus events) though no window is.
        self.send(
            "Emulation.setFocusEmulationEnabled",
            json!({"enabled": true}),
            s,
        );
        self.metrics(&session, viewport);
        self.send("Page.navigate", json!({"url": url}), s);
    }

    fn metrics(&self, session: &str, (w, h, scale): (u32, u32, f32)) {
        self.send(
            "Emulation.setDeviceMetricsOverride",
            json!({"width": w, "height": h, "deviceScaleFactor": scale, "mobile": false}),
            Some(session),
        );
    }

    /// Whether the page is attached and takes commands.
    // The design canvas draws with WebKit on macOS; these serve it elsewhere.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn ready(&self, key: u64) -> bool {
        self.session_of(key).is_some()
    }

    /// Run JavaScript in the page and return its value, after any promise it returns
    /// settles. None when it threw or the page didn't answer. Blocks for the reply.
    // The design canvas draws with WebKit on macOS; these serve it elsewhere.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn eval(&self, key: u64, js: &str) -> Option<Value> {
        let session = self.session_of(key)?;
        let reply = self.call(
            "Runtime.evaluate",
            json!({"expression": js, "returnByValue": true, "awaitPromise": true}),
            Some(&session),
        )?;
        if reply.get("exceptionDetails").is_some() {
            return None;
        }
        Some(reply["result"]["value"].clone())
    }

    /// Load `url` at this CSS size and scale, in front so it renders, and wait until it
    /// and its fonts have loaded. False when it didn't within `wait`.
    // The design canvas draws with WebKit on macOS; these serve it elsewhere.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn load(&self, key: u64, url: &str, viewport: (u32, u32, f32), wait: Duration) -> bool {
        let Some(session) = self.session_of(key) else {
            return false;
        };
        if let Some(page) = self.shared.locked().pages.get_mut(&key) {
            page.viewport = viewport;
        }
        self.call("Page.bringToFront", json!({}), Some(&session));
        self.metrics(&session, viewport);
        if self
            .call("Page.navigate", json!({"url": url}), Some(&session))
            .is_none()
        {
            return false;
        }
        let start = std::time::Instant::now();
        while start.elapsed() < wait {
            let done = self.eval(
                key,
                "document.readyState === 'complete' ? document.fonts.ready.then(() => true) : false",
            );
            if done.and_then(|v| v.as_bool()) == Some(true) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        false
    }

    /// Show the page with the dark or the light colour scheme preferred.
    // The design canvas draws with WebKit on macOS; these serve it elsewhere.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn color_scheme(&self, key: u64, dark: bool) {
        if let Some(session) = self.session_of(key) {
            self.call(
                "Emulation.setEmulatedMedia",
                json!({"features": [{"name": "prefers-color-scheme",
                    "value": if dark { "dark" } else { "light" }}]}),
                Some(&session),
            );
        }
    }

    /// The top `width` by `height` CSS pixels of the page, past the viewport if need be,
    /// as an image GPUI can draw. Blocks for the reply.
    // The design canvas draws with WebKit on macOS; these serve it elsewhere.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn capture(&self, key: u64, width: f32, height: f32) -> Option<Arc<RenderImage>> {
        let session = self.session_of(key)?;
        let reply = self.call(
            "Page.captureScreenshot",
            json!({"format": "png", "captureBeyondViewport": true,
                "clip": {"x": 0, "y": 0, "width": width, "height": height, "scale": 1}}),
            Some(&session),
        )?;
        decode_image(reply["data"].as_str()?, image::ImageFormat::Png)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.send("Browser.close", json!({}), None);
        std::thread::sleep(Duration::from_millis(60));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// What `Input.dispatchKeyEvent` needs for a non-text key.
pub struct KeySpec {
    pub key: &'static str,
    pub code: &'static str,
    pub vk: u32,
    pub text: &'static str,
    /// The editing command macOS would run for the key; headless Chromium needs it named.
    pub command: &'static str,
}

/// The CDP description of a GPUI key name, for keys that do something besides type.
pub fn key_spec(name: &str) -> Option<KeySpec> {
    let (key, code, vk, text, command) = match name {
        "enter" => ("Enter", "Enter", 13, "\r", ""),
        "backspace" => ("Backspace", "Backspace", 8, "", "deleteBackward"),
        "tab" => ("Tab", "Tab", 9, "", ""),
        "escape" => ("Escape", "Escape", 27, "", ""),
        "delete" => ("Delete", "Delete", 46, "", "deleteForward"),
        "left" => ("ArrowLeft", "ArrowLeft", 37, "", "moveLeft"),
        "up" => ("ArrowUp", "ArrowUp", 38, "", "moveUp"),
        "right" => ("ArrowRight", "ArrowRight", 39, "", "moveRight"),
        "down" => ("ArrowDown", "ArrowDown", 40, "", "moveDown"),
        "home" => ("Home", "Home", 36, "", "moveToBeginningOfLine"),
        "end" => ("End", "End", 35, "", "moveToEndOfLine"),
        "pageup" => ("PageUp", "PageUp", 33, "", ""),
        "pagedown" => ("PageDown", "PageDown", 34, "", ""),
        _ => return None,
    };
    Some(KeySpec {
        key,
        code,
        vk,
        text,
        command,
    })
}

/// Read CDP messages until the pipe closes: route replies, track pages, decode frames.
#[cfg(unix)]
fn read_loop(
    mut reader: std::fs::File,
    mut writer: std::fs::File,
    shared: Arc<Mutex<Shared>>,
    seq: Arc<AtomicU64>,
    ids: Arc<AtomicU64>,
) {
    let mut buf = Vec::new();
    let mut chunk = vec![0u8; 1 << 16];
    loop {
        let n = match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        buf.extend_from_slice(&chunk[..n]);
        for raw in split_messages(&mut buf) {
            let Ok(msg) = serde_json::from_slice::<Value>(&raw) else {
                continue;
            };
            if let Some(id) = msg["id"].as_u64() {
                let reply = shared.locked().replies.remove(&id);
                if let Some(tx) = reply {
                    let _ = tx.send(msg["result"].clone());
                }
                continue;
            }
            let session = msg["sessionId"].as_str().unwrap_or_default();
            let params = &msg["params"];
            if std::env::var_os("SQUADCODE_CDP_LOG").is_some() {
                eprintln!(
                    "cdp< {}",
                    msg.to_string().chars().take(300).collect::<String>()
                );
            }
            match msg["method"].as_str().unwrap_or_default() {
                "Target.targetCreated" | "Target.targetInfoChanged"
                    if adopt_popup(&params["targetInfo"], &shared, &mut writer, &ids) => {}
                "Target.targetInfoChanged" => {
                    let info = &params["targetInfo"];
                    let target = info["targetId"].as_str().unwrap_or_default();
                    let mut s = shared.locked();
                    if let Some(page) = s.pages.values_mut().find(|p| p.target == target) {
                        page.info.url = info["url"].as_str().unwrap_or_default().to_string();
                        seq.fetch_add(1, Ordering::Relaxed);
                    }
                }
                "Browser.downloadWillBegin" => {
                    let name = params["suggestedFilename"].as_str().unwrap_or("file");
                    let guid = params["guid"].as_str().unwrap_or_default().to_string();
                    let mut s = shared.locked();
                    s.downloads.insert(guid, name.to_string());
                    s.notes.push(format!("Downloading {name}…"));
                    seq.fetch_add(1, Ordering::Relaxed);
                }
                "Browser.downloadProgress" => {
                    let state = params["state"].as_str().unwrap_or_default();
                    if state == "inProgress" {
                        continue;
                    }
                    let guid = params["guid"].as_str().unwrap_or_default();
                    let mut s = shared.locked();
                    let name = s.downloads.remove(guid).unwrap_or_else(|| "file".into());
                    s.notes.push(if state == "completed" {
                        format!("Downloaded {name} to your Downloads folder.")
                    } else {
                        format!("The download of {name} was canceled.")
                    });
                    seq.fetch_add(1, Ordering::Relaxed);
                }
                method @ ("Runtime.consoleAPICalled"
                | "Runtime.exceptionThrown"
                | "Log.entryAdded") => {
                    let Some(line) = console_error(method, params) else {
                        continue;
                    };
                    let mut s = shared.locked();
                    if let Some(page) = s.pages.values_mut().find(|p| p.session == session) {
                        page.info.console.push(line);
                        let extra = page.info.console.len().saturating_sub(MAX_CONSOLE);
                        page.info.console.drain(..extra);
                        seq.fetch_add(1, Ordering::Relaxed);
                    }
                }
                method @ ("Page.frameStartedLoading"
                | "Page.loadEventFired"
                | "Page.javascriptDialogOpening") => {
                    let mut s = shared.locked();
                    if let Some(page) = s.pages.values_mut().find(|p| p.session == session) {
                        match method {
                            "Page.frameStartedLoading" => page.info.loading = true,
                            "Page.loadEventFired" => page.info.loading = false,
                            _ => {
                                page.info.dialog = Some((
                                    params["type"].as_str().unwrap_or("alert").to_string(),
                                    params["message"].as_str().unwrap_or_default().to_string(),
                                ))
                            }
                        }
                        seq.fetch_add(1, Ordering::Relaxed);
                    }
                }
                _ => {}
            }
        }
    }
    let mut s = shared.locked();
    s.dead = true;
    s.replies.clear();
    seq.fetch_add(1, Ordering::Relaxed);
}

/// When `info` is a new tab or window one of our pages opened, load its URL in the
/// opener and close it. Returns whether the target was such a popup.
#[cfg(unix)]
fn adopt_popup(
    info: &Value,
    shared: &Arc<Mutex<Shared>>,
    writer: &mut std::fs::File,
    ids: &Arc<AtomicU64>,
) -> bool {
    let target = info["targetId"].as_str().unwrap_or_default().to_string();
    let url = info["url"].as_str().unwrap_or_default();
    let mut s = shared.locked();
    let opener = info["openerId"].as_str().unwrap_or_default();
    let key = match s.popups.get(&target) {
        Some(key) => *key,
        None if info["type"] == "page" && !opener.is_empty() => {
            let Some((key, _)) = s.pages.iter().find(|(_, p)| p.target == opener) else {
                return false;
            };
            let key = *key;
            s.popups.insert(target.clone(), key);
            key
        }
        None => return false,
    };
    if url.is_empty() || url == "about:blank" {
        return true;
    }
    s.popups.remove(&target);
    let Some(session) = s.pages.get(&key).map(|p| p.session.clone()) else {
        return true;
    };
    drop(s);
    let mut send = |msg: Value| {
        let _ = writer.write_all(&frame_message(&msg));
    };
    send(
        json!({"id": ids.fetch_add(1, Ordering::Relaxed), "sessionId": session,
        "method": "Page.navigate", "params": {"url": url}}),
    );
    send(json!({"id": ids.fetch_add(1, Ordering::Relaxed),
        "method": "Target.closeTarget", "params": {"targetId": target}}));
    true
}

/// A base64 image as a BGRA image GPUI can draw.
pub(crate) fn decode_image(data: &str, format: image::ImageFormat) -> Option<Arc<RenderImage>> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    let mut rgba = image::load_from_memory_with_format(&bytes, format)
        .ok()?
        .into_rgba8();
    if std::env::var_os("SQUADCODE_CDP_LOG").is_some() {
        eprintln!("cdp  decoded {}x{}", rgba.width(), rgba.height());
    }
    for px in rgba.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(rgba)])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_poisoned_lock_still_hands_out_its_data() {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let poisoner = shared.clone();
        let _ = std::thread::spawn(move || {
            let mut s = poisoner.locked();
            s.notes.push("kept".into());
            panic!("reader failed while holding the lock");
        })
        .join();
        assert!(shared.is_poisoned());
        assert_eq!(shared.locked().notes, vec!["kept".to_string()]);
        shared.locked().dead = true;
        assert!(shared.locked().dead);
    }

    #[test]
    #[cfg(unix)]
    fn messages_are_nul_framed_and_split() {
        let bytes = frame_message(&json!({"id": 1, "method": "Page.enable"}));
        assert_eq!(*bytes.last().unwrap(), 0);
        let mut buf = bytes.clone();
        buf.extend_from_slice(b"{\"id\":2}\0{\"id\":");
        let msgs = split_messages(&mut buf);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1], b"{\"id\":2}");
        assert_eq!(buf, b"{\"id\":");
        buf.extend_from_slice(b"3}\0");
        assert_eq!(split_messages(&mut buf), vec![b"{\"id\":3}".to_vec()]);
        assert!(buf.is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn console_errors_become_lines() {
        let log = json!({"type": "error", "args": [{"type": "string", "value": "boom"}, {"type": "number", "value": 3}]});
        assert_eq!(
            console_error("Runtime.consoleAPICalled", &log).as_deref(),
            Some("console.error: boom 3")
        );
        let info = json!({"type": "log", "args": [{"value": "hi"}]});
        assert_eq!(console_error("Runtime.consoleAPICalled", &info), None);
        let thrown = json!({"exceptionDetails": {"text": "Uncaught", "url": "http://x/app.js", "lineNumber": 9,
            "exception": {"description": "TypeError: x is undefined\n    at f (app.js:10)"}}});
        assert_eq!(
            console_error("Runtime.exceptionThrown", &thrown).as_deref(),
            Some("TypeError: x is undefined (http://x/app.js:10)")
        );
        let net = json!({"entry": {"level": "error", "text": "Failed to load resource", "url": "http://x/a.png"}});
        assert_eq!(
            console_error("Log.entryAdded", &net).as_deref(),
            Some("Failed to load resource (http://x/a.png)")
        );
    }

    #[test]
    fn the_default_browser_is_recognised() {
        let ls = json!({"LSHandlers": [
            {"LSHandlerURLScheme": "mailto", "LSHandlerRoleAll": "com.apple.mail"},
            {"LSHandlerURLScheme": "https", "LSHandlerRoleAll": "com.brave.browser"},
        ]});
        let id = https_handler(&ls).unwrap();
        assert_eq!(known_for(&id).unwrap().app, "Brave Browser");
        assert_eq!(known_for("com.Brave.Browser").unwrap().app, "Brave Browser");
        assert_eq!(
            known_for("vivaldi-stable.desktop\n").unwrap().app,
            "Vivaldi"
        );
        assert_eq!(known_for("company.thebrowser.Browser").unwrap().app, "Arc");
        // Safari and Firefox can't be driven, so they fall through to the list.
        assert!(known_for("com.apple.safari").is_none());
        assert!(known_for("firefox.desktop").is_none());
        assert_eq!(https_handler(&json!({})), None);
    }

    #[test]
    fn chromium_is_found_in_system_or_user_applications() {
        let home = Path::new("/Users/me");
        let user_brave = "/Users/me/Applications/Brave Browser.app/Contents/MacOS/Brave Browser";
        let found = pick_chromium(Some(home), |p| {
            p.to_str() == Some(user_brave) || p.ends_with("Microsoft Edge")
        });
        if cfg!(target_os = "macos") {
            // Edge comes before Brave in the list.
            assert!(found.unwrap().ends_with("Microsoft Edge"));
            let brave = pick_chromium(Some(home), |p| p.to_str() == Some(user_brave));
            assert_eq!(brave.as_deref(), Some(Path::new(user_brave)));
            assert_eq!(browser_name(Path::new(user_brave)), "Brave Browser");
        }
        assert_eq!(pick_chromium(Some(home), |_| false), None);
        let path = std::env::join_paths(["/nope", "/home/me/bin"]).unwrap();
        assert_eq!(
            search_path(&path, |p| p == Path::new("/home/me/bin/chromium")),
            Some(PathBuf::from("/home/me/bin/chromium"))
        );
        assert_eq!(search_path(&path, |_| false), None);
        assert_eq!(key_spec("enter").unwrap().vk, 13);
        assert!(key_spec("a").is_none());
    }
}
