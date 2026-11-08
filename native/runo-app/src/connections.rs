//! Connections: the places a session can run besides this machine.
//!
//! A connection is a Cove server (this machine's, or a remote one by URL) or an SSH
//! server. A session picks one ("Runs on") and can move to another between turns.
//!
//! - **Local Cove** shares the session folder into the VM, so nothing needs copying.
//! - **Remote connections** (SSH, or a Cove server elsewhere) keep their own copy of the
//!   session folder. Every agent turn runs through `runo --connection-run`, which
//!   syncs the folder up with git (uncommitted changes included), runs the agent
//!   there, and syncs the result back. The agent's own conversation file travels too,
//!   so a session can move between machines and resume the same conversation.
//!
//! Git data moves over the connection itself (`ssh`, or `cove exec` through git's
//! `ext::` transport), so a VM or server never needs GitHub access just to sync.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

/// API keys forwarded to a new connection unless the person changes the list: every
/// variable a supported agent signs in with.
pub const DEFAULT_KEYS: &str = "ANTHROPIC_API_KEY, CLAUDE_CODE_OAUTH_TOKEN, OPENAI_API_KEY, CODEX_API_KEY, OPENROUTER_API_KEY, GEMINI_API_KEY, GOOGLE_API_KEY, CURSOR_API_KEY, COPILOT_GITHUB_TOKEN, AMP_API_KEY, FACTORY_API_KEY, DASHSCOPE_API_KEY, GROK_API_KEY, XAI_API_KEY";

/// The list connections were saved with before, which now means the full one above.
const OLD_DEFAULT_KEYS: &str =
    "ANTHROPIC_API_KEY, CLAUDE_CODE_OAUTH_TOKEN, OPENAI_API_KEY, OPENROUTER_API_KEY";

/// How an agent signs in on a machine that doesn't share this one's home folder.
pub struct SignIn {
    pub provider: &'static str,
    /// API key variables the CLI reads.
    pub keys: &'static [&'static str],
    /// A login file the CLI keeps in the home folder and that works when copied.
    pub file: Option<&'static str>,
}

pub const SIGN_INS: [SignIn; 12] = [
    SignIn {
        provider: "claude",
        keys: &["CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_API_KEY"],
        // Linux keeps the login here; macOS keeps it in the keychain, which can't travel.
        file: Some(".claude/.credentials.json"),
    },
    SignIn {
        provider: "codex",
        keys: &["OPENAI_API_KEY", "CODEX_API_KEY"],
        file: Some(".codex/auth.json"),
    },
    SignIn {
        provider: "opencode",
        keys: &[
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "OPENROUTER_API_KEY",
            "GEMINI_API_KEY",
        ],
        file: Some(".local/share/opencode/auth.json"),
    },
    SignIn {
        provider: "cursor",
        keys: &["CURSOR_API_KEY"],
        file: None,
    },
    SignIn {
        provider: "gemini",
        keys: &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        file: None,
    },
    SignIn {
        provider: "copilot",
        keys: &["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"],
        file: None,
    },
    SignIn {
        provider: "amp",
        keys: &["AMP_API_KEY"],
        file: None,
    },
    SignIn {
        provider: "droid",
        keys: &["FACTORY_API_KEY"],
        file: None,
    },
    SignIn {
        provider: "qwen",
        keys: &["DASHSCOPE_API_KEY", "OPENAI_API_KEY"],
        file: None,
    },
    SignIn {
        provider: "grok",
        keys: &["XAI_API_KEY"],
        file: None,
    },
    SignIn {
        provider: "antigravity",
        keys: &["GEMINI_API_KEY"],
        file: None,
    },
    SignIn {
        provider: "pi",
        keys: &[
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "GEMINI_API_KEY",
            "OPENROUTER_API_KEY",
        ],
        file: None,
    },
];

pub fn sign_in(provider: &str) -> Option<&'static SignIn> {
    SIGN_INS.iter().find(|s| s.provider == provider)
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Cove,
    Ssh,
    /// Tests only: "remote" commands run in a local folder.
    #[cfg(test)]
    Folder,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Connection {
    pub id: u64,
    pub name: String,
    pub kind: Kind,
    /// Cove: the server's URL; empty for this machine's server.
    pub url: String,
    /// Cove: the API key of a remote server, from pairing (`cove pair` on that server)
    /// or `cove key` there.
    pub api_key: String,
    /// Cove: a pairing code typed in Settings, until it is traded for a key.
    #[serde(skip)]
    pub pair_code: String,
    /// Cove VM size and limits.
    pub cpus: u32,
    pub memory_mb: u64,
    /// "full" or "off".
    pub network: String,
    /// Stop an idle VM after this many minutes; 0 keeps it running.
    pub idle_stop: u32,
    /// Pause an idle VM instead of stopping it: its memory and running programs are
    /// kept, and it carries on as soon as it is used. Needs a Cove with auto-pause.
    pub idle_pause: bool,
    /// Cove: the server's SSH login (user@host). Set, Runo installs and pairs Cove over
    /// SSH and reaches it through a tunnel it keeps open itself.
    pub ssh: String,
    /// Cove over SSH: the local port of the tunnel; `url` points at it.
    pub tunnel_port: u16,
    /// SSH: a host from ~/.ssh/config, or user@host.
    pub host: String,
    /// SSH: 0 for the default port.
    pub port: u16,
    /// SSH: a private key file; empty uses the SSH agent and ~/.ssh/config.
    pub identity: String,
    /// Remote connections: the folder session copies live in, relative to the home folder.
    pub workdir: String,
    /// Environment variables passed to the agent, by name, comma separated.
    pub keys: String,
    /// Pass this machine's GitHub login (`gh auth token`) to the agent's commands.
    pub github: bool,
    /// Agents may run commands here themselves (Runo's `run_on` tool).
    pub agents: bool,
    /// Groups of Runo's tools (`RELAY_GROUPS`) that agents running here may call through
    /// the relay, beyond the ones that stay inside their session. Empty: none.
    pub relay_tools: Vec<String>,
}

impl Default for Connection {
    fn default() -> Self {
        Connection {
            id: 0,
            name: String::new(),
            kind: Kind::Cove,
            url: String::new(),
            api_key: String::new(),
            pair_code: String::new(),
            cpus: 2,
            memory_mb: 4096,
            network: "full".into(),
            idle_stop: 0,
            idle_pause: true,
            ssh: String::new(),
            tunnel_port: 0,
            host: String::new(),
            port: 0,
            identity: String::new(),
            workdir: "runo-work".into(),
            keys: DEFAULT_KEYS.into(),
            github: false,
            agents: true,
            relay_tools: Vec::new(),
        }
    }
}

/// A connection's SSH host, ready to pass to ssh: set, and not starting with `-`, which
/// ssh would read as one of its options (`-oProxyCommand=…` runs a command here).
pub(crate) fn ssh_host<'a>(host: &'a str, name: &str) -> Result<&'a str, String> {
    let host = host.trim();
    if host.is_empty() {
        return Err(format!("{name} has no host set."));
    }
    if host.starts_with('-') || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!(
            "{name}'s host “{host}” isn't a host name: it can't start with - or contain spaces."
        ));
    }
    Ok(host)
}

impl Connection {
    /// Keeps its own copy of the session folder, synced every turn.
    pub fn remote(&self) -> bool {
        match self.kind {
            Kind::Cove => !self.url.trim().is_empty(),
            _ => true,
        }
    }

    /// One line for menus and cards: "Cove · 2 CPU · 4 GB" or "anish@build-box".
    pub fn summary(&self) -> String {
        match self.kind {
            Kind::Cove => {
                let place = if !self.ssh.trim().is_empty() {
                    self.ssh.trim().to_string()
                } else if self.remote() {
                    self.url.trim().to_string()
                } else {
                    "Cove on this machine".into()
                };
                let mut parts = vec![
                    place,
                    format!("{} CPU", self.cpus),
                    format!("{} GB", self.memory_mb / 1024),
                ];
                if self.network == "off" {
                    parts.push("no network".into());
                }
                parts.join(" · ")
            }
            Kind::Ssh => {
                if self.host.trim().is_empty() {
                    "SSH server, no host set".into()
                } else {
                    format!("SSH · {}", self.host.trim())
                }
            }
            #[cfg(test)]
            Kind::Folder => self.host.clone(),
        }
    }

    pub fn key_names(&self) -> Vec<String> {
        let keys = if self.keys.trim() == OLD_DEFAULT_KEYS {
            DEFAULT_KEYS
        } else {
            &self.keys
        };
        keys.split([',', ' ', '\n'])
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(String::from)
            .collect()
    }

    /// The forwarded variables that are set on this machine (in Runo's environment or
    /// the login shell's), plus GitHub when allowed.
    pub fn forwarded_env(&self) -> Vec<(String, String)> {
        let mut env: Vec<(String, String)> = self
            .key_names()
            .into_iter()
            .filter_map(|k| local_var(&k).map(|v| (k, v)))
            .collect();
        if self.github {
            if let Some(token) = gh_token() {
                env.extend(github_env(&token));
            }
        }
        env
    }

    /// The `cove` command for this connection, pointed at its server.
    pub(crate) fn cove(&self) -> Result<Command, String> {
        let bin = crate::cove::binary().ok_or("Cove is not installed on this machine.")?;
        crate::coveserver::ensure_tunnel(self)?;
        let mut c = Command::new(bin);
        if self.remote() {
            c.env("COVE_URL", self.url.trim());
            if !self.api_key.trim().is_empty() {
                c.env("COVE_API_KEY", self.api_key.trim());
            }
        }
        Ok(c)
    }

    /// `cove exec` arguments that create or start the VM with this connection's settings.
    pub fn cove_ensure_args(&self, vm: &str) -> Vec<String> {
        let mut args = vec![
            "exec".to_string(),
            vm.to_string(),
            "--create".into(),
            "--cpus".into(),
            self.cpus.max(1).to_string(),
            "--memory".into(),
            self.memory_mb.max(512).to_string(),
        ];
        if self.network == "off" {
            args.extend(["--network".into(), "off".into()]);
        }
        // The scratch VM agents use for one-off commands stops itself when left idle.
        let idle = if vm == scratch_vm() && self.idle_stop == 0 {
            15
        } else {
            self.idle_stop
        };
        if idle > 0 {
            // A Cove too old to pause gets only the stop timer; a newer one checks the
            // pause timer first, and a server too old for it still stops the VM.
            if self.idle_pause && crate::cove::has_auto_pause() {
                args.extend(["--auto-pause".into(), idle.to_string()]);
            }
            args.extend(["--idle-stop".into(), idle.to_string()]);
        }
        args
    }

    fn ssh_base(&self) -> Command {
        let mut c = Command::new("ssh");
        c.args(ssh_options(self));
        c
    }

    /// Run a bash script on the connection, in the home folder. Environment values
    /// travel without appearing on the remote command line. With `pty`, the script
    /// gets a terminal (stdin stays with the caller).
    pub fn shell(
        &self,
        vm: &str,
        script: &str,
        env: &[(String, String)],
        pty: bool,
    ) -> Result<(Command, Option<String>), String> {
        match self.kind {
            Kind::Cove => {
                let mut c = self.cove()?;
                c.args(self.cove_ensure_args(vm));
                c.args(["--cwd", "/root"]);
                if pty {
                    c.arg("-t");
                }
                for (k, v) in env {
                    c.args(["-e", &format!("{k}={v}")]);
                }
                c.args(["--", "bash", "-lc", script]);
                Ok((c, None))
            }
            Kind::Ssh => {
                let host = ssh_host(&self.host, &self.name)?;
                let mut c = self.ssh_base();
                if pty {
                    c.arg("-tt");
                    c.args([host, "--", &format!("bash -lc {}", quote(script))]);
                    return Ok((c, None));
                }
                c.arg("-T");
                c.args([host, "--", "bash -l -s"]);
                // The script, with its environment, goes over stdin.
                let mut body = String::new();
                for (k, v) in env {
                    body.push_str(&format!("export {k}={}\n", quote(v)));
                }
                body.push_str(script);
                body.push('\n');
                Ok((c, Some(body)))
            }
            #[cfg(test)]
            Kind::Folder => {
                let mut c = Command::new("bash");
                c.args(["-c", script]).current_dir(&self.host);
                c.env("HOME", &self.host);
                for (k, v) in env {
                    c.env(k, v);
                }
                Ok((c, None))
            }
        }
    }
}

fn ssh_options(c: &Connection) -> Vec<String> {
    let mut args = vec![
        "-o".to_string(),
        "BatchMode=yes".into(),
        "-o".into(),
        "ConnectTimeout=15".into(),
        "-o".into(),
        "ServerAliveInterval=30".into(),
    ];
    // One login shared by the agent, terminals and syncs.
    if !cfg!(windows) {
        args.extend([
            "-o".into(),
            "ControlMaster=auto".into(),
            "-o".into(),
            "ControlPath=~/.ssh/runo-%C".into(),
            "-o".into(),
            "ControlPersist=10m".into(),
        ]);
    }
    if c.port != 0 {
        args.extend(["-p".into(), c.port.to_string()]);
    }
    if !c.identity.trim().is_empty() {
        args.extend(["-i".into(), c.identity.trim().to_string()]);
    }
    args
}

/// Single-quote for a POSIX shell.
pub fn quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:,@+%".contains(c))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// A variable from Runo's environment, or the login shell's when Runo was opened from
/// the Dock and didn't inherit it.
fn local_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| crate::shellenv::login_var(name))
}

/// This machine's GitHub CLI token, if it is signed in.
fn gh_token() -> Option<String> {
    let out = Command::new("gh")
        .args(["auth", "token"])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !token.is_empty()).then_some(token)
}

/// `gh` reads GH_TOKEN; git gets a credential helper that hands it out for github.com.
fn github_env(token: &str) -> Vec<(String, String)> {
    vec![
        ("GH_TOKEN".into(), token.into()),
        ("GIT_CONFIG_COUNT".into(), "1".into()),
        (
            "GIT_CONFIG_KEY_0".into(),
            "credential.https://github.com.helper".into(),
        ),
        (
            "GIT_CONFIG_VALUE_0".into(),
            r#"!f() { echo username=x-access-token; echo "password=$GH_TOKEN"; }; f"#.into(),
        ),
    ]
}

// ---------- the connections Runo knows, for code without access to settings ----------

fn store() -> &'static Mutex<Vec<Connection>> {
    static ALL: OnceLock<Mutex<Vec<Connection>>> = OnceLock::new();
    ALL.get_or_init(|| Mutex::new(Vec::new()))
}

/// Called whenever settings change.
pub fn set_all(all: &[Connection]) {
    if let Ok(mut g) = store().lock() {
        *g = all.to_vec();
    }
}

/// The VM agents' one-off commands run in, on a Cove connection.
pub fn scratch_vm() -> String {
    "runo-scratch".into()
}

// ---------- running a session's commands on a connection ----------

// ---------- sync ----------

// ---------- turns that keep running on the connection ----------

// ---------- dev servers on a connection ----------

impl Connection {}

// ---------- Runo's MCP server for agents that run elsewhere ----------

// ---------- agents running commands on connections (Runo's run_on tool) ----------

/// Run one command for an agent on a connection that allows it. Runs in the connection's
/// scratch space: the `runo-scratch` VM on Cove, `<workdir>/scratch` on a server.
pub fn run_for_agent(conn: &Connection, command: &str) -> Result<String, String> {
    if !conn.agents {
        return Err(format!(
            "{} doesn't allow agents to run commands. The person can turn this on in Settings > Connections.",
            conn.name
        ));
    }
    let scratch = format!("{}/scratch", conn.workdir.trim().trim_matches('/'));
    let script = format!(
        "mkdir -p {s} && cd {s} && {{ {command}\n}} 2>&1",
        s = quote(&scratch)
    );
    let (mut c, stdin) = conn.shell(&scratch_vm(), &script, &conn.forwarded_env(), false)?;
    let mut child = c
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut pipe) = child.stdin.take() {
        let body = stdin.unwrap_or_default();
        std::thread::spawn(move || {
            let _ = pipe.write_all(body.as_bytes());
        });
    }
    let mut out = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_end(&mut out);
    }
    let mut err = Vec::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_end(&mut err);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let mut text = String::from_utf8_lossy(&out).into_owned();
    if !err.is_empty() {
        text.push_str(&String::from_utf8_lossy(&err));
    }
    const MAX: usize = 30_000;
    if text.len() > MAX {
        let mut cut = text.len() - MAX;
        while !text.is_char_boundary(cut) {
            cut += 1;
        }
        text = format!("[… {cut} bytes cut]\n{}", &text[cut..]);
    }
    Ok(format!(
        "{text}\n[exit code {}]",
        status.code().unwrap_or(-1)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ssh_host_cant_be_an_option() {
        assert_eq!(
            ssh_host(" ubuntu@203.0.113.7 ", "box"),
            Ok("ubuntu@203.0.113.7")
        );
        assert!(ssh_host("-oProxyCommand=touch /tmp/x", "box").is_err());
        assert!(ssh_host("host -oX", "box").is_err());
        assert!(ssh_host("", "box").unwrap_err().contains("no host"));
    }

    #[test]
    fn github_access_uses_a_credential_helper() {
        let env = github_env("tok");
        assert!(env.contains(&("GH_TOKEN".into(), "tok".into())));
        assert!(env
            .iter()
            .any(|(k, v)| k == "GIT_CONFIG_VALUE_0" && v.contains("$GH_TOKEN")));
    }
}
