//! Which engine runs under each bot, and asking a bot a question for real.
//!
//! An engine is one of the coding agents Runo already drives (Claude Code, Codex,
//! Gemini, OpenCode…) and the model it runs on. Each bot can have its own; the rest
//! follow the default agent from Settings. Runo's own model calls (shaping jobs,
//! writing the one question) use the assistant agent and model, and a second agent,
//! best from another vendor, can double-check decisions.

use std::sync::mpsc::{channel, Receiver};
use std::time::Instant;

use crate::agent::{self, Event};
use crate::settings::{AppSettings, BotEngine};

use super::data::Member;

/// The settings key for a bot.
pub fn bot_key(who: Member) -> String {
    who.name().to_lowercase()
}

/// The company behind an agent, so the challenger can be from another one.
pub fn vendor(agent_id: &str) -> &'static str {
    match agent_id {
        "claude" => "Anthropic",
        "codex" => "OpenAI",
        "gemini" | "antigravity" => "Google",
        "copilot" => "GitHub",
        "cursor" => "Cursor",
        "amp" => "Sourcegraph",
        "droid" => "Factory",
        "qwen" => "Alibaba",
        "grok" => "xAI",
        "opencode" => "Any model",
        _ => "Other",
    }
}

/// Index into `agent::PROVIDERS` of an agent id, or of the default agent for "".
pub fn index_of(st: &AppSettings, id: &str) -> usize {
    let id = if id.is_empty() {
        st.default_provider.as_str()
    } else {
        id
    };
    agent::PROVIDERS
        .iter()
        .position(|p| p.id == id)
        .unwrap_or(0)
}

/// Whether an agent can run on this computer.
pub fn ready(st: &AppSettings, i: usize) -> bool {
    let p = &agent::PROVIDERS[i];
    agent::available(p.bin, &st.agent(p.id).binary)
}

/// The engine a bot runs on: an agent index and a model ("" for its default).
pub fn engine(st: &AppSettings, who: Member) -> (usize, String) {
    let own = st
        .bot_engines
        .get(&bot_key(who))
        .cloned()
        .unwrap_or_default();
    let i = index_of(st, &own.agent);
    let model = if own.agent.is_empty() && own.model.is_empty() {
        st.agent(agent::PROVIDERS[i].id).model_override
    } else {
        own.model
    };
    (i, model)
}

/// "Claude Code" or "Claude Code · opus".
pub fn label(st: &AppSettings, who: Member) -> String {
    let (i, model) = engine(st, who);
    let name = agent::PROVIDERS[i].name;
    match crate::agent_pick::AgentPick::model_label(i, &model) {
        Some(m) => format!("{name} · {m}"),
        None => name.to_string(),
    }
}

pub fn set(st: &mut AppSettings, who: Member, engine: BotEngine) {
    if engine == BotEngine::default() {
        st.bot_engines.remove(&bot_key(who));
    } else {
        st.bot_engines.insert(bot_key(who), engine);
    }
}

/// A question on its way to a bot's engine.
pub struct Ask {
    pub who: Member,
    pub engine: String,
    pub agent_id: &'static str,
    pub started: Instant,
    rx: Receiver<Event>,
}

/// What came back.
pub enum Reply {
    Answer(String),
    Failed(String),
}

impl Ask {
    /// Asks `who` the question on its own engine, read-only, in the project folder.
    pub fn start(
        st: &AppSettings,
        who: Member,
        folder: &super::store::Folder,
        question: &str,
        history: &[(bool, String)],
    ) -> Result<Ask, String> {
        let (i, model) = engine(st, who);
        let p = &agent::PROVIDERS[i];
        let configured = st.agent(p.id).binary;
        if !agent::available(p.bin, &configured) {
            return Err(format!(
                "{} isn't installed on this computer. Pick another engine for {} in Engines.",
                p.name,
                who.name()
            ));
        }
        let (project, dir) = (&folder.name, &folder.path);
        let mut prompt = format!(
            "{} Read the code to be sure, and never change it.\n\nThe project is \"{project}\", in the current folder.\n",
            who.persona()
        );
        let recent: Vec<_> = history.iter().rev().take(6).collect();
        if !recent.is_empty() {
            prompt.push_str("\nThe conversation so far:\n");
            for (mine, text) in recent.into_iter().rev() {
                prompt.push_str(if *mine { "Person: " } else { "You: " });
                prompt.push_str(text);
                prompt.push('\n');
            }
        }
        prompt.push_str(&format!(
            "\nQuestion: {question}\n\nAnswer in under 150 words."
        ));
        let command = agent::ask_command_on(p.id, &configured, &prompt, &model);
        let (tx, rx) = channel();
        agent::oneshot("v2-ask".into(), command, dir, tx);
        Ok(Ask {
            who,
            engine: label(st, who),
            agent_id: p.id,
            started: Instant::now(),
            rx,
        })
    }

    /// The reply, once it has arrived.
    pub fn poll(&self) -> Option<Reply> {
        match self.rx.try_recv() {
            Ok(Event::Output(_, Ok(text))) => {
                Some(Reply::Answer(agent::strip_ansi(&text).trim().to_string()))
            }
            Ok(Event::Output(_, Err(err))) => {
                let help = agent::sign_in_hint(self.agent_id, &err);
                let first = err
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("it stopped without saying why")
                    .trim()
                    .to_string();
                Some(Reply::Failed(
                    help.unwrap_or_else(|| format!("The engine stopped: {first}")),
                ))
            }
            Ok(_) => None,
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Some(Reply::Failed("The engine stopped before answering.".into()))
            }
        }
    }
}
