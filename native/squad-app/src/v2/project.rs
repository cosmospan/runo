//! A project's own file, `squad.toml` at the top of the repository, checked
//! in so everyone's crew sets up the same way. Every key is optional; what's
//! left out Squad works out for itself, as before.
//!
//! ```toml
//! # Commands a job's fresh worktree needs before its bot starts.
//! setup = ["pnpm install"]
//! # Files git ignores that each job's worktree should get a copy of.
//! copy = [".env.local"]
//! # How to start the app, and where it serves. $PORT is the job's own port.
//! run = "pnpm dev --port $PORT"
//! url = "http://localhost:$PORT"
//! # Each job's app gets its own port counting from here, so two jobs never
//! # fight over 3000.
//! port = 3100
//! # The checks Checker runs, instead of the ones Squad finds.
//! checks = ["pnpm lint", "pnpm test"]
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The file's name, at the top of the project.
pub const FILE: &str = "squad.toml";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub setup: Vec<String>,
    pub copy: Vec<String>,
    pub run: String,
    pub url: String,
    pub port: u16,
    pub checks: Vec<String>,
}

/// The project file in `dir`, when there is one. A file that doesn't parse
/// is reported, not ignored, so a typo doesn't quietly change nothing.
pub fn read(dir: &str) -> Result<Option<Project>, String> {
    let path = Path::new(dir).join(FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    toml::from_str::<Project>(&text)
        .map(Some)
        .map_err(|e| format!("{FILE} has a mistake: {}", e.message()))
}

/// The project file in `dir`, or nothing when it's missing or broken.
pub fn load(dir: &str) -> Option<Project> {
    read(dir).ok().flatten()
}

impl Project {
    /// The port job `id`'s app uses, when the file gives a starting port.
    pub fn port_for(&self, id: u64) -> Option<u16> {
        (self.port > 0).then(|| self.port.saturating_add((id % 100) as u16))
    }

    /// How to start the app for job `id`, and its address, with `$PORT` and
    /// `PORT` filled in. None when the file doesn't say how.
    pub fn start(&self, id: u64) -> Option<(String, Option<String>)> {
        let run = self.run.trim();
        if run.is_empty() {
            return None;
        }
        let port = self.port_for(id);
        let fill = |s: &str| match port {
            Some(p) => s
                .replace("$PORT", &p.to_string())
                .replace("${PORT}", &p.to_string()),
            None => s.to_string(),
        };
        let cmd = match port {
            Some(p) => format!("PORT={p} {}", fill(run)),
            None => fill(run),
        };
        let url = (!self.url.trim().is_empty()).then(|| fill(self.url.trim()));
        Some((cmd, url))
    }

    /// What a job's bot is told about setting up and running the project.
    pub fn brief(&self, id: u64) -> String {
        let mut out = Vec::new();
        if !self.setup.is_empty() {
            out.push(format!(
                "This is a fresh worktree: before anything else, set it up by running {}.",
                self.setup
                    .iter()
                    .map(|c| format!("`{c}`"))
                    .collect::<Vec<_>>()
                    .join(", then ")
            ));
        }
        if let Some((cmd, url)) = self.start(id) {
            out.push(format!(
                "To run the app, use `{cmd}`{}. Use that port, not the project's usual one: other jobs may be running their own copy.",
                url.map(|u| format!(" (it serves on {u})")).unwrap_or_default()
            ));
        }
        out.join(" ")
    }
}

/// Copies the files the project file names from the project into a job's
/// worktree, when they exist there and not here. Returns the ones copied.
pub fn copy_files(project: &Project, from: &str, to: &str) -> Vec<String> {
    let mut done = Vec::new();
    for rel in &project.copy {
        let rel = rel.trim().trim_start_matches("./");
        // Only paths inside the project.
        if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|p| p == "..") {
            continue;
        }
        let (src, dst) = (Path::new(from).join(rel), Path::new(to).join(rel));
        if !src.is_file() || dst.exists() {
            continue;
        }
        if let Some(parent) = dst.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::copy(&src, &dst).is_ok() {
            done.push(rel.to_string());
        }
    }
    done
}

/// The file's text for `p`, with the same notes as a starting file, so a
/// file saved from the Overview's form still explains itself.
pub fn render(p: &Project) -> String {
    let list = |v: &[String]| {
        v.iter()
            .map(|c| format!("{c:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "# Squad Code's project file: how every job sets up, runs and checks this project.\n\
         # Every key is optional. https://github.com/hyperhale/squadcode#the-project-file\n\n\
         # Commands a job's fresh worktree needs before its bot starts.\n\
         setup = [{}]\n\n\
         # Files git ignores that each job's worktree should get a copy of.\n\
         copy = [{}]\n\n\
         # How to start the app, and where it serves. $PORT is the job's own port.\n\
         run = {:?}\n\
         url = {:?}\n\n\
         # Each job's app gets its own port counting from here (0 turns it off).\n\
         port = {}\n\n\
         # The checks Checker runs on every finished job.\n\
         checks = [{}]\n",
        list(&p.setup),
        list(&p.copy),
        p.run,
        p.url,
        p.port,
        list(&p.checks),
    )
}

/// Saves `p` as the project file in `dir`.
pub fn write(dir: &str, p: &Project) -> Result<(), String> {
    std::fs::write(Path::new(dir).join(FILE), render(p)).map_err(|e| e.to_string())
}

/// A comma-separated line from the form as a list, dropping empty entries.
pub fn split_list(line: &str) -> Vec<String> {
    line.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// A starting file for `dir`, from what Squad finds there.
pub fn starter(dir: &str) -> String {
    let (run, url) = super::work::detect_start(dir).unwrap_or_default();
    let checks = super::work::detect_checks(dir);
    let list = |v: &[String]| {
        v.iter()
            .map(|c| format!("{c:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "# Squad Code's project file: how every job sets up, runs and checks this project.\n\
         # Every key is optional. https://github.com/hyperhale/squadcode#the-project-file\n\n\
         # Commands a job's fresh worktree needs before its bot starts.\n\
         setup = []\n\n\
         # Files git ignores that each job's worktree should get a copy of.\n\
         copy = []\n\n\
         # How to start the app, and where it serves. $PORT is the job's own port.\n\
         run = {run:?}\n\
         url = {url:?}\n\n\
         # Each job's app gets its own port counting from here (0 turns it off).\n\
         port = 0\n\n\
         # The checks Checker runs on every finished job.\n\
         checks = [{}]\n",
        list(&checks),
        url = url.unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_file_and_fills_in_the_port() {
        let p: Project = toml::from_str(
            "setup = [\"pnpm install\"]\nrun = \"pnpm dev --port $PORT\"\nurl = \"http://localhost:$PORT\"\nport = 3100\nchecks = [\"pnpm test\"]",
        )
        .unwrap();
        let (cmd, url) = p.start(7).unwrap();
        assert_eq!(cmd, "PORT=3107 pnpm dev --port 3107");
        assert_eq!(url.as_deref(), Some("http://localhost:3107"));
        assert_eq!(p.port_for(205), Some(3105));
        let b = p.brief(7);
        assert!(b.contains("`pnpm install`") && b.contains("3107"), "{b}");
    }

    #[test]
    fn no_port_leaves_the_command_alone() {
        let p = Project {
            run: "npm start".into(),
            ..Project::default()
        };
        assert_eq!(p.start(1), Some(("npm start".into(), None)));
        assert!(Project::default().start(1).is_none());
        assert!(Project::default().brief(1).is_empty());
    }

    #[test]
    fn copies_only_files_inside_the_project() {
        let base = std::env::temp_dir().join(format!("squad-proj-{}", std::process::id()));
        let (a, b) = (base.join("a"), base.join("b"));
        std::fs::create_dir_all(a.join("cfg")).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join(".env"), "K=1").unwrap();
        std::fs::write(a.join("cfg/local.json"), "{}").unwrap();
        let p = Project {
            copy: vec![
                ".env".into(),
                "cfg/local.json".into(),
                "../x".into(),
                "missing".into(),
            ],
            ..Project::default()
        };
        let done = copy_files(&p, a.to_str().unwrap(), b.to_str().unwrap());
        assert_eq!(done, vec![".env".to_string(), "cfg/local.json".to_string()]);
        assert!(b.join("cfg/local.json").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_saved_file_reads_back_the_same() {
        let p = Project {
            setup: vec!["pnpm install".into()],
            copy: vec![".env.local".into()],
            run: "pnpm dev --port $PORT".into(),
            url: "http://localhost:$PORT".into(),
            port: 3100,
            checks: vec!["pnpm lint".into(), "pnpm \"test\"".into()],
        };
        let back: Project = toml::from_str(&render(&p)).unwrap();
        assert_eq!(back, p);
        assert_eq!(
            split_list(" pnpm lint, ,pnpm test "),
            vec!["pnpm lint".to_string(), "pnpm test".to_string()]
        );
    }

    #[test]
    fn a_broken_file_says_so() {
        let dir = std::env::temp_dir().join(format!("squad-proj-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), "port = \"x\"").unwrap();
        assert!(read(dir.to_str().unwrap()).unwrap_err().contains(FILE));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
