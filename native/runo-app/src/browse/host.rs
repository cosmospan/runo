//! Each job's own headless browser: a Chromium-based browser already on the machine,
//! started once per job folder and shared by the bot (through Squad's MCP tools) and the
//! person (through the Browser page). It listens for the DevTools protocol on a port of
//! 127.0.0.1 only, and outlives a bot's turn, so the page is still there to look at and
//! the next turn carries on where this one stopped. Ones nobody used for a while are
//! closed by `reap`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The page size bots and people see: a laptop's.
pub const WIDTH: u32 = 1280;
pub const HEIGHT: u32 = 800;

/// A browser that has gone this long without a bot or a viewer is closed.
pub const IDLE: Duration = Duration::from_secs(30 * 60);

/// Where a job's browser listens.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Endpoint {
    pub pid: u32,
    pub port: u16,
    /// The browser's own DevTools path, `/devtools/browser/<id>`.
    pub path: String,
    /// The job folder it belongs to.
    #[serde(default)]
    pub dir: String,
    /// When a bot or a viewer last used it, in seconds since 1970.
    #[serde(default)]
    pub used: u64,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A short, stable name for a folder: the same on the desktop and on the server, so the
/// app can find a server job's browser from the job's folder there.
pub fn key(dir: &str) -> String {
    let dir = dir.trim_end_matches('/');
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in dir.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

pub fn folder() -> PathBuf {
    crate::store::data_dir().join("browsers")
}

fn state_file(base: &Path, dir: &str) -> PathBuf {
    base.join(format!("{}.json", key(dir)))
}

/// `DevToolsActivePort` as Chromium writes it: the port, then the browser's path.
#[cfg_attr(not(unix), allow(dead_code))]
pub fn parse_active_port(text: &str) -> Option<(u16, String)> {
    let mut lines = text.lines();
    let port = lines.next()?.trim().parse().ok()?;
    let path = lines.next()?.trim();
    path.starts_with("/devtools/browser/")
        .then(|| (port, path.to_string()))
}

fn answers(port: u16) -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(400)).is_ok()
}

fn read_state(file: &Path) -> Option<Endpoint> {
    serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()
}

fn write_state(file: &Path, e: &Endpoint) {
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(e) {
        let tmp = file.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(tmp, file);
        }
    }
}

fn find_in(base: &Path, dir: &str) -> Option<Endpoint> {
    let e = read_state(&state_file(base, dir))?;
    answers(e.port).then_some(e)
}

/// Notes that the job's browser is in use, so it isn't closed for being idle.
pub fn touch(dir: &str) {
    let file = state_file(&folder(), dir);
    if let Some(mut e) = read_state(&file) {
        if now().saturating_sub(e.used) >= 30 {
            e.used = now();
            write_state(&file, &e);
        }
    }
}

/// The job's browser, started when it isn't running.
pub fn ensure(dir: &str) -> Result<Endpoint, String> {
    let base = folder();
    if let Some(e) = find_in(&base, dir) {
        touch(dir);
        return Ok(e);
    }
    let browser = crate::browser_engine::find_chromium()
        .ok_or_else(|| crate::browser_engine::not_found_message().to_string())?;
    let profile = base.join(format!("{}-profile", key(dir)));
    let e = launch(&browser, &profile, dir)?;
    write_state(&state_file(&base, dir), &e);
    Ok(e)
}

#[cfg(unix)]
fn launch(browser: &Path, profile: &Path, dir: &str) -> Result<Endpoint, String> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let _ = std::fs::create_dir_all(profile);
    let active = profile.join("DevToolsActivePort");
    let _ = std::fs::remove_file(&active);
    let mut cmd = Command::new(browser);
    cmd.args([
        "--headless=new",
        "--remote-debugging-port=0",
        "--remote-debugging-address=127.0.0.1",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--disable-sync",
        "--disable-features=Translate,MediaRouter",
        "--mute-audio",
        "--hide-scrollbars",
    ])
    .arg(format!("--window-size={WIDTH},{HEIGHT}"))
    .arg(format!("--user-data-dir={}", profile.display()));
    // Chromium won't start its sandbox as root (servers, containers).
    if unsafe { libc::geteuid() } == 0 {
        cmd.arg("--no-sandbox");
    }
    cmd.arg("about:blank")
        .current_dir(if Path::new(dir).is_dir() { dir } else { "/" })
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Its own process group, so it outlives the bot's turn and a Ctrl-C there.
        .process_group(0);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("couldn't start {}: {e}", browser.display()))?;
    let pid = child.id();
    let started = std::time::Instant::now();
    let found = loop {
        if let Some(found) = std::fs::read_to_string(&active)
            .ok()
            .and_then(|t| parse_active_port(&t))
        {
            break Some(found);
        }
        if let Ok(Some(_)) = child.try_wait() {
            break None;
        }
        if started.elapsed() > Duration::from_secs(15) {
            let _ = child.kill();
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let Some((port, path)) = found else {
        return Err(format!(
            "{} didn't start headless. Try it from a terminal, or set CHROME_PATH to another browser.",
            crate::browser_engine::browser_name(browser)
        ));
    };
    // Reaped when it exits, so a long-lived app doesn't keep a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(Endpoint {
        pid,
        port,
        path,
        dir: dir.to_string(),
        used: now(),
    })
}

#[cfg(not(unix))]
fn launch(_browser: &Path, _profile: &Path, _dir: &str) -> Result<Endpoint, String> {
    Err("The bots' browser needs macOS or Linux for now.".into())
}

#[cfg(unix)]
fn stop(e: &Endpoint) {
    // Only while its port still answers, so a recycled pid is left alone.
    if e.pid > 1 && answers(e.port) {
        unsafe {
            libc::kill(e.pid as i32, libc::SIGTERM);
        }
    }
}

#[cfg(not(unix))]
fn stop(_e: &Endpoint) {}

/// Closes the job's browser, if it's running.
pub fn close(dir: &str) {
    let file = state_file(&folder(), dir);
    if let Some(e) = read_state(&file) {
        stop(&e);
    }
    let _ = std::fs::remove_file(file.with_extension("outline"));
    let _ = std::fs::remove_file(file);
}

/// Closes the browsers nobody has used for `idle`, and forgets ones that are gone.
pub fn reap(idle: Duration) {
    reap_in(&folder(), idle, now());
}

/// `reap`, at most every five minutes and off the caller's thread: for loops that
/// tick often, like the app's and `squad serve`'s.
pub fn reap_now_and_then() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = now();
    let last = LAST.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 300
        || LAST
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
    {
        return;
    }
    std::thread::spawn(|| reap(IDLE));
}

fn reap_in(base: &Path, idle: Duration, now: u64) {
    let Ok(list) = std::fs::read_dir(base) else {
        return;
    };
    for entry in list.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(e) = read_state(&path) else {
            let _ = std::fs::remove_file(&path);
            continue;
        };
        if !answers(e.port) {
            let _ = std::fs::remove_file(&path);
        } else if now.saturating_sub(e.used) > idle.as_secs() {
            stop(&e);
            let _ = std::fs::remove_file(path.with_extension("outline"));
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_stable_and_ignore_a_trailing_slash() {
        assert_eq!(key("/work/app"), key("/work/app/"));
        assert_ne!(key("/work/app"), key("/work/api"));
        assert_eq!(key("/work/app").len(), 16);
        // Fixed, since the desktop works out a server job's key on its own.
        assert_eq!(key(""), "cbf29ce484222325");
    }

    #[test]
    fn reads_devtools_active_port() {
        assert_eq!(
            parse_active_port("41235\n/devtools/browser/abc-123\n"),
            Some((41235, "/devtools/browser/abc-123".into()))
        );
        assert_eq!(parse_active_port("41235\n"), None);
        assert_eq!(parse_active_port("x\n/devtools/browser/a"), None);
    }

    #[test]
    fn reaping_forgets_browsers_that_are_gone() {
        let base = std::env::temp_dir().join(format!("squad-reap-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&base);
        // Nothing listens on port 1.
        let gone = Endpoint {
            pid: 0,
            port: 1,
            path: "/devtools/browser/x".into(),
            dir: "/w".into(),
            used: 0,
        };
        write_state(&state_file(&base, "/w"), &gone);
        assert!(find_in(&base, "/w").is_none());
        reap_in(&base, IDLE, 10_000);
        assert!(!state_file(&base, "/w").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
