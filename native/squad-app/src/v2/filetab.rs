//! The Files tab of the right sidebar: the folder in view as a tree, with the
//! files that changed marked, and a box to find a file by name.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Input;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::ui::{self, c};
use super::Shell;
use crate::files::TreeRow;

/// Files listed at most, so a huge folder stays quick.
const MAX_FILES: usize = 4000;
/// Rows shown at most, in the tree or as matches.
const MAX_ROWS: usize = 400;
/// How long a listing is trusted before the folder is looked at again.
const FRESH: Duration = Duration::from_secs(12);

/// Folders never worth walking into outside git: they are generated or huge.
const SKIP: [&str; 8] = [
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    "__pycache__",
    ".venv",
];

/// Every file under `dir`, as paths from its top: what git knows or would add
/// in a repository, else a walk that skips hidden and generated folders.
pub(super) fn list_files(dir: &str) -> Vec<String> {
    if crate::git::is_repo(dir) {
        let mut files = crate::git::tracked_files(dir);
        files.truncate(MAX_FILES);
        return files;
    }
    let root = std::path::Path::new(dir);
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(at) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if !name.starts_with('.') && !SKIP.contains(&name.as_str()) {
                    stack.push(path);
                }
            } else if kind.is_file() {
                if let Ok(rel) = path.strip_prefix(root) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
                if out.len() >= MAX_FILES {
                    out.sort();
                    return out;
                }
            }
        }
    }
    out.sort();
    out
}

/// The colour of git's status letter for a changed file.
fn mark_color(code: &str) -> u32 {
    match code {
        "A" | "U" | "?" => ui::add(),
        "D" => ui::del(),
        _ => ui::t_amber(),
    }
}

impl Shell {
    /// The files in `dir`, listed again when they're stale or the folder changed.
    fn files_in(&mut self, dir: &str) -> Arc<Vec<String>> {
        if let Some((d, at, files)) = &self.files_seen {
            if d == dir && at.elapsed() < FRESH {
                return files.clone();
            }
        }
        let files = Arc::new(list_files(dir));
        self.files_seen = Some((dir.to_string(), Instant::now(), files.clone()));
        files
    }

    /// The Files tab for `dir`. `marks` are changed files, by path, with git's
    /// one-letter status; `title` says whose files these are.
    pub(super) fn files_side(
        &mut self,
        dir: &str,
        title: String,
        marks: HashMap<String, String>,
        cx: &mut Context<Self>,
    ) -> Div {
        let files = if dir.is_empty() {
            Arc::new(Vec::new())
        } else {
            self.files_in(dir)
        };
        let query = self.files_input.read(cx).value().trim().to_string();

        let find = div()
            .h(px(32.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .bg(c(ui::well()))
            .border_1()
            .border_color(c(ui::line2()))
            .child(
                Icon::new(IconName::Search)
                    .size(px(13.))
                    .text_color(c(ui::faint())),
            )
            .child(
                div().flex_1().min_w_0().text_size(px(12.5)).child(
                    Input::new(&self.files_input)
                        .appearance(false)
                        .bordered(false),
                ),
            );
        let changed = marks.len();
        let about = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(11.5))
            .text_color(c(ui::faint()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(ui::MONO)
                    .child(title),
            )
            .child(match files.len() {
                1 => "1 file".to_string(),
                n if n >= MAX_FILES => format!("{MAX_FILES}+ files"),
                n => format!("{n} files"),
            })
            .when(changed > 0, |d| {
                d.child("\u{b7}").child(
                    div()
                        .text_color(c(ui::t_amber()))
                        .child(format!("{changed} changed")),
                )
            });

        let mut list = div()
            .id("files-side")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(8.))
            .pb(px(24. + super::DOCK_ROOM))
            .flex()
            .flex_col();
        let quiet = |t: &str| {
            div()
                .px(px(12.))
                .py(px(16.))
                .text_size(px(12.5))
                .text_color(c(ui::faint()))
                .child(t.to_string())
        };
        if files.is_empty() {
            list = list.child(quiet(if dir.is_empty() {
                "Open a project folder to see its files."
            } else {
                "No files here yet."
            }));
        } else if !query.is_empty() {
            let hits = crate::files::filter_files(&files, &query);
            if hits.is_empty() {
                list = list.child(quiet("No file matches that."));
            }
            for (n, path) in hits.into_iter().take(MAX_ROWS).enumerate() {
                let name = path.rsplit('/').next().unwrap_or(path).to_string();
                let folder = path
                    .strip_suffix(name.as_str())
                    .unwrap_or("")
                    .trim_end_matches('/')
                    .to_string();
                let mark = marks.get(path.as_str()).cloned();
                list = list.child(self.file_row(
                    ("file-hit", n),
                    0,
                    name,
                    Some(folder),
                    mark,
                    path.clone(),
                    dir,
                    cx,
                ));
            }
        } else {
            // Folders holding a changed file show a dot, so a change is easy
            // to find while they're closed.
            let mut hot: std::collections::HashSet<String> = Default::default();
            for p in marks.keys() {
                let mut at = p.as_str();
                while let Some((up, _)) = at.rsplit_once('/') {
                    hot.insert(up.to_string());
                    at = up;
                }
            }
            let rows = crate::files::tree_rows(&files, &self.files_open);
            let more = rows.len().saturating_sub(MAX_ROWS);
            for (n, row) in rows.into_iter().take(MAX_ROWS).enumerate() {
                match row {
                    TreeRow::Folder {
                        path,
                        name,
                        depth,
                        open,
                    } => {
                        let warm = !open && hot.contains(&path);
                        list = list.child(
                            div()
                                .id(("file-folder", n))
                                .h(px(28.))
                                .pl(px(8. + depth as f32 * 14.))
                                .pr(px(8.))
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .rounded(px(6.))
                                .cursor_pointer()
                                .map(|d| super::motion::hover_fill(d, None, ui::hover()))
                                .child(
                                    Icon::new(if open {
                                        IconName::ChevronDown
                                    } else {
                                        IconName::ChevronRight
                                    })
                                    .size(px(12.))
                                    .text_color(c(ui::faint())),
                                )
                                .child(
                                    Icon::new(if open {
                                        IconName::FolderOpen
                                    } else {
                                        IconName::Folder
                                    })
                                    .size(px(14.))
                                    .text_color(c(ui::muted())),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(12.5))
                                        .text_color(c(ui::ink()))
                                        .child(name),
                                )
                                .when(warm, |d| {
                                    d.child(div().size(px(6.)).rounded_full().bg(c(ui::t_amber())))
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.files_open.remove(&path) {
                                        this.files_open.insert(path.clone());
                                    }
                                    cx.notify();
                                })),
                        );
                    }
                    TreeRow::File { path, name, depth } => {
                        let mark = marks.get(&path).cloned();
                        list = list.child(self.file_row(
                            ("file-row", n),
                            depth,
                            name,
                            None,
                            mark,
                            path,
                            dir,
                            cx,
                        ));
                    }
                }
            }
            if more > 0 {
                list = list.child(quiet(&format!(
                    "and {more} more. Open fewer folders, or find a file by name."
                )));
            }
        }

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .px(px(16.))
                    .pt(px(12.))
                    .pb(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(find)
                    .child(about),
            )
            .child(list)
    }

    /// One file in the Files tab: its icon, name, the folder it's in when
    /// listed flat, and git's letter when it changed. A click opens it.
    #[allow(clippy::too_many_arguments)]
    fn file_row(
        &mut self,
        id: (&'static str, usize),
        depth: usize,
        name: String,
        folder: Option<String>,
        mark: Option<String>,
        path: String,
        dir: &str,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let dir = dir.to_string();
        let act = super::rich::Act::File(path);
        let changed = mark.is_some();
        div()
            .id(id)
            .min_h(px(28.))
            .py(px(4.))
            .pl(px(8.
                + depth as f32 * 14.
                + if folder.is_some() { 0. } else { 18. }))
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(6.))
            .cursor_pointer()
            .map(|d| super::motion::hover_fill(d, None, ui::hover()))
            .child(
                Icon::new(IconName::FileText)
                    .size(px(13.))
                    .flex_none()
                    .text_color(c(ui::faint())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.5))
                            .when(changed, |d| d.font_weight(FontWeight::MEDIUM))
                            .text_color(c(if changed { ui::ink() } else { ui::body() }))
                            .child(name),
                    )
                    .when_some(folder.filter(|f| !f.is_empty()), |d, f| {
                        d.child(
                            div()
                                .truncate()
                                .text_size(px(11.))
                                .text_color(c(ui::faint()))
                                .child(f),
                        )
                    }),
            )
            .when_some(mark, |d, m| {
                d.child(
                    div()
                        .flex_none()
                        .font_family(ui::MONO)
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(c(mark_color(&m)))
                        .child(m),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| this.act(&act, &dir, window, cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::list_files;

    #[test]
    fn a_plain_folder_is_walked_without_hidden_or_generated_folders() {
        let base = std::env::temp_dir().join(format!("runo-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for p in [
            "src/main.rs",
            "README.md",
            "node_modules/x/i.js",
            ".hidden/a",
        ] {
            let f = base.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(&f, "x").unwrap();
        }
        let files = list_files(&base.to_string_lossy());
        assert_eq!(
            files,
            vec!["README.md".to_string(), "src/main.rs".to_string()]
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
