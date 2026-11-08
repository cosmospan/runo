//! Runo's own log file, crash reports, and the check for a newer Runo release.
//!
//! Each launch writes what used to go to stderr into `~/.runo/logs/runo-<time>.log`,
//! keeping the latest few. A log that grows past `LOG_LIMIT` (a Runo left open for weeks)
//! moves to `runo-<time>.log.1` and a fresh file starts, keeping `PARTS_KEPT` pieces. A
//! panic writes `crash-<time>.txt` there too, which the next launch offers to show.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Launch logs kept; older ones are deleted.
const LOGS_KEPT: usize = 10;
/// Size a launch's log may reach before it moves aside and a fresh one starts.
const LOG_LIMIT: u64 = 10 << 20;
/// Pieces of one launch's log kept, the current one included: at most about 30 MB.
const PARTS_KEPT: usize = 3;
/// How often the log's size is looked at.
const SIZE_CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(30);
/// Crash reports kept.
const CRASHES_KEPT: usize = 20;

pub fn logs_dir() -> PathBuf {
    crate::store::data_dir().join("logs")
}

fn stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

/// Delete all but the newest `keep` files in `dir` whose names start with `prefix`.
fn prune(dir: &Path, prefix: &str, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(prefix))
        })
        .collect();
    // Names carry the time, so they sort oldest first.
    files.sort();
    let extra = files.len().saturating_sub(keep);
    for path in files.into_iter().take(extra) {
        let _ = std::fs::remove_file(path);
    }
}

/// How much a log line matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

/// Longest message kept on one line; the rest is cut.
const LINE_LIMIT: usize = 1000;

/// Something worth knowing later, such as a turn starting or ending.
pub fn info(area: &str, message: impl AsRef<str>) {
    write(Level::Info, area, message.as_ref());
}

/// Something went wrong that the person can work around, such as a failed git command.
pub fn warn(area: &str, message: impl AsRef<str>) {
    write(Level::Warn, area, message.as_ref());
}

/// Something went wrong that stops a piece of Runo working, such as a save that failed.
pub fn error(area: &str, message: impl AsRef<str>) {
    write(Level::Error, area, message.as_ref());
}

/// Like `warn`, but a message repeated by a background retry is written once per launch.
pub fn warn_once(area: &str, message: impl AsRef<str>) {
    once(Level::Warn, area, message.as_ref());
}

/// Like `error`, but a message repeated by a background retry is written once per launch.
pub fn error_once(area: &str, message: impl AsRef<str>) {
    once(Level::Error, area, message.as_ref());
}

fn once(level: Level, area: &str, message: &str) {
    static SEEN: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (area, message).hash(&mut hasher);
    let key = hasher.finish();
    let fresh = SEEN.lock().map_or(true, |mut seen| {
        if seen.contains(&key) {
            false
        } else {
            // Bounded, so a flood of distinct messages can't grow it forever.
            if seen.len() >= 512 {
                seen.clear();
            }
            seen.push(key);
            true
        }
    });
    if fresh {
        write(level, area, message);
    }
}

/// Write one line to the log (stderr, which `init` sends to the log file). Writing never
/// panics: a log that can't be written is skipped.
fn write(level: Level, area: &str, message: &str) {
    let time = chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S%.3f")
        .to_string();
    let line = format_line(&time, level, area, message);
    let _ = std::io::stderr().lock().write_all(line.as_bytes());
}

/// `<time> <LEVEL> [area] message`, on one line with secrets masked.
pub fn format_line(time: &str, level: Level, area: &str, message: &str) -> String {
    let mut text = String::with_capacity(message.len().min(LINE_LIMIT) + 48);
    for (i, line) in message
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .enumerate()
    {
        if i > 0 {
            text.push_str(" | ");
        }
        text.push_str(line);
    }
    let mut text = redact(&text);
    if text.chars().count() > LINE_LIMIT {
        text = text.chars().take(LINE_LIMIT).collect::<String>() + "…";
    }
    format!("{time} {:<5} [{area}] {text}\n", level.tag())
}

/// Prefixes of well-known API keys and tokens.
const TOKEN_PREFIXES: [&str; 14] = [
    "github_pat_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "glpat-",
    "sk-",
    "xoxb-",
    "xoxp-",
    "xapp-",
    "AKIA",
    "AIza",
    "npm_",
];

/// Names whose value is a secret, in `name=value` or `name: value`.
const SECRET_NAMES: [&str; 9] = [
    "token",
    "secret",
    "password",
    "passwd",
    "api_key",
    "apikey",
    "api-key",
    "authorization",
    "credential",
];

const MASK: &str = "***";

fn secret_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    SECRET_NAMES.iter().any(|s| name.contains(s))
}

fn token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '+' | '=')
}

/// Mask tokens, API keys, passwords in URLs and `secret=value` pairs, so text can be
/// logged. Errors from git, gh and agent CLIs sometimes echo them.
pub fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    // The next word is a secret: after `Bearer`, or after `Authorization:` and the like.
    let mut mask_next = false;
    for piece in text.split_inclusive(char::is_whitespace) {
        let word = piece.trim_end_matches(char::is_whitespace);
        let space = &piece[word.len()..];
        if word.is_empty() {
            out.push_str(space);
            continue;
        }
        let lower = word.to_ascii_lowercase();
        // An auth scheme: the credential follows it.
        let scheme =
            matches!(lower.as_str(), "bearer" | "basic") || (mask_next && lower == "token");
        if scheme {
            out.push_str(word);
            mask_next = true;
        } else if mask_next {
            out.push_str(MASK);
            mask_next = false;
        } else if (word.ends_with(':') || word.ends_with('='))
            && secret_name(word.trim_end_matches([':', '=']))
        {
            out.push_str(word);
            mask_next = true;
        } else {
            out.push_str(&redact_word(word));
        }
        out.push_str(space);
    }
    out
}

/// One word without spaces: `name=secret`, `"name":"secret"`, a URL with a password, or a
/// known token.
fn redact_word(word: &str) -> String {
    // name=value or "name": "value" in one word.
    for sep in ['=', ':'] {
        if let Some((name, value)) = word.split_once(sep) {
            let value_body = value.trim_matches(|c: char| c == '"' || c == '\'');
            if !value_body.is_empty()
                && !value_body.starts_with("//")
                && secret_name(name.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
            {
                // Keep the quotes and brackets around the value.
                let punct = |c: char| matches!(c, '"' | '\'' | '}' | ']' | ')' | ',' | ';');
                let open = &value[..value.len() - value.trim_start_matches(punct).len()];
                let rest = &value[open.len()..];
                let close = &rest[rest.trim_end_matches(punct).len()..];
                return format!("{name}{sep}{open}{MASK}{close}");
            }
        }
    }
    let mut word = word.to_string();
    // scheme://user:password@host
    if let Some(start) = word.find("://").map(|i| i + 3) {
        let rest = &word[start..];
        let host_end = rest.find('/').unwrap_or(rest.len());
        if let Some(at) = rest[..host_end].rfind('@') {
            word = format!("{}{MASK}{}", &word[..start], &rest[at..]);
        }
    }
    for prefix in TOKEN_PREFIXES {
        let mut from = 0;
        while let Some(found) = word[from..].find(prefix).map(|i| i + from) {
            let boundary = word[..found]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_ascii_alphanumeric());
            let body_start = found + prefix.len();
            let body_len = word[body_start..]
                .find(|c: char| !token_char(c))
                .unwrap_or(word.len() - body_start);
            if boundary && body_len >= 12 {
                word.replace_range(body_start..body_start + body_len, MASK);
                from = body_start + MASK.len();
            } else {
                from = body_start;
            }
        }
    }
    word
}

/// This launch's log file, once `init` made it.
static CURRENT_LOG: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Start this launch's log file and send stderr to it (and still to the terminal, when
/// Runo was started from one), then install the crash reporter.
pub fn init() {
    let dir = logs_dir();
    let _ = std::fs::create_dir_all(&dir);
    prune_launches(&dir, LOGS_KEPT - 1);
    let path = dir.join(format!("runo-{}.log", stamp()));
    if let Ok(mut file) = std::fs::File::create(&path) {
        let _ = START.set(now());
        let _ = writeln!(
            file,
            "{}, started {}",
            about(),
            START.get().map_or("", |s| s.as_str())
        );
        let sink = redirect_stderr(file);
        let _ = CURRENT_LOG.set(path.clone());
        watch_size(path, sink);
    }
    install_panic_hook();
}

/// "Runo 0.5.9 on macos aarch64".
fn about() -> String {
    format!(
        "Squad Code {} on {} {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Delete all but the newest `keep` launch logs in `dir`, with their older pieces.
fn prune_launches(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("runo-"))
        .collect();
    let mut launches: Vec<&str> = names
        .iter()
        .map(|n| n.split_once(".log").map_or(n.as_str(), |(base, _)| base))
        .collect();
    // Names carry the time, so they sort oldest first.
    launches.sort_unstable();
    launches.dedup();
    let extra = launches.len().saturating_sub(keep);
    for old in &launches[..extra] {
        for name in names
            .iter()
            .filter(|n| n.split_once(".log").is_some_and(|(b, _)| b == *old))
        {
            let _ = std::fs::remove_file(dir.join(name));
        }
    }
}

/// Move `path` to `<path>.1`, `.1` to `.2` and so on, so that with the fresh file about
/// to start at `path` there are `parts` pieces; the oldest goes.
fn shift_parts(path: &Path, parts: usize) {
    let piece = |n: usize| PathBuf::from(format!("{}.{n}", path.display()));
    let _ = std::fs::remove_file(piece(parts.saturating_sub(1)));
    for n in (1..parts.saturating_sub(1)).rev() {
        let _ = std::fs::rename(piece(n), piece(n + 1));
    }
    if parts > 1 {
        let _ = std::fs::rename(path, piece(1));
    }
}

/// Where stderr's lines end up, so a fresh file can take over when the log is full.
/// Windows only ever points stderr at the file.
#[cfg_attr(windows, allow(dead_code))]
enum Sink {
    /// Stderr is the file itself.
    Stderr,
    /// A thread copies stderr to the terminal and to this file.
    Shared(std::sync::Arc<std::sync::Mutex<std::fs::File>>),
    None,
}

/// Every little while, start a fresh file when the log has grown past `LOG_LIMIT`.
fn watch_size(path: PathBuf, sink: Sink) {
    if matches!(sink, Sink::None) {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("runo-log-size".into())
        .spawn(move || loop {
            std::thread::sleep(SIZE_CHECK_EVERY);
            if std::fs::metadata(&path).map_or(0, |m| m.len()) < LOG_LIMIT {
                continue;
            }
            shift_parts(&path, PARTS_KEPT);
            let Ok(mut file) = std::fs::File::create(&path) else {
                continue;
            };
            let _ = writeln!(
                file,
                "{}, started {}, log continued {}",
                about(),
                START.get().map_or("", |s| s.as_str()),
                now()
            );
            match &sink {
                Sink::Stderr => point_stderr_at(file),
                Sink::Shared(shared) => {
                    if let Ok(mut current) = shared.lock() {
                        *current = file;
                    }
                }
                Sink::None => {}
            }
        });
}

/// When this launch started, for the first line of each fresh log piece.
static START: std::sync::OnceLock<String> = std::sync::OnceLock::new();

#[cfg(unix)]
fn point_stderr_at(file: std::fs::File) {
    use std::os::fd::IntoRawFd;
    // SAFETY: replaces descriptor 2 with one owned here, then closes the spare.
    unsafe {
        let fd = file.into_raw_fd();
        libc::dup2(fd, 2);
        libc::close(fd);
    }
}

#[cfg(unix)]
fn redirect_stderr(file: std::fs::File) -> Sink {
    use std::os::fd::FromRawFd;
    // SAFETY: plain descriptor calls on stderr and on descriptors owned here.
    unsafe {
        if libc::isatty(2) == 0 {
            point_stderr_at(file);
            return Sink::Stderr;
        }
        // A terminal is attached: copy everything to both through a pipe.
        let mut fds = [0; 2];
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            return Sink::None;
        }
        let terminal = libc::dup(2);
        if terminal < 0 || libc::dup2(fds[1], 2) < 0 {
            return Sink::None;
        }
        libc::close(fds[1]);
        let mut pipe = std::fs::File::from_raw_fd(fds[0]);
        let mut terminal = std::fs::File::from_raw_fd(terminal);
        let shared = std::sync::Arc::new(std::sync::Mutex::new(file));
        let file = shared.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = [0u8; 4096];
            while let Ok(n) = pipe.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if let Ok(mut file) = file.lock() {
                    let _ = file.write_all(&buf[..n]);
                }
                let _ = terminal.write_all(&buf[..n]);
            }
        });
        Sink::Shared(shared)
    }
}

#[cfg(windows)]
fn point_stderr_at(file: std::fs::File) {
    use std::os::windows::io::IntoRawHandle;
    #[link(name = "kernel32")]
    extern "system" {
        fn SetStdHandle(which: u32, handle: *mut std::ffi::c_void) -> i32;
    }
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    // SAFETY: hands the process a handle it keeps for its lifetime; Rust's stderr looks
    // the standard handle up on every write. The handle it replaces is left open, as a
    // write may be using it.
    unsafe {
        SetStdHandle(STD_ERROR_HANDLE, file.into_raw_handle() as _);
    }
}

#[cfg(windows)]
fn redirect_stderr(file: std::fs::File) -> Sink {
    point_stderr_at(file);
    Sink::Stderr
}

#[cfg(not(any(unix, windows)))]
fn point_stderr_at(_: std::fs::File) {}

#[cfg(not(any(unix, windows)))]
fn redirect_stderr(_: std::fs::File) -> Sink {
    Sink::None
}

/// Write a crash report for every panic, then let the usual message print to the log.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let report = crash_report(
            thread.name().unwrap_or("unnamed"),
            &info.to_string(),
            &std::backtrace::Backtrace::force_capture().to_string(),
        );
        let dir = logs_dir();
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(format!("crash-{}.txt", stamp())), report);
        prune(&dir, "crash-", CRASHES_KEPT);
        previous(info);
    }));
}

fn crash_report(thread: &str, message: &str, backtrace: &str) -> String {
    format!(
        "Squad Code {} on {} {}\nTime: {}\nThread: {thread}\n\n{message}\n\nBacktrace:\n{backtrace}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
    )
}

/// Mark a download as coming from the internet, as a browser would, so macOS checks
/// its signature and notarization when it is opened.
#[cfg(target_os = "macos")]
fn mark_downloaded(path: &Path) -> Result<(), String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let ok = std::process::Command::new("xattr")
        .args(["-w", "com.apple.quarantine", &format!("0081;{now:x};Runo;")])
        .arg(path)
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        return Err("Could not mark the download for macOS to check.".into());
    }
    // A disk image can be checked now: refuse one Gatekeeper wouldn't open.
    if path.extension().is_some_and(|e| e == "dmg") {
        let out = std::process::Command::new("spctl")
            .args([
                "--assess",
                "--type",
                "open",
                "--context",
                "context:primary-signature",
            ])
            .arg(path)
            .output()
            .map_err(|e| format!("Could not run spctl: {e}"))?;
        if !out.status.success() {
            let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
            warn(
                "app-update",
                format!("Gatekeeper rejected {}: {}", path.display(), gist(&why)),
            );
            return Err("macOS says the disk image isn't signed and notarized.".into());
        }
    }
    Ok(())
}

/// Mark a download as coming from the internet (Mark of the Web), so Windows checks it
/// with SmartScreen when it is opened.
#[cfg(windows)]
fn mark_downloaded(path: &Path) -> Result<(), String> {
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    std::fs::write(&stream, "[ZoneTransfer]\r\nZoneId=3\r\n")
        .map_err(|e| format!("Could not mark the download for Windows to check: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("runo-applog-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn log_lines_carry_time_level_and_area_on_one_line() {
        let line = format_line(
            "2026-10-14 09:30:00.123",
            Level::Warn,
            "git",
            "Push failed\n  remote: rejected\n\n",
        );
        assert_eq!(
            line,
            "2026-10-14 09:30:00.123 WARN  [git] Push failed | remote: rejected\n"
        );
        assert!(format_line("t", Level::Error, "save", "x").starts_with("t ERROR [save] x"));
        assert!(format_line("t", Level::Info, "turn", "x").starts_with("t INFO  [turn] x"));
        let long = format_line("t", Level::Info, "a", &"y".repeat(5000));
        assert!(long.chars().count() < LINE_LIMIT + 40);
        assert!(long.ends_with("…\n"));
        // Writing never panics, whatever is in the message.
        warn("test", "a line from the tests");
        warn_once("test", "only once");
        warn_once("test", "only once");
    }

    #[test]
    fn secrets_are_masked() {
        let cases = [
            (
                "remote: https://x-access-token:ghp_abcdefghijklmnop1234@github.com/o/r.git",
                "remote: https://***@github.com/o/r.git",
            ),
            ("using ghp_abcdefghijklmnop1234 now", "using ghp_*** now"),
            (
                "key github_pat_11ABCDEFG0123456789_abcdefghijk",
                "key github_pat_***",
            ),
            (
                "ANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnop",
                "ANTHROPIC_API_KEY=***",
            ),
            (
                "Invalid key sk-proj-abcdefghijklmnopqrstu",
                "Invalid key sk-***",
            ),
            (
                "Authorization: Bearer abc.def.ghi",
                "Authorization: Bearer ***",
            ),
            ("authorization: token abcdef123", "authorization: token ***"),
            ("--token=s3cret --verbose", "--token=*** --verbose"),
            (r#"{"api_key":"abc123"}"#, r#"{"api_key":"***"}"#),
            ("password: hunter2", "password: ***"),
        ];
        for (input, want) in cases {
            assert_eq!(redact(input), want, "{input}");
        }
        // Ordinary errors pass through untouched.
        for plain in [
            "fatal: not a git repository (or any of the parent directories): .git",
            "error: failed to push some refs to 'https://github.com/o/r.git'",
            "the task-list is skipped",
            "could not start \"claude\": No such file or directory (os error 2)",
        ] {
            assert_eq!(redact(plain), plain);
        }
        // And the log line masks too.
        let line = format_line(
            "t",
            Level::Warn,
            "gh",
            "token=abcdef ghp_abcdefghijklmnop1234",
        );
        assert!(
            !line.contains("abcdef ") && !line.contains("1234"),
            "{line}"
        );
    }

    #[test]
    fn old_logs_are_pruned() {
        let dir = temp("prune");
        for n in 0..5 {
            std::fs::write(dir.join(format!("runo-2026100{n}.log")), "").unwrap();
        }
        std::fs::write(dir.join("crash-1.txt"), "").unwrap();
        prune(&dir, "runo-", 2);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            ["crash-1.txt", "runo-20261003.log", "runo-20261004.log"]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    fn logs_temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("runo-applog-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_full_log_moves_aside_keeping_a_few_pieces() {
        let dir = logs_temp("parts");
        let path = dir.join("runo-20261025-100000.log");
        for n in 0..5 {
            std::fs::write(&path, format!("piece {n}")).unwrap();
            shift_parts(&path, 3);
        }
        std::fs::write(&path, "current").unwrap();
        assert_eq!(
            names_in(&dir),
            [
                "runo-20261025-100000.log",
                "runo-20261025-100000.log.1",
                "runo-20261025-100000.log.2"
            ]
        );
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
        assert_eq!(read("runo-20261025-100000.log.1"), "piece 4");
        assert_eq!(read("runo-20261025-100000.log.2"), "piece 3");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn old_launches_go_with_all_their_pieces() {
        let dir = logs_temp("launches");
        for name in [
            "runo-20261023-090000.log",
            "runo-20261023-090000.log.1",
            "runo-20261024-090000.log",
            "runo-20261025-090000.log",
            "runo-20261025-090000.log.1",
            "runo-20261025-090000.log.2",
            "crash-20261023-090000.txt",
        ] {
            std::fs::write(dir.join(name), "x").unwrap();
        }
        prune_launches(&dir, 2);
        assert_eq!(
            names_in(&dir),
            [
                "crash-20261023-090000.txt",
                "runo-20261024-090000.log",
                "runo-20261025-090000.log",
                "runo-20261025-090000.log.1",
                "runo-20261025-090000.log.2"
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
