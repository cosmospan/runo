//! Smart Peek's own server: Squad's model, run by a `llama-server` that Squad
//! starts itself, only when the person has turned Smart Peek on.
//!
//! Nothing here is downloaded or started unless the person opts in. Turning
//! Smart Peek on downloads the model once into `~/.squadcode/models/`. After
//! that the server is started the first time the rules aren't sure of a job,
//! answers on localhost, and stops on its own after [`IDLE_MINUTES`] without a
//! question, or when Squad quits. It runs in its own process, so a model that
//! crashes or runs out of memory never takes Squad down; the rules decide.
//!
//! The server program is `llama-server` from llama.cpp: shipped next to the
//! app in the Mac release, else found on PATH (`brew install llama.cpp`).

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::settings::AppSettings;

use super::model::{Local, DEFAULT_URL};

/// The name Squad's own model answers to.
pub const MODEL: &str = "peek";
/// Its name before it was called Peek; settings saved with it still work.
pub const OLD_MODEL: &str = "fin";
/// Where Squad's own model is published. Each release is a new file there plus
/// an updated `latest.json`, so people get new versions with no app release.
pub const MODEL_REPO: &str = "https://huggingface.co/cosmospan/peek/resolve/main";
/// Roughly how big the model is, for the progress line before `latest.json` says.
pub const MODEL_BYTES: u64 = 529_297_056;
/// The server stops after this long without a question.
pub const IDLE_MINUTES: u32 = 5;
const PORT: &str = "8088";

/// The model file, once downloaded.
pub fn model_path() -> PathBuf {
    let dir = super::super::super::store::data_dir().join("models");
    let path = dir.join("peek.gguf");
    // Downloaded before the rename.
    let old = dir.join("fin.gguf");
    if !path.exists() && old.is_file() {
        let _ = fs::rename(&old, &path);
    }
    path
}

/// The release that's installed, as `latest.json` described it.
fn installed_path() -> PathBuf {
    model_path().with_extension("json")
}

/// Whether `name` is Squad's own model, by its current or old name.
pub fn is_ours(name: &str) -> bool {
    let n = name.trim();
    n == MODEL || n == OLD_MODEL
}

fn used_path() -> PathBuf {
    model_path().with_extension("used")
}

/// Whether Squad runs the server itself: Smart Peek is on with Squad's own
/// model, at the usual address, and the model is downloaded. Someone who
/// points Smart Peek at their own Ollama or LM Studio runs that themselves.
pub fn applies(st: &AppSettings) -> bool {
    is_ours(&st.brain_model) && st.brain_url.trim().is_empty() && model_path().is_file()
}

/// The `llama-server` to run: next to Squad's own binary first (the Mac app
/// ships one), then `$SQUADCODE_LLAMA_SERVER`, then PATH.
pub fn server_bin() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(p) = std::env::var_os("SQUADCODE_LLAMA_SERVER").map(PathBuf::from) {
        if p.is_file() {
            return Some(p);
        }
    }
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    })
}

/// The server's flags. Small on purpose: one slot with a 4K context is all a
/// routing question needs (about 250 MB, against 3.3 GB for the defaults), two
/// threads keep it from taking over the machine, and thinking is off so the
/// whole answer budget goes on the JSON.
pub fn server_args(model: &Path) -> Vec<String> {
    [
        "-m",
        &model.to_string_lossy(),
        "--host",
        "127.0.0.1",
        "--port",
        PORT,
        "--alias",
        MODEL,
        "-c",
        "4096",
        "-np",
        "1",
        "-t",
        "2",
        "--chat-template-kwargs",
        r#"{"enable_thinking":false}"#,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// The server Squad started, if it's still running.
static RUNNING: Mutex<Option<(std::process::Child, Instant)>> = Mutex::new(None);

/// Note that Peek was asked something just now, so the server stays up.
fn touch() {
    let _ = fs::write(used_path(), b"");
    if let Ok(mut r) = RUNNING.lock() {
        if let Some((_, last)) = r.as_mut() {
            *last = Instant::now();
        }
    }
}

/// Make sure Squad's own server is answering, starting it if need be, and wait
/// up to `wait` for it. False when it isn't up (yet): the rules decide this
/// time, and a server that's still loading answers the next question.
pub fn ensure(st: &AppSettings, wait: Duration) -> bool {
    if !applies(st) {
        return false;
    }
    let local = Local {
        url: DEFAULT_URL.to_string(),
        model: MODEL.to_string(),
    };
    touch();
    if local.alive() {
        return true;
    }
    if !started() && !start() {
        return false;
    }
    let until = Instant::now() + wait;
    while Instant::now() < until {
        if local.alive() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

/// Whether the server Squad started is still running.
fn started() -> bool {
    let Ok(mut r) = RUNNING.lock() else {
        return false;
    };
    match r.as_mut() {
        Some((child, _)) => {
            if matches!(child.try_wait(), Ok(None)) {
                true
            } else {
                *r = None;
                false
            }
        }
        None => false,
    }
}

fn start() -> bool {
    let Some(bin) = server_bin() else {
        return false;
    };
    let args = server_args(&model_path());
    let child = if cfg!(unix) {
        // A small watcher shell runs the server and stops it when Squad quits
        // or Peek hasn't been asked anything for IDLE_MINUTES.
        let watch = format!(
            r#""$0" "$@" >/dev/null 2>&1 & s=$!
trap 'kill $s 2>/dev/null; exit' TERM INT
while kill -0 $s 2>/dev/null; do
  sleep 10
  if ! kill -0 $PPID 2>/dev/null || [ -n "$(find "$SQUAD_PEEK_USED" -mmin +{IDLE_MINUTES} 2>/dev/null)" ]; then
    kill $s 2>/dev/null; exit
  fi
done"#
        );
        Command::new("sh")
            .arg("-c")
            .arg(watch)
            .arg(&bin)
            .args(&args)
            .env("SQUAD_PEEK_USED", used_path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    } else {
        Command::new(&bin)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };
    let Ok(child) = child else {
        return false;
    };
    if let Ok(mut r) = RUNNING.lock() {
        *r = Some((child, Instant::now()));
    }
    if !cfg!(unix) {
        // Without the watcher shell, a thread stops an idle server.
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_secs(30));
            let Ok(mut r) = RUNNING.lock() else { return };
            match r.as_mut() {
                Some((child, last)) => {
                    if last.elapsed() > Duration::from_secs(IDLE_MINUTES as u64 * 60) {
                        let _ = child.kill();
                        *r = None;
                        return;
                    }
                }
                None => return,
            }
        });
    }
    true
}

/// Stop the server Squad started, if any.
pub fn stop() {
    if let Ok(mut r) = RUNNING.lock() {
        if let Some((mut child, _)) = r.take() {
            #[cfg(unix)]
            {
                // The watcher shell stops the server when it's asked to stop.
                // SAFETY: plain kill(2) on a pid we spawned.
                unsafe {
                    libc::kill(child.id() as i32, libc::SIGTERM);
                }
            }
            #[cfg(not(unix))]
            {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

/// One published release of Squad's own model: `latest.json` in [`MODEL_REPO`].
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Release {
    /// Goes up by one with each release.
    pub version: u32,
    /// The model file's name in the repo, such as `peek-q4_k_m-v2.gguf`.
    pub file: String,
    pub bytes: u64,
    /// Lowercase hex SHA-256 of the file; empty skips the check.
    #[serde(default)]
    pub sha256: String,
    /// One line on what's better, for the settings page.
    #[serde(default)]
    pub notes: String,
}

/// What's published now. `base` empty is [`MODEL_REPO`].
pub fn latest(base: &str) -> Result<Release, String> {
    let base = if base.trim().is_empty() {
        MODEL_REPO
    } else {
        base.trim().trim_end_matches('/')
    };
    let out = Command::new("curl")
        .args(["-fsSL", "-m", "20"])
        .arg(format!("{base}/latest.json"))
        .output()
        .map_err(|e| format!("couldn't run curl: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "couldn't read {base}/latest.json: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("latest.json: {e}"))
}

/// The release that's installed, if Squad downloaded it.
pub fn installed() -> Option<Release> {
    fs::read(installed_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}

/// Download progress: 0 idle, 1 downloading, 2 done, 3 failed.
static STATE: AtomicU8 = AtomicU8::new(0);
static GOT: AtomicU64 = AtomicU64::new(0);
static TOTAL: AtomicU64 = AtomicU64::new(MODEL_BYTES);
static FAILED: Mutex<String> = Mutex::new(String::new());

/// Where a download stands, for the settings page.
#[derive(Clone, Debug, PartialEq)]
pub enum Download {
    Idle,
    Running { got: u64, total: u64 },
    Done,
    Failed(String),
}

pub fn download_state() -> Download {
    match STATE.load(Ordering::Relaxed) {
        1 => Download::Running {
            got: GOT.load(Ordering::Relaxed),
            total: TOTAL.load(Ordering::Relaxed),
        },
        2 => Download::Done,
        3 => Download::Failed(FAILED.lock().map(|s| s.clone()).unwrap_or_default()),
        _ => Download::Idle,
    }
}

/// In the background: install the latest release, or update to it when
/// `update` and a newer one is out. `base` empty is [`MODEL_REPO`].
pub fn download(base: &str, update: bool) {
    if STATE.load(Ordering::Relaxed) == 1 {
        return;
    }
    if model_path().is_file() && !update {
        STATE.store(2, Ordering::Relaxed);
        return;
    }
    let base = base.to_string();
    STATE.store(1, Ordering::Relaxed);
    GOT.store(0, Ordering::Relaxed);
    std::thread::spawn(move || match install_latest(&base, update) {
        Ok(_) => STATE.store(2, Ordering::Relaxed),
        Err(e) => {
            if let Ok(mut f) = FAILED.lock() {
                *f = e;
            }
            STATE.store(3, Ordering::Relaxed);
        }
    });
}

/// Install the latest release, or with `update` replace an older one.
/// `Ok(None)` when the installed one is already the latest.
pub fn install_latest(base: &str, update: bool) -> Result<Option<Release>, String> {
    let rel = latest(base)?;
    if model_path().is_file() {
        let have = installed().map(|r| r.version).unwrap_or(0);
        if !update || have >= rel.version {
            return Ok(None);
        }
    }
    if rel.file.contains('/') || rel.file.contains("..") {
        return Err("latest.json names a file outside the repo".into());
    }
    TOTAL.store(rel.bytes, Ordering::Relaxed);
    let base = if base.trim().is_empty() {
        MODEL_REPO
    } else {
        base.trim().trim_end_matches('/')
    };
    let url = format!("{base}/{}", rel.file);
    let part = model_path().with_extension("part");
    fetch_to(&url, &part)?;
    if let Err(e) = check(&part, Some(&rel)) {
        let _ = fs::remove_file(&part);
        return Err(e);
    }
    // The server has the old file open; it starts again on the next question.
    stop();
    fs::rename(&part, model_path()).map_err(|e| e.to_string())?;
    let _ = fs::write(
        installed_path(),
        serde_json::to_vec_pretty(&rel).unwrap_or_default(),
    );
    Ok(Some(rel))
}

/// Install a model file from this computer, or from any `https://` address.
pub fn install_file(from: &str) -> Result<(), String> {
    let part = model_path().with_extension("part");
    if from.starts_with("http://") || from.starts_with("https://") {
        fetch_to(from, &part)?;
    } else {
        if let Some(dir) = part.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        fs::copy(from, &part).map_err(|e| format!("couldn't copy {from}: {e}"))?;
    }
    if let Err(e) = check(&part, None) {
        let _ = fs::remove_file(&part);
        return Err(e);
    }
    stop();
    fs::rename(&part, model_path()).map_err(|e| e.to_string())?;
    // Not a published release: an update replaces it.
    let _ = fs::remove_file(installed_path());
    Ok(())
}

fn fetch_to(url: &str, part: &Path) -> Result<(), String> {
    let dir = part.parent().ok_or("no folder for the model")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut child = Command::new("curl")
        .args(["-fsSL", "--retry", "3", "-C", "-", "-o"])
        .arg(part)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't run curl: {e}"))?;
    loop {
        if let Ok(m) = fs::metadata(part) {
            GOT.store(m.len(), Ordering::Relaxed);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let mut err = String::new();
                    if let Some(mut e) = child.stderr.take() {
                        let _ = e.read_to_string(&mut err);
                    }
                    return Err(format!("download failed: {}", err.trim()));
                }
                return Ok(());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(300)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// A GGUF model file, and for a release, the size and checksum it published.
fn check(path: &Path, rel: Option<&Release>) -> Result<(), String> {
    let mut magic = [0u8; 4];
    fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .map_err(|e| e.to_string())?;
    if &magic != b"GGUF" {
        return Err("that isn't a GGUF model file".into());
    }
    let Some(rel) = rel else { return Ok(()) };
    let len = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len != rel.bytes {
        return Err(format!(
            "the download is {len} bytes, not the expected {}",
            rel.bytes
        ));
    }
    if !rel.sha256.is_empty() {
        if let Some(sum) = sha256(path) {
            if !sum.eq_ignore_ascii_case(rel.sha256.trim()) {
                return Err("the download's checksum doesn't match latest.json".into());
            }
        }
    }
    Ok(())
}

/// The file's SHA-256 from the system's own tool, when it has one.
fn sha256(path: &Path) -> Option<String> {
    let tries: [(&str, &[&str]); 3] = [
        ("shasum", &["-a", "256"]),
        ("sha256sum", &[]),
        ("certutil", &["-hashfile"]),
    ];
    for (bin, args) in tries {
        let mut c = Command::new(bin);
        c.args(args).arg(path);
        if bin == "certutil" {
            c.arg("SHA256");
        }
        if let Ok(out) = c.output() {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                let hex = text
                    .split_whitespace()
                    .find(|w| w.len() == 64 && w.chars().all(|c| c.is_ascii_hexdigit()))?;
                return Some(hex.to_lowercase());
            }
        }
    }
    None
}

/// Turn Smart Peek off and delete the downloaded model.
pub fn remove() {
    stop();
    let _ = fs::remove_file(model_path());
    let _ = fs::remove_file(used_path());
    let _ = fs::remove_file(installed_path());
    STATE.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_is_small_and_never_thinks() {
        let a = server_args(Path::new("/m/peek.gguf")).join(" ");
        assert!(a.contains("-c 4096 -np 1"));
        assert!(a.contains("--host 127.0.0.1"));
        assert!(a.contains(r#"{"enable_thinking":false}"#));
        assert!(a.contains("--alias peek"));
    }

    #[test]
    fn only_a_gguf_file_is_kept() {
        let dir = std::env::temp_dir().join(format!("peek-check-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.part");
        fs::write(&bad, b"<html>not found</html>").unwrap();
        assert!(check(&bad, None).is_err());
        let good = dir.join("good.part");
        fs::write(&good, b"GGUF....").unwrap();
        assert!(check(&good, None).is_ok());
        let rel = Release {
            version: 1,
            file: "peek.gguf".into(),
            bytes: 8,
            sha256: String::new(),
            notes: String::new(),
        };
        assert!(check(&good, Some(&rel)).is_ok());
        let wrong = Release {
            bytes: 9,
            ..rel.clone()
        };
        assert!(
            check(&good, Some(&wrong)).is_err(),
            "a release has a fixed size"
        );
        let sum = Release {
            sha256: "00".repeat(32),
            ..rel
        };
        if sha256(&good).is_some() {
            assert!(check(&good, Some(&sum)).is_err(), "and a checksum");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn someone_elses_server_is_left_to_them() {
        let mut st = AppSettings {
            brain_model: "qwen3:0.6b".into(),
            ..Default::default()
        };
        assert!(!applies(&st));
        st.brain_model = MODEL.into();
        st.brain_url = "http://127.0.0.1:11434/v1".into();
        assert!(!applies(&st));
    }

    #[test]
    fn the_old_name_still_counts() {
        assert!(is_ours("fin") && is_ours(" peek ") && !is_ours("qwen3"));
    }

    #[test]
    fn a_release_reads_from_latest_json() {
        let r: Release = serde_json::from_str(
            r#"{"version": 2, "file": "peek-q4_k_m-v2.gguf", "bytes": 529297056, "sha256": "afa9"}"#,
        )
        .unwrap();
        assert_eq!((r.version, r.bytes), (2, 529_297_056));
        assert!(r.notes.is_empty());
    }
}
