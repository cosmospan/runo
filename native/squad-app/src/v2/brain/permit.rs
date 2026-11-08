//! What a bot may do on its own, what it asks about, and what it never does.
//!
//! Every step a bot wants to take (a shell command, a file it reads or
//! writes, a web page, a connected app) is sorted into one of four tiers by
//! plain rules. The line is drawn at the job's own copy of the project:
//! inside it nothing reaches the person until they approve the result, so it
//! just happens; past it, the bot asks.
//!
//! The same judgement gives the approval card its words: a headline, one line
//! of why, and whether "Always allow" is on offer.

use serde_json::Value;

/// How a step is handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Inside the job's copy: it just happens.
    Auto,
    /// Past the job's copy: Allow once, Always allow or Deny.
    Ask,
    /// Hard to undo: Allow once or Deny, every time.
    EveryTime,
    /// The person's keys and logins: refused.
    Never,
}

/// What the card shows for one step.
#[derive(Clone, Debug, PartialEq)]
pub struct Judgement {
    pub tier: Tier,
    /// "Builder wants to install a tool for your whole computer."
    pub headline: String,
    /// "It reaches outside this project."
    pub why: &'static str,
    /// Whether "Always allow" is offered: only for the ask tier, and only
    /// when the step narrows to a rule (see `approvals::always_rule`).
    pub can_always: bool,
}

/// Commands that are hard to undo or reach past the job: pushing and
/// shipping (Shipper does those after the person's yes), deploys,
/// publishing, wiping files or history, and admin rights.
pub const EVERY_TIME: &[&str] = &[
    "rm -rf",
    "rm -fr",
    "rm -r /",
    "sudo",
    "su",
    "chmod 777",
    "chown",
    "git push",
    "git reset --hard",
    "git clean",
    "git checkout --",
    "git branch -D",
    "git rebase",
    "git filter-branch",
    "npm publish",
    "yarn publish",
    "pnpm publish",
    "cargo publish",
    "twine upload",
    "docker push",
    "kubectl",
    "terraform apply",
    "terraform destroy",
    "vercel",
    "netlify deploy",
    "fly deploy",
    "flyctl deploy",
    "firebase deploy",
    "heroku",
    "aws",
    "gcloud",
    "mkfs",
    "dd if=",
    "shutdown",
    "reboot",
];

/// Installs that land on the whole computer rather than in the project.
const GLOBAL_INSTALL: &[&str] = &[
    "npm install -g",
    "npm i -g",
    "npm install --global",
    "pnpm add -g",
    "yarn global add",
    "pip install --user",
    "pipx install",
    "cargo install",
    "go install",
    "gem install",
    "brew install",
    "brew upgrade",
    "apt install",
    "apt-get install",
    "port install",
];

/// Where the person's own keys and logins live.
const KEYS: &[&str] = &[
    ".ssh/",
    ".ssh\\",
    ".gnupg",
    ".aws/credentials",
    ".config/gh/hosts",
    ".netrc",
    "keychain",
    "login.keychain",
    "security find-generic-password",
    "security find-internet-password",
    "cookies.sqlite",
    "/cookies",
];

/// Files that hold secrets.
fn secret_file(path: &str) -> bool {
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_lowercase();
    name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
        || name.contains("credentials")
        || name.contains("secret")
}

/// Whether `path` lies outside the project at `project` (a relative path is
/// inside unless it climbs out).
fn outside(path: &str, project: &str) -> bool {
    let p = path.trim().trim_matches(|c| c == '"' || c == '\'');
    if p.is_empty() {
        return false;
    }
    if p.starts_with('~') {
        return true;
    }
    let absolute = p.starts_with('/') || p.chars().nth(1) == Some(':');
    if absolute {
        let root = project.trim_end_matches(['/', '\\']);
        return root.is_empty() || !(p == root || p.starts_with(&format!("{root}/")));
    }
    p.split(['/', '\\']).filter(|s| *s == "..").count() > 0
}

/// A command's parts, split where the shell runs another command.
fn parts(command: &str) -> Vec<String> {
    command
        .replace("&&", "\n")
        .replace("||", "\n")
        .split(['\n', ';', '|', '&'])
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Whether `part` runs `prefix`: the same words at its start, so "git
/// pushd" isn't "git push" and "sudoku" isn't "sudo".
fn runs(part: &str, prefix: &str) -> bool {
    part == prefix
        || part.starts_with(&format!("{prefix} "))
        || (prefix.ends_with('=') && part.starts_with(prefix))
}

/// The host a URL points at, lowercased.
fn host(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let h = rest.split(['/', '?', '#']).next()?;
    let h = h.rsplit('@').next()?.split(':').next()?;
    (!h.is_empty()).then(|| h.to_lowercase())
}

fn local(h: &str) -> bool {
    matches!(h, "localhost" | "127.0.0.1" | "0.0.0.0" | "::1" | "[::1]")
}

/// A shell command's tier and why.
fn command(cmd: &str, project: &str) -> (Tier, &'static str, &'static str) {
    let lower = cmd.to_lowercase();
    if KEYS.iter().any(|k| lower.contains(k)) {
        return (
            Tier::Never,
            "read your keys or logins",
            "Bots never touch your keys and logins.",
        );
    }
    let mut out = (Tier::Auto, "run a command in its copy of the project", "");
    let mut raise = |t: (Tier, &'static str, &'static str)| {
        if t.0 > out.0 {
            out = t;
        }
    };
    for part in parts(cmd) {
        let p = part.to_lowercase();
        if EVERY_TIME.iter().any(|x| runs(&p, x)) {
            let what = if runs(&p, "git push") {
                "push to git"
            } else if p.starts_with("rm ")
                || p.starts_with("git clean")
                || p.starts_with("git reset")
            {
                "delete or rewrite files"
            } else if p.starts_with("sudo")
                || p.starts_with("su ")
                || p.starts_with("chmod")
                || p.starts_with("chown")
            {
                "use admin rights"
            } else if p.contains("publish") || p.contains("upload") || p.starts_with("docker push")
            {
                "publish something"
            } else {
                "deploy or change something outside this computer"
            };
            raise((Tier::EveryTime, what, "This can't easily be undone."));
            continue;
        }
        if GLOBAL_INSTALL.iter().any(|x| runs(&p, x)) {
            raise((
                Tier::Ask,
                "install a tool for your whole computer",
                "It reaches outside this project.",
            ));
            continue;
        }
        let words: Vec<&str> = part.split_whitespace().collect();
        if matches!(
            words.first(),
            Some(&"curl") | Some(&"wget") | Some(&"http") | Some(&"https")
        ) {
            let far = words
                .iter()
                .filter_map(|w| {
                    let w = w.trim_matches(|c| c == '"' || c == '\'');
                    if w.contains("://") {
                        host(w)
                    } else if w.contains('.') && !w.starts_with('-') && !w.starts_with('/') {
                        host(&format!("http://{w}"))
                    } else {
                        None
                    }
                })
                .any(|h| !local(&h));
            if far {
                raise((
                    Tier::Ask,
                    "fetch something from the internet",
                    "It reaches outside this computer.",
                ));
                continue;
            }
        }
        if words.iter().skip(1).any(|w| {
            let w = w.trim_start_matches(['>', '<']);
            (w.starts_with('/') || w.starts_with('~')) && outside(w, project)
        }) {
            raise((
                Tier::Ask,
                "work with files outside this project",
                "It reaches outside this project.",
            ));
            continue;
        }
        if words.iter().skip(1).any(|w| secret_file(w)) {
            raise((
                Tier::Ask,
                "read a secrets file",
                "That file may hold passwords or keys.",
            ));
        }
    }
    out
}

/// How a step the bot `bot` wants to take with `tool` and `input`, in the
/// project at `project`, is handled.
pub fn judge(bot: &str, tool: &str, input: &Value, project: &str) -> Judgement {
    let path = input["file_path"]
        .as_str()
        .or_else(|| input["path"].as_str())
        .or_else(|| input["notebook_path"].as_str())
        .unwrap_or("");
    let (tier, what, why) = match tool {
        "Bash" => command(input["command"].as_str().unwrap_or(""), project),
        "Read" | "Glob" | "Grep" | "LS" | "NotebookRead" => {
            if KEYS.iter().any(|k| path.to_lowercase().contains(k)) {
                (
                    Tier::Never,
                    "read your keys or logins",
                    "Bots never touch your keys and logins.",
                )
            } else if secret_file(path) {
                (
                    Tier::Ask,
                    "read a secrets file",
                    "That file may hold passwords or keys.",
                )
            } else if outside(path, project) {
                (
                    Tier::Ask,
                    "read files outside this project",
                    "It reaches outside this project.",
                )
            } else {
                (Tier::Auto, "read files", "")
            }
        }
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            if KEYS.iter().any(|k| path.to_lowercase().contains(k)) {
                (
                    Tier::Never,
                    "change your keys or logins",
                    "Bots never touch your keys and logins.",
                )
            } else if outside(path, project) {
                (
                    Tier::Ask,
                    "change files outside this project",
                    "It reaches outside this project.",
                )
            } else if secret_file(path) {
                (
                    Tier::Ask,
                    "change a secrets file",
                    "That file may hold passwords or keys.",
                )
            } else {
                (Tier::Auto, "edit files", "")
            }
        }
        "WebFetch" => match input["url"].as_str().and_then(host) {
            Some(h) if local(&h) => (Tier::Auto, "open a page on this computer", ""),
            _ => (
                Tier::Ask,
                "fetch a web page",
                "It reaches outside this computer.",
            ),
        },
        "WebSearch" => (
            Tier::Ask,
            "search the web",
            "It reaches outside this computer.",
        ),
        "TodoWrite" | "Task" | "ExitPlanMode" => (Tier::Auto, "keep its notes", ""),
        t if t.starts_with("mcp__") => (
            Tier::Ask,
            "use a connected app",
            "It may act for you in that app.",
        ),
        _ => (
            Tier::Ask,
            "use a tool Squad Code doesn't know",
            "Squad Code can't tell what it does.",
        ),
    };
    // Writing past the project isn't something to allow for good.
    let narrowable = crate::approvals::always_rule(tool, input).is_some()
        && !(matches!(tool, "Edit" | "Write" | "MultiEdit" | "NotebookEdit")
            && outside(path, project));
    Judgement {
        tier,
        headline: format!("{bot} wants to {what}."),
        why,
        can_always: tier == Tier::Ask && narrowable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const P: &str = "/Users/anish/dev/shop";

    fn bash(c: &str) -> Judgement {
        judge("Builder", "Bash", &json!({ "command": c }), P)
    }

    #[test]
    fn work_inside_the_project_just_happens() {
        for c in [
            "npm test",
            "npm install express",
            "node server.js &",
            "curl -s localhost:3000/api/hello",
            "cargo build && cargo test",
            "git status",
            "git diff HEAD~1",
            "rm src/old.js",
            "ls -la src",
        ] {
            assert_eq!(bash(c).tier, Tier::Auto, "{c}");
        }
        let edit = judge("Builder", "Edit", &json!({ "file_path": "src/cart.js" }), P);
        assert_eq!(edit.tier, Tier::Auto);
        let inside = judge(
            "Builder",
            "Read",
            &json!({ "file_path": format!("{P}/README.md") }),
            P,
        );
        assert_eq!(inside.tier, Tier::Auto);
    }

    #[test]
    fn reaching_past_the_project_asks() {
        let g = bash("npm install -g typescript");
        assert_eq!(g.tier, Tier::Ask);
        assert!(g.can_always, "{g:?}");
        assert_eq!(
            g.headline,
            "Builder wants to install a tool for your whole computer."
        );
        assert_eq!(bash("curl https://api.github.com/repos").tier, Tier::Ask);
        assert_eq!(bash("cat ~/Downloads/data.csv").tier, Tier::Ask);
        assert_eq!(bash("cat .env").tier, Tier::Ask);
        let fetch = judge(
            "Builder",
            "WebFetch",
            &json!({ "url": "https://docs.stripe.com/x" }),
            P,
        );
        assert!(fetch.tier == Tier::Ask && fetch.can_always);
        let out = judge("Builder", "Write", &json!({ "file_path": "/etc/hosts" }), P);
        assert_eq!(out.tier, Tier::Ask);
        assert!(!out.can_always, "writing outside is never allowed for good");
        assert_eq!(
            judge("Builder", "mcp__slack__post", &json!({}), P).tier,
            Tier::Ask
        );
    }

    #[test]
    fn hard_to_undo_asks_every_time() {
        for c in [
            "git push origin main",
            "npm run build && vercel --prod",
            "rm -rf node_modules dist",
            "sudo npm install -g x",
            "git reset --hard origin/main",
            "npm publish",
        ] {
            let j = bash(c);
            assert_eq!(j.tier, Tier::EveryTime, "{c}");
            assert!(!j.can_always, "{c}");
            assert_eq!(j.why, "This can't easily be undone.");
        }
        assert_eq!(bash("git push").headline, "Builder wants to push to git.");
        // Same words at the start only.
        assert_eq!(bash("git pushd").tier, Tier::Auto);
        assert_eq!(bash("echo sudoku").tier, Tier::Auto);
    }

    #[test]
    fn keys_and_logins_are_never_touched() {
        assert_eq!(bash("cat ~/.ssh/id_ed25519").tier, Tier::Never);
        assert_eq!(
            judge(
                "Builder",
                "Read",
                &json!({ "file_path": "/Users/anish/.ssh/config" }),
                P
            )
            .tier,
            Tier::Never
        );
        assert_eq!(
            bash("security find-generic-password -s github").tier,
            Tier::Never
        );
    }
}
