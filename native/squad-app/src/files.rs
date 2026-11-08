//! The Files tab: the session folder as a tree, a filter on file names, and search
//! in files (`git grep`, or a plain scan outside git).

/// One row of the tree: a folder or a file, at a depth.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TreeRow {
    Folder {
        path: String,
        name: String,
        depth: usize,
        open: bool,
    },
    File {
        path: String,
        name: String,
        depth: usize,
    },
}

/// The visible rows of a tree over sorted `files`: folders always show, their contents
/// only when the folder and every folder above it are open.
pub(crate) fn tree_rows(
    files: &[String],
    open: &std::collections::HashSet<String>,
) -> Vec<TreeRow> {
    let mut sorted: Vec<&String> = files.iter().collect();
    sorted.sort();
    let mut rows = Vec::new();
    let mut last: Vec<&str> = Vec::new();
    for path in sorted {
        let parts: Vec<&str> = path.split('/').collect();
        let (dirs, name) = parts.split_at(parts.len() - 1);
        let common = last
            .iter()
            .zip(dirs.iter())
            .take_while(|(a, b)| a == b)
            .count();
        // Folders below the first closed one are not shown.
        let shown = |depth: usize| (0..depth).all(|d| open.contains(&dirs[..=d].join("/")));
        for depth in common..dirs.len() {
            if shown(depth) {
                let folder = dirs[..=depth].join("/");
                rows.push(TreeRow::Folder {
                    open: open.contains(&folder),
                    path: folder,
                    name: dirs[depth].to_string(),
                    depth,
                });
            }
        }
        if shown(dirs.len()) {
            rows.push(TreeRow::File {
                path: path.clone(),
                name: name[0].to_string(),
                depth: dirs.len(),
            });
        }
        last = dirs.to_vec();
    }
    rows
}

/// Files whose path contains every word of `query`, any case.
pub(crate) fn filter_files<'a>(files: &'a [String], query: &str) -> Vec<&'a String> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    files
        .iter()
        .filter(|f| {
            let f = f.to_lowercase();
            words.iter().all(|w| f.contains(w.as_str()))
        })
        .collect()
}

