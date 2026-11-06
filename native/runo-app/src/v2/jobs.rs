//! Running a job for real: the bot's engine (a coding agent) works in the
//! person's folder, and what it says and does streams into the job's feed.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver};

use crate::agent::{self, Event, Target};
use crate::harness::AgentEvent;
use crate::settings::AppSettings;

use super::data::Mode;
use super::engine;
use super::store::{now, Item, Job, Status};

/// A job's agent process, while it runs.
pub struct Runner {
    pub id: u64,
    handle: agent::Handle,
    rx: Receiver<Event>,
    parser: Option<crate::stream::Parser>,
    errors: Vec<String>,
    stopped: bool,
}

/// Commands a bot that changes files may run without asking: the project's
/// own checks, and git's read-only views. Anything else is still refused,
/// since a headless run can't ask.
pub fn may_run(checks: &[String]) -> Vec<String> {
    let mut out: Vec<String> = checks.iter().map(|c| format!("Bash({c}:*)")).collect();
    for c in ["git status", "git diff", "git log", "git show", "ls"] {
        out.push(format!("Bash({c}:*)"));
    }
    out
}

/// What a bot that changes files is told about the checks.
fn check_rule(checks: &[String]) -> String {
    if checks.is_empty() {
        return String::new();
    }
    format!(
        " You may run the project's checks ({}) to see your change work; Squad Code runs them again when you finish.",
        checks.join(", ")
    )
}

/// What the bot is told, around the person's words.
fn prompt(job: &Job, text: &str, follow_up: bool, checks: &[String]) -> String {
    if follow_up {
        return text.to_string();
    }
    let rules = match job.mode {
        Mode::Ask | Mode::Review => {
            "Read the code to be sure, and don't change any files.".to_string()
        }
        _ => format!(
            "Make the change in this folder. Keep a short todo list as you work.{}",
            check_rule(checks)
        ),
    };
    let part = if job.context.is_empty() {
        String::new()
    } else {
        format!("{}\n\n", job.context)
    };
    format!(
        "{}\n\n{part}{rules} When you finish, end with two or three plain sentences for someone who may not code: what you found or changed, and how to check it.\n\nThe job: {text}",
        job.bot.persona()
    )
}

/// Starts `job` (or a follow-up in it) on the bot's engine.
pub fn start(
    st: &AppSettings,
    job: &mut Job,
    text: &str,
    follow_up: bool,
) -> Result<Runner, String> {
    let (i, model) = engine::for_job(st, job);
    let p = &agent::PROVIDERS[i];
    let mut opts = st.agent(p.id);
    if !agent::available(p.bin, &opts.binary) {
        return Err(format!(
            "{} isn't installed on this computer. Pick another engine for {} in Engines.",
            p.name,
            job.bot.name()
        ));
    }
    let mode = if job.mode.writes() {
        agent::effective_mode(p.id, "edit")
    } else {
        "plan"
    };
    if let Some(why) = agent::mode_error(p.id, mode) {
        return Err(why);
    }
    let mut sel: HashMap<String, String> = HashMap::new();
    sel.insert("mode".into(), mode.into());
    if !model.is_empty() {
        sel.insert("model".into(), model.clone());
    }
    let resume = follow_up
        && job.agent == p.id
        && !job.conversation.is_empty()
        && agent::can_resume(p.id, &job.conversation);
    if !resume {
        job.conversation = if agent::names_conversation(p.id) {
            agent::new_uuid()
        } else {
            String::new()
        };
    }
    let dir = job.dir().to_string();
    // A bot that changes files may run the project's checks, so it can see
    // its change work instead of being refused.
    let checks = if job.mode.writes() {
        super::work::checks_for(&dir)
    } else {
        Vec::new()
    };
    if !checks.is_empty() {
        let mut rules: Vec<String> = opts
            .allowed_tools
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        for r in may_run(&checks) {
            if !rules.contains(&r) {
                rules.push(r);
            }
        }
        opts.allowed_tools = rules.join(",");
    }
    let sent = prompt(job, text, resume, &checks);
    let spec = agent::RunSpec {
        provider: p.id,
        prompt: &sent,
        resume,
        sel: &sel,
        opts: &opts,
        in_git: std::path::Path::new(&dir).join(".git").exists(),
        session_id: &job.conversation,
        images: &[],
        memory: None,
        local: true,
        thread: 0,
        trusted: false,
    };
    let (tx, rx) = channel();
    let handle = agent::run_agent(Target::Terminal(job.id), &spec, &dir, None, tx)?;
    job.agent = p.id.to_string();
    job.engine = engine::label_of(i, &model);
    job.status = Status::Running;
    job.ended = None;
    job.seen = false;
    Ok(Runner {
        id: job.id,
        handle,
        rx,
        parser: crate::stream::Parser::for_provider(p.id, &dir),
        errors: Vec::new(),
        stopped: false,
    })
}

impl Runner {
    pub fn stop(&mut self) {
        self.stopped = true;
        self.handle.kill();
    }

    /// Moves what arrived into the job. Returns true once the run has ended.
    pub fn pump(&mut self, job: &mut Job) -> bool {
        let mut ended = None;
        for _ in 0..400 {
            let Ok(ev) = self.rx.try_recv() else { break };
            match ev {
                Event::Line(_, line) => match &mut self.parser {
                    Some(p) => {
                        for e in p.feed(&line) {
                            apply(job, e);
                        }
                    }
                    None => say(job, &agent::strip_ansi(&line), true),
                },
                Event::Json(_, line, v) => {
                    if let Some(p) = &mut self.parser {
                        for e in p.feed_parsed(&line, Some(v)) {
                            apply(job, e);
                        }
                    }
                }
                Event::Err(_, line) => {
                    self.errors.push(agent::strip_ansi(&line));
                    if self.errors.len() > 40 {
                        self.errors.remove(0);
                    }
                }
                Event::Done(_, ok) => ended = Some(ok),
                Event::Output(..) | Event::Partial(..) => {}
            }
        }
        let Some(ok) = ended else { return false };
        // Steps still marked running ended with the run.
        for it in &mut job.feed {
            if let Item::Tool { state, .. } = it {
                if *state == 0 {
                    *state = if ok { 1 } else { 2 };
                }
            }
        }
        job.ended = Some(now());
        job.status = if self.stopped {
            Status::Stopped
        } else if ok {
            Status::Done
        } else {
            Status::Failed
        };
        if job.status == Status::Failed && !job.feed.iter().any(|i| matches!(i, Item::Error(_))) {
            let log = self.errors.join("\n");
            let why = agent::sign_in_hint(&job.agent, &log).unwrap_or_else(|| {
                self.errors
                    .iter()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .map(|l| format!("The engine stopped: {}", l.trim()))
                    .unwrap_or_else(|| "The engine stopped without saying why.".into())
            });
            job.feed.push(Item::Error(why));
        }
        if job.status == Status::Done {
            for s in &mut job.plan {
                if s.1 == 1 {
                    s.1 = 2;
                }
            }
        }
        true
    }
}

/// Appends reply text, joining it to the bot's last words when they are still open.
fn say(job: &mut Job, text: &str, line: bool) {
    if text.is_empty() {
        return;
    }
    if let Some(Item::Text(last)) = job.feed.last_mut() {
        if line {
            last.push('\n');
        }
        last.push_str(text);
        return;
    }
    job.feed.push(Item::Text(text.trim_start().to_string()));
}

fn apply(job: &mut Job, e: AgentEvent) {
    use crate::harness::ToolKind;
    match e {
        AgentEvent::Text(t) => say(job, &t, false),
        AgentEvent::Plan(t) => job.feed.push(Item::Text(t)),
        AgentEvent::Tool(step) => {
            if matches!(step.kind, ToolKind::Todo) {
                return;
            }
            if matches!(
                step.kind,
                ToolKind::Edit | ToolKind::Write | ToolKind::Delete
            ) {
                if let Some(file) = step
                    .label
                    .split_once(' ')
                    .map(|(_, f)| f.trim().to_string())
                {
                    if !file.is_empty() && !job.files.contains(&file) {
                        job.files.push(file);
                    }
                }
            }
            let state = match step.state {
                crate::harness::StepState::Running => 0,
                crate::harness::StepState::Failed => 2,
                _ => 1,
            };
            job.feed.push(Item::Tool {
                id: step.id,
                label: step.label,
                kind: format!("{:?}", step.kind).to_lowercase(),
                state,
            });
        }
        AgentEvent::ToolDone { id, ok, .. } => {
            for it in job.feed.iter_mut().rev() {
                if let Item::Tool { id: i, state, .. } = it {
                    if *i == id && !id.is_empty() {
                        *state = if ok { 1 } else { 2 };
                        break;
                    }
                }
            }
        }
        AgentEvent::Summary(s) => job.summary = s,
        // A run reports its tokens so far; a follow-up starts a new count, so
        // keep the larger rather than adding one run's count twice.
        AgentEvent::Usage(u) => job.tokens = job.tokens.max(u.input + u.output),
        // The plan's limit windows, so the status bar follows them live.
        AgentEvent::Limit(l) => crate::usage::note_live(&job.agent, &l, crate::usage::now_secs()),
        AgentEvent::Error(s) => job.feed.push(Item::Error(s)),
        AgentEvent::Notice(s) => job.feed.push(Item::Note(s)),
        AgentEvent::Denied(tools) => job.feed.push(Item::Note(format!(
            "It wanted to use {}, which this kind of job doesn't allow.",
            tools.join(", ")
        ))),
        AgentEvent::Todos(steps) => {
            use crate::crew::Mark;
            job.plan = steps
                .into_iter()
                .map(|s| {
                    let m = match s.mark {
                        Mark::Todo => 0,
                        Mark::Doing => 1,
                        Mark::Done => 2,
                    };
                    (s.text, m)
                })
                .collect();
        }
        AgentEvent::Conversation(c) => job.conversation = c,
        AgentEvent::Questions(qs) => {
            for q in qs {
                job.feed
                    .push(Item::Note(format!("It asked: {}", q.question)));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bot_that_changes_files_may_run_the_checks() {
        let checks = vec!["npm test".to_string(), "cargo test".to_string()];
        let rules = may_run(&checks);
        assert!(rules.contains(&"Bash(npm test:*)".to_string()));
        assert!(rules.contains(&"Bash(git diff:*)".to_string()));
        assert!(!rules.iter().any(|r| r == "Bash"));
        assert!(check_rule(&checks).contains("npm test, cargo test"));
        assert!(check_rule(&[]).is_empty());
    }
}
