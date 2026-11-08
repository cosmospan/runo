//! Turns agents' structured output into conversation updates: reply text as it streams,
//! a short label for each tool step, token use and a run summary. Claude Code's
//! `--output-format stream-json` (also spoken by Amp and Qwen Code) is read by
//! `ClaudeStream`; the JSON lines of Codex, Cursor, Droid, Gemini, OpenCode and Copilot
//! by `JsonStream`.

use serde_json::Value;

use crate::crew::{Mark, Step};
use crate::harness::{AgentEvent, ToolKind, ToolStep};
use crate::store::{Question, Usage};

/// The parser for one run, by the format the agent prints.
pub enum Parser {
    Claude(ClaudeStream),
    Json(Box<JsonStream>),
    /// Pi's events, from its RPC or JSON mode (see `harness::pi`).
    Pi(Box<crate::harness::pi::PiStream>),
    /// Antigravity CLI's NDJSON events (see `AntigravityStream`).
    Antigravity(Box<AntigravityStream>),
}

impl Parser {
    /// The parser for a run of `provider` in `cwd`, or `None` for plain text output.
    pub fn for_provider(provider: &str, cwd: &str) -> Option<Parser> {
        Parser::for_format(crate::agent::output(provider), cwd)
    }

    /// The parser for output in `format`, or `None` for plain text.
    pub fn for_format(format: crate::agent::Output, cwd: &str) -> Option<Parser> {
        match format {
            crate::agent::Output::Claude => Some(Parser::Claude(ClaudeStream::new(cwd))),
            crate::agent::Output::Json => Some(Parser::Json(Box::new(JsonStream::new(cwd)))),
            crate::agent::Output::Pi => {
                Some(Parser::Pi(Box::new(crate::harness::pi::PiStream::new(cwd))))
            }
            crate::agent::Output::Antigravity => {
                Some(Parser::Antigravity(Box::new(AntigravityStream::new(cwd))))
            }
            crate::agent::Output::Text => None,
        }
    }

    pub fn feed(&mut self, line: &str) -> Vec<AgentEvent> {
        self.feed_parsed(line, parse_line(line))
    }

    /// `feed` for a line already parsed as JSON off the main thread (see `parse_line`).
    pub fn feed_parsed(&mut self, line: &str, parsed: Option<Value>) -> Vec<AgentEvent> {
        match self {
            Parser::Claude(p) => p.feed_parsed(line, parsed),
            Parser::Json(p) => p.feed_parsed(line, parsed),
            Parser::Pi(p) => p.feed_parsed(line, parsed),
            Parser::Antigravity(p) => p.feed_parsed(line, parsed),
        }
    }
}

/// A line of an agent's output as JSON, when it is JSON. Agents' reader threads call this,
/// so the parsing is done before the line reaches the main thread.
pub fn parse_line(line: &str) -> Option<Value> {
    serde_json::from_str::<Value>(line.trim()).ok()
}

/// The conversation id a JSON event names, under any of the keys the CLIs use.
fn conversation_of(event: &Value) -> Option<String> {
    ["session_id", "sessionID", "sessionId", "thread_id"]
        .iter()
        .find_map(|k| event[*k].as_str())
        .or_else(|| event["data"]["sessionId"].as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Per-run parser state.
pub struct ClaudeStream {
    /// Working directory, so file paths in steps can be shown relative to it.
    cwd: String,
    /// Whether text for the current API message already arrived as deltas, so the
    /// complete `assistant` message does not add it a second time.
    streamed_text: bool,
    /// Whether any reply text has been emitted, to separate later text blocks.
    any_text: bool,
    usage: Usage,
    /// Whether the conversation id was already reported.
    named: bool,
    /// Subagent calls started in the background: their tool result only says they started,
    /// so they stay running until a `task_notification` says they ended.
    background: Vec<String>,
    /// Background tasks' ids and the calls that started them.
    tasks: Vec<(String, String)>,
    /// What a failed tool call last printed. Grok Build ends a stuck turn with
    /// `{"subtype":"error_during_execution"}` and no explanation of its own, so the
    /// final `result` event borrows this to say what cut the turn short.
    last_tool_error: Option<String>,
}

impl ClaudeStream {
    pub fn new(cwd: &str) -> Self {
        Self {
            cwd: cwd.trim_end_matches('/').to_string(),
            streamed_text: false,
            any_text: false,
            usage: Usage::default(),
            named: false,
            background: Vec::new(),
            tasks: Vec::new(),
            last_tool_error: None,
        }
    }

    /// The call that started background task `event` names, by its own id or the task's.
    /// With one background subagent running, it's that one.
    fn task_call(&self, event: &Value) -> Option<String> {
        if let Some(id) = event["tool_use_id"].as_str().filter(|id| !id.is_empty()) {
            return Some(id.to_string());
        }
        let task = event["task_id"].as_str().unwrap_or("");
        self.tasks
            .iter()
            .find(|(t, _)| !task.is_empty() && t == task)
            .map(|(_, call)| call.clone())
            .or_else(|| match self.background.as_slice() {
                [only] => Some(only.clone()),
                _ => None,
            })
    }

    /// A background task started, moved on or ended.
    fn task_event(&mut self, event: &Value) -> Option<AgentEvent> {
        match event["subtype"].as_str().unwrap_or("") {
            "task_started" => {
                let (task, call) = (event["task_id"].as_str()?, event["tool_use_id"].as_str()?);
                self.tasks.push((task.to_string(), call.to_string()));
                None
            }
            "task_progress" => {
                let label = event["last_tool_name"]
                    .as_str()
                    .filter(|t| !t.is_empty())
                    .map(|t| format!("Used {t}"))?;
                Some(AgentEvent::SubStep {
                    parent: self.task_call(event)?,
                    label,
                })
            }
            "task_notification" => {
                let id = self.task_call(event)?;
                self.background.retain(|b| *b != id);
                Some(AgentEvent::ToolDone {
                    ok: !matches!(event["status"].as_str(), Some("failed")),
                    output: event["summary"].as_str().unwrap_or("").to_string(),
                    id,
                })
            }
            _ => None,
        }
    }

    /// Feed one stdout line. Lines that are not JSON pass through as text.
    /// Tests feed lines unparsed; the app hands them over parsed (see `feed_parsed`).
    #[cfg(test)]
    pub fn feed(&mut self, line: &str) -> Vec<AgentEvent> {
        self.feed_parsed(line, parse_line(line))
    }

    /// `feed`, with the line already parsed if it is JSON.
    pub fn feed_parsed(&mut self, line: &str, parsed: Option<Value>) -> Vec<AgentEvent> {
        let Some(event) = parsed else {
            return if line.trim().is_empty() {
                vec![]
            } else {
                vec![self.text(line.to_string())]
            };
        };
        let mut out = Vec::new();
        let kind = event["type"].as_str().unwrap_or("");
        if !self.named && matches!(kind, "system" | "result") {
            if let Some(id) = conversation_of(&event) {
                self.named = true;
                out.push(AgentEvent::Conversation(id));
            }
        }
        match kind {
            "system" => out.extend(self.task_event(&event)),
            "stream_event" => {
                if !event["parent_tool_use_id"].is_null() {
                    return out;
                }
                let inner = &event["event"];
                match inner["type"].as_str().unwrap_or("") {
                    "message_start" => self.streamed_text = false,
                    "content_block_start" if inner["content_block"]["type"] == "text" => {
                        if self.any_text {
                            out.push(AgentEvent::Text("\n\n".into()));
                        }
                    }
                    "content_block_delta" if inner["delta"]["type"] == "text_delta" => {
                        if let Some(text) = inner["delta"]["text"].as_str() {
                            self.streamed_text = true;
                            self.any_text = true;
                            out.push(AgentEvent::Text(text.to_string()));
                        }
                    }
                    _ => {}
                }
            }
            "assistant" => {
                // Of a subagent's traffic only its latest step shows, on its parent's step.
                if let Some(parent) = event["parent_tool_use_id"].as_str() {
                    for block in event["message"]["content"].as_array().into_iter().flatten() {
                        if block["type"] == "tool_use" {
                            let tool = block["name"].as_str().unwrap_or("tool");
                            out.push(AgentEvent::SubStep {
                                parent: parent.to_string(),
                                label: self.step_label(tool, &block["input"]),
                            });
                        }
                    }
                    return out;
                }
                let u = &event["message"]["usage"];
                if u.is_object() {
                    let n = |k: &str| u[k].as_u64().unwrap_or(0);
                    self.usage.input = n("input_tokens");
                    self.usage.cached =
                        n("cache_read_input_tokens") + n("cache_creation_input_tokens");
                    out.push(AgentEvent::Usage(self.usage));
                }
                for block in event["message"]["content"].as_array().into_iter().flatten() {
                    if block["type"] == "tool_use" {
                        match block["name"].as_str().unwrap_or("") {
                            "ExitPlanMode" => {
                                if let Some(plan) = block["input"]["plan"].as_str() {
                                    out.push(AgentEvent::Plan(plan.to_string()));
                                }
                            }
                            name if is_todo_tool(name) => {
                                out.extend(todos(&block["input"]).map(AgentEvent::Todos));
                            }
                            "AskUserQuestion" => {
                                let questions: Vec<Question> = block["input"]["questions"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .map(|q| Question {
                                        question: q["question"].as_str().unwrap_or("").to_string(),
                                        options: q["options"]
                                            .as_array()
                                            .into_iter()
                                            .flatten()
                                            .filter_map(|o| {
                                                o["label"].as_str().or(o.as_str()).map(String::from)
                                            })
                                            .collect(),
                                    })
                                    .filter(|q| !q.question.is_empty())
                                    .collect();
                                if !questions.is_empty() {
                                    out.push(AgentEvent::Questions(questions));
                                }
                            }
                            _ => {}
                        }
                    }
                    match block["type"].as_str().unwrap_or("") {
                        "text" if !self.streamed_text => {
                            if let Some(text) = block["text"].as_str().filter(|t| !t.is_empty()) {
                                let sep = if self.any_text { "\n\n" } else { "" };
                                out.push(self.text(format!("{sep}{text}")));
                            }
                        }
                        "tool_use" => {
                            let tool = block["name"].as_str().unwrap_or("tool");
                            let label = self.step_label(tool, &block["input"]);
                            if ToolKind::of(tool) == ToolKind::Subagent
                                && block["input"]["run_in_background"] == true
                            {
                                if let Some(id) = block["id"].as_str() {
                                    self.background.push(id.to_string());
                                }
                            }
                            out.push(AgentEvent::Tool(
                                match block["id"].as_str().filter(|id| !id.is_empty()) {
                                    Some(id) => ToolStep::running(id, tool, label),
                                    None => ToolStep::of(tool, label),
                                },
                            ));
                        }
                        _ => {}
                    }
                }
            }
            // Tool results come back as a user message, one block per finished call.
            "user" => {
                if !event["parent_tool_use_id"].is_null() {
                    return out;
                }
                for block in event["message"]["content"].as_array().into_iter().flatten() {
                    if block["type"] != "tool_result" {
                        continue;
                    }
                    if let Some(id) = block["tool_use_id"].as_str().filter(|id| !id.is_empty()) {
                        let ok = block["is_error"].as_bool() != Some(true);
                        // A background subagent has only started; it ends later.
                        if self.background.iter().any(|b| b == id) {
                            if ok {
                                continue;
                            }
                            self.background.retain(|b| b != id);
                        }
                        let output = grok_tool_text(&block["content"])
                            .unwrap_or_else(|| result_text(&block["content"]));
                        if !ok {
                            let trimmed = output.trim();
                            if !trimmed.is_empty() {
                                self.last_tool_error = Some(short(trimmed, 240));
                            }
                        }
                        out.push(AgentEvent::ToolDone {
                            id: id.to_string(),
                            ok,
                            output,
                        });
                    }
                }
            }
            "result" => {
                let n = |k: &str| event["usage"][k].as_u64().unwrap_or(0);
                if event["usage"].is_object() {
                    self.usage.output = n("output_tokens");
                    if self.usage.input == 0 && self.usage.cached == 0 {
                        self.usage.input = n("input_tokens");
                        self.usage.cached =
                            n("cache_read_input_tokens") + n("cache_creation_input_tokens");
                    }
                }
                self.usage.cost_cents = event["total_cost_usd"].as_f64().unwrap_or(0.0) * 100.0;
                if let Some(models) = event["modelUsage"].as_object() {
                    self.usage.window = models
                        .values()
                        .filter_map(|m| m["contextWindow"].as_u64())
                        .max()
                        .unwrap_or(0);
                }
                if !self.usage.is_empty() {
                    out.push(AgentEvent::Usage(self.usage));
                }
                let mut denied: Vec<String> = Vec::new();
                for d in event["permission_denials"].as_array().into_iter().flatten() {
                    if let Some(tool) = d["tool_name"].as_str() {
                        // Plan and question tools are answered in the UI, not by allowing them.
                        if !matches!(tool, "ExitPlanMode" | "AskUserQuestion")
                            && !denied.iter().any(|t| t == tool)
                        {
                            denied.push(tool.to_string());
                        }
                    }
                }
                if !denied.is_empty() {
                    out.push(AgentEvent::Denied(denied));
                }
                let mut parts = Vec::new();
                if let Some(ms) = event["duration_ms"].as_u64() {
                    parts.push(duration(ms));
                }
                if let Some(turns) = event["num_turns"].as_u64() {
                    parts.push(format!("{turns} turn{}", if turns == 1 { "" } else { "s" }));
                }
                if let Some(cost) = event["total_cost_usd"].as_f64().filter(|c| *c > 0.0) {
                    parts.push(format!("${cost:.2}"));
                }
                if let Some(share) = cache_share(&event["usage"]) {
                    parts.push(format!("{share}% from cache"));
                }
                if !parts.is_empty() {
                    out.push(AgentEvent::Summary(parts.join(" · ")));
                }
                if event["is_error"].as_bool() == Some(true) {
                    let reason = self.failure_reason(&event);
                    // A used-up plan says so in words only when no rate_limit_event came.
                    if let Some(limit) = crate::planlimit::from_text(&reason, chrono::Local::now())
                    {
                        out.push(AgentEvent::Limit(limit));
                    }
                    out.push(AgentEvent::Error(reason));
                } else if !self.any_text {
                    if let Some(text) = event["result"].as_str().filter(|t| !t.is_empty()) {
                        out.push(self.text(text.to_string()));
                    }
                }
            }
            // Where the plan's usage window stands, sent with each turn.
            "rate_limit_event" => {
                if let Some(limit) = crate::planlimit::from_claude(&event["rate_limit_info"]) {
                    out.push(AgentEvent::Limit(limit));
                }
            }
            _ => {}
        }
        out
    }

    fn text(&mut self, text: String) -> AgentEvent {
        self.any_text = true;
        AgentEvent::Text(text)
    }

    fn step_label(&self, tool: &str, input: &Value) -> String {
        step_label(&self.cwd, tool, input)
    }

    /// What to show for a `result` event with `is_error: true`. Claude Code puts the
    /// reason in the `result` text, so that is used when it is there. Grok Build leaves
    /// `result` out and only emits a short `subtype` such as `error_during_execution`,
    /// plus an `errors` list of short codes and (just above) the tool_result that cut
    /// the turn short; this stitches them into a sentence a person can read.
    fn failure_reason(&mut self, event: &Value) -> String {
        if let Some(text) = event["result"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return text.to_string();
        }
        let subtype = event["subtype"].as_str().unwrap_or("");
        let header = failure_header(subtype);
        let detail = self.last_tool_error.take().filter(|d| !d.trim().is_empty());
        let codes: Vec<String> = event["errors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::trim).filter(|s| !s.is_empty()))
            .map(str::to_string)
            .collect();
        let mut parts: Vec<String> = Vec::new();
        match header {
            Some(h) => parts.push(h.to_string()),
            // Fall back to the raw subtype so the person at least sees Grok's own label.
            None if !subtype.is_empty() && codes.iter().all(|c| c != subtype) => {
                parts.push(subtype.to_string())
            }
            None => {}
        }
        if let Some(d) = detail {
            parts.push(d);
        }
        let codes: Vec<String> = codes
            .into_iter()
            .filter(|c| {
                let lower = c.to_lowercase();
                !parts.iter().any(|p| p.to_lowercase().contains(&lower))
            })
            .collect();
        if !codes.is_empty() {
            parts.push(codes.join(", "));
        }
        if parts.is_empty() {
            "The agent reported an error.".to_string()
        } else {
            parts.join(": ")
        }
    }
}

/// A readable headline for the `error_*` subtypes Claude Code and Grok Build emit on a
/// failed `result`. Other subtypes (`success`, custom) pass through.
fn failure_header(subtype: &str) -> Option<&'static str> {
    match subtype {
        "error_during_execution" => Some("The turn was interrupted while a tool was running"),
        "error_max_turns" => Some("The turn hit the maximum turn limit"),
        "error_max_structured_output_retries" => {
            Some("The model couldn't produce output matching the requested schema")
        }
        _ => None,
    }
}

/// First line of `text`, cut to `max` characters.
fn short(text: &str, max: usize) -> String {
    let line = text.lines().next().unwrap_or("").to_string();
    if line.chars().count() > max || text.contains('\n') {
        format!("{}…", line.chars().take(max).collect::<String>())
    } else {
        line
    }
}

/// What a tool printed, from a result given as text or as content blocks.
pub(crate) fn result_text(content: &Value) -> String {
    let text = match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b["text"].as_str().or(b.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    crate::harness::event::tail(&text, 4096)
}

/// What a Grok Build tool printed. Its results arrive as the tool's own record in JSON,
/// such as `{"type":"Bash","output_for_prompt":"exit: 0\nok\n",…}`, or as a list of
/// content blocks; the text the model was shown is what a step shows.
fn grok_tool_text(content: &Value) -> Option<String> {
    let text = content.as_str()?.trim_start();
    if !text.starts_with(['{', '[']) {
        return None;
    }
    let record: Value = serde_json::from_str(text).ok()?;
    let shown = |v: &Value| {
        ["output_for_prompt", "tool_output_for_prompt", "raw_output"]
            .iter()
            .find_map(|k| v[*k].as_str().map(str::to_string))
    };
    let found = match &record {
        Value::Object(fields) => {
            shown(&record).or_else(|| fields.values().filter(|v| v.is_object()).find_map(shown))
        }
        Value::Array(blocks) => {
            let texts: Vec<&str> = blocks
                .iter()
                .filter_map(|b| b["content"]["text"].as_str())
                .collect();
            (!texts.is_empty()).then(|| texts.join("\n"))
        }
        _ => None,
    }?;
    Some(crate::harness::event::tail(&found, 4096))
}

/// A short label for a tool call, such as "Read src/main.rs" or "Ran cargo test". Tool
/// names and argument keys differ between CLIs; unknown tools read "Used <tool>".
pub(crate) fn step_label(cwd: &str, tool: &str, input: &Value) -> String {
    let s = |keys: &[&str]| {
        keys.iter()
            .find_map(|k| match &input[*k] {
                Value::String(v) => Some(v.clone()),
                // Codex and some others pass a command as its argument list.
                Value::Array(parts) => Some(
                    parts
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" "),
                ),
                _ => None,
            })
            .unwrap_or_default()
    };
    let rel = |path: String| {
        path.strip_prefix(cwd)
            .map(|p| p.trim_start_matches('/'))
            .filter(|p| !p.is_empty())
            .unwrap_or(&path)
            .replace('`', "'")
    };
    let path = || {
        rel(s(&[
            "file_path",
            "filePath",
            "path",
            "absolute_path",
            "target_file",
        ]))
    };
    match tool {
        "Read" | "read" | "view" | "read_file" | "ReadFile" | "hashline_read" => {
            format!("Read {}", path())
        }
        "Edit" | "MultiEdit" | "edit" | "edit_file" | "replace" | "str_replace_editor"
        | "ApplyPatch" | "apply_patch" | "search_replace" | "hashline_edit" => match path() {
            p if p.is_empty() => "Edited files".to_string(),
            p => format!("Edited {p}"),
        },
        "Write" | "write" | "create" | "Create" | "write_file" | "create_file" => {
            format!("Wrote {}", path())
        }
        "NotebookEdit" => format!("Edited {}", rel(s(&["notebook_path"]))),
        "Bash"
        | "bash"
        | "shell"
        | "Execute"
        | "run_shell_command"
        | "exec_command"
        | "run_terminal_command"
        | "run_terminal_cmd" => {
            format!("Ran {}", short(&s(&["command", "cmd"]), 70))
        }
        "Grep" | "grep" | "search_file_content" | "search" | "hashline_grep" => {
            format!("Searched for {}", short(&s(&["pattern", "query"]), 50))
        }
        "Glob" | "glob" | "LS" | "ls" | "list" | "list_directory" | "list_dir" => {
            let what = s(&["pattern", "path", "dir_path"]);
            format!("Listed {}", short(&rel(what), 50))
        }
        "WebSearch" | "web_search" | "google_web_search" | "websearch" => {
            format!("Searched the web for {}", short(&s(&["query"]), 50))
        }
        "WebFetch" | "web_fetch" | "webfetch" | "fetch" => {
            format!("Fetched {}", short(&s(&["url", "prompt"]), 60))
        }
        "Task" | "Agent" | "task" | "spawn_subagent" => format!(
            "Ran a subagent: {}",
            short(&s(&["description", "prompt"]), 60)
        ),
        tool if is_todo_tool(tool) => "Updated the plan".to_string(),
        "ExitPlanMode" | "exit_plan_mode" => "Proposed a plan".to_string(),
        "AskUserQuestion" | "ask_user_question" => "Asked a question".to_string(),
        other => format!("Used {other}"),
    }
}

/// Whether `tool` is one of the CLIs' own todo tools.
fn is_todo_tool(tool: &str) -> bool {
    matches!(
        tool,
        "TodoWrite" | "todowrite" | "write_todos" | "update_todo" | "todo_write" | "update_plan"
    )
}

/// The todo list a todo tool was given, under any of the shapes the CLIs use:
/// `{"todos":[{"content":…,"status":"in_progress"}]}` (Claude, Gemini, OpenCode, Cursor),
/// `{"plan":[{"step":…,"status":…}]}` and Codex's `{"items":[{"text":…,"completed":true}]}`.
pub(crate) fn todos(input: &Value) -> Option<Vec<Step>> {
    let items = ["todos", "plan", "items"]
        .iter()
        .find_map(|k| input[*k].as_array())?;
    let steps: Vec<Step> = items
        .iter()
        .filter_map(|item| {
            let text = ["content", "text", "step", "description", "title", "task"]
                .iter()
                .find_map(|k| item[*k].as_str())
                .or(item.as_str())?;
            let status = ["status", "state"]
                .iter()
                .find_map(|k| item[*k].as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let mark = if item["completed"] == true
                || status.contains("complete")
                || status.contains("done")
            {
                Mark::Done
            } else if status.contains("progress")
                || status.contains("doing")
                || status.contains("active")
            {
                Mark::Doing
            } else {
                Mark::Todo
            };
            Some(Step {
                text: text.to_string(),
                mark,
            })
        })
        .collect();
    let steps = crate::crew::clean_steps(steps);
    (!steps.is_empty()).then_some(steps)
}

/// Per-run parser for the JSON lines of Codex (`exec --json`), Cursor and Droid
/// (`stream-json`), Gemini (`stream-json`), OpenCode (`--format json`) and Copilot
/// (`--output-format json`). Unknown events are skipped; lines that are not JSON pass
/// through as text.
pub struct JsonStream {
    cwd: String,
    any_text: bool,
    /// Copilot sent the current message in pieces, so its full copy is not added again.
    streamed: bool,
    usage: Usage,
    named: bool,
    /// Codex items already shown as a step, by id.
    shown: std::collections::HashSet<String>,
    /// Codex calls shown while they ran, whose result is still to come, by id.
    running: std::collections::HashSet<String>,
    /// How much of each Codex message is already shown, by item id.
    written: std::collections::HashMap<String, usize>,
    /// The last error reported, so the same failure isn't shown twice.
    last_error: String,
    /// Warnings already shown, so a repeated one isn't.
    notices: std::collections::HashSet<String>,
}

impl JsonStream {
    pub fn new(cwd: &str) -> Self {
        Self {
            cwd: cwd.trim_end_matches('/').to_string(),
            any_text: false,
            streamed: false,
            usage: Usage::default(),
            named: false,
            shown: Default::default(),
            running: Default::default(),
            written: Default::default(),
            last_error: String::new(),
            notices: Default::default(),
        }
    }

    /// A warning, unless it was already shown in this run.
    fn notice(&mut self, message: String) -> Option<AgentEvent> {
        let message = message.trim().to_string();
        if message.is_empty() || !self.notices.insert(message.clone()) {
            return None;
        }
        Some(AgentEvent::Notice(message))
    }

    /// An error, unless it repeats the one just reported.
    fn error(&mut self, message: String) -> Option<AgentEvent> {
        if message == self.last_error {
            return None;
        }
        self.last_error = message.clone();
        Some(AgentEvent::Error(message))
    }

    /// A Codex item as it starts, grows (`done` false) or finishes. Messages stream in as
    /// their text grows; other items show as a step once, as soon as they can be named.
    fn codex_item(&mut self, item: &Value, done: bool) -> Vec<AgentEvent> {
        let id = item["id"].as_str().unwrap_or("").to_string();
        let kind = item["type"].as_str().unwrap_or("");
        if kind == "agent_message" {
            let text = item["text"].as_str().unwrap_or("");
            if id.is_empty() {
                return if done {
                    self.block(text).into_iter().collect()
                } else {
                    vec![]
                };
            }
            let written = self.written.get(&id).copied().unwrap_or(0);
            // Text that shrank or was rewritten mid-character is left as first shown.
            let Some(rest) = text.get(written..) else {
                return vec![];
            };
            let rest = if written == 0 {
                rest.trim_start()
            } else {
                rest
            };
            let rest = if done { rest.trim_end() } else { rest };
            if rest.is_empty() {
                return vec![];
            }
            self.written.insert(id, text.len());
            let sep = if written == 0 && self.any_text {
                "\n\n"
            } else {
                ""
            };
            self.any_text = true;
            return vec![AgentEvent::Text(format!("{sep}{rest}"))];
        }
        // A todo list is named once but each change to it is passed on.
        if kind == "todo_list" {
            let mut out = Vec::new();
            if id.is_empty() || self.shown.insert(id) {
                out.push(AgentEvent::Tool(ToolStep::of(
                    "update_plan",
                    "Updated the plan",
                )));
            }
            out.extend(todos(item).map(AgentEvent::Todos));
            return out;
        }
        if !id.is_empty() && self.shown.contains(&id) {
            // A call shown while it ran: its result.
            if done && self.running.remove(&id) {
                return vec![AgentEvent::ToolDone {
                    ok: codex_ok(item),
                    output: codex_output(item),
                    id,
                }];
            }
            return vec![];
        }
        let mut out = Vec::new();
        match kind {
            "command_execution" => {
                let command = item["command"].as_str().unwrap_or("");
                if !command.is_empty() {
                    let label = format!("Ran {}", short(&shell_body(command), 70));
                    out.extend(self.codex_call(&id, "shell", label, item, done));
                }
            }
            "file_change" => {
                for change in item["changes"].as_array().into_iter().flatten() {
                    let path = change["path"].as_str().unwrap_or("").to_string();
                    let verb = match change["kind"].as_str().unwrap_or("") {
                        "add" => "Wrote",
                        "delete" => "Deleted",
                        _ => "Edited",
                    };
                    let path = path
                        .strip_prefix(&self.cwd)
                        .map(|p| p.trim_start_matches('/').to_string())
                        .unwrap_or(path);
                    let kind = match verb {
                        "Wrote" => ToolKind::Write,
                        "Deleted" => ToolKind::Delete,
                        _ => ToolKind::Edit,
                    };
                    let mut step = ToolStep::of("apply_patch", format!("{verb} {path}")).kind(kind);
                    if !codex_ok(item) {
                        step.finish(false);
                    }
                    out.push(AgentEvent::Tool(step));
                }
            }
            "mcp_tool_call" => {
                let tool = item["tool"].as_str().unwrap_or("a tool");
                let label = format!("Used {} {tool}", item["server"].as_str().unwrap_or(""));
                let calls = self.codex_call(&id, tool, label, item, done);
                out.extend(calls.into_iter().map(|c| match c {
                    AgentEvent::Tool(step) => AgentEvent::Tool(step.kind(ToolKind::Mcp)),
                    other => other,
                }));
            }
            "web_search" => {
                let query = item["query"].as_str().unwrap_or("");
                if !query.is_empty() || done {
                    out.push(AgentEvent::Tool(ToolStep::of(
                        "web_search",
                        format!("Searched the web for {}", short(query, 50)),
                    )));
                }
            }
            // Warnings that don't stop the run, such as "Model metadata for … not found".
            "error" | "warning" => {
                let message = error_message(&item["message"])
                    .or_else(|| error_message(&item["text"]))
                    .unwrap_or_default();
                out.extend(self.notice(message));
            }
            // Reasoning summaries start with a bold heading, such as "**Reading the tests**".
            "reasoning" => {
                let text = item["text"].as_str().unwrap_or("");
                let heading = text
                    .lines()
                    .map(|l| l.trim().trim_matches('*').trim())
                    .find(|l| !l.is_empty())
                    .unwrap_or("");
                if !heading.is_empty() {
                    out.push(AgentEvent::Tool(
                        ToolStep::done(format!("Thought: {}", short(heading, 70)))
                            .kind(ToolKind::Think),
                    ));
                }
            }
            _ => {}
        }
        if !out.is_empty() && !id.is_empty() {
            self.shown.insert(id);
        }
        out
    }

    /// A Codex call (a command, an MCP tool): running until its item completes, or
    /// finished at once when it is first seen complete.
    fn codex_call(
        &mut self,
        id: &str,
        tool: &str,
        label: String,
        item: &Value,
        done: bool,
    ) -> Vec<AgentEvent> {
        if id.is_empty() {
            let mut step = ToolStep::of(tool, label);
            if done && !codex_ok(item) {
                step.finish(false);
            }
            return vec![AgentEvent::Tool(step)];
        }
        let mut out = vec![AgentEvent::Tool(ToolStep::running(id, tool, label))];
        if done {
            out.push(AgentEvent::ToolDone {
                id: id.to_string(),
                ok: codex_ok(item),
                output: codex_output(item),
            });
        } else {
            self.running.insert(id.to_string());
        }
        out
    }

    /// A complete block of reply text, set apart from earlier ones.
    fn block(&mut self, text: &str) -> Option<AgentEvent> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let sep = if self.any_text { "\n\n" } else { "" };
        self.any_text = true;
        Some(AgentEvent::Text(format!("{sep}{text}")))
    }

    /// A piece of reply text arriving as it is written.
    fn piece(&mut self, text: &str) -> Option<AgentEvent> {
        if text.is_empty() {
            return None;
        }
        self.any_text = true;
        Some(AgentEvent::Text(text.to_string()))
    }

    /// A tool call's step, and the todo list it wrote when it is a todo tool. With an
    /// `id` the call runs until a result names it; without, it is already done.
    fn tool(&self, tool: &str, input: &Value, id: &str) -> Vec<AgentEvent> {
        let label = step_label(&self.cwd, tool, input);
        let step = if id.is_empty() {
            ToolStep::of(tool, label)
        } else {
            ToolStep::running(id, tool, label)
        };
        let mut out = vec![AgentEvent::Tool(step)];
        if is_todo_tool(tool) {
            out.extend(todos(input).map(AgentEvent::Todos));
        }
        out
    }

    /// `feed`, with the line already parsed if it is JSON.
    pub fn feed_parsed(&mut self, line: &str, parsed: Option<Value>) -> Vec<AgentEvent> {
        let Some(event) = parsed else {
            return if line.trim().is_empty() {
                vec![]
            } else {
                vec![AgentEvent::Text(line.to_string())]
            };
        };
        let mut out = Vec::new();
        if !event.is_object() {
            return out;
        }
        if !self.named {
            if let Some(id) = conversation_of(&event) {
                self.named = true;
                out.push(AgentEvent::Conversation(id));
            }
        }
        let num = |v: &Value| v.as_u64().unwrap_or(0);
        let data = &event["data"];
        match event["type"].as_str().unwrap_or("") {
            // Codex: items show as they start and grow, not only once they finish.
            "item.started" | "item.updated" | "item.completed" => {
                let done = event["type"] == "item.completed";
                out.extend(self.codex_item(&event["item"], done));
            }
            "turn.completed" => {
                let u = &event["usage"];
                let cached = num(&u["cached_input_tokens"]);
                // Codex counts cached tokens inside the input.
                self.usage.input = num(&u["input_tokens"]).saturating_sub(cached);
                self.usage.cached = cached;
                self.usage.output = num(&u["output_tokens"]);
                if !self.usage.is_empty() {
                    out.push(AgentEvent::Usage(self.usage));
                }
            }
            "turn.failed" => out.extend(
                self.error(
                    error_message(&event["error"])
                        .unwrap_or_else(|| "The agent reported an error.".into()),
                ),
            ),
            // Cursor: complete assistant messages; Amp-style content blocks.
            "assistant" => {
                for block in event["message"]["content"].as_array().into_iter().flatten() {
                    match block["type"].as_str().unwrap_or("") {
                        "text" => out.extend(self.block(block["text"].as_str().unwrap_or(""))),
                        "tool_use" => out.extend(self.tool(
                            block["name"].as_str().unwrap_or("a tool"),
                            &block["input"],
                            "",
                        )),
                        _ => {}
                    }
                }
            }
            "tool_call" => {
                if let Some(call) = event["tool_call"].as_object() {
                    // Cursor: {"subtype":"started","call_id":…,"tool_call":{"readToolCall":
                    // {"args":{…}}}}, then "completed" with the call's `result` added.
                    let id = event["call_id"].as_str().unwrap_or("");
                    if let Some((kind, call)) = call.iter().next() {
                        if event["subtype"] == "started" {
                            out.extend(self.cursor_step(kind, call, id));
                        } else if event["subtype"] == "completed" && !id.is_empty() {
                            let result = &call["result"];
                            let failed = result.get("success").is_none()
                                && ["error", "failure", "rejected", "permissionDenied"]
                                    .iter()
                                    .any(|k| result.get(*k).is_some());
                            out.push(AgentEvent::ToolDone {
                                id: id.to_string(),
                                ok: !failed,
                                output: String::new(),
                            });
                        }
                    }
                } else if let Some(tool) = event["toolName"].as_str() {
                    // Droid: {"type":"tool_call","toolName":"Read","parameters":{…}}.
                    out.extend(self.tool(tool, &event["parameters"], ""));
                }
            }
            // Droid and Gemini.
            "message" if event["role"] == "assistant" => {
                if let Some(text) = event["content"].as_str() {
                    // Gemini sends deltas.
                    if event["delta"] == true {
                        out.extend(self.piece(text));
                    } else {
                        out.extend(self.block(text));
                    }
                } else if let Some(text) = event["text"].as_str() {
                    out.extend(self.block(text));
                }
            }
            "tool_use" => {
                if let Some(tool) = event["tool_name"].as_str() {
                    // Gemini; its result follows as `tool_result` under `tool_id`.
                    let id = event["tool_id"].as_str().unwrap_or("");
                    out.extend(self.tool(tool, &event["parameters"], id));
                } else if let Some(tool) = event["part"]["tool"].as_str() {
                    // OpenCode, once the call is over.
                    let state = &event["part"]["state"];
                    let mut calls = self.tool(tool, &state["input"], "");
                    if state["status"] == "error" {
                        if let Some(AgentEvent::Tool(step)) = calls.first_mut() {
                            step.finish(false);
                        }
                    }
                    out.extend(calls);
                }
            }
            // Gemini.
            "tool_result" => {
                if let Some(id) = event["tool_id"].as_str().filter(|id| !id.is_empty()) {
                    let output = match &event["output"] {
                        Value::String(text) => text.clone(),
                        _ => error_message(&event["error"]).unwrap_or_default(),
                    };
                    out.push(AgentEvent::ToolDone {
                        id: id.to_string(),
                        ok: event["status"] != "error",
                        output: crate::harness::event::tail(&output, 4096),
                    });
                }
            }
            // OpenCode.
            "text" => out.extend(self.block(event["part"]["text"].as_str().unwrap_or(""))),
            "step_finish" => {
                let part = &event["part"];
                let tokens = &part["tokens"];
                if tokens.is_object() {
                    // The latest step's prompt is the context; output adds up.
                    self.usage.input = num(&tokens["input"]);
                    self.usage.cached =
                        num(&tokens["cache"]["read"]) + num(&tokens["cache"]["write"]);
                    self.usage.output += num(&tokens["output"]) + num(&tokens["reasoning"]);
                }
                self.usage.cost_cents += part["cost"].as_f64().unwrap_or(0.0) * 100.0;
                if !self.usage.is_empty() {
                    out.push(AgentEvent::Usage(self.usage));
                }
            }
            // Cursor and Gemini.
            "result" => {
                let stats = &event["stats"];
                let usage = if stats.is_object() {
                    stats
                } else {
                    &event["usage"]
                };
                if usage.is_object() {
                    let cached = num(&usage["cached"])
                        + num(&usage["cache_read_input_tokens"])
                        + num(&usage["cache_creation_input_tokens"]);
                    self.usage.input =
                        num(&usage["input_tokens"]).saturating_sub(num(&usage["cached"]));
                    self.usage.cached = cached;
                    self.usage.output = num(&usage["output_tokens"]);
                    if !self.usage.is_empty() {
                        out.push(AgentEvent::Usage(self.usage));
                    }
                }
                let ms = event["duration_ms"]
                    .as_u64()
                    .or(stats["duration_ms"].as_u64());
                if let Some(ms) = ms {
                    out.push(AgentEvent::Summary(duration(ms)));
                }
                if event["is_error"] == true || event["status"] == "error" {
                    let reason = event["result"]
                        .as_str()
                        .or(event["error"]["message"].as_str())
                        .unwrap_or("The agent reported an error.");
                    out.push(AgentEvent::Error(reason.to_string()));
                } else if !self.any_text {
                    out.extend(self.block(event["result"].as_str().unwrap_or("")));
                }
            }
            // Droid's final line.
            "completion" => {
                let u = &event["usage"];
                if u.is_object() {
                    self.usage.input = num(&u["input_tokens"]);
                    self.usage.cached =
                        num(&u["cache_read_input_tokens"]) + num(&u["cache_creation_input_tokens"]);
                    self.usage.output = num(&u["output_tokens"]);
                    if !self.usage.is_empty() {
                        out.push(AgentEvent::Usage(self.usage));
                    }
                }
                let mut parts = Vec::new();
                if let Some(ms) = event["durationMs"].as_u64() {
                    parts.push(duration(ms));
                }
                if let Some(turns) = event["numTurns"].as_u64() {
                    parts.push(format!("{turns} turn{}", if turns == 1 { "" } else { "s" }));
                }
                if !parts.is_empty() {
                    out.push(AgentEvent::Summary(parts.join(" · ")));
                }
                if !self.any_text {
                    out.extend(self.block(event["finalText"].as_str().unwrap_or("")));
                }
            }
            // Copilot's session events; a subagent's are summed up by its parent's step.
            "assistant.message_delta" if data["parentToolCallId"].is_null() => {
                self.streamed = true;
                out.extend(self.piece(data["deltaContent"].as_str().unwrap_or("")));
            }
            "assistant.message" if data["parentToolCallId"].is_null() => {
                if std::mem::take(&mut self.streamed) {
                    // Already shown piece by piece; keep the next message apart.
                    if self.any_text {
                        out.push(AgentEvent::Text("\n\n".into()));
                    }
                } else {
                    out.extend(self.block(data["content"].as_str().unwrap_or("")));
                }
            }
            "tool.execution_start" if data["parentToolCallId"].is_null() => {
                let tool = data["toolName"].as_str().unwrap_or("a tool");
                let args = match &data["arguments"] {
                    Value::String(text) => serde_json::from_str(text).unwrap_or(Value::Null),
                    other => other.clone(),
                };
                let id = data["toolCallId"].as_str().unwrap_or("");
                out.extend(self.tool(tool, &args, id));
            }
            "tool.execution_complete" if data["parentToolCallId"].is_null() => {
                if let Some(id) = data["toolCallId"].as_str().filter(|id| !id.is_empty()) {
                    out.push(AgentEvent::ToolDone {
                        id: id.to_string(),
                        ok: data["success"] != false,
                        output: result_text(&data["result"]["content"]),
                    });
                }
            }
            "assistant.usage" => {
                self.usage.input = num(&data["inputTokens"]);
                self.usage.cached = num(&data["cacheReadTokens"]) + num(&data["cacheWriteTokens"]);
                self.usage.output += num(&data["outputTokens"]);
                if !self.usage.is_empty() {
                    out.push(AgentEvent::Usage(self.usage));
                }
            }
            "session.error" => out.push(AgentEvent::Error(
                data["message"]
                    .as_str()
                    .unwrap_or("The agent reported an error.")
                    .to_string(),
            )),
            "error" => {
                // Codex puts the text in `message`, OpenCode in `error.data.message`.
                let message = error_message(&event["message"])
                    .or_else(|| error_message(&event["error"]))
                    .unwrap_or_default();
                // Codex reports its own retries as errors; only the final failure counts.
                if !message.is_empty() && !message.starts_with("Reconnecting") {
                    out.extend(self.error(message));
                }
            }
            _ => {}
        }
        out
    }

    /// A Cursor tool call, keyed by its kind (`readToolCall`, `shellToolCall`…).
    fn cursor_step(&self, kind: &str, call: &Value, id: &str) -> Vec<AgentEvent> {
        let args = &call["args"];
        let tool = match kind {
            "readToolCall" => "Read",
            "editToolCall" => "Edit",
            "writeToolCall" => "Write",
            "shellToolCall" => "Bash",
            "grepToolCall" => "Grep",
            "globToolCall" => "Glob",
            "lsToolCall" => "LS",
            "updateTodosToolCall" | "todoToolCall" => "TodoWrite",
            "deleteToolCall" => {
                let path = args["path"].as_str().unwrap_or("");
                let label = format!(
                    "Deleted {}",
                    path.strip_prefix(&self.cwd)
                        .map(|p| p.trim_start_matches('/'))
                        .unwrap_or(path)
                );
                return vec![AgentEvent::Tool(cursor_call(id, "delete", label))];
            }
            "function" => {
                let name = call["name"].as_str().unwrap_or("a tool");
                return vec![AgentEvent::Tool(cursor_call(
                    id,
                    name,
                    format!("Used {name}"),
                ))];
            }
            other => {
                let name = other.strip_suffix("ToolCall").unwrap_or(other);
                return vec![AgentEvent::Tool(cursor_call(
                    id,
                    name,
                    format!("Used {name}"),
                ))];
            }
        };
        let mut input = args.clone();
        if tool == "Glob" {
            if let Some(p) = args["globPattern"].as_str() {
                input["pattern"] = p.into();
            }
        }
        self.tool(tool, &input, id)
    }
}

/// A Cursor call: running until its "completed" event when it has an id.
fn cursor_call(id: &str, tool: &str, label: String) -> ToolStep {
    if id.is_empty() {
        ToolStep::of(tool, label)
    } else {
        ToolStep::running(id, tool, label)
    }
}

/// Whether a Codex item finished well: not failed or declined, and a command that
/// exited 0.
fn codex_ok(item: &Value) -> bool {
    !matches!(item["status"].as_str(), Some("failed" | "declined"))
        && item["exit_code"].as_i64().is_none_or(|code| code == 0)
        && item["error"].is_null()
}

/// The end of what a Codex command or MCP call printed.
fn codex_output(item: &Value) -> String {
    if let Some(output) = item["aggregated_output"].as_str() {
        return crate::harness::event::tail(output, 4096);
    }
    match error_message(&item["error"]) {
        Some(error) => error,
        None => result_text(&item["result"]["content"]),
    }
}

/// The readable text of an error: a string, or the message inside an object such as
/// OpenCode's `{"name":…,"data":{"message":…}}`. Messages that are themselves JSON, as
/// Codex passes on from the API, or end in it ("unexpected status 400: {…}"), give the
/// message inside.
fn error_message(error: &Value) -> Option<String> {
    match error {
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            let inner = text
                .find('{')
                .and_then(|at| serde_json::from_str::<Value>(&text[at..]).ok())
                .filter(Value::is_object)
                .and_then(|v| error_message(&v));
            Some(inner.unwrap_or_else(|| text.to_string()))
        }
        Value::Object(map) => ["message", "detail", "error_description", "error", "data"]
            .iter()
            .find_map(|k| map.get(*k).and_then(error_message))
            .or_else(|| map.get("name").and_then(Value::as_str).map(str::to_string)),
        _ => None,
    }
}

/// The command inside the shell wrapper Codex runs it in, such as `bash -lc '…'` or
/// `/bin/zsh -lc …` on macOS.
fn shell_body(command: &str) -> String {
    let mut words = command.splitn(3, ' ');
    let (Some(shell), Some(flag), Some(rest)) = (words.next(), words.next(), words.next()) else {
        return command.to_string();
    };
    let name = shell.rsplit('/').next().unwrap_or(shell);
    if !matches!(name, "bash" | "sh" | "zsh") || !matches!(flag, "-c" | "-lc") {
        return command.to_string();
    }
    let rest = rest.trim();
    rest.strip_prefix('\'')
        .and_then(|r| r.strip_suffix('\''))
        .or_else(|| rest.strip_prefix('"').and_then(|r| r.strip_suffix('"')))
        .unwrap_or(rest)
        .to_string()
}

fn duration(ms: u64) -> String {
    let secs = ms / 1000;
    match secs {
        0 => format!("{ms}ms"),
        1..=59 => format!("{secs}s"),
        _ => format!("{}m {}s", secs / 60, secs % 60),
    }
}

/// Per-run parser for Google Antigravity CLI's `--output-format stream-json`: an NDJSON
/// stream whose events are each `{"event":"init|step_update|result","<event>":{…}}`.
/// Reply text arrives as `text_delta`s on `agent_response` steps; tool calls are their
/// own step kind; the `result` event has the final usage and status.
pub struct AntigravityStream {
    cwd: String,
    usage: Usage,
    named: bool,
    any_text: bool,
    /// Tool steps shown by step_index, so repeated updates don't re-emit them. The value
    /// is the id Squad Code gives the step, which the matching `tool_done` carries.
    tools: std::collections::HashMap<u64, String>,
}

impl AntigravityStream {
    pub fn new(cwd: &str) -> Self {
        Self {
            cwd: cwd.trim_end_matches('/').to_string(),
            usage: Usage::default(),
            named: false,
            any_text: false,
            tools: Default::default(),
        }
    }

    #[cfg(test)]
    pub fn feed(&mut self, line: &str) -> Vec<AgentEvent> {
        self.feed_parsed(line, parse_line(line))
    }

    pub fn feed_parsed(&mut self, line: &str, parsed: Option<Value>) -> Vec<AgentEvent> {
        let Some(event) = parsed else {
            return if line.trim().is_empty() {
                vec![]
            } else {
                self.any_text = true;
                vec![AgentEvent::Text(line.to_string())]
            };
        };
        let mut out = Vec::new();
        let kind = event["event"].as_str().unwrap_or("");
        let body = match kind {
            "init" => &event["init"],
            "step_update" => &event["step_update"],
            "result" => &event["result"],
            _ => return out,
        };
        if !self.named {
            if let Some(id) = body["conversation_id"].as_str().filter(|s| !s.is_empty()) {
                self.named = true;
                out.push(AgentEvent::Conversation(id.to_string()));
            }
        }
        match kind {
            "step_update" => out.extend(self.step(body)),
            "result" => out.extend(self.result(body)),
            _ => {}
        }
        out
    }

    /// A step as it starts, grows (`state` ACTIVE) or finishes (`state` DONE).
    fn step(&mut self, step: &Value) -> Vec<AgentEvent> {
        let mut out = Vec::new();
        if let Some(u) = antigravity_usage(step) {
            self.usage = combine(self.usage, u);
            out.push(AgentEvent::Usage(self.usage));
        }
        let state = step["state"].as_str().unwrap_or("");
        let step_type = step["step_type"].as_str().unwrap_or("");
        match step_type {
            "agent_response" => {
                if let Some(piece) = step["text_delta"].as_str().filter(|t| !t.is_empty()) {
                    self.any_text = true;
                    out.push(AgentEvent::Text(piece.to_string()));
                }
            }
            "tool" => {
                let index = step["step_index"].as_u64().unwrap_or(0);
                let tool = step["tool_name"].as_str().unwrap_or("tool");
                let input = &step["tool_input"];
                let label = step_label(&self.cwd, tool, input);
                match (self.tools.get(&index).cloned(), state) {
                    (None, "DONE") => {
                        // The CLI reported the call and its result in one event; a step
                        // without an id reads as already finished.
                        let mut step_entry = ToolStep::of(tool, label);
                        if !tool_ok(step) {
                            step_entry.finish(false);
                        }
                        out.push(AgentEvent::Tool(step_entry));
                    }
                    (None, _) => {
                        let id = format!("agy-{index}");
                        self.tools.insert(index, id.clone());
                        out.push(AgentEvent::Tool(ToolStep::running(&id, tool, label)));
                    }
                    (Some(id), "DONE") => {
                        self.tools.remove(&index);
                        out.push(AgentEvent::ToolDone {
                            id,
                            ok: tool_ok(step),
                            output: tool_output(step),
                        });
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        out
    }

    fn result(&mut self, result: &Value) -> Vec<AgentEvent> {
        let mut out = Vec::new();
        if let Some(u) = antigravity_usage(result) {
            self.usage = combine(self.usage, u);
            out.push(AgentEvent::Usage(self.usage));
        }
        let mut parts = Vec::new();
        if let Some(secs) = result["duration_seconds"].as_f64().filter(|s| *s > 0.0) {
            parts.push(duration((secs * 1000.0) as u64));
        }
        if let Some(turns) = result["num_turns"].as_u64() {
            parts.push(format!("{turns} turn{}", if turns == 1 { "" } else { "s" }));
        }
        if !parts.is_empty() {
            out.push(AgentEvent::Summary(parts.join(" · ")));
        }
        match result["status"].as_str().unwrap_or("") {
            "ERROR" | "CANCELED" | "INTERRUPTED" | "INVALID" => {
                let message = result["error"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        result["status"]
                            .as_str()
                            .unwrap_or("The agent reported an error.")
                            .to_string()
                    });
                out.push(AgentEvent::Error(message));
            }
            _ if !self.any_text => {
                if let Some(text) = result["response"].as_str().filter(|t| !t.is_empty()) {
                    self.any_text = true;
                    out.push(AgentEvent::Text(text.to_string()));
                }
            }
            _ => {}
        }
        out
    }
}

/// Antigravity's per-step usage, if the event names one. The CLI reports thinking
/// tokens separately; Squad Code folds them into the output count so the token limit sees them.
fn antigravity_usage(event: &Value) -> Option<Usage> {
    let u = &event["usage"];
    if !u.is_object() {
        return None;
    }
    let n = |k: &str| u[k].as_u64().unwrap_or(0);
    Some(Usage {
        input: n("input_tokens"),
        output: n("output_tokens") + n("thinking_tokens"),
        cached: n("cache_read_tokens"),
        ..Usage::default()
    })
}

/// Keep the larger of each count: step events report running totals, so a later one
/// supersedes an earlier one.
fn combine(mut acc: Usage, next: Usage) -> Usage {
    acc.input = acc.input.max(next.input);
    acc.output = acc.output.max(next.output);
    acc.cached = acc.cached.max(next.cached);
    acc
}

/// Whether a finished Antigravity tool step succeeded.
fn tool_ok(step: &Value) -> bool {
    !matches!(
        step["tool_status"].as_str(),
        Some("ERROR" | "FAILED" | "DENIED")
    ) && step["error"].is_null()
}

/// What a finished Antigravity tool step printed, trimmed to the end.
fn tool_output(step: &Value) -> String {
    let text = step["tool_output"]
        .as_str()
        .or_else(|| step["output"].as_str())
        .or_else(|| step["error"].as_str())
        .unwrap_or("");
    crate::harness::event::tail(text, 4096)
}

/// The share of a run's prompt read from the cache, in percent: low means each call
/// sent the context afresh, which costs more and uses more of a plan.
fn cache_share(usage: &serde_json::Value) -> Option<u64> {
    let n = |k: &str| usage[k].as_u64().unwrap_or(0);
    let read = n("cache_read_input_tokens");
    let prompt = read + n("cache_creation_input_tokens") + n("input_tokens");
    // A tiny prompt says nothing about the cache.
    (prompt >= 10_000).then(|| read * 100 / prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_events_and_the_limit_line_are_read() {
        let mut p = ClaudeStream::new("/x");
        let out = p.feed(r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","resetsAt":1792840000,"rateLimitType":"five_hour","utilization":0.9},"session_id":"s"}"#);
        assert!(matches!(
            &out[..],
            [AgentEvent::Limit(l)] if l.status == crate::planlimit::Status::Warning && l.kind == "five_hour"
        ));
        let out = p.feed(r#"{"type":"result","is_error":true,"result":"You've hit your session limit · resets 3pm (Asia/Calcutta)"}"#);
        assert!(
            matches!(&out[0], AgentEvent::Limit(l) if l.status == crate::planlimit::Status::Rejected)
        );
        assert!(matches!(&out[1], AgentEvent::Error(_)));
    }

    #[test]
    fn the_summary_says_how_much_came_from_the_cache() {
        let mut p = ClaudeStream::new("/x");
        let out = p.feed(r#"{"type":"result","is_error":false,"duration_ms":2000,"usage":{"input_tokens":1000,"cache_read_input_tokens":90000,"cache_creation_input_tokens":9000,"output_tokens":50},"result":"ok"}"#);
        assert!(out
            .iter()
            .any(|e| matches!(e, AgentEvent::Summary(s) if s == "2s · 90% from cache")));
    }

    fn feed_all(lines: &[&str]) -> Vec<AgentEvent> {
        let mut parser = ClaudeStream::new("/work/app/");
        lines
            .iter()
            .flat_map(|l| parser.feed(l))
            .map(plain)
            .collect()
    }

    /// Events as most tests write them: a step reduced to its id, label and state, a
    /// result to its id and outcome. Kinds and outputs have tests of their own.
    fn plain(event: AgentEvent) -> AgentEvent {
        match event {
            AgentEvent::Tool(s) => AgentEvent::Tool(ToolStep {
                id: s.id,
                label: s.label,
                state: s.state,
                ..ToolStep::default()
            }),
            AgentEvent::ToolDone { id, ok, .. } => AgentEvent::ToolDone {
                id,
                ok,
                output: String::new(),
            },
            other => other,
        }
    }

    fn done(label: &str) -> AgentEvent {
        AgentEvent::Tool(ToolStep::done(label))
    }

    fn started(id: &str, label: &str) -> AgentEvent {
        AgentEvent::Tool(ToolStep {
            id: id.into(),
            label: label.into(),
            state: crate::harness::StepState::Running,
            ..ToolStep::default()
        })
    }

    fn finished(id: &str, ok: bool) -> AgentEvent {
        AgentEvent::ToolDone {
            id: id.into(),
            ok,
            output: String::new(),
        }
    }

    #[test]
    fn complete_messages_become_steps_and_text() {
        let out = feed_all(&[
            r#"{"type":"system","subtype":"init","cwd":"/work/app"}"#,
            r#"{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/work/app/src/main.rs"}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"..."}]}}"#,
            r#"{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"cargo test\nmore"}},{"type":"tool_use","name":"Edit","input":{"file_path":"/elsewhere/x.rs"}}]}}"#,
            r#"{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"text","text":"All done."}]}}"#,
            r#"{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"text","text":"Second block."}]}}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":12400,"num_turns":3,"total_cost_usd":0.0412,"result":"All done."}"#,
        ]);
        assert_eq!(
            out,
            [
                done("Read src/main.rs"),
                done("Ran cargo test…"),
                done("Edited /elsewhere/x.rs"),
                AgentEvent::Text("All done.".into()),
                AgentEvent::Text("\n\nSecond block.".into()),
                AgentEvent::Summary("12s · 3 turns · $0.04".into()),
            ]
        );
    }

    #[test]
    fn grok_builds_stream_reads_like_claude_codes() {
        // A real run of xAI's Grok Build 1.0.70 with `--output-format
        // streaming-messages-json`, signed in with a subscription. Ids, paths and the
        // tool list are shortened; signatures and thinking are left out.
        let lines = [
            r#"{"type":"system","subtype":"init","session_id":"0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f","apiKeySource":"oauth","model":"grok-4.7","cwd":"/work/app","permissionMode":"acceptEdits","tools":["run_terminal_command","read_file","search_replace"],"mcp_servers":[],"uuid":"u-1"}"#,
            r#"{"type":"assistant","message":{"id":"msg_0","type":"message","role":"assistant","model":"grok-4.7","content":[{"type":"text","text":"I'll read `note.txt` and run `echo ok`."},{"type":"tool_use","id":"call-a-0","name":"read_file","input":{"target_file":"/work/app/note.txt"}},{"type":"tool_use","id":"call-a-1","name":"run_terminal_command","input":{"command":"echo ok","description":"Print ok to confirm shell works"}}],"stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":6318,"output_tokens":91,"cache_read_input_tokens":10112,"cache_creation_input_tokens":0}},"parent_tool_use_id":null,"session_id":"0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f","uuid":"u-2"}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-a-0","content":"{\"type\":\"ReadFile\",\"FileContent\":{\"content\":\"1→hello from runo\\n\",\"absolute_path\":\"/work/app/note.txt\",\"offset\":null,\"raw_output\":\"hello from runo\\n\",\"total_lines\":1}}","is_error":false},{"type":"tool_result","tool_use_id":"call-a-1","content":"{\"type\":\"Bash\",\"output\":[111,107,10],\"output_for_prompt\":\"exit: 0\\nok\\n\",\"exit_code\":0,\"command\":\"echo ok\",\"truncated\":false,\"signal\":null,\"timed_out\":false}","is_error":false}]},"parent_tool_use_id":null,"session_id":"0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f","uuid":"u-3"}"#,
            r#"{"type":"assistant","message":{"id":"msg_1","type":"message","role":"assistant","model":"grok-4.7","content":[{"type":"text","text":"Read note.txt (hello from runo) and the shell printed ok."}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":166,"output_tokens":52,"cache_read_input_tokens":16384,"cache_creation_input_tokens":0}},"parent_tool_use_id":null,"session_id":"0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f","uuid":"u-4"}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":3794,"duration_api_ms":3649,"num_turns":2,"result":"Read note.txt (hello from runo) and the shell printed ok.","stop_reason":"end_turn","total_cost_usd":0.027074,"usage":{"input_tokens":6484,"output_tokens":143,"cache_read_input_tokens":26496,"cache_creation_input_tokens":0,"server_tool_use":{"web_search_requests":0}},"modelUsage":{"grok-4.7-build":{"inputTokens":6484,"outputTokens":143,"cacheReadInputTokens":26496,"cacheCreationInputTokens":0,"webSearchRequests":0,"costUSD":0.027074}},"session_id":"0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f","uuid":"u-5"}"#,
        ];
        let mut parser = ClaudeStream::new("/work/app");
        let out: Vec<AgentEvent> = lines.iter().flat_map(|l| parser.feed(l)).collect();
        assert_eq!(
            out[0],
            AgentEvent::Conversation("0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f".into())
        );
        let steps: Vec<(String, String)> = out
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Tool(s) => Some((s.id.clone(), s.label.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            steps,
            [
                ("call-a-0".to_string(), "Read note.txt".to_string()),
                ("call-a-1".to_string(), "Ran echo ok".to_string()),
            ]
        );
        // A step shows what the tool printed, not its JSON record.
        let outputs: Vec<&str> = out
            .iter()
            .filter_map(|e| match e {
                AgentEvent::ToolDone {
                    ok: true, output, ..
                } => Some(output.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(outputs, ["hello from runo", "exit: 0\nok"]);
        let text: String = out
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            text,
            "I'll read `note.txt` and run `echo ok`.\n\nRead note.txt (hello from runo) and the shell printed ok."
        );
        let usage = out
            .iter()
            .rev()
            .find_map(|e| match e {
                AgentEvent::Usage(u) => Some(*u),
                _ => None,
            })
            .unwrap();
        assert_eq!(usage.output, 143);
        assert!((usage.cost_cents - 2.7074).abs() < 1e-9);
        assert!(out
            .iter()
            .any(|e| matches!(e, AgentEvent::Summary(s) if s.starts_with("3s · 2 turns · $0.03"))));
        assert!(!out.iter().any(|e| matches!(e, AgentEvent::Error(_))));
    }

    #[test]
    fn grok_builds_error_during_execution_says_what_cut_the_turn_short() {
        // From a real Grok Build 1.0.70 run of "simple node server": its `write` tool
        // was cancelled, and the terminal result says only `error_during_execution`
        // and `errors: ["cancelled"]`. Ids and paths are shortened.
        let lines = [
            r#"{"type":"system","subtype":"init","session_id":"01a1754e-f2f9-7f80-acd2-a20947bfb786","apiKeySource":"oauth","model":"grok-4.7","cwd":"/work/app","permissionMode":"default","tools":["run_terminal_command","write"],"mcp_servers":[],"uuid":"u-1"}"#,
            r#"{"type":"assistant","message":{"id":"msg_0","type":"message","role":"assistant","model":"grok-4.7","content":[{"type":"text","text":"I'll look at the workspace and add a small Node server."},{"type":"tool_use","id":"call-a-0","name":"list_dir","input":{"target_directory":"/work/app"}}],"stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":10,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}},"parent_tool_use_id":null,"session_id":"01a1754e-f2f9-7f80-acd2-a20947bfb786","uuid":"u-2"}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-a-0","content":"{\"type\":\"ListDir\",\"Content\":{\"content\":\"- /work/app/\\n\",\"absolute_root_path\":\"/work/app\"}}","is_error":false}]},"parent_tool_use_id":null,"session_id":"01a1754e-f2f9-7f80-acd2-a20947bfb786","uuid":"u-3"}"#,
            r#"{"type":"assistant","message":{"id":"msg_1","type":"message","role":"assistant","model":"grok-4.7","content":[{"type":"text","text":"The workspace is empty. I'll add a single-file Node server."},{"type":"tool_use","id":"call-a-1","name":"write","input":{"file_path":"/work/app/server.js","content":"const http = require('http');\n"}}],"stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":20,"output_tokens":20,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}},"parent_tool_use_id":null,"session_id":"01a1754e-f2f9-7f80-acd2-a20947bfb786","uuid":"u-4"}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-a-1","content":"[{\"type\":\"content\",\"content\":{\"type\":\"text\",\"text\":\"User cancelled the execution for tool `write`\"}}]","is_error":true}]},"parent_tool_use_id":null,"session_id":"01a1754e-f2f9-7f80-acd2-a20947bfb786","uuid":"u-5"}"#,
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"duration_ms":6739,"duration_api_ms":5862,"num_turns":2,"stop_reason":"cancelled","total_cost_usd":0.03,"usage":{"input_tokens":30,"output_tokens":30,"cache_read_input_tokens":0,"cache_creation_input_tokens":0},"modelUsage":{"grok-4.7-build":{"inputTokens":30,"outputTokens":30,"cacheReadInputTokens":0,"cacheCreationInputTokens":0,"webSearchRequests":0,"costUSD":0.03}},"errors":["cancelled"],"session_id":"01a1754e-f2f9-7f80-acd2-a20947bfb786","uuid":"u-6"}"#,
        ];
        let mut parser = ClaudeStream::new("/work/app");
        let out: Vec<AgentEvent> = lines.iter().flat_map(|l| parser.feed(l)).collect();
        let error = out
            .iter()
            .find_map(|e| match e {
                AgentEvent::Error(e) => Some(e.as_str()),
                _ => None,
            })
            .expect("the failed result emits an error");
        // The error names what cut the turn short — Grok's raw subtype no longer leaks.
        assert!(error.contains("The turn was interrupted while a tool was running"));
        assert!(error.contains("User cancelled the execution for tool `write`"));
        assert!(error.contains("cancelled"));
        assert!(!error.contains("error_during_execution"));
    }

    #[test]
    fn grok_errors_fall_back_to_the_subtype_when_nothing_else_is_known() {
        // A `result` event alone, with no tool failure above it and no `errors` list:
        // the mapped headline is still shown.
        let mut p = ClaudeStream::new("/x");
        let out = p.feed(
            r#"{"type":"result","subtype":"error_max_structured_output_retries","is_error":true}"#,
        );
        assert_eq!(
            out.into_iter().find_map(|e| match e {
                AgentEvent::Error(e) => Some(e),
                _ => None,
            }),
            Some("The model couldn't produce output matching the requested schema".into())
        );
        // An unknown subtype still passes through so the person sees something.
        let mut p = ClaudeStream::new("/x");
        let out = p.feed(r#"{"type":"result","subtype":"error_banana","is_error":true}"#);
        assert_eq!(
            out.into_iter().find_map(|e| match e {
                AgentEvent::Error(e) => Some(e),
                _ => None,
            }),
            Some("error_banana".into())
        );
    }

    #[test]
    fn grok_builds_partial_messages_stream_its_text_once() {
        // From a real Plan only run with `--include-partial-messages`, cut short: the
        // text arrives as deltas, then once more in the whole message, which is skipped.
        let s = r#","parent_tool_use_id":null,"session_id":"0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f","uuid":"u"}"#;
        let lines = [
            format!(
                r#"{{"type":"stream_event","event":{{"type":"message_start","message":{{"id":"msg_2","type":"message","role":"assistant","model":"grok-4.7","content":[],"stop_reason":null,"stop_sequence":null,"usage":{{"input_tokens":0,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}}}}{s}"#
            ),
            format!(
                r#"{{"type":"stream_event","event":{{"type":"content_block_start","index":0,"content_block":{{"type":"thinking","thinking":"","signature":""}}}}{s}"#
            ),
            format!(
                r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"thinking_delta","thinking":"Plan mode remains active."}}}}{s}"#
            ),
            format!(
                r#"{{"type":"stream_event","event":{{"type":"content_block_stop","index":0}}{s}"#
            ),
            format!(
                r#"{{"type":"stream_event","event":{{"type":"content_block_start","index":1,"content_block":{{"type":"text","text":""}}}}{s}"#
            ),
            format!(
                r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":1,"delta":{{"type":"text_delta","text":"note"}}}}{s}"#
            ),
            format!(
                r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":1,"delta":{{"type":"text_delta","text":".txt said: hello from runo."}}}}{s}"#
            ),
            format!(
                r#"{{"type":"stream_event","event":{{"type":"content_block_stop","index":1}}{s}"#
            ),
            format!(
                r#"{{"type":"stream_event","event":{{"type":"message_delta","delta":{{"stop_reason":"end_turn","stop_sequence":null}},"usage":{{"input_tokens":122,"output_tokens":207,"cache_read_input_tokens":18432,"cache_creation_input_tokens":0}}}}{s}"#
            ),
            format!(r#"{{"type":"stream_event","event":{{"type":"message_stop"}}{s}"#),
            format!(
                r#"{{"type":"assistant","message":{{"id":"msg_2","type":"message","role":"assistant","model":"grok-4.7","content":[{{"type":"thinking","thinking":"Plan mode remains active."}},{{"type":"text","text":"note.txt said: hello from runo."}}],"stop_reason":"end_turn","stop_sequence":null,"usage":{{"input_tokens":122,"output_tokens":207,"cache_read_input_tokens":18432,"cache_creation_input_tokens":0}}}}{s}"#
            ),
        ];
        let mut parser = ClaudeStream::new("/work/app");
        let text: String = lines
            .iter()
            .flat_map(|l| parser.feed(l))
            .filter_map(|e| match e {
                AgentEvent::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(text, "note.txt said: hello from runo.");
        // Plan mode's exit and its tool record read as Grok's own steps.
        assert_eq!(
            step_label("/w", "exit_plan_mode", &Value::Null),
            "Proposed a plan"
        );
        assert_eq!(
            step_label(
                "/w",
                "search_replace",
                &serde_json::json!({"file_path": "/w/plan.md"})
            ),
            "Edited plan.md"
        );
        assert_eq!(
            grok_tool_text(&Value::String(r#"[{"type":"content","content":{"type":"text","text":"Non-interactive plan-only run: plan mode stays on."}}]"#.into())).as_deref(),
            Some("Non-interactive plan-only run: plan mode stays on.")
        );
        assert_eq!(grok_tool_text(&Value::String("plain output".into())), None);
    }

    #[test]
    fn antigravity_streams_its_ndjson_events() {
        // The shape the CLI's docs define for `agy -p ... --output-format stream-json`:
        // one event per line, each `{"event":"init|step_update|result","<event>":{…}}`.
        // The reply text arrives on `agent_response` steps as `text_delta`s; a tool runs
        // from ACTIVE to DONE, with usage reported on each `step_update`.
        let lines = [
            r#"{"event":"init","init":{"cwd":"/work/app","tools":["read_file","run_terminal_command"],"permission_mode":"request-review","model":"gemini-3-pro"}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f","step_index":0,"state":"DONE","step_type":"user_input"}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f","step_index":1,"state":"ACTIVE","step_type":"tool","tool_name":"read_file","tool_input":{"path":"/work/app/note.txt"}}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f","step_index":1,"state":"DONE","step_type":"tool","tool_name":"read_file","tool_input":{"path":"/work/app/note.txt"},"tool_output":"hello from runo\n","tool_status":"SUCCESS"}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f","step_index":2,"state":"ACTIVE","step_type":"agent_response","text_delta":"note.txt said: "}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f","step_index":2,"state":"ACTIVE","step_type":"agent_response","text_delta":"hello from runo."}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f","step_index":2,"state":"DONE","step_type":"agent_response","usage":{"input_tokens":180,"output_tokens":42,"thinking_tokens":8,"cache_read_tokens":1024,"total_tokens":1254}}}"#,
            r#"{"event":"result","result":{"conversation_id":"0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f","status":"SUCCESS","response":"note.txt said: hello from runo.","duration_seconds":4.21,"num_turns":1,"usage":{"input_tokens":180,"output_tokens":42,"thinking_tokens":8,"cache_read_tokens":1024,"total_tokens":1254}}}"#,
        ];
        let mut parser = AntigravityStream::new("/work/app");
        let out: Vec<AgentEvent> = lines.iter().flat_map(|l| parser.feed(l)).collect();
        // The first event with a conversation id names the conversation, so follow-ups
        // resume exactly it.
        assert!(matches!(
            out.first(),
            Some(AgentEvent::Conversation(id)) if id == "0b5c2f9e-1d2a-4c3b-9e8f-7a6b5c4d3e2f"
        ));
        let step = out
            .iter()
            .find_map(|e| match e {
                AgentEvent::Tool(s) => Some(s),
                _ => None,
            })
            .unwrap();
        assert_eq!(step.label, "Read note.txt");
        assert_eq!(step.id, "agy-1");
        let outputs: Vec<&str> = out
            .iter()
            .filter_map(|e| match e {
                AgentEvent::ToolDone {
                    ok: true, output, ..
                } => Some(output.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(outputs, ["hello from runo"]);
        // Deltas stream in without the result's full copy repeating them.
        let text: String = out
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text, "note.txt said: hello from runo.");
        let usage = out
            .iter()
            .rev()
            .find_map(|e| match e {
                AgentEvent::Usage(u) => Some(*u),
                _ => None,
            })
            .unwrap();
        // Thinking tokens fold into the output count, so the token limit sees them.
        assert_eq!(usage.input, 180);
        assert_eq!(usage.output, 50);
        assert_eq!(usage.cached, 1024);
        assert_eq!(usage.cost_cents, 0.0);
        assert!(out
            .iter()
            .any(|e| matches!(e, AgentEvent::Summary(s) if s.starts_with("4s · 1 turn"))));
        assert!(!out.iter().any(|e| matches!(e, AgentEvent::Error(_))));

        // An ERROR status becomes an Error event with the message the CLI reported.
        let mut err = AntigravityStream::new("/work/app");
        let out: Vec<AgentEvent> = err
            .feed(r#"{"event":"result","result":{"conversation_id":"c","status":"ERROR","error":"Model unavailable","duration_seconds":1.0,"num_turns":0}}"#);
        assert!(out
            .iter()
            .any(|e| matches!(e, AgentEvent::Error(m) if m == "Model unavailable")));
    }

    #[test]
    fn tool_steps_run_until_their_result_arrives() {
        let out = feed_all(&[
            r#"{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"tool_use","id":"toolu_a","name":"Bash","input":{"command":"sleep 60"}},{"type":"tool_use","id":"toolu_b","name":"Bash","input":{"command":"false"}}]}}"#,
            r#"{"type":"user","parent_tool_use_id":"toolu_x","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_inner","content":"..."}]}}"#,
            r#"{"type":"user","parent_tool_use_id":null,"message":{"content":[{"type":"tool_result","tool_use_id":"toolu_b","is_error":true,"content":"exit 1"}]}}"#,
            r#"{"type":"user","parent_tool_use_id":null,"message":{"content":[{"type":"tool_result","tool_use_id":"toolu_a","content":""}]}}"#,
        ]);
        assert_eq!(
            out,
            [
                started("toolu_a", "Ran sleep 60"),
                started("toolu_b", "Ran false"),
                finished("toolu_b", false),
                finished("toolu_a", true),
            ]
        );
    }

    #[test]
    fn streamed_text_is_not_repeated() {
        let out = feed_all(&[
            r#"{"type":"stream_event","event":{"type":"message_start"}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"text"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hel"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"lo"}}}"#,
            r#"{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"text","text":"Hello"}]}}"#,
            r#"{"type":"stream_event","event":{"type":"message_start"}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"text"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Bye"}}}"#,
        ]);
        assert_eq!(
            out,
            [
                AgentEvent::Text("Hel".into()),
                AgentEvent::Text("lo".into()),
                AgentEvent::Text("\n\n".into()),
                AgentEvent::Text("Bye".into()),
            ]
        );
    }

    #[test]
    fn subagent_messages_are_skipped_and_errors_reported() {
        let out = feed_all(&[
            r#"{"type":"assistant","parent_tool_use_id":"toolu_1","message":{"content":[{"type":"text","text":"inner"}]}}"#,
            r#"{"type":"result","subtype":"error_max_turns","is_error":true,"duration_ms":900,"num_turns":1}"#,
        ]);
        assert_eq!(
            out,
            [
                AgentEvent::Summary("900ms · 1 turn".into()),
                AgentEvent::Error("The turn hit the maximum turn limit".into())
            ]
        );
    }

    #[test]
    fn subagent_steps_show_on_their_parent_step() {
        let out = feed_all(&[
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Task","input":{"description":"Find the parser"}}]}}"#,
            r#"{"type":"assistant","parent_tool_use_id":"toolu_1","message":{"content":[{"type":"text","text":"Looking"},{"type":"tool_use","id":"toolu_s1","name":"Read","input":{"file_path":"/work/app/src/stream.rs"}}]}}"#,
            r#"{"type":"user","parent_tool_use_id":"toolu_1","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_s1","content":"…"}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"Found it"}]}}"#,
        ]);
        assert_eq!(
            out,
            [
                started("toolu_1", "Ran a subagent: Find the parser"),
                AgentEvent::SubStep {
                    parent: "toolu_1".into(),
                    label: "Read src/stream.rs".into()
                },
                AgentEvent::ToolDone {
                    id: "toolu_1".into(),
                    ok: true,
                    output: String::new()
                },
            ]
        );
    }

    #[test]
    fn background_subagents_run_until_their_task_ends() {
        let out = feed_all(&[
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_b","name":"Agent","input":{"description":"Review","run_in_background":true}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_b","content":"Async agent launched"}]}}"#,
            r#"{"type":"system","subtype":"task_started","task_id":"t9","tool_use_id":"toolu_b","description":"Review"}"#,
            r#"{"type":"system","subtype":"task_progress","task_id":"t9","last_tool_name":"Grep"}"#,
            r#"{"type":"system","subtype":"task_notification","task_id":"t9","status":"failed","summary":"Stopped"}"#,
        ]);
        assert_eq!(
            &out[1..],
            [
                AgentEvent::SubStep {
                    parent: "toolu_b".into(),
                    label: "Used Grep".into()
                },
                AgentEvent::ToolDone {
                    id: "toolu_b".into(),
                    ok: false,
                    output: String::new()
                },
            ],
            "the launch result doesn't end the step"
        );
        // One that couldn't start ends at once.
        let out = feed_all(&[
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_c","name":"Agent","input":{"description":"x","run_in_background":true}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_c","is_error":true,"content":"No"}]}}"#,
        ]);
        assert!(matches!(&out[1], AgentEvent::ToolDone { ok: false, .. }));
    }

    #[test]
    fn plain_lines_pass_through_and_result_fills_silent_runs() {
        let out = feed_all(&[
            "not json",
            "",
            r#"{"type":"result","is_error":false,"result":"Only the final answer","duration_ms":61000}"#,
        ]);
        assert_eq!(
            out,
            [
                AgentEvent::Text("not json".into()),
                AgentEvent::Summary("1m 1s".into()),
            ],
            "text already shown, so the result is not repeated"
        );
        let out =
            feed_all(&[r#"{"type":"result","is_error":false,"result":"Only the final answer"}"#]);
        assert_eq!(out, [AgentEvent::Text("Only the final answer".into())]);
    }

    #[test]
    fn codex_commands_lose_their_shell_wrapper() {
        assert_eq!(shell_body("bash -lc 'cargo test'"), "cargo test");
        assert_eq!(shell_body("/bin/zsh -lc ls"), "ls");
        assert_eq!(
            shell_body("/bin/zsh -lc 'ls /nonexistent'"),
            "ls /nonexistent"
        );
        assert_eq!(shell_body("/bin/sh -c \"make\""), "make");
        assert_eq!(shell_body("python -c 'x'"), "python -c 'x'");
        assert_eq!(shell_body("ls"), "ls");
    }

    #[test]
    fn claude_format_reports_the_conversation_once() {
        let out = feed_all(&[
            r#"{"type":"system","subtype":"init","session_id":"T-1"}"#,
            r#"{"type":"result","is_error":false,"result":"ok","session_id":"T-1"}"#,
        ]);
        assert_eq!(out[0], AgentEvent::Conversation("T-1".into()));
        assert_eq!(
            out.iter()
                .filter(|u| matches!(u, AgentEvent::Conversation(_)))
                .count(),
            1
        );
    }

    #[test]
    fn step_labels_for_common_tools() {
        let p = ClaudeStream::new("/w");
        let label =
            |tool: &str, input: &str| p.step_label(tool, &serde_json::from_str(input).unwrap());
        assert_eq!(label("Write", r#"{"file_path":"/w/a.txt"}"#), "Wrote a.txt");
        assert_eq!(
            label("Grep", r#"{"pattern":"fn main"}"#),
            "Searched for fn main"
        );
        assert_eq!(label("TodoWrite", "{}"), "Updated the plan");
        assert_eq!(
            label("Task", r#"{"description":"explore"}"#),
            "Ran a subagent: explore"
        );
        assert_eq!(label("mcp__x__y", "{}"), "Used mcp__x__y");
    }
}
