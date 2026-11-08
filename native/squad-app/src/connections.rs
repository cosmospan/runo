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
use std::sync::{Mutex, OnceLock};

/// API keys forwarded to a new connection unless the person changes the list: every
/// variable a supported agent signs in with.
pub const DEFAULT_KEYS: &str = "ANTHROPIC_API_KEY, CLAUDE_CODE_OAUTH_TOKEN, OPENAI_API_KEY, CODEX_API_KEY, OPENROUTER_API_KEY, GEMINI_API_KEY, GOOGLE_API_KEY, CURSOR_API_KEY, COPILOT_GITHUB_TOKEN, AMP_API_KEY, FACTORY_API_KEY, DASHSCOPE_API_KEY, GROK_API_KEY, XAI_API_KEY";

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

impl Connection {}

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
