//! Interactive shells: a real PTY per terminal tab, parsed with a VT100 emulator.

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

/// Lines kept above the visible screen, from the Terminal settings.
static SCROLLBACK: AtomicUsize = AtomicUsize::new(10_000);

/// Set how many lines new shells keep above the screen.
pub fn set_scrollback(lines: usize) {
    SCROLLBACK.store(lines.clamp(500, 100_000), Ordering::Relaxed);
}

pub struct Shell {
    parser: Arc<Mutex<vt100::Parser>>,
    /// Bytes for the program, written in order by the shell's writer thread so a program
    /// that stops reading never blocks the window.
    input: Sender<Vec<u8>>,
    master: Box<dyn MasterPty + Send>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    exited: Arc<AtomicBool>,
    size: Mutex<(u16, u16)>,
    /// When the person last typed, in the same clock.
    last_input: AtomicU64,
}

/// A program to run in the PTY instead of the login shell.
pub struct Program {
    pub bin: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Variables to drop from the inherited environment.
    pub unset: Vec<String>,
}

impl Program {}

/// Interactive shell for a new Windows terminal tab. Prefer PowerShell over cmd.exe,
/// and skip the profile so a slow or blocking profile cannot leave a blank ConPTY.
fn windows_shell() -> Option<Program> {
    if !cfg!(windows) {
        return None;
    }
    Some(Program {
        bin: "powershell.exe".into(),
        args: vec!["-NoLogo".into(), "-NoProfile".into()],
        env: Vec::new(),
        unset: Vec::new(),
    })
}

/// Reap `child` and store 1 or 2 in `outcome`. The lock is not held while waiting,
/// so the shell can still be killed. Returns without a status if waiting fails.
/// Safe to call from more than one thread; the first status wins. It looks again after
/// 10 ms, then less and less often, up to once a second: the read ending (Unix) calls it
/// again and finds the status at once, so the slow looks only matter where the read never
/// ends (Windows), and a quiet terminal doesn't wake 100 times a second.
fn record_exit(child: &Mutex<Box<dyn Child + Send + Sync>>, outcome: &AtomicU8) {
    let mut wait = std::time::Duration::from_millis(10);
    loop {
        if outcome.load(Ordering::Acquire) != 0 {
            return;
        }
        let polled = {
            let Ok(mut child) = child.lock() else {
                return;
            };
            child.try_wait()
        };
        match polled {
            Ok(Some(status)) => {
                let value = if status.success() { 1 } else { 2 };
                let _ = outcome.compare_exchange(0, value, Ordering::Release, Ordering::Relaxed);
                return;
            }
            Ok(None) => {
                std::thread::sleep(wait);
                wait = (wait * 2).min(std::time::Duration::from_secs(1));
            }
            Err(_) => return,
        }
    }
}

/// Say the program has ended, once. `outcome` must already be stored.
fn note_exit(exited: &AtomicBool, wake: &Mutex<Box<dyn Fn(u64) + Send>>, id: u64) {
    if !exited.swap(true, Ordering::AcqRel) {
        if let Ok(wake) = wake.lock() {
            wake(id);
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Shell {
    /// Start `program` (or the login shell) in `dir`.
    pub fn spawn_program(
        id: u64,
        dir: &str,
        rows: u16,
        cols: u16,
        program: Option<Program>,
        wake: impl Fn(u64) + Send + 'static,
    ) -> Result<Self, String> {
        crate::shellenv::ready();
        let (rows, cols) = (rows.max(2), cols.max(10));
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;
        // On Windows, prefer PowerShell over cmd.exe: ConPTY often leaves cmd with a
        // blank screen and no prompt until something else wakes it.
        let program = program.or_else(windows_shell);
        let mut cmd = match &program {
            Some(p) => {
                let mut c = CommandBuilder::new(&p.bin);
                c.args(&p.args);
                for k in &p.unset {
                    c.env_remove(k);
                }
                for (k, v) in &p.env {
                    c.env(k, v);
                }
                c
            }
            None => CommandBuilder::new_default_prog(),
        };
        if !dir.is_empty() {
            cmd.cwd(dir);
        }
        // xterm TERM confuses some Windows hosts in ConPTY; keep it for Unix only.
        if !cfg!(windows) {
            cmd.env("TERM", "xterm-256color");
            cmd.env("COLORTERM", "truecolor");
        }
        // Light text on a dark background, or the reverse, for programs that read it
        // rather than ask the terminal.
        cmd.env(
            "COLORFGBG",
            if crate::settings::is_dark() {
                "15;0"
            } else {
                "0;15"
            },
        );
        let child = Arc::new(Mutex::new(
            pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?,
        ));
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(
            rows,
            cols,
            SCROLLBACK.load(Ordering::Relaxed),
        )));
        let (input, inbox) = std::sync::mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut writer = writer;
            for bytes in inbox {
                if writer
                    .write_all(&bytes)
                    .and_then(|_| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        let exited = Arc::new(AtomicBool::new(false));
        let outcome = Arc::new(AtomicU8::new(0));
        let dirty = Arc::new(AtomicBool::new(false));
        let last_output = Arc::new(AtomicU64::new(now_ms()));
        let wake = Arc::new(Mutex::new(Box::new(wake) as Box<dyn Fn(u64) + Send>));
        {
            // The process can end before the PTY read does (Windows often never
            // signals EOF), or the read can end first (Unix). Either way the status
            // is stored before `exited` flips.
            let (outcome, child, exited, last, wake) = (
                outcome.clone(),
                Arc::clone(&child),
                exited.clone(),
                last_output.clone(),
                Arc::clone(&wake),
            );
            std::thread::spawn(move || {
                record_exit(&child, &outcome);
                let started = std::time::Instant::now();
                let mut quiet = 0u8;
                while started.elapsed() < std::time::Duration::from_millis(500) {
                    if exited.load(Ordering::Acquire) {
                        return;
                    }
                    let mark = last.load(Ordering::Acquire);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    if last.load(Ordering::Acquire) == mark {
                        quiet += 1;
                        if quiet >= 2 {
                            break;
                        }
                    } else {
                        quiet = 0;
                    }
                }
                note_exit(&exited, &wake, id);
            });
        }
        {
            let (parser, exited, outcome, child, dirty, last, answers, wake) = (
                parser.clone(),
                exited.clone(),
                outcome.clone(),
                Arc::clone(&child),
                dirty.clone(),
                last_output.clone(),
                input.clone(),
                Arc::clone(&wake),
            );
            std::thread::spawn(move || {
                // Large reads, so a flood of output takes the parser's lock (which the
                // view needs to draw) fewer times.
                let mut buf = vec![0u8; 64 * 1024];
                let mut queries = Queries::default();
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            last.store(now_ms(), Ordering::Release);
                            let mut reply = Vec::new();
                            if let Ok(mut p) = parser.lock() {
                                // Answer each question after the output before it, so a
                                // cursor report says where that output left the cursor.
                                let mut start = 0;
                                for i in 0..n {
                                    if let Some(q) = queries.feed(buf[i]) {
                                        p.process(&buf[start..=i]);
                                        start = i + 1;
                                        reply.extend(q.answer(p.screen()));
                                    }
                                }
                                p.process(&buf[start..n]);
                            }
                            if !reply.is_empty() {
                                let _ = answers.send(reply);
                            }
                            if !dirty.swap(true, Ordering::AcqRel) {
                                if let Ok(wake) = wake.lock() {
                                    wake(id);
                                }
                            }
                        }
                    }
                }
                // The PTY can close before the process is reaped. Record the status
                // first, then say it has exited, so success() is never a guess.
                record_exit(&child, &outcome);
                note_exit(&exited, &wake, id);
            });
        }
        let shell = Self {
            parser,
            input,
            master: pair.master,
            child,
            exited,
            size: Mutex::new((rows, cols)),
            last_input: AtomicU64::new(0),
        };
        // ConPTY sometimes holds the first paint until the size changes: nudge it.
        let _ = shell.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        Ok(shell)
    }

    pub fn write(&self, bytes: &[u8]) {
        self.last_input.store(now_ms(), Ordering::Release);
        let _ = self.input.send(bytes.to_vec());
        // Typing jumps back to the live screen.
        self.scroll_to(0);
    }

    /// Paste text, wrapped in bracketed-paste markers when the program asked for them.
    pub fn paste(&self, text: &str) {
        let bracketed = self.with_screen(|s| s.bracketed_paste());
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        if bracketed {
            self.write(format!("\x1b[200~{text}\x1b[201~").as_bytes());
        } else {
            self.write(text.as_bytes());
        }
    }

    pub fn resize(&self, rows: u16, cols: u16) {
        let (rows, cols) = (rows.max(2), cols.max(10));
        let Ok(mut size) = self.size.lock() else {
            return;
        };
        if *size == (rows, cols) {
            return;
        }
        *size = (rows, cols);
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        if let Ok(mut p) = self.parser.lock() {
            set_size_keeping_cursor(&mut p, rows, cols);
        }
    }

    pub fn with_screen<R>(&self, f: impl FnOnce(&vt100::Screen) -> R) -> R {
        let p = self.parser.lock().unwrap_or_else(|e| e.into_inner());
        f(p.screen())
    }

    fn scroll_to(&self, lines: usize) {
        if let Ok(mut p) = self.parser.lock() {
            p.set_scrollback(lines);
        }
    }

    pub fn exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    pub fn kill(&self) {
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
        }
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        self.kill();
        // Closing the input channel ends the writer thread before ConPTY tears down.
        // Field drop would close `master` first; take the sender here instead.
        drop(std::mem::replace(
            &mut self.input,
            std::sync::mpsc::channel().0,
        ));
    }
}

/// A question a program asks the terminal itself rather than the person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Query {
    /// `CSI 5 n`: are you working?
    Status,
    /// `CSI 6 n`, or `CSI ? 6 n`: where is the cursor?
    Cursor { private: bool },
    /// `CSI c`: what kind of terminal are you?
    Primary,
    /// `CSI > c`: which version?
    Secondary,
    /// `OSC 10 ; ?` or `OSC 11 ; ?`: what are your text and background colours? CLIs
    /// such as Claude Code ask, to pick shades that suit the theme; one left guessing
    /// takes the terminal for a light one and paints light panels over a dark theme.
    Color { background: bool, bel: bool },
}

impl Query {
    /// What xterm would reply.
    fn answer(self, screen: &vt100::Screen) -> Vec<u8> {
        match self {
            Query::Status => b"\x1b[0n".to_vec(),
            Query::Cursor { private } => {
                let (row, col) = screen.cursor_position();
                let mark = if private { "?" } else { "" };
                format!("\x1b[{mark}{};{}R", row + 1, col + 1).into_bytes()
            }
            Query::Primary => b"\x1b[?62;22c".to_vec(),
            Query::Secondary => b"\x1b[>0;10;1c".to_vec(),
            Query::Color { background, bel } => {
                let c = if background {
                    crate::settings::c_term_bg()
                } else {
                    crate::settings::c_text()
                };
                // xterm's form: each channel as four hex digits.
                let ch = |shift: u32| (c >> shift) & 0xff;
                let (r, g, b) = (ch(16), ch(8), ch(0));
                format!(
                    "\x1b]{};rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}{}",
                    if background { 11 } else { 10 },
                    if bel { "\x07" } else { "\x1b\\" }
                )
                .into_bytes()
            }
        }
    }
}

/// Spots questions in a program's output as it streams in, across reads. Prompts such
/// as gh's Y/n ask where the cursor is and take every key as part of the answer until
/// one arrives, so a terminal that stays silent looks frozen, Ctrl-C included.
#[derive(Default)]
struct Queries {
    /// 0 outside a sequence, 1 after ESC, 2 inside a CSI sequence, 3 inside an OSC
    /// string, 4 after an ESC inside one.
    state: u8,
    params: Vec<u8>,
}

impl Queries {
    /// Take one byte of output; returns the question it completes, if any.
    fn feed(&mut self, b: u8) -> Option<Query> {
        match (self.state, b) {
            (3, 0x07) | (4, b'\\') => {
                let bel = self.state == 3;
                self.state = 0;
                match self.params.as_slice() {
                    b"10;?" => Some(Query::Color {
                        background: false,
                        bel,
                    }),
                    b"11;?" => Some(Query::Color {
                        background: true,
                        bel,
                    }),
                    _ => None,
                }
            }
            (3, 0x1b) => {
                self.state = 4;
                None
            }
            (3, _) => {
                if self.params.len() < 16 {
                    self.params.push(b);
                }
                None
            }
            (_, 0x1b) => {
                self.state = 1;
                self.params.clear();
                None
            }
            (1, b'[') => {
                self.state = 2;
                None
            }
            (1, b']') => {
                self.state = 3;
                None
            }
            (2, 0x20..=0x3f) => {
                if self.params.len() < 16 {
                    self.params.push(b);
                }
                None
            }
            (2, 0x40..=0x7e) => {
                self.state = 0;
                match (b, self.params.as_slice()) {
                    (b'n', b"5") => Some(Query::Status),
                    (b'n', b"6") => Some(Query::Cursor { private: false }),
                    (b'n', b"?6") => Some(Query::Cursor { private: true }),
                    (b'c', b"" | b"0") => Some(Query::Primary),
                    (b'c', b">" | b">0") => Some(Query::Secondary),
                    _ => None,
                }
            }
            _ => {
                self.state = 0;
                None
            }
        }
    }
}

/// Bytes a keystroke sends to the program, xterm style.
pub fn key_bytes(
    key: &str,
    key_char: Option<&str>,
    ctrl: bool,
    alt: bool,
    shift: bool,
    app_cursor: bool,
) -> Option<Vec<u8>> {
    let cursor = |c: char| -> Vec<u8> {
        let mods = 1 + shift as u8 + 2 * alt as u8 + 4 * ctrl as u8;
        if mods > 1 {
            format!("\x1b[1;{mods}{c}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{c}").into_bytes()
        } else {
            format!("\x1b[{c}").into_bytes()
        }
    };
    let bytes = match key {
        "enter" => b"\r".to_vec(),
        "backspace" if alt => b"\x1b\x7f".to_vec(),
        "backspace" if ctrl => b"\x08".to_vec(),
        "backspace" => b"\x7f".to_vec(),
        "tab" if shift => b"\x1b[Z".to_vec(),
        "tab" => b"\t".to_vec(),
        "escape" => b"\x1b".to_vec(),
        "up" => cursor('A'),
        "down" => cursor('B'),
        "right" => cursor('C'),
        "left" => cursor('D'),
        "home" => cursor('H'),
        "end" => cursor('F'),
        "pageup" => b"\x1b[5~".to_vec(),
        "pagedown" => b"\x1b[6~".to_vec(),
        "delete" => b"\x1b[3~".to_vec(),
        "insert" => b"\x1b[2~".to_vec(),
        "space" if ctrl => vec![0],
        _ if ctrl && key.chars().count() == 1 => {
            let c = key.chars().next()?.to_ascii_lowercase();
            let b = match c {
                'a'..='z' => c as u8 - b'a' + 1,
                '[' | '3' => 0x1b,
                '\\' | '4' => 0x1c,
                ']' | '5' => 0x1d,
                '6' => 0x1e,
                '/' | '-' | '7' => 0x1f,
                '@' | '2' => 0,
                _ => return None,
            };
            if alt {
                vec![0x1b, b]
            } else {
                vec![b]
            }
        }
        _ => {
            // GPUI on Windows sometimes omits key_char; fall back to the key name
            // when it is a single printable character.
            let text = key_char
                .map(str::to_string)
                .or_else(|| (key == "space").then(|| " ".to_string()))
                .or_else(|| {
                    let mut chars = key.chars();
                    let c = chars.next()?;
                    if chars.next().is_none() && !c.is_control() {
                        Some(c.to_string())
                    } else {
                        None
                    }
                })?;
            let mut v = if alt { vec![0x1b] } else { vec![] };
            v.extend(text.into_bytes());
            v
        }
    };
    Some(bytes)
}

/// xterm's 256-colour palette, with the first 16 tuned to the app's theme.
pub fn palette(i: u8, dark: bool) -> u32 {
    match i {
        0..=15 => crate::themes::get(dark).ansi[i as usize],
        16..=231 => {
            let i = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v as u32 };
            (level(i / 36) << 16) | (level((i / 6) % 6) << 8) | level(i % 6)
        }
        _ => {
            let v = 8 + 10 * (i as u32 - 232);
            (v << 16) | (v << 8) | v
        }
    }
}

/// Resize the screen without losing its newest lines. Made shorter, vt100 drops the
/// rows at the bottom, which are the newest output and the prompt; so the rows above the
/// cursor scroll up into the scrollback first, as a terminal does. A full-screen program
/// redraws itself for the new size, so its screen is left as it is.
fn set_size_keeping_cursor(p: &mut vt100::Parser, rows: u16, cols: u16) {
    let (old_rows, _) = p.screen().size();
    let (row, col) = p.screen().cursor_position();
    if rows < old_rows && row >= rows && !p.screen().alternate_screen() {
        let shift = row + 1 - rows;
        // From the bottom row, each line feed scrolls the screen up by one.
        let mut bytes = format!("\x1b[{old_rows};1H").into_bytes();
        bytes.extend(std::iter::repeat_n(b'\n', usize::from(shift)));
        p.process(&bytes);
        p.set_size(rows, cols);
        p.process(format!("\x1b[{};{}H", row - shift + 1, col + 1).as_bytes());
    } else {
        p.set_size(rows, cols);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shorter_screen_keeps_the_newest_lines() {
        let mut p = vt100::Parser::new(10, 20, 100);
        for i in 1..=15 {
            p.process(format!("line {i}\r\n").as_bytes());
        }
        p.process(b"$ ");
        set_size_keeping_cursor(&mut p, 6, 20);
        let text = p.screen().contents();
        let last: Vec<&str> = text.lines().collect();
        assert_eq!(last.len(), 6);
        assert_eq!(last[4], "line 15");
        assert_eq!(last[5].trim_end(), "$");
        assert_eq!(p.screen().cursor_position(), (5, 2));
        // What scrolled off is in the scrollback.
        p.set_scrollback(4);
        assert!(p.screen().contents().starts_with("line 7"));
        // Short output that still fits stays where it is.
        let mut q = vt100::Parser::new(10, 20, 100);
        q.process(b"one\r\ntwo");
        set_size_keeping_cursor(&mut q, 6, 20);
        assert!(q.screen().contents().starts_with("one\ntwo"));
        assert_eq!(q.screen().cursor_position(), (1, 3));
    }

    #[test]
    fn keys_map_to_xterm_sequences() {
        let k = |key, ch: Option<&str>, ctrl, alt| key_bytes(key, ch, ctrl, alt, false, false);
        assert_eq!(k("c", Some("c"), true, false), Some(vec![3]));
        assert_eq!(k("a", Some("a"), false, false), Some(b"a".to_vec()));
        assert_eq!(k("a", None, false, false), Some(b"a".to_vec()));
        assert_eq!(k("b", Some("b"), false, true), Some(b"\x1bb".to_vec()));
        assert_eq!(k("enter", None, false, false), Some(b"\r".to_vec()));
        assert_eq!(k("up", None, false, false), Some(b"\x1b[A".to_vec()));
        assert_eq!(
            key_bytes("up", None, false, false, false, true),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(k("left", None, true, false), Some(b"\x1b[1;5D".to_vec()));
        assert_eq!(k("shift", None, false, false), None);
        assert_eq!(palette(196, true), 0xff0000);
        assert_eq!(palette(232, true), 0x080808);
    }

    /// Questions to the terminal are spotted even when they arrive split across reads.
    #[test]
    fn queries_are_found_across_reads() {
        let mut q = Queries::default();
        let found = |q: &mut Queries, bytes: &[u8]| -> Vec<Query> {
            bytes.iter().filter_map(|b| q.feed(*b)).collect()
        };
        assert_eq!(found(&mut q, b"hi \x1b[6"), vec![]);
        assert_eq!(
            found(&mut q, b"n there"),
            vec![Query::Cursor { private: false }]
        );
        assert_eq!(
            found(&mut q, b"\x1b[c\x1b[>c\x1b[5n\x1b[?6n"),
            vec![
                Query::Primary,
                Query::Secondary,
                Query::Status,
                Query::Cursor { private: true }
            ]
        );
        // Colours, cursor moves and other sequences are not questions.
        assert_eq!(
            found(&mut q, b"\x1b[31m\x1b[2J\x1b[6;1H\x1b]0;t\x07"),
            vec![]
        );
        // Colour questions, ended by BEL or by ST.
        assert_eq!(
            found(&mut q, b"\x1b]11;?\x07\x1b]10;?\x1b\\"),
            vec![
                Query::Color {
                    background: true,
                    bel: true
                },
                Query::Color {
                    background: false,
                    bel: false
                }
            ]
        );
    }

    #[test]
    fn dim_and_strikethrough_reach_the_cells() {
        let mut p = vt100::Parser::new(2, 20, 0);
        p.process(b"\x1b[1;2ma\x1b[22mb\x1b[9mc\x1b[29md");
        let s = p.screen();
        let cell = |col| s.cell(0, col).unwrap();
        assert!(cell(0).dim() && cell(0).bold());
        assert!(!cell(1).dim() && !cell(1).bold());
        assert!(cell(2).strikethrough());
        assert!(!cell(3).strikethrough());
        // The formatted copy keeps both, so a restored screen looks the same.
        let mut copy = vt100::Parser::new(2, 20, 0);
        copy.process(&s.contents_formatted());
        assert!(copy.screen().cell(0, 0).unwrap().dim());
        assert!(copy.screen().cell(0, 2).unwrap().strikethrough());
        assert!(!copy.screen().cell(0, 3).unwrap().strikethrough());
    }
}
