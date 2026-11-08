//! Local agent turns that outlive Squad Code. Each runs in its own process session with its
//! output going to files in the session's folder (`turn/out`, `turn/err`, and `turn/exit`
//! once it ends), so closing or a crash leaves it running and the next launch follows it
//! again from those files.

use crate::agent::{Event, Handle, Target};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Marks Squad Code's wrapper in the process list, so an unrelated process that reused a pid is
/// never mistaken for a turn.
pub(crate) const MARK: &str = "runo-turn";
/// Runs the agent with its output in files, then records how it ended.
const SCRIPT: &str = r#"d="$1"; shift; "$@" >"$d/out" 2>"$d/err" </dev/null; echo $? >"$d/exit.tmp"; mv "$d/exit.tmp" "$d/exit""#;

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Turns stopped on purpose, by their process group, so their end isn't logged as a fault.
static STOPPED: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// Whether the turn led by `pid` was stopped on purpose; forgets it either way.
fn was_stopped(pid: u32) -> bool {
    let Ok(mut stopped) = STOPPED.lock() else {
        return false;
    };
    let before = stopped.len();
    stopped.retain(|g| *g != pid);
    stopped.len() != before
}

/// Run local turns detached, keeping their output under `root`. Only the app does this;
/// elsewhere turns run as plain child processes.
pub fn enable(root: PathBuf) {
    if cfg!(unix) {
        let _ = ROOT.set(root);
    }
}

pub fn root() -> Option<&'static Path> {
    ROOT.get().map(PathBuf::as_path)
}

pub(crate) fn turn_dir(root: &Path, id: u64) -> PathBuf {
    crate::persist::session_dir(root, id).join("turn")
}

/// What a session's last local turn left behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Left {
    /// Still running.
    Running,
    /// It ended (or was killed) while Squad Code wasn't watching; its output is there to read.
    Ended,
}

pub fn left_behind(root: &Path, id: u64) -> Option<Left> {
    let dir = turn_dir(root, id);
    let pid: u32 = std::fs::read_to_string(dir.join("pid"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    if dir.join("exit").is_file() {
        Some(Left::Ended)
    } else if is_turn(pid) {
        Some(Left::Running)
    } else {
        Some(Left::Ended)
    }
}

/// Whether `pid` is alive.
fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    // SAFETY: signal 0 only checks that the process exists.
    unsafe {
        libc::kill(pid as i32, 0) == 0
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// Whether `pid` is alive and is one of Squad Code's turn wrappers.
fn is_turn(pid: u32) -> bool {
    alive(pid)
        && Command::new("ps")
            .args(["-o", "command=", "-p", &pid.to_string()])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(MARK))
}

/// Stop a detached turn and everything it started.
pub fn stop(group: u32) {
    #[cfg(unix)]
    {
        if !is_turn(group) {
            return;
        }
        if let Ok(mut stopped) = STOPPED.lock() {
            stopped.push(group);
        }
        // SAFETY: signals to the process group of a turn wrapper Squad Code started.
        unsafe {
            libc::kill(-(group as i32), libc::SIGTERM);
        }
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(2));
            // SAFETY: as above; a group that already ended just returns an error.
            unsafe {
                libc::kill(-(group as i32), libc::SIGKILL);
            }
        });
    }
    #[cfg(not(unix))]
    let _ = group;
}

/// Start `command` as a detached turn of session `id`, streaming its output to `tx` as a
/// normal run would.
pub fn spawn(
    root: &Path,
    id: u64,
    command: Command,
    dir: &str,
    tx: Sender<Event>,
) -> Result<Handle, String> {
    let turn = fresh_turn(root, id)?;
    let mut sh = Command::new("/bin/sh");
    sh.arg("-c")
        .arg(SCRIPT)
        .arg(MARK)
        .arg(&turn)
        .arg(command.get_program())
        .args(command.get_args());
    start(id, turn, sh, &command, dir, tx)
}

/// Start `command` as a detached turn that takes its messages on stdin: Squad Code's agent
/// host runs it, sends it `prompt` first, and forwards each message added while it works
/// (see `harness::host`).
pub fn spawn_live(
    root: &Path,
    id: u64,
    command: Command,
    prompt: &str,
    dir: &str,
    tx: Sender<Event>,
) -> Result<Handle, String> {
    let turn = fresh_turn(root, id)?;
    crate::harness::host::send(&turn, prompt)
        .map_err(|e| format!("could not set up the turn: {e}"))?;
    let exe = std::env::current_exe().map_err(|e| format!("could not find Squad Code: {e}"))?;
    let mut host = Command::new(exe);
    host.arg("--agent-host")
        .arg(MARK)
        .arg(&turn)
        .arg(command.get_program())
        .args(command.get_args());
    start(id, turn, host, &command, dir, tx)
}

/// Start a turn that one of the agent host's protocol drivers runs (`--acp`, `--codex`
/// or `--pi`), with its settings written into the turn folder by `setup` and `prompt` as
/// its first message. `command`, the print-mode command, lends the turn its environment.
#[allow(clippy::too_many_arguments)]
pub fn spawn_driver(
    root: &Path,
    id: u64,
    driver: &str,
    setup: impl FnOnce(&Path) -> std::io::Result<()>,
    command: Command,
    prompt: &str,
    dir: &str,
    tx: Sender<Event>,
) -> Result<Handle, String> {
    let turn = fresh_turn(root, id)?;
    setup(&turn)
        .and_then(|_| crate::harness::host::send(&turn, prompt))
        .map_err(|e| format!("could not set up the turn: {e}"))?;
    let exe = std::env::current_exe().map_err(|e| format!("could not find Squad Code: {e}"))?;
    let mut host = Command::new(exe);
    host.arg("--agent-host").arg(MARK).arg(&turn).arg(driver);
    start(id, turn, host, &command, dir, tx)
}

/// An empty folder for session `id`'s next turn.
fn fresh_turn(root: &Path, id: u64) -> Result<PathBuf, String> {
    let turn = turn_dir(root, id);
    let _ = std::fs::remove_dir_all(&turn);
    std::fs::create_dir_all(&turn).map_err(|e| format!("could not set up the turn: {e}"))?;
    Ok(turn)
}

/// Start `wrapper`, which runs `command` in `turn`, in a process session of its own.
fn start(
    id: u64,
    turn: PathBuf,
    mut wrapper: Command,
    command: &Command,
    dir: &str,
    tx: Sender<Event>,
) -> Result<Handle, String> {
    crate::shellenv::ready();
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => wrapper.env(key, value),
            None => wrapper.env_remove(key),
        };
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid is async-signal-safe; it gives the turn its own session and
        // process group, so it survives Squad Code and can be stopped as a whole.
        unsafe {
            wrapper.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    let child = wrapper
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("could not start {:?}: {error}", command.get_program()))?;
    let pid = child.id();
    let _ = std::fs::write(turn.join("pid"), pid.to_string());
    follow(Target::Session(id), turn, pid, Some(child), tx);
    Ok(Handle::detached(pid))
}

/// New complete lines of a file, from `offset` on; a partial last line waits unless `all`.
fn read_lines(path: &Path, offset: &mut u64, all: bool) -> Vec<String> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    if file.seek(SeekFrom::Start(*offset)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let end = if all {
        bytes.len()
    } else {
        bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1)
    };
    *offset += end as u64;
    String::from_utf8_lossy(&bytes[..end])
        .lines()
        .map(str::to_string)
        .collect()
}

/// How the turn ended: its exit code, if it got to write one.
fn exit_code(turn: &Path) -> Option<i32> {
    std::fs::read_to_string(turn.join("exit"))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// How a turn's process ended, for the log.
fn exit_text(code: Option<i32>) -> String {
    match code {
        Some(code) => format!("the agent exited with code {code}"),
        None => {
            "the agent ended without an exit code (killed, or Squad Code lost track of it)".into()
        }
    }
}

fn follow(target: Target, turn: PathBuf, pid: u32, mut child: Option<Child>, tx: Sender<Event>) {
    std::thread::spawn(move || {
        let (mut out, mut err) = (0u64, 0u64);
        let watched = child.is_none();
        let mut checks = 0u32;
        loop {
            let ended = turn.join("exit").is_file()
                || match child.as_mut() {
                    Some(c) => c.try_wait().ok().flatten().is_some(),
                    // Not ours: it has gone when the pid is free (checked about once a
                    // second, to spare the process list).
                    None => {
                        checks += 1;
                        !alive(pid) || (checks.is_multiple_of(10) && !is_turn(pid))
                    }
                };
            let lines = read_lines(&turn.join("out"), &mut out, ended)
                .into_iter()
                .map(|l| crate::agent::line_event(target, false, l))
                .chain(
                    read_lines(&turn.join("err"), &mut err, ended)
                        .into_iter()
                        .map(Event::Err),
                );
            for event in lines {
                if tx.send(event).is_err() {
                    return;
                }
            }
            if ended {
                // A child of this Squad Code is reaped; give the wrapper a moment to write how
                // it ended.
                if let Some(mut c) = child.take() {
                    let _ = c.wait();
                }
                if !watched || exit_code(&turn).is_none() {
                    std::thread::sleep(Duration::from_millis(100));
                }
                let code = exit_code(&turn);
                // A stop on purpose is logged with the turn ("turn stopped").
                let stopped = was_stopped(pid);
                if code != Some(0) && !stopped {
                    crate::applog::warn("agent", format!("{target:?}: {}", exit_text(code)));
                }
                let _ = tx.send(Event::Done(code == Some(0)));
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("runo-detach-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn collect(rx: &std::sync::mpsc::Receiver<Event>) -> (Vec<String>, Vec<String>, bool) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        loop {
            match rx.recv_timeout(Duration::from_secs(10)).unwrap() {
                Event::Line(l) | Event::Json(l, _) => out.push(l),
                Event::Err(l) => err.push(l),
                Event::Done(ok) => return (out, err, ok),
                _ => {}
            }
        }
    }

    #[test]
    fn a_detached_turn_streams_and_reports_how_it_ended() {
        let root = temp("run");
        let (tx, rx) = std::sync::mpsc::channel();
        let mut command = Command::new("sh");
        command
            .args([
                "-c",
                "echo \"one $SQUAD_TEST_X\"; echo oops >&2; printf two; exit 3",
            ])
            .env("SQUAD_TEST_X", "x");
        spawn(&root, 4, command, root.to_str().unwrap(), tx).unwrap();
        let (out, err, ok) = collect(&rx);
        assert_eq!(out, ["one x", "two"]);
        assert_eq!(err, ["oops"]);
        assert!(!ok);
        assert_eq!(exit_code(&turn_dir(&root, 4)), Some(3));
        assert_eq!(left_behind(&root, 4), Some(Left::Ended));
        assert_eq!(left_behind(&root, 5), None);
        let _ = std::fs::remove_dir_all(root);
    }
}
