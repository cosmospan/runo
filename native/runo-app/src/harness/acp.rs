//! The Agent Client Protocol (ACP) driver. Agents that speak ACP (OpenCode's `acp`,
//! Gemini CLI and Qwen Code's `--acp`, Copilot's `--acp --stdio`, Cursor's `acp`) run
//! under Runo's agent host as one live session per turn instead of a print-mode run:
//! `Runo --agent-host runo-turn <turn folder> --acp` reads `acp.json` from the turn
//! folder, starts the agent, opens or reloads the conversation and sends it each prompt
//! from `in`, one after another.
//!
//! While it works, the host answers the agent's requests: permission requests by the
//! session's mode, asking the person in Runo (the corner card and the Inbox) for anything
//! the mode doesn't settle, and file reads and writes inside the session folder. MCP
//! servers go in `session/new`, so nothing is merged into the agent's project files.
//!
//! The agent's `session/update` notifications go to `out` as they are, with the host's
//! own lines (`{"runo": …}`) for what only it knows: the conversation id, the end of a
//! prompt, an error. `AcpStream` turns those into `AgentEvent`s. When the agent can't
//! start a session at all (an older release without ACP, a crash at start), the host
//! runs the turn's print-mode command instead and says so in `out`, so the turn still
//! runs and is read as before.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::rpc::{self, Agent, Message, Out, Reply};

/// The turn's settings, written by Runo into the turn folder.
pub const CONFIG: &str = "acp.json";

/// How long the agent has to start and answer `initialize` or open a session. Agents
/// run through npx can take a while the first time.
const START: Duration = Duration::from_secs(90);
/// After a prompt ends, how long a message sent just then still makes it into the
/// same session.
const GRACE: Duration = Duration::from_millis(1500);

/// What a turn runs: the agent's ACP command, the print-mode command to fall back on,
/// and the session's folder, conversation, model, mode and MCP servers.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub provider: String,
    /// The program and its arguments.
    pub command: Vec<String>,
    /// The print-mode command with the prompt in it, run when ACP can't start.
    #[serde(default)]
    pub fallback: Vec<String>,
    pub cwd: String,
    /// The conversation to pick up; empty starts a new one.
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub model: String,
    /// Runo's mode: "plan", "edit" (or empty) or "full".
    #[serde(default)]
    pub mode: String,
    /// MCP servers in ACP's shape (`McpSpec::acp_json`).
    #[serde(default)]
    pub servers: Vec<Value>,
}

impl Config {
    pub fn write(&self, turn: &Path) -> std::io::Result<()> {
        let data = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(turn.join(CONFIG), data)
    }

    fn read(turn: &Path) -> Option<Config> {
        serde_json::from_slice(&std::fs::read(turn.join(CONFIG)).ok()?).ok()
    }
}

/// `Runo --agent-host <mark> <turn folder> --acp`. Returns the exit code, which is
/// also written to `turn/exit`.
pub fn main(turn: &Path) -> i32 {
    let code = match Config::read(turn) {
        Some(config) => run(turn, &config),
        None => {
            let _ = std::fs::write(
                turn.join("err"),
                "Runo couldn't read the turn's settings.\n",
            );
            2
        }
    };
    super::host::finish(turn, code);
    code
}

/// Run the turn: an ACP session when the agent can open one, else the fallback.
fn run(turn: &Path, config: &Config) -> i32 {
    let Some(out) = rpc::create_out(turn) else {
        return 1;
    };
    match session(turn, config, &out) {
        Ok(code) => code,
        Err(why) => rpc::fall_back(turn, &config.fallback, &config.cwd, &out, &why),
    }
}

/// Handles what the agent sends besides answers: its notifications go to `out` (except
/// while a reloaded conversation replays its history), its requests are answered.
struct Handler<'a> {
    out: &'a Out,
    config: &'a Config,
    quiet: bool,
}

impl Handler<'_> {
    fn on(&self, agent: &mut Agent, message: Message) {
        match message {
            Message::Notification { line, .. } => {
                if !self.quiet {
                    rpc::write_line(self.out, &line);
                }
            }
            Message::Request {
                method,
                id,
                params,
                line,
            } => {
                rpc::write_line(self.out, &line);
                self.handle(agent, &method, id, &params);
            }
            _ => {}
        }
    }

    /// Answer a request from the agent.
    fn handle(&self, agent: &Agent, method: &str, id: Value, params: &Value) {
        let input = agent.input.clone();
        match method {
            "session/request_permission" => match decide(&self.config.mode, params) {
                Decision::Pick(option) => rpc::respond(&input, &id, selected(&option)),
                // The person may take minutes; the session carries on reading meanwhile.
                Decision::Ask => {
                    let (params, cwd) = (params.clone(), self.config.cwd.clone());
                    std::thread::spawn(move || {
                        let result = ask_person(&cwd, &params);
                        rpc::respond(&input, &id, result);
                    });
                }
            },
            "fs/read_text_file" => match read_file(params) {
                Ok(content) => rpc::respond(&input, &id, json!({ "content": content })),
                Err(e) => rpc::respond_error(&input, &id, -32603, &e),
            },
            "fs/write_text_file" => match write_file(&self.config.cwd, params) {
                Ok(()) => rpc::respond(&input, &id, Value::Null),
                Err(e) => rpc::respond_error(&input, &id, -32603, &e),
            },
            _ => rpc::respond_error(&input, &id, -32601, "Runo doesn't offer this"),
        }
    }

    fn wait(&mut self, agent: &mut Agent, id: u64, limit: Option<Duration>) -> Reply {
        agent.wait(id, limit, &mut |agent, message| self.on(agent, message))
    }
}

/// Open the session and run the prompts. `Err` means no session could be opened, so
/// nothing was sent yet and the fallback can run instead.
fn session(turn: &Path, config: &Config, out: &Out) -> Result<i32, String> {
    let (program, args) = config
        .command
        .split_first()
        .ok_or("the agent has no ACP command")?;
    let mut agent = Agent::spawn(turn, program, args, &config.cwd)?;
    let mut handler = Handler {
        out,
        config,
        quiet: false,
    };
    let sid = match open(&mut agent, &mut handler) {
        Ok(sid) => sid,
        Err(why) => {
            agent.kill();
            return Err(why);
        }
    };
    rpc::note(out, json!({ "session": sid }));
    let failed = prompts(turn, &mut agent, &mut handler, &sid);
    agent.close();
    Ok(if failed { 1 } else { 0 })
}

/// Initialize and open the conversation, set its mode and model. Returns its id.
fn open(agent: &mut Agent, h: &mut Handler) -> Result<String, String> {
    let config = h.config;
    let id = agent.request(
        "initialize",
        json!({
            "protocolVersion": 1,
            "clientCapabilities": {
                "fs": { "readTextFile": true, "writeTextFile": true },
                "terminal": false,
            },
            "clientInfo": { "name": "Runo", "version": env!("CARGO_PKG_VERSION") },
        }),
    );
    let init = match h.wait(agent, id, Some(START)) {
        Reply::Result(init) => init,
        Reply::Error(e) => return Err(format!("it refused to start a session: {e}")),
        Reply::Gone => return Err("it exited before starting a session".into()),
        Reply::TimedOut => return Err("it didn't answer".into()),
    };
    let caps = &init["agentCapabilities"];
    let servers: Vec<Value> = config
        .servers
        .iter()
        .filter(|server| match server["type"].as_str() {
            Some("http") => caps["mcpCapabilities"]["http"] == true,
            Some("sse") => caps["mcpCapabilities"]["sse"] == true,
            _ => true,
        })
        .cloned()
        .collect();
    let mut sid = String::new();
    let mut opened = Value::Null;
    if !config.session.is_empty() && caps["loadSession"] == true {
        h.quiet = true;
        let id = agent.request(
            "session/load",
            json!({ "sessionId": config.session, "cwd": config.cwd, "mcpServers": servers }),
        );
        let reply = h.wait(agent, id, Some(START));
        h.quiet = false;
        match reply {
            Reply::Result(result) => {
                sid = config.session.clone();
                opened = result;
            }
            Reply::Gone => return Err("it exited while opening the conversation".into()),
            _ => rpc::note(
                h.out,
                json!({ "notice": "The agent couldn't pick up the conversation, so this turn starts a new one." }),
            ),
        }
    }
    if sid.is_empty() {
        let id = agent.request(
            "session/new",
            json!({ "cwd": config.cwd, "mcpServers": servers }),
        );
        match h.wait(agent, id, Some(START)) {
            Reply::Result(result) => {
                sid = result["sessionId"].as_str().unwrap_or_default().to_string();
                opened = result;
            }
            Reply::Error(e) => return Err(format!("it couldn't start a session: {e}")),
            Reply::Gone => return Err("it exited while starting a session".into()),
            Reply::TimedOut => return Err("it didn't start a session".into()),
        }
        if sid.is_empty() {
            return Err("it started a session without an id".into());
        }
    }
    if let Some(mode) = pick_mode(&config.mode, &opened["modes"]) {
        let id = agent.request(
            "session/set_mode",
            json!({ "sessionId": sid, "modeId": mode }),
        );
        h.wait(agent, id, Some(Duration::from_secs(20)));
    }
    if !config.model.is_empty() {
        match pick_model(&config.model, &opened["models"]) {
            Some(model) => {
                let id = agent.request(
                    "session/set_model",
                    json!({ "sessionId": sid, "modelId": model }),
                );
                if let Reply::Error(e) = h.wait(agent, id, Some(Duration::from_secs(20))) {
                    rpc::note(
                        h.out,
                        json!({ "notice": format!("The agent kept its own model: {e}") }),
                    );
                }
            }
            None => rpc::note(
                h.out,
                json!({ "notice": format!("{} isn't one of the agent's models here, so it used its default.", config.model) }),
            ),
        }
    }
    Ok(sid)
}

/// Send each prompt in `in`, one after another, until none is left. Returns true when
/// a prompt failed.
fn prompts(turn: &Path, agent: &mut Agent, h: &mut Handler, sid: &str) -> bool {
    let path = turn.join("in");
    let mut offset = 0u64;
    let mut waiting: std::collections::VecDeque<String> = Default::default();
    let mut sent = 0u64;
    let mut failed = false;
    loop {
        waiting.extend(super::host::new_lines(&path, &mut offset));
        if waiting.is_empty() {
            // A message sent as the prompt ended still joins this session.
            let start = Instant::now();
            while waiting.is_empty() && start.elapsed() < GRACE {
                std::thread::sleep(Duration::from_millis(100));
                waiting.extend(super::host::new_lines(&path, &mut offset));
            }
        }
        let Some(line) = waiting.pop_front() else {
            return failed;
        };
        let id = agent.request(
            "session/prompt",
            json!({ "sessionId": sid, "prompt": [{ "type": "text", "text": rpc::prompt_text(&line) }] }),
        );
        sent += 1;
        let _ = std::fs::write(turn.join("sent"), sent.to_string());
        match h.wait(agent, id, None) {
            Reply::Result(result) => {
                let stop = result["stopReason"].as_str().unwrap_or("end_turn");
                rpc::note(h.out, json!({ "done": stop }));
                if stop == "refusal" {
                    failed = true;
                }
            }
            Reply::Error(e) => {
                rpc::note(h.out, json!({ "error": e }));
                failed = true;
            }
            Reply::Gone => {
                rpc::note(
                    h.out,
                    json!({ "error": "The agent exited in the middle of its turn." }),
                );
                return true;
            }
            Reply::TimedOut => {}
        }
    }
}

/// How a permission request is settled.
#[derive(Debug, PartialEq)]
enum Decision {
    /// Answer with this option at once.
    Pick(String),
    /// Ask the person.
    Ask,
}

/// Settle what the session's mode settles: Full access allows everything, Auto edit
/// allows reading and editing, Plan only refuses changes. The rest goes to the person.
fn decide(mode: &str, params: &Value) -> Decision {
    let kind = params["toolCall"]["kind"].as_str().unwrap_or("other");
    let options = params["options"].as_array().cloned().unwrap_or_default();
    let reads = matches!(kind, "read" | "search" | "think" | "fetch");
    let changes = matches!(kind, "edit" | "delete" | "move");
    let allow = mode == "full" || reads || (changes && mode != "plan");
    if allow {
        if let Some(option) = option_of(&options, true) {
            return Decision::Pick(option);
        }
    }
    if changes && mode == "plan" {
        if let Some(option) = option_of(&options, false) {
            return Decision::Pick(option);
        }
    }
    Decision::Ask
}

/// The option that allows (or refuses) this once, else always.
fn option_of(options: &[Value], allow: bool) -> Option<String> {
    let (once, always) = if allow {
        ("allow_once", "allow_always")
    } else {
        ("reject_once", "reject_always")
    };
    [once, always].iter().find_map(|kind| {
        options
            .iter()
            .find(|o| o["kind"] == *kind)
            .and_then(|o| o["optionId"].as_str())
            .map(str::to_string)
    })
}

fn selected(option: &str) -> Value {
    json!({ "outcome": { "outcome": "selected", "optionId": option } })
}

/// Ask the person in Runo and turn the answer into the agent's option.
fn ask_person(cwd: &str, params: &Value) -> Value {
    let call = &params["toolCall"];
    let tool = call["title"]
        .as_str()
        .filter(|t| !t.is_empty())
        .or(call["kind"].as_str())
        .unwrap_or("a tool");
    let input = call.get("rawInput").cloned().unwrap_or_else(|| json!({}));
    let base = crate::approvals::folder();
    let answer = crate::approvals::ask(&base, cwd, tool, &input, crate::approvals::wait());
    let options = params["options"].as_array().cloned().unwrap_or_default();
    let allowed = matches!(answer, crate::approvals::Answer::Allow { .. });
    match option_of(&options, allowed) {
        Some(option) => selected(&option),
        None => json!({ "outcome": { "outcome": "cancelled" } }),
    }
}

/// The agent's mode for Runo's: its plan mode for Plan only. Other modes are kept to
/// by answering permission requests.
fn pick_mode(mode: &str, modes: &Value) -> Option<String> {
    if mode != "plan" {
        return None;
    }
    let current = modes["currentModeId"].as_str().unwrap_or("");
    modes["availableModes"]
        .as_array()?
        .iter()
        .filter_map(|m| m["id"].as_str())
        .find(|id| id.to_ascii_lowercase().contains("plan"))
        .filter(|id| *id != current)
        .map(str::to_string)
}

/// The agent's id for the model picked in Runo: the same id, or one that ends in it
/// (`anthropic/claude-sonnet-4` for `claude-sonnet-4`).
fn pick_model(model: &str, models: &Value) -> Option<String> {
    let ids: Vec<&str> = models["availableModels"]
        .as_array()?
        .iter()
        .filter_map(|m| m["modelId"].as_str())
        .collect();
    ids.iter()
        .find(|id| **id == model)
        .or_else(|| ids.iter().find(|id| id.ends_with(&format!("/{model}"))))
        .map(|id| id.to_string())
}

/// A file the agent reads through Runo.
fn read_file(params: &Value) -> Result<String, String> {
    let path = params["path"].as_str().ok_or("no path")?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("couldn't read {path}: {e}"))?;
    let line = params["line"].as_u64().unwrap_or(1).max(1) as usize;
    let limit = params["limit"].as_u64().map(|l| l as usize);
    if line == 1 && limit.is_none() {
        return Ok(text);
    }
    let lines = text.split_inclusive('\n').skip(line - 1);
    Ok(match limit {
        Some(limit) => lines.take(limit).collect(),
        None => lines.collect(),
    })
}

/// A file the agent writes through Runo: only inside the session folder.
fn write_file(cwd: &str, params: &Value) -> Result<(), String> {
    let path = PathBuf::from(params["path"].as_str().ok_or("no path")?);
    let content = params["content"].as_str().ok_or("no content")?;
    if !inside(Path::new(cwd), &path) {
        return Err(format!(
            "{} is outside the session folder, so Runo didn't write it",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, content).map_err(|e| format!("couldn't write {}: {e}", path.display()))
}

/// Whether `path` is inside `folder`, after links and `..` are resolved as far as the
/// path exists.
fn inside(folder: &Path, path: &Path) -> bool {
    let Ok(folder) = folder.canonicalize() else {
        return false;
    };
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        folder.join(path)
    };
    // The nearest part of the path that exists, resolved, with the rest put back.
    let mut existing = path.as_path();
    let mut rest = Vec::new();
    while !existing.exists() {
        let (Some(parent), Some(name)) = (existing.parent(), existing.file_name()) else {
            return false;
        };
        rest.push(name.to_os_string());
        existing = parent;
    }
    let Ok(mut resolved) = existing.canonicalize() else {
        return false;
    };
    for name in rest.iter().rev() {
        if name == ".." {
            return false;
        }
        resolved.push(name);
    }
    resolved.starts_with(&folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Value {
        json!([
            { "optionId": "a1", "name": "Allow", "kind": "allow_once" },
            { "optionId": "a2", "name": "Always", "kind": "allow_always" },
            { "optionId": "r1", "name": "Reject", "kind": "reject_once" },
        ])
    }

    fn asking(kind: &str) -> Value {
        json!({ "sessionId": "s", "toolCall": { "toolCallId": "t", "kind": kind }, "options": options() })
    }

    #[test]
    fn the_mode_settles_what_it_can_and_asks_the_rest() {
        let pick = |s: &str| Decision::Pick(s.into());
        assert_eq!(decide("full", &asking("execute")), pick("a1"));
        assert_eq!(decide("edit", &asking("edit")), pick("a1"));
        assert_eq!(decide("", &asking("read")), pick("a1"));
        assert_eq!(decide("edit", &asking("execute")), Decision::Ask);
        assert_eq!(decide("plan", &asking("edit")), pick("r1"));
        assert_eq!(decide("plan", &asking("search")), pick("a1"));
        assert_eq!(decide("plan", &asking("execute")), Decision::Ask);
    }

    #[test]
    fn plan_only_uses_the_agents_plan_mode() {
        let modes = json!({
            "currentModeId": "build",
            "availableModes": [{ "id": "build" }, { "id": "plan" }],
        });
        assert_eq!(pick_mode("plan", &modes).as_deref(), Some("plan"));
        assert_eq!(pick_mode("edit", &modes), None);
        assert_eq!(pick_mode("plan", &Value::Null), None);
    }

    #[test]
    fn a_model_matches_by_id_or_its_last_part() {
        let models = json!({ "availableModels": [
            { "modelId": "anthropic/claude-sonnet-4" }, { "modelId": "gpt-5" },
        ]});
        assert_eq!(pick_model("gpt-5", &models).as_deref(), Some("gpt-5"));
        assert_eq!(
            pick_model("claude-sonnet-4", &models).as_deref(),
            Some("anthropic/claude-sonnet-4")
        );
        assert_eq!(pick_model("o3", &models), None);
    }

    #[test]
    fn writes_stay_inside_the_session_folder() {
        let dir = std::env::temp_dir().join(format!("runo-acp-fs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let cwd = dir.to_string_lossy().to_string();
        let file = dir.join("src/new/a.txt");
        write_file(&cwd, &json!({ "path": file, "content": "hi\nthere\n" })).unwrap();
        assert_eq!(
            read_file(&json!({ "path": file, "line": 2, "limit": 1 })).unwrap(),
            "there\n"
        );
        let outside = dir.join("../escape.txt");
        assert!(write_file(&cwd, &json!({ "path": outside, "content": "x" })).is_err());
        assert!(write_file(&cwd, &json!({ "path": "/etc/runo-x", "content": "x" })).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(all(test, unix))]
mod host_tests {}
