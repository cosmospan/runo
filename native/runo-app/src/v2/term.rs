//! The terminal drawer: a real shell at the bottom of a job, opened in the
//! job's own worktree, so you can poke at its branch (run it, test it, look at
//! git) without touching your checkout. `runo` works in it too, for driving the
//! crew from the keyboard.

use std::path::PathBuf;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::pty;

use super::store::Job;
use super::ui::{self, c};
use super::Shell;

/// Text size and row height in the drawer.
const FONT: f32 = 12.5;
const LINE: f32 = 17.;
/// Rows the drawer shows.
pub const ROWS: u16 = 14;
/// Width of one column, near enough for a monospace face at FONT.
const CELL: f32 = FONT * 0.6;

/// A folder with a `runo` command in it, put first on the drawer's PATH so
/// typing `runo` drives this app's crew.
fn shim_dir() -> Option<PathBuf> {
    let dir = crate::store::data_dir().join("bin");
    std::fs::create_dir_all(&dir).ok()?;
    let exe = std::env::current_exe().ok()?;
    if cfg!(windows) {
        let file = dir.join("runo.cmd");
        let text = format!("@echo off\r\n\"{}\" v2 %*\r\n", exe.display());
        std::fs::write(file, text).ok()?;
    } else {
        let file = dir.join("runo");
        let text = format!("#!/bin/sh\nexec \"{}\" v2 \"$@\"\n", exe.display());
        std::fs::write(&file, text).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755));
        }
    }
    Some(dir)
}

/// The shell the drawer runs: the person's own, with `runo` on its PATH.
fn program() -> Option<pty::Program> {
    if cfg!(windows) {
        return None;
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let mut env = vec![("RUNO_V2".to_string(), "1".to_string())];
    if let Some(dir) = shim_dir() {
        let path = std::env::var("PATH").unwrap_or_default();
        env.push(("PATH".into(), format!("{}:{path}", dir.display())));
    }
    Some(pty::Program {
        bin: shell,
        args: vec!["-l".into()],
        env,
        unset: Vec::new(),
    })
}

/// Quick commands over the drawer: what they're called and what they type.
pub fn quick(job: &Job) -> Vec<(&'static str, String)> {
    let mut v = vec![
        ("Status", "git status --short".to_string()),
        ("What changed", "git diff --stat".to_string()),
    ];
    if let Some(check) = super::work::checks_for(job.dir()).first() {
        v.push(("Run checks", check.clone()));
    }
    v.push(("Crew", "runo ls".to_string()));
    v
}

impl Shell {
    /// Opens or closes the drawer on a job, starting its shell the first time.
    pub(super) fn toggle_term(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.term_open == Some(id) {
            self.term_open = None;
            cx.notify();
            return;
        }
        let dir = match self.store.job(id) {
            Some(j) => j.dir().to_string(),
            None => return,
        };
        let gone = self.terms.get(&id).is_some_and(|t| t.exited());
        if gone {
            self.terms.remove(&id);
        }
        if !self.terms.contains_key(&id) {
            let cols = self.term_cols;
            match pty::Shell::spawn_program(id, id, &dir, ROWS, cols, program(), |_| {}) {
                Ok(sh) => {
                    self.terms.insert(id, sh);
                }
                Err(why) => {
                    self.say(format!("Couldn't open a terminal: {why}"));
                    cx.notify();
                    return;
                }
            }
        }
        self.term_open = Some(id);
        window.focus(&self.term_focus, cx);
        // Not handed back to the window's own focus while the drawer appears.
        self.focus_moved = std::time::Instant::now();
        self.term_grab = 6;
        // The drawer isn't drawn yet: focus it again once it is.
        let h = self.term_focus.clone();
        window.on_next_frame(move |window, cx| window.focus(&h, cx));
        cx.notify();
    }

    /// Types a command into the drawer's shell and runs it.
    pub(super) fn term_run(
        &mut self,
        id: u64,
        command: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.term_open != Some(id) {
            self.toggle_term(id, window, cx);
        }
        if let Some(t) = self.terms.get(&id) {
            t.write(format!("{command}\r").as_bytes());
        }
        window.focus(&self.term_focus, cx);
    }

    /// The drawer under a job, when it's open.
    pub(super) fn term_drawer(&mut self, job: &Job, cx: &mut Context<Self>) -> Option<Div> {
        let id = job.id;
        if self.term_open != Some(id) {
            return None;
        }
        // Fit the shell to the drawer's width.
        let width = self.view_w - 560.;
        let cols = ((width / CELL).floor() as u16).clamp(40, 400);
        if cols != self.term_cols {
            self.term_cols = cols;
        }
        let sh = self.terms.get(&id)?;
        sh.resize(ROWS, cols);
        let focused = self.term_focused;
        let exited = sh.exited();
        let color = |col: vt100::Color, default: u32| match col {
            vt100::Color::Default => default,
            vt100::Color::Idx(i) => pty::palette(i, true),
            vt100::Color::Rgb(r, g, b) => ((r as u32) << 16) | ((g as u32) << 8) | b as u32,
        };
        let lines = sh.with_screen(|screen| {
            let mut lines = div().flex().flex_col();
            let (rows, cols) = screen.size();
            let (cur_row, cur_col) = screen.cursor_position();
            let show_cursor = !screen.hide_cursor() && screen.scrollback() == 0 && !exited;
            for row in 0..rows {
                let mut line = div().h(px(LINE)).flex().whitespace_nowrap();
                let mut run = String::new();
                let mut style: Option<(u32, Option<u32>, bool)> = None;
                let flush =
                    |line: Div, run: &mut String, style: Option<(u32, Option<u32>, bool)>| {
                        let Some((fg, bg, bold)) = style else {
                            return line;
                        };
                        if run.is_empty() {
                            return line;
                        }
                        let text = std::mem::take(run);
                        line.child(
                            div()
                                .flex_none()
                                .text_color(c(fg))
                                .when_some(bg, |d, bg| d.bg(c(bg)))
                                .when(bold, |d| d.font_weight(FontWeight::SEMIBOLD))
                                .child(text),
                        )
                    };
                for col in 0..cols {
                    let Some(cell) = screen.cell(row, col) else {
                        break;
                    };
                    if cell.is_wide_continuation() {
                        continue;
                    }
                    let mut fg = color(cell.fgcolor(), ui::BODY);
                    let mut bg = match cell.bgcolor() {
                        vt100::Color::Default => None,
                        x => Some(color(x, 0)),
                    };
                    if cell.inverse() {
                        let back = bg.unwrap_or(0x09090C);
                        bg = Some(fg);
                        fg = back;
                    }
                    if show_cursor && focused && row == cur_row && col == cur_col {
                        let back = bg.unwrap_or(0x09090C);
                        bg = Some(ui::INK);
                        fg = back;
                    }
                    let s = (fg, bg, cell.bold());
                    if style != Some(s) {
                        line = flush(line, &mut run, style);
                        style = Some(s);
                    }
                    match cell.first_char() {
                        Some(ch) => run.push(ch),
                        None => run.push(' '),
                    }
                }
                line = flush(line, &mut run, style);
                lines = lines.child(line);
            }
            lines
        });
        let mut bar = div()
            .h(px(36.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(6.))
            .border_b_1()
            .border_color(c(ui::LINE2))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c(ui::INK))
                    .child("Terminal"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(ui::MONO)
                    .text_size(px(11.))
                    .text_color(c(ui::FAINT))
                    .child(if job.branch.is_empty() {
                        job.dir().to_string()
                    } else {
                        format!("{} \u{b7} {}", job.branch, job.dir())
                    }),
            );
        for (k, (label, command)) in quick(job).into_iter().enumerate() {
            bar = bar.child(
                ui::pill(("term-quick", k), label, false)
                    .h(px(24.))
                    .text_size(px(12.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.term_run(id, &command, window, cx)
                    })),
            );
        }
        bar = bar.child(
            div()
                .id("term-close")
                .px(px(8.))
                .text_size(px(12.))
                .text_color(c(ui::MUTED))
                .cursor_pointer()
                .hover(|s| s.text_color(c(ui::INK)))
                .child("Close")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.term_open = None;
                    cx.notify();
                })),
        );
        Some(
            div()
                .flex_none()
                .bg(c(0x09090C))
                .border_t_1()
                .border_color(c(ui::LINE))
                .child(bar)
                .child(
                    div()
                        .id("term-screen")
                        .track_focus(&self.term_focus)
                        .px(px(12.))
                        .py(px(8.))
                        .font_family(ui::MONO)
                        .text_size(px(FONT))
                        .line_height(px(LINE))
                        .cursor_text()
                        .on_click(
                            cx.listener(|this, _, window, cx| window.focus(&this.term_focus, cx)),
                        )
                        .on_key_down(cx.listener(move |this, ev: &KeyDownEvent, _, cx| {
                            this.term_key(id, ev, cx)
                        }))
                        .child(lines)
                        .when(exited, |d| {
                            d.child(div().text_color(c(ui::FAINT)).child(
                                "The shell ended. Close and open the terminal again for a new one.",
                            ))
                        }),
                ),
        )
    }

    fn term_key(&mut self, id: u64, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(sh) = self.terms.get(&id) else {
            return;
        };
        let ks = &ev.keystroke;
        let m = ks.modifiers;
        let paste = (m.platform && ks.key == "v") || (m.control && m.shift && ks.key == "v");
        if paste {
            if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                sh.paste(&text);
            }
            cx.stop_propagation();
            return;
        }
        if m.platform {
            return;
        }
        let app_cursor = sh.with_screen(|s| s.application_cursor());
        if let Some(bytes) = pty::key_bytes(
            &ks.key,
            ks.key_char.as_deref(),
            m.control,
            m.alt,
            m.shift,
            app_cursor,
        ) {
            sh.write(&bytes);
            cx.stop_propagation();
            cx.notify();
        }
    }
}
