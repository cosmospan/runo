//! Persistent projects and sessions, stored as JSON under `~/.squadcode` (see `persist`).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// Saved enums take a value they don't know, as a newer Squad Code may have written it, as
// their plainest variant (`serde(other)`), so going back to an older build keeps the
// session instead of failing to read it.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    User,
    Agent,
    #[serde(other)]
    System,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Status {
    Working,
    Done,
    Failed,
    /// The person stopped the turn on purpose: not an error.
    Stopped,
    /// Also any status saved by a newer Squad Code that this one doesn't know. Serde needs
    /// the `other` variant last.
    #[default]
    #[serde(other)]
    Idle,
}

impl Status {}

pub use crate::harness::ToolStep;

#[derive(Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub text: String,
    /// Agent turns: the tool calls shown above the reply, such as "Read src/main.rs",
    /// each with its kind and how it stands. Sessions saved when steps were plain labels
    /// load those as finished steps.
    #[serde(default, deserialize_with = "steps_or_labels")]
    pub steps: Vec<ToolStep>,
    #[serde(default)]
    pub time: String,
    /// Run summary shown under the reply, such as "12s · 3 turns · $0.04".
    #[serde(default)]
    pub meta: String,
    /// User turns: a snapshot commit of the worktree taken just before the agent ran,
    /// so the turn can be undone.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub checkpoint: String,
    /// Agent turns: the files the turn changed, found at its end by comparing the folder
    /// with the checkpoint taken before it ran. Its files changed card lists only these.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<String>,
    /// Agent turns: "kept" or "undone" once the person decided on the turn's changes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resolved: String,
    /// Agent turns that were undone: a snapshot from just before the undo, for Redo.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub redo: String,
    /// User turns: images attached to the prompt (absolute paths).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// Agent turns: token use reported by the CLI.
    #[serde(default, skip_serializing_if = "Usage::is_empty")]
    pub usage: Usage,
    /// Agent turns: tools the agent asked for but was not allowed to use.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied: Vec<String>,
    /// Agent turns: a plan the agent proposed (plan mode), shown as a card.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub plan: String,
    /// Agent turns: questions the agent asked, each with its options.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<Question>,
    /// User turns: the playbook this message ran. `text` holds the full prompt sent;
    /// the conversation shows the playbook's name instead.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub playbook: String,
    /// User turns run from a playbook: what the person typed with it, if anything.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// In a Crew chat: a card drawn with the message, such as the threads it started.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "or_none"
    )]
    pub crew: Option<crate::crew::Card>,
    /// User turns in a Crew chat: the thread whose message woke Crew, drawn as a chip
    /// naming the thread instead of as something the person typed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_thread: Option<u64>,
}

/// Token use of one agent turn.
#[derive(Clone, Copy, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    /// Tokens of the prompt read from or written to the cache; they fill the context too.
    pub cached: u64,
    /// Total cost in US cents, when the CLI reports it.
    pub cost_cents: f64,
    /// The model's context window, when the CLI reports it.
    #[serde(default)]
    pub window: u64,
}

impl Usage {
    pub fn is_empty(&self) -> bool {
        self.input == 0 && self.output == 0 && self.cached == 0
    }
}

/// A multiple-choice question the agent asked (Claude's AskUserQuestion tool).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub question: String,
    pub options: Vec<String>,
}

impl Session {}

impl Session {}

/// An optional part of a session, or none when it can't be read (a newer Squad Code saved it
/// in a shape this one doesn't know), so the rest of the session still loads.
fn or_none<'de, D: serde::Deserializer<'de>, T: serde::de::DeserializeOwned>(
    d: D,
) -> Result<Option<T>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(value).ok())
}

/// Steps as saved now, or as the plain labels sessions were saved with before.
fn steps_or_labels<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<ToolStep>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Saved {
        Label(String),
        Step(ToolStep),
    }
    Ok(Vec::<Saved>::deserialize(d)?
        .into_iter()
        .map(|s| match s {
            Saved::Label(label) => ToolStep::done(label),
            Saved::Step(step) => step,
        })
        .collect())
}

impl Message {
    pub fn new(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            text: text.into(),
            steps: Vec::new(),
            time: String::new(),
            meta: String::new(),
            checkpoint: String::new(),
            changed: Vec::new(),
            resolved: String::new(),
            redo: String::new(),
            images: Vec::new(),
            usage: Usage::default(),
            denied: Vec::new(),
            plan: String::new(),
            questions: Vec::new(),
            playbook: String::new(),
            note: String::new(),
            crew: None,
            from_thread: None,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Project {
    pub id: u64,
    pub name: String,
    pub path: String,
    /// Accent color key from the New project popup ("" for none).
    #[serde(default)]
    pub color: String,
    /// Icon key from the New project popup ("" for the plain folder).
    #[serde(default)]
    pub icon: String,
    /// Where new sessions in this project run by default: a connection id, or 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub connection: u64,
    /// Command run in each new worktree of this project before its agent starts; empty
    /// uses the one in Settings.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub setup: String,
    /// How pull requests are merged on GitHub: "merge", "squash" or "rebase"; empty
    /// picks one the repository allows.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub merge_method: String,
    /// Commands run after each turn that changed files, one per line (tests, lint,
    /// typecheck, build). Empty runs none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub checks: String,
    /// Which checks run: `proof::LEVEL_*`, where empty follows Settings. `None` only for
    /// a project saved before levels, until `State::load` migrates it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_level: Option<String>,
    /// How many times in a row failing checks go back to the agent on their own; 0 asks.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub fix_rounds: u32,
    /// What the person last let the checks run (`proof::fingerprint`), for sessions whose
    /// mode asks before shell commands: they run on their own while it stays the same.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub checks_ok: String,
    /// How the person works here: `crew` through Crew's chat, empty directly in
    /// sessions. See `crew::is_crew`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mode: String,
    /// Let agent CLIs use settings committed in the repository that start programs
    /// (Cursor and Gemini MCP servers, hooks, tool commands): Squad Code trusts the folder
    /// for them and approves the servers up front. Off, Squad Code trusts a folder only when
    /// it holds no such settings.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trust_agent_config: bool,
    /// How Squad Code watches CI for this project: `provider::CiTracking::parse` reads it.
    /// Empty and "auto" use the three-signal detection; "off"/"none" turns the watcher
    /// off; "github_actions"/"gitlab_pipelines"/"bitbucket_pipelines" pin a kind;
    /// "custom:<url>" links a status page.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ci_tracking: String,
}

fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Session {
    pub id: u64,
    pub project_id: u64,
    pub title: String,
    pub provider: String,
    pub branch: String,
    /// Directory the agent and terminal run in (a worktree, or the project itself).
    pub dir: String,
    pub isolated: bool,
    /// Per-session choices for the agent's discovered options (model, effort, mode, tier…).
    #[serde(default)]
    pub opts: HashMap<String, String>,
    pub status: Status,
    pub messages: Vec<Message>,
    /// The agent CLI's own conversation id (Claude Code `--session-id`), so follow-ups
    /// resume this session's conversation and not whichever ran last in the folder.
    #[serde(default)]
    pub agent_session: String,
    /// Index of the first message of the current agent conversation. Switching agents
    /// starts a new conversation, so earlier replies must not trigger a resume.
    #[serde(default)]
    pub conversation_start: usize,
    /// Pinned sessions sort to the top of their project.
    #[serde(default)]
    pub pinned: bool,
    /// Archived sessions leave the sidebar but stay under All sessions.
    #[serde(default)]
    pub archived: bool,
    /// The agent finished while this session was not on screen.
    #[serde(default)]
    pub unread: bool,
    /// Review notes left on diff lines, waiting to be sent to the agent.
    #[serde(default)]
    pub notes: Vec<Note>,
    /// Sessions started together on one task share the id of the first one; 0 if none.
    #[serde(default)]
    pub race: u64,
    /// Prompts typed while the agent was working; each is sent when the turn ends.
    #[serde(default)]
    pub queue: Vec<Queued>,
    /// A handoff brief from the previous agent, sent with the next prompt.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub handoff: String,
    /// The last handoff, kept for agents that cannot resume their own conversation.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub carried: String,
    /// Shown in the CLI view: the agent's own CLI on the same conversation.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cli: bool,
    /// The agent this session just switched away from, kept until the next message so
    /// switching straight back resumes its own conversation instead of starting over.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switched_from: Option<SwitchedFrom>,
    /// Where the session runs: a connection's id, or 0 for this machine.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub connection: u64,
    /// The running turn is on a connection, where it keeps going while Squad Code is closed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub remote_turn: bool,
    /// The branch this session's branch started from and lands in, such as `main`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base: String,
    /// The pull request opened for this session's branch.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pr_url: String,
    /// The canvas the session was asked from, `design` or `architecture`: it opens there
    /// and stays out of the session lists, with its chat a click away on that canvas.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub canvas: String,
    /// The canvas context last sent with an ask, so the next ask in the same conversation
    /// sends only what changed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub canvas_context: String,
    /// The newest snapshot of the folder Squad Code took (end of a turn, an undo or a redo),
    /// so Undo can leave alone what changed after it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub snapshot: String,
    /// The branch is a pull request from someone else's fork: its scripts are theirs, so
    /// Squad Code never runs its checks on its own.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fork: bool,
    /// Told to the agent with the next prompt, such as that its last turn was undone.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub undo_note: String,
    /// Connections it ran on before, whose copies are deleted along with it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub left_on: Vec<u64>,
    /// The playbook that started it, or 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub playbook: u64,
    /// What that playbook declared it may do (`read`, `edit`, `shell`, `network`); every
    /// turn of the session keeps to it. Empty means no limits.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limits: Vec<String>,
    /// Proof of done for the last turn: its checks and the second opinion on it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "or_none"
    )]
    pub proof: Option<crate::proof::Proof>,
    /// What Crew keeps for this session when it is one of Crew's threads.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "or_none"
    )]
    pub crew: Option<crate::crew::ThreadInfo>,
}

/// A prompt waiting for the agent, with the images attached to it.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct Queued {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// A thread's message to Crew waiting for Crew's turn: the thread that sent it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_thread: Option<u64>,
}

impl Queued {}

/// What a switch replaced, enough to undo it before anything is sent.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct SwitchedFrom {
    pub provider: String,
    pub agent_session: String,
    pub opts: HashMap<String, String>,
    pub conversation_start: usize,
    pub carried: String,
}

impl Session {}

/// A review comment on one line of the session's diff.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Note {
    pub path: String,
    pub line: usize,
    /// The diff line as shown, with its +/− marker, so the agent sees what was meant.
    pub code: String,
    pub text: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub next_id: u64,
    pub projects: Vec<Project>,
    pub sessions: Vec<Session>,
    /// Free-form notes per project id.
    #[serde(default)]
    pub notes: HashMap<u64, String>,
    /// Pinning, sharing with agents and last edit of each project's notes.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub notes_meta: HashMap<u64, NoteMeta>,
    /// Recent events across sessions, newest last.
    #[serde(default)]
    pub activity: Vec<Activity>,
    #[serde(default)]
    pub automations: Vec<Automation>,
    /// Open tabs and panels, restored on the next launch.
    #[serde(default)]
    pub layout: Layout,
    /// Cost reported by the agents per day ("%Y-%m-%d"), in US cents; the last 31 days.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub spend: std::collections::BTreeMap<String, f64>,
    /// The same cost split by agent ("claude", "opencode"), for the Usage popover; the
    /// last 31 days. Days from before it was kept have only the total in `spend`.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub agent_spend: std::collections::BTreeMap<String, std::collections::BTreeMap<String, f64>>,
    /// Turns started and tokens reported per day ("%Y-%m-%d"), for the turn and token
    /// limits; the last 31 days, like `spend`.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub day_use: std::collections::BTreeMap<String, DayUse>,
}

/// What agents did on one day, across every session.
#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct DayUse {
    /// Turns started.
    #[serde(default)]
    pub turns: u32,
    /// Input and output tokens the agents reported, cached ones not counted.
    #[serde(default)]
    pub tokens: u64,
}

/// How a project's notes are kept and used.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct NoteMeta {
    /// Listed first on the Project notes page.
    #[serde(default)]
    pub pinned: bool,
    /// Sent to the agent at the start of each new conversation in the project.
    #[serde(default)]
    pub shared: bool,
    /// Unix seconds of the last edit.
    #[serde(default)]
    pub updated: u64,
}

/// Something that happened in a session, for the Activity feed.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Activity {
    /// Local time, "%Y-%m-%d %H:%M".
    pub time: String,
    pub session: u64,
    pub text: String,
}

/// A playbook: a saved task (instructions and agent) that runs in any project on demand,
/// and optionally on a schedule in one project.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug, Default)]
pub struct Automation {
    pub id: u64,
    pub name: String,
    /// The project it always runs in, or 0 to run in whichever project it is started from.
    pub project_id: u64,
    pub provider: String,
    pub prompt: String,
    /// One line on what it does, for menus and agents; empty uses the prompt's first sentence.
    #[serde(default)]
    pub description: String,
    /// Minutes between scheduled runs; 0 runs it only when started by hand.
    pub every_minutes: u64,
    /// A cron expression it runs on instead, in local time; a time of day on chosen
    /// weekdays is stored this way too. Empty uses `every_minutes`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cron: String,
    /// Unix seconds of the last run; 0 if it never ran.
    #[serde(default)]
    pub last_run: u64,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Unix seconds it last actually ran. `last_run` also moves when a
    /// schedule starts counting, so it can't say whether it ever ran.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ran: u64,
    /// Who wrote it and which version, for playbooks installed from a file.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// Where it was installed from (a link, repository or file) and the file inside it,
    /// so it can be updated; empty for playbooks made in Squad Code.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin: String,
    /// What it may do (`read`, `edit`, `shell`, `network`); empty means not declared,
    /// which runs like any session.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,
    /// Labels and defaults for its `{{key}}` placeholders.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<crate::pbfile::Input>,
    /// Its latest runs, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<PlaybookRun>,
    /// A fingerprint of the playbook as it was installed, to tell edits made here from
    /// changes at the source.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub installed: String,
}

/// One run of a playbook, for its history.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug, Default)]
pub struct PlaybookRun {
    /// Unix seconds it started, or was due.
    pub at: u64,
    /// The session it ran in, or 0 when it didn't start.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub session: u64,
    /// "running", "done", "failed", "skipped" or "not started".
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

fn yes() -> bool {
    true
}

impl Automation {}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct Layout {
    pub tabs: Vec<u64>,
    pub selected: Option<u64>,
    #[serde(default)]
    pub hide_sidebar: bool,
    #[serde(default)]
    pub hide_panel: bool,
    /// The mode on the rail, by its label ("Home", "Sessions", ...).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mode: String,
    /// The right panel's tab: "changes", "browser" or "files".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub right_tab: String,
    /// Projects open in the sidebar; `None` (files from before) opens them all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expanded: Option<Vec<u64>>,
    /// The project in the top bar's switcher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<u64>,
    /// Sidebar width in pixels; 0 is the default.
    #[serde(default)]
    pub sidebar_width: f32,
    /// Right panel width in pixels; 0 is the default.
    #[serde(default)]
    pub panel_width: f32,
    /// Terminal panel height in pixels; 0 is the default.
    #[serde(default)]
    pub term_height: f32,
    /// Home lists every project instead of the one in the top bar.
    #[serde(default)]
    pub home_all: bool,
    /// Where the window was and how big.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowPlace>,
}

/// The window's last position and size, in logical pixels.
#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct WindowPlace {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub maximized: bool,
}

/// Today's date as the spend log keys it.
pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

impl State {}

/// Read a JSON file, or the default when it is missing. A file that exists but cannot be
/// parsed is moved aside first, so the next save cannot silently overwrite the user's data,
/// and whatever of it can still be read is kept (see `salvage`).
pub fn read_json_or_backup<T: Default + Serialize + serde::de::DeserializeOwned>(path: &Path) -> T {
    let Ok(text) = std::fs::read_to_string(path) else {
        return T::default();
    };
    match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => {
            move_aside(path, &error.to_string());
            serde_json::from_str(&text)
                .ok()
                .and_then(salvage)
                .unwrap_or_default()
        }
    }
}

/// The parts of a JSON object that can still be read as a `T`, the rest left at their
/// defaults: one field saved in a shape this Squad Code doesn't know (by a newer one, or a
/// hand edit) then costs that field, not the whole file. `None` when `value` isn't an
/// object.
pub fn salvage<T: Default + Serialize + serde::de::DeserializeOwned>(
    value: serde_json::Value,
) -> Option<T> {
    let serde_json::Value::Object(fields) = value else {
        return None;
    };
    let serde_json::Value::Object(mut kept) = serde_json::to_value(T::default()).ok()? else {
        return None;
    };
    for (key, field) in fields {
        let before = kept.insert(key.clone(), field);
        if serde_json::from_value::<T>(serde_json::Value::Object(kept.clone())).is_err() {
            match before {
                Some(old) => kept.insert(key, old),
                None => kept.remove(&key),
            };
        }
    }
    serde_json::from_value(serde_json::Value::Object(kept)).ok()
}

/// Move a file Squad Code can't read out of the way, keeping it next to where it was.
pub fn move_aside(path: &Path, error: &str) {
    set_aside(path, error, true);
}

/// Copy a file Squad Code could read only in part next to where it was, before it is saved over.
pub fn keep_aside(path: &Path, error: &str) {
    set_aside(path, error, false);
}

fn set_aside(path: &Path, error: &str, moved: bool) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup = path.with_extension(format!("json.corrupt-{stamp}"));
    let done = if moved {
        std::fs::rename(path, &backup).is_ok()
    } else {
        std::fs::copy(path, &backup).is_ok()
    };
    crate::applog::error(
        "load",
        format!(
            "could not read all of {} ({error}); {} it to {}",
            path.display(),
            match (moved, done) {
                (true, true) => "moved",
                (false, true) => "copied",
                _ => "could not keep",
            },
            backup.display()
        ),
    );
}

/// Write JSON atomically and privately (see `write_private`).
pub fn write_json<T: Serialize>(path: &Path, value: &T) {
    let written = serde_json::to_string_pretty(value)
        .map_err(|e| e.to_string())
        .and_then(|text| write_private(path, text.as_bytes()).map_err(|e| e.to_string()));
    if let Err(e) = written {
        crate::applog::error_once("save", format!("could not save {}: {e}", path.display()));
    }
}

/// Write `data` to `path` so only the person can read it, without anyone seeing it half
/// written: into a new temporary file created 0600 (`create_new`, so a link planted there
/// fails rather than being followed), then renamed over the target, which replaces a link
/// there instead of writing through it. A missing folder is made private (0700).
pub fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        create_private_dir(parent)?;
    }
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = path.with_file_name(format!(".{name}.{}-{n}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // Flushed to disk before the rename, so a crash or power cut right after
    // leaves the old file or the new one, never an empty one.
    let written = options
        .open(&tmp)
        .and_then(|mut file| file.write_all(data).and_then(|_| file.sync_all()));
    match written.and_then(|_| std::fs::rename(&tmp, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Make `dir` and any missing parents; the ones made here are private (0700).
pub fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Make Squad Code's data folder private: 0700, and its files and request folders readable
/// only by the person. Installs from before this kept it 0755 with 0644 files, which
/// left settings (API keys, env lines), transcripts and approvals open to every
/// account on the computer. On Windows the folder sits in the person's profile, which
/// is theirs alone already.
pub fn tighten_data_dir() {
    let dir = data_dir();
    let _ = create_private_dir(&dir);
    #[cfg(unix)]
    tighten(&dir, 1);
}

/// Folders 0700 and files 0600, `depth` levels down. A file that runs (the `squad`
/// command in `bin`) stays runnable, by the person only: 0700. Links are left alone:
/// they may point anywhere.
#[cfg(unix)]
fn tighten(dir: &Path, depth: u32) {
    use std::os::unix::fs::PermissionsExt;
    let set = |path: &Path, mode: u32| {
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            let now = meta.permissions().mode();
            if !meta.file_type().is_symlink() && now & 0o077 != 0 {
                let mode = mode | (now & 0o100);
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
            }
        }
    };
    set(dir, 0o700);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_file() {
            set(&entry.path(), 0o600);
        } else if kind.is_dir() && depth > 0 {
            tighten(&entry.path(), depth - 1);
        }
    }
}

/// When this Squad Code is not the one that last ran on `root` (an update, or going back to an
/// older build), keep a copy of the files it is about to rewrite: `state.json.prev` and
/// `settings.json.prev`. When the last one was newer, every session file is copied too, to
/// `backups/<version>-<time>/`, since an older build drops what it doesn't know as it saves.
/// The version that ran is kept in `version`.
pub fn keep_before_version_change(root: &Path, current: &str) {
    let marker = root.join("version");
    let last = std::fs::read_to_string(&marker).unwrap_or_default();
    let last = last.trim();
    if last == current {
        return;
    }
    for name in ["state.json", "settings.json"] {
        let from = root.join(name);
        if from.is_file() {
            let _ = std::fs::copy(&from, root.join(format!("{name}.prev")));
        }
    }
    if crate::alerts::is_newer(last, current) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup = root.join("backups").join(format!("{last}-{stamp}"));
        let copied = copy_saved_files(root, &backup);
        crate::applog::warn(
            "load",
            format!(
                "Squad Code {last} last used this data and this is {current}; copied {copied} files \
                 to {} before saving over them",
                backup.display()
            ),
        );
    }
    if std::fs::create_dir_all(root).is_ok() {
        let _ = std::fs::write(&marker, current);
    }
}

/// Copy `state.json`, `settings.json` and every `sessions/<id>/session.json` under `root`
/// to the same places under `to`. Returns how many files were copied.
fn copy_saved_files(root: &Path, to: &Path) -> usize {
    let mut files = vec![PathBuf::from("state.json"), PathBuf::from("settings.json")];
    if let Ok(entries) = std::fs::read_dir(root.join("sessions")) {
        files.extend(entries.flatten().map(|e| {
            Path::new("sessions")
                .join(e.file_name())
                .join("session.json")
        }));
    }
    files
        .into_iter()
        .filter(|file| root.join(file).is_file())
        .filter(|file| {
            let target = to.join(file);
            target
                .parent()
                .is_some_and(|dir| create_private_dir(dir).is_ok())
                && std::fs::copy(root.join(file), target).is_ok()
        })
        .count()
}

/// The person's home folder: `$HOME`, else `%USERPROFILE%` (Windows sets only that).
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|h| !h.is_empty()))
        .map(PathBuf::from)
}

/// Squad Code's data folder: `$SQUADCODE_HOME`, else `~/.squadcode`.
pub fn data_dir() -> PathBuf {
    std::env::var_os("SQUADCODE_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".squadcode")))
        .unwrap_or_else(|| PathBuf::from(".squadcode"))
}

/// The first time Squad Code opens, it takes a copy of what Runo had saved (its
/// settings and v2 jobs, the files at the top of ~/.runo), so nothing has to be set up
/// again. Runo's own folder is left as it is.
pub fn adopt_runo_data() {
    if std::env::var_os("SQUADCODE_HOME").is_some() {
        return;
    }
    let Some(home) = home_dir() else {
        return;
    };
    adopt_from(&home.join(".runo"), &home.join(".squadcode"));
}

fn adopt_from(old: &Path, new: &Path) {
    if new.exists() || !old.is_dir() {
        return;
    }
    if std::fs::create_dir_all(new).is_err() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(old) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_json = path.extension().is_some_and(|e| e == "json");
        if is_json && entry.file_type().is_ok_and(|t| t.is_file()) {
            let _ = std::fs::copy(&path, new.join(entry.file_name()));
        }
    }
}

/// Give a project saved before check levels its level: checks Squad Code suggested follow the
/// Settings level from now on, and commands the person typed stay as they are.
fn migrate_check_level(project: &mut Project) {
    if project.check_level.is_some() {
        return;
    }
    let suggested = if crate::proof::commands(&project.checks).is_empty() {
        Vec::new()
    } else {
        crate::proof::detect(Path::new(&project.path))
    };
    let (level, keep) = crate::proof::migrate(&project.checks, &suggested);
    project.check_level = Some(level.into());
    if !keep {
        project.checks.clear();
    }
}

fn state_path() -> PathBuf {
    data_dir().join("state.json")
}

impl State {
    pub fn load() -> Self {
        Self::load_from(&state_path())
    }

    /// Load the state whose index is at `path`, with the session files next to it.
    pub fn load_from(path: &Path) -> Self {
        let root = path.parent().unwrap_or(Path::new("."));
        let mut state = match crate::persist::read(path) {
            Ok((state, index_error)) => {
                if let Some(error) = index_error {
                    keep_aside(path, &error);
                }
                state
            }
            Err(error) => {
                move_aside(path, &error);
                State::default()
            }
        };
        // A turn on a connection keeps going there, and a local turn runs detached from
        // Squad Code; both are picked up again on start. Anything else could not survive.
        for session in &mut state.sessions {
            if session.status == Status::Working
                && !session.remote_turn
                && crate::detach::left_behind(root, session.id).is_none()
            {
                session.status = Status::Failed;
                session.messages.push(Message::new(
                    Role::System,
                    "Interrupted: Squad Code closed while this run was active.",
                ));
            }
        }
        for project in &mut state.projects {
            migrate_check_level(project);
        }
        state
    }
}

#[cfg(test)]
impl State {}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("squadcode-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    #[test]
    fn an_old_open_data_folder_is_made_private() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let open = |p: &Path, m| std::fs::set_permissions(p, std::fs::Permissions::from_mode(m));
        let dir = temp("tighten");
        std::fs::write(dir.join("settings.json"), "{}").unwrap();
        std::fs::create_dir_all(dir.join("approvals")).unwrap();
        std::fs::write(dir.join("approvals").join("1-1.json"), "{}").unwrap();
        open(&dir, 0o755).unwrap();
        open(&dir.join("settings.json"), 0o644).unwrap();
        open(&dir.join("approvals"), 0o755).unwrap();
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin").join("squad"), "#!/bin/sh\n").unwrap();
        open(&dir.join("bin").join("squad"), 0o755).unwrap();
        tighten(&dir, 1);
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("settings.json")), 0o600);
        assert_eq!(mode(&dir.join("approvals")), 0o700);
        assert_eq!(mode(&dir.join("approvals").join("1-1.json")), 0o600);
        // The terminal's `squad` command still runs, for the person alone.
        assert_eq!(mode(&dir.join("bin").join("squad")), 0o700);
        // New saves are private from the start.
        write_json(&dir.join("state.json"), &serde_json::json!({ "a": 1 }));
        assert_eq!(mode(&dir.join("state.json")), 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_state_is_moved_aside_not_overwritten() {
        let dir = temp("corrupt");
        let path = dir.join("state.json");
        std::fs::write(&path, "{ not json").unwrap();
        let loaded = State::load_from(&path);
        assert!(loaded.projects.is_empty());
        assert!(!path.exists());
        let backups: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("state.json.corrupt-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            std::fs::read_to_string(backups[0].path()).unwrap(),
            "{ not json"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_state_is_empty() {
        let dir = temp("missing");
        let loaded = State::load_from(&dir.join("nope.json"));
        assert!(loaded.projects.is_empty() && loaded.sessions.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn salvage_keeps_what_reads() {
        #[derive(Default, Serialize, Deserialize, PartialEq, Debug)]
        struct Saved {
            name: String,
            count: u32,
            #[serde(default)]
            on: bool,
        }
        let value = serde_json::json!({ "name": "kept", "count": "many", "on": true, "new": 1 });
        let saved: Saved = salvage(value).unwrap();
        assert_eq!(
            saved,
            Saved {
                name: "kept".into(),
                count: 0,
                on: true
            }
        );
        assert!(salvage::<Saved>(serde_json::json!([1])).is_none());
    }

    #[test]
    fn old_layouts_still_load() {
        let layout: Layout = serde_json::from_str(r#"{"tabs":[3],"selected":3}"#).unwrap();
        assert_eq!(layout.tabs, vec![3]);
        assert_eq!(layout.expanded, None);
        assert!(layout.mode.is_empty() && layout.window.is_none());
        let full = Layout {
            mode: "Home".into(),
            expanded: Some(vec![1, 2]),
            window: Some(WindowPlace {
                x: 10.,
                y: 20.,
                width: 1200.,
                height: 800.,
                maximized: false,
            }),
            ..Layout::default()
        };
        let back: Layout = serde_json::from_str(&serde_json::to_string(&full).unwrap()).unwrap();
        assert_eq!(back, full);
    }

    /// Held by any test that changes process-wide environment variables,
    /// since cargo runs tests on parallel threads.
    pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Puts the named variables back as they were when dropped.
    struct RestoreEnv(Vec<(&'static str, Option<std::ffi::OsString>)>);

    impl Drop for RestoreEnv {
        fn drop(&mut self) {
            for (key, old) in &self.0 {
                match old {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    #[test]
    fn data_dir_uses_squadcode_home_else_falls_back_to_home_dot_squadcode() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = RestoreEnv(
            ["SQUADCODE_HOME", "HOME"]
                .into_iter()
                .map(|k| (k, std::env::var_os(k)))
                .collect(),
        );
        let base = std::env::temp_dir().join(format!("squad-data-dir-{}", std::process::id()));

        let squad_home = base.join("squad-home");
        std::env::set_var("SQUADCODE_HOME", &squad_home);
        assert_eq!(data_dir(), squad_home);

        let home = base.join("home");
        std::env::remove_var("SQUADCODE_HOME");
        std::env::set_var("HOME", &home);
        assert_eq!(data_dir(), home.join(".squadcode"));
    }

    #[test]
    fn squad_code_copies_runos_saved_files_once() {
        let base = std::env::temp_dir().join(format!("squad-adopt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let old = base.join(".runo");
        let new = base.join(".squadcode");
        std::fs::create_dir_all(old.join("logs")).unwrap();
        std::fs::write(old.join("v2.json"), "{}").unwrap();
        std::fs::write(old.join("logs/a.log"), "x").unwrap();
        adopt_from(&old, &new);
        assert_eq!(std::fs::read_to_string(new.join("v2.json")).unwrap(), "{}");
        assert!(!new.join("logs").exists());
        // Not again once Squad Code has its own.
        std::fs::write(old.join("v2.json"), "{\"later\":1}").unwrap();
        adopt_from(&old, &new);
        assert_eq!(std::fs::read_to_string(new.join("v2.json")).unwrap(), "{}");
        let _ = std::fs::remove_dir_all(&base);
    }
}
