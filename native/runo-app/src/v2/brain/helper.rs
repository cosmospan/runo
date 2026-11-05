//! Smart Fin's own server: Squad's model, run by a `llama-server` that Squad
//! starts itself, only when the person has turned Smart Fin on.
//!
//! Nothing here is downloaded or started unless the person opts in. Turning
//! Smart Fin on downloads the model once into `~/.squadcode/models/`. After
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
pub const MODEL: &str = "fin";
/// Where Squad's own model is downloaded from, unless `brain_model_url` says otherwise.
pub const MODEL_URL: &str =
    "https://huggingface.co/cosmospan/squad-fin/resolve/main/fin-q4_k_m.gguf";
/// The released model's size in bytes, checked after the download.
pub const MODEL_BYTES: u64 = 529_297_056;
/// The server stops after this long without a question.
pub const IDLE_MINUTES: u32 = 5;
const PORT: &str = "8088";

/// The model file, once downloaded.
pub fn model_path() -> PathBuf {
    super::super::super::store::data_dir()
        .join("models")
        .join("fin.gguf")
}

fn used_path() -> PathBuf {
    model_path().with_extension("used")
}

/// Whether Squad runs the server itself: Smart Fin is on with Squad's own
/// model, at the usual address, and the model is downloaded. Someone who
/// points Smart Fin at their own Ollama or LM Studio runs that themselves.
pub fn applies(st: &AppSettings) -> bool {
    st.brain_model.trim() == MODEL && st.brain_url.trim().is_empty() && model_path().is_file()
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

/// Note that Fin was asked something just now, so the server stays up.
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
        // or Fin hasn't been asked anything for IDLE_MINUTES.
        let watch = format!(
            r#""$0" "$@" >/dev/null 2>&1 & s=$!
trap 'kill $s 2>/dev/null; exit' TERM INT
while kill -0 $s 2>/dev/null; do
  sleep 10
  if ! kill -0 $PPID 2>/dev/null || [ -n "$(find "$SQUAD_FIN_USED" -mmin +{IDLE_MINUTES} 2>/dev/null)" ]; then
    kill $s 2>/dev/null; exit
  fi
done"#
        );
        Command::new("sh")
            .arg("-c")
            .arg(watch)
            .arg(&bin)
            .args(&args)
            .env("SQUAD_FIN_USED", used_path())
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

/// Download progress: 0 idle, 1 downloading, 2 done, 3 failed.
static STATE: AtomicU8 = AtomicU8::new(0);
static GOT: AtomicU64 = AtomicU64::new(0);
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
            total: MODEL_BYTES,
        },
        2 => Download::Done,
        3 => Download::Failed(FAILED.lock().map(|s| s.clone()).unwrap_or_default()),
        _ => Download::Idle,
    }
}

/// Start downloading the model in the background, unless it's already here
/// or on its way. `url` empty uses [`MODEL_URL`].
pub fn download(url: &str) {
    if STATE.load(Ordering::Relaxed) == 1 {
        return;
    }
    if model_path().is_file() {
        STATE.store(2, Ordering::Relaxed);
        return;
    }
    let url = if url.trim().is_empty() {
        MODEL_URL.to_string()
    } else {
        url.trim().to_string()
    };
    STATE.store(1, Ordering::Relaxed);
    GOT.store(0, Ordering::Relaxed);
    std::thread::spawn(move || match fetch(&url, &model_path()) {
        Ok(()) => STATE.store(2, Ordering::Relaxed),
        Err(e) => {
            if let Ok(mut f) = FAILED.lock() {
                *f = e;
            }
            STATE.store(3, Ordering::Relaxed);
        }
    });
}

/// Download `url` (or copy a local file) to `to`, checked: it must be a GGUF
/// file and, for Squad's own model, the released size.
pub fn fetch(url: &str, to: &Path) -> Result<(), String> {
    let dir = to.parent().ok_or("no folder for the model")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let part = to.with_extension("part");
    if url.starts_with("http://") || url.starts_with("https://") {
        let mut child = Command::new("curl")
            .args(["-fsSL", "--retry", "3", "-C", "-", "-o"])
            .arg(&part)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("couldn't run curl: {e}"))?;
        loop {
            if let Ok(m) = fs::metadata(&part) {
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
                    break;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(300)),
                Err(e) => return Err(e.to_string()),
            }
        }
    } else {
        fs::copy(url, &part).map_err(|e| format!("couldn't copy {url}: {e}"))?;
    }
    check(&part, url == MODEL_URL)?;
    fs::rename(&part, to).map_err(|e| e.to_string())
}

fn check(path: &Path, released: bool) -> Result<(), String> {
    let mut magic = [0u8; 4];
    fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .map_err(|e| e.to_string())?;
    if &magic != b"GGUF" {
        let _ = fs::remove_file(path);
        return Err("that isn't a GGUF model file".into());
    }
    let len = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if released && len != MODEL_BYTES {
        let _ = fs::remove_file(path);
        return Err(format!(
            "the download is {len} bytes, not the expected {MODEL_BYTES}"
        ));
    }
    Ok(())
}

/// Turn Smart Fin off and delete the downloaded model.
pub fn remove() {
    stop();
    let _ = fs::remove_file(model_path());
    let _ = fs::remove_file(used_path());
    STATE.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_is_small_and_never_thinks() {
        let a = server_args(Path::new("/m/fin.gguf")).join(" ");
        assert!(a.contains("-c 4096 -np 1"));
        assert!(a.contains("--host 127.0.0.1"));
        assert!(a.contains(r#"{"enable_thinking":false}"#));
        assert!(a.contains("--alias fin"));
    }

    #[test]
    fn only_a_gguf_file_is_kept() {
        let dir = std::env::temp_dir().join(format!("fin-check-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.part");
        fs::write(&bad, b"<html>not found</html>").unwrap();
        assert!(check(&bad, false).is_err());
        assert!(!bad.exists());
        let good = dir.join("good.part");
        fs::write(&good, b"GGUF....").unwrap();
        assert!(check(&good, false).is_ok());
        assert!(
            check(&good, true).is_err(),
            "the released model has a fixed size"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn someone_elses_server_is_left_to_them() {
        let mut st = AppSettings::default();
        st.brain_model = "qwen3:0.6b".into();
        assert!(!applies(&st));
        st.brain_model = MODEL.into();
        st.brain_url = "http://127.0.0.1:11434/v1".into();
        assert!(!applies(&st));
    }
}
