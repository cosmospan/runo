//! Running a session inside its own Cove VM (https://github.com/deltalark/cove).
//!
//! Cove is a separate product. Runo only talks to it through the `cove` command:
//! every process a Cove session starts (the agent, the setup command, terminal tabs)
//! is wrapped as `cove exec runo-<id> --create … -- <program>`, which creates or
//! starts the VM on first use. The worktree is shared into the VM at the same path,
//! so git, diffs and commits keep running on the Mac against the same files.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The `cove` executable, looking in the usual install folders too because apps
/// started from the Dock don't get the shell's PATH.
pub fn binary() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(Path::new(&home).join(".cargo/bin"));
        // Where Cove's install script puts it.
        dirs.push(Path::new(&home).join(".cove/bin"));
    }
    dirs.into_iter()
        .map(|d| d.join("cove"))
        .find(|p| p.is_file())
}

/// Whether this machine's Cove server answers now (not cached).
fn answers() -> bool {
    binary().is_some_and(|bin| {
        Command::new(bin)
            .args(["--json", "ls"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}

fn availability() -> &'static Mutex<(Option<Instant>, bool)> {
    static STATE: OnceLock<Mutex<(Option<Instant>, bool)>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new((None, false)))
}

/// Whether Cove is installed and its server answers. Cached and refreshed in the
/// background, so it is cheap to call while rendering.
pub fn available() -> bool {
    let state = availability();
    let Ok(mut guard) = state.lock() else {
        return false;
    };
    let stale = guard
        .0
        .is_none_or(|t| t.elapsed() > Duration::from_secs(15));
    if stale {
        guard.0 = Some(Instant::now());
        std::thread::spawn(|| {
            let ok = answers();
            if let Ok(mut g) = availability().lock() {
                g.1 = ok;
            }
        });
    }
    guard.1
}

/// Shell helpers for installers: apt with sudo when not root, curl, and npm with a
/// private Node.js when the machine has none. Nothing here needs a password.
const INSTALL_HELPERS: &str = r#"runo_apt() { command -v apt-get >/dev/null 2>&1 || return 1; if [ "$(id -u)" = 0 ]; then apt-get update -qq && apt-get install -y -qq "$@"; else sudo -n apt-get update -qq && sudo -n apt-get install -y -qq "$@"; fi; }
runo_curl() { command -v curl >/dev/null 2>&1 || runo_apt curl ca-certificates; }
runo_npm() {
  runo_curl || return 1
  if ! command -v npm >/dev/null 2>&1; then
    case "$(uname -s)" in Linux) os=linux ;; Darwin) os=darwin ;; *) return 1 ;; esac
    case "$(uname -m)" in x86_64|amd64) arch=x64 ;; aarch64|arm64) arch=arm64 ;; *) return 1 ;; esac
    base=https://nodejs.org/dist/latest-v22.x
    f="$(curl -fsSL "$base/SHASUMS256.txt" | awk '{print $2}' | grep -E "^node-v[0-9.]+-$os-$arch\.tar\.gz$" | head -n1)"
    [ -n "$f" ] || return 1
    echo "Installing Node.js for npm…"
    mkdir -p "$HOME/.local/runo-node" && curl -fsSL "$base/$f" | tar -xzf - -C "$HOME/.local/runo-node" --strip-components=1 || return 1
  fi
  npm install -g --prefix "$HOME/.local" "$1@${want:-latest}"
}"#;

/// How to install an agent CLI on a Linux or macOS machine, by its program name: the
/// vendor's script, or its npm package (into ~/.local, so no sudo is needed).
fn install_command(program: &str) -> Option<String> {
    let provider = crate::agent::PROVIDERS
        .iter()
        .find(|p| p.bin == program || (p.id == "cursor" && program == "agent"))?;
    Some(match provider.id {
        "claude" => {
            r#"runo_curl && curl -fsSL https://claude.ai/install.sh | bash -s "${want:-latest}""#
                .to_string()
        }
        id => match crate::alerts::npm_package(id) {
            Some(package) => format!("runo_npm {package}"),
            None => format!(
                "runo_curl && {}",
                crate::registry::spec(id)?.install.unix.as_deref()?
            ),
        },
    })
}

/// A `bash` script that installs `provider`'s CLI on a Linux server without
/// root, for setting up your own server from the desktop app.
pub(crate) fn install_script(provider: &str) -> Option<String> {
    let bin = crate::agent::PROVIDERS
        .iter()
        .find(|p| p.id == provider)?
        .bin;
    let install = install_command(bin)?;
    Some(format!(
        r#"export PATH="$HOME/.local/bin:$HOME/.local/runo-node/bin:$HOME/.opencode/bin:$HOME/.factory/bin:$HOME/.grok/bin:$PATH"
{INSTALL_HELPERS}
want=""
if command -v {bin} >/dev/null 2>&1; then exit 0; fi
{{ {install}; }} >&2 && command -v {bin} >/dev/null 2>&1"#
    ))
}
