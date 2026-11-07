//! Peek reads each message with the person's own engine.
//!
//! The keyword rules in [`super::front`] guess; a language model reads. So
//! when an engine is signed in (Claude Code, Codex, Grok…, on the person's
//! own plan), every message Peek gets is one short call on its smallest
//! model, which answers with a decision as JSON: reply itself, ask back,
//! stop what's running, answer a question about the project, or hand the
//! work to a named bot with a one-line reason. Peek's words are the model's.
//!
//! The rules stay for what needs no thought ("stop app"), and as the answer
//! when no engine can run or its reply doesn't parse. The guard rails still
//! apply to whatever the model hands on: a risky ask waits for a yes.

use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use serde_json::Value;

use super::front::Front;
use super::route::{self, Route};
use crate::agent::{self, Event};
use crate::settings::AppSettings;
use crate::v2::data::{Member, Mode};

/// How long Peek waits for the engine before the rules decide.
pub const LIMIT: Duration = Duration::from_secs(40);

/// What Peek knows when it reads a message.
pub struct Seen<'a> {
    /// The project's name.
    pub project: &'a str,
    /// The conversation so far, oldest first, as (who, words).
    pub recent: &'a [(String, String)],
    /// The bots on the crew.
    pub crew: &'a [Member],
    /// Jobs running here, as "#4 Builder: add a login page".
    pub running: &'a [String],
}

fn clip(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "\u{2026}"
    }
}

/// The words the engine reads.
pub fn prompt(text: &str, seen: &Seen) -> String {
    let crew: Vec<String> = seen
        .crew
        .iter()
        .filter(|m| **m != Member::Fin)
        .map(|m| {
            format!(
                "- {} ({} work): {}",
                m.name(),
                m.mode().label().to_lowercase(),
                m.role()
            )
        })
        .collect();
    let recent: Vec<String> = seen
        .recent
        .iter()
        .rev()
        .take(8)
        .rev()
        .map(|(who, said)| format!("{who}: {}", clip(said, 300)))
        .collect();
    let running = if seen.running.is_empty() {
        "nothing".to_string()
    } else {
        seen.running.join("; ")
    };
    format!(
        "You are Peek, who leads a crew of AI bots in Squad Code, an app for developers and for people who don't code. \
You read every message the person sends before anyone works on it, and decide what happens. Don't use any tools; just reply.\n\n\
Project: {project}\n\
The crew:\n{crew}\n\
Running right now: {running}\n\
The conversation so far:\n{recent}\n\n\
The person just wrote: \"{text}\"\n\n\
Decide, reading it the way a sharp colleague would: typos, slang and half sentences are normal (\"whoa re you\" is \"who are you\"). \
Never start work on a guess.\n\
- \"reply\": chat, greetings, thanks, questions about you or the crew, and general questions you can answer well yourself. Write the reply.\n\
- \"ask\": you can't tell what they want, or real work is too vague to start. Write one short question, with up to 3 choices if they help.\n\
- \"stop\": they want what's running stopped or cancelled.\n\
- \"answer\": a question about this project's code or files, which needs reading them. A bot answers without changing anything.\n\
- \"hand\": real work on the project. Pick the bot whose role fits best, the kind of work, and say why in one short, friendly line in your own voice, like \"Sounds like a bug, so I'm bringing in Fixer to hunt it down.\"\n\n\
Reply with JSON only, on one line:\n\
{{\"do\": \"reply|ask|stop|answer|hand\", \"say\": \"what Peek says\", \"choices\": [], \"bot\": \"a name from the crew, or empty\", \"kind\": \"build|fix|test|review|tidy|ask, or empty\"}}\n\
Keep \"say\" warm, plain and short: two sentences at most, no markdown.",
        project = seen.project,
        crew = crew.join("\n"),
        recent = if recent.is_empty() {
            "(none)".to_string()
        } else {
            recent.join("\n")
        },
        text = clip(text, 2000).replace('"', "'"),
    )
}

/// The engine's decision as Peek acts on it, and Peek's own line for a
/// hand-off. None when the reply doesn't hold a decision that makes sense,
/// so the rules decide instead.
pub fn parse(reply: &str, text: &str, crew: &[Member]) -> Option<(Front, String)> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    let v: Value = serde_json::from_str(reply.get(start..=end)?).ok()?;
    let say = v["say"].as_str().unwrap_or("").trim().to_string();
    let front = match v["do"].as_str()?.trim() {
        "reply" if !say.is_empty() => Front::Chat { reply: say.clone() },
        "ask" if !say.is_empty() => Front::AskBack {
            question: say.clone(),
            choices: v["choices"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|c| c.as_str())
                        .map(|c| c.trim().to_string())
                        .filter(|c| !c.is_empty() && c.chars().count() <= 60)
                        .take(3)
                        .collect()
                })
                .unwrap_or_default(),
        },
        "stop" => Front::Stop,
        "answer" => Front::Answer {
            question: text.to_string(),
            code: true,
        },
        "hand" => {
            let mode = match v["kind"].as_str().unwrap_or("").trim() {
                "fix" => Mode::Fix,
                "test" => Mode::Test,
                "review" => Mode::Review,
                "tidy" => Mode::Tidy,
                "ask" => Mode::Ask,
                _ => Mode::Build,
            };
            let named = v["bot"].as_str().unwrap_or("").trim().to_lowercase();
            let bot = crew
                .iter()
                .copied()
                .find(|m| *m != Member::Fin && m.name().to_lowercase() == named)
                .unwrap_or_else(|| route::doer(mode, crew));
            let r = Route {
                mode,
                bot,
                sure: 0.9,
                why: say.clone(),
                by: "engine".into(),
            };
            // The guard rails hold, whoever decides.
            match super::guard::risk(text) {
                Some(why) => Front::Confirm { why, route: r },
                None => Front::Hand(r),
            }
        }
        _ => return None,
    };
    Some((front, say))
}

/// Asks the person's engine to read `text`, on another thread. Err when no
/// engine can run here; the rules decide then.
pub fn ask(
    st: &AppSettings,
    dir: &str,
    text: &str,
    seen: &Seen,
) -> Result<Receiver<Event>, String> {
    let i = crate::assistant::assistant_provider(st);
    let p = &agent::PROVIDERS[i];
    let configured = st.agent(p.id).binary;
    if !agent::available(p.bin, &configured) {
        return Err(format!("{} isn't installed", p.name));
    }
    let model = crate::assistant::assistant_model(st, p.id);
    let command = agent::ask_command_on(p.id, &configured, &prompt(text, seen), &model);
    let (tx, rx) = channel();
    agent::oneshot_within("v2-peek-reads".into(), command, dir, LIMIT, tx);
    Ok(rx)
}

/// Whether Peek should read with the engine: on unless the person turned it
/// off (`peek_reads_with_engine: false` in settings).
pub fn on(st: &AppSettings) -> bool {
    st.extra
        .get("peek_reads_with_engine")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crew() -> Vec<Member> {
        Member::CREW.to_vec()
    }

    #[test]
    fn the_prompt_carries_the_crew_and_what_is_running() {
        let recent = vec![("You".to_string(), "add a login page".to_string())];
        let running = vec!["#4 Builder: add a login page".to_string()];
        let c = crew();
        let p = prompt(
            "whoa re you",
            &Seen {
                project: "shop",
                recent: &recent,
                crew: &c,
                running: &running,
            },
        );
        for want in ["Builder", "Fixer", "#4 Builder", "whoa re you", "\"do\""] {
            assert!(p.contains(want), "{want}");
        }
    }

    #[test]
    fn replies_become_what_peek_does() {
        let c = crew();
        let (f, _) = parse(
            r#"{"do":"reply","say":"I'm Peek, I run your crew.","choices":[],"bot":"","kind":""}"#,
            "whoa re you",
            &c,
        )
        .unwrap();
        assert_eq!(
            f,
            Front::Chat {
                reply: "I'm Peek, I run your crew.".into()
            }
        );
        let (f, line) = parse(
            "Sure.\n{\"do\":\"hand\",\"say\":\"A new page, so Builder's on it.\",\"bot\":\"builder\",\"kind\":\"build\"}",
            "add a pricing page",
            &c,
        )
        .unwrap();
        match f {
            Front::Hand(r) => {
                assert_eq!(r.bot, Member::Builder);
                assert_eq!(r.mode, Mode::Build);
            }
            other => panic!("{other:?}"),
        }
        assert!(line.contains("Builder"));
        assert_eq!(
            parse(r#"{"do":"stop","say":""}"#, "stop it", &c).unwrap().0,
            Front::Stop
        );
        // The guard rails still hold.
        assert!(matches!(
            parse(
                r#"{"do":"hand","say":"Shipping it.","bot":"Shipper","kind":"build"}"#,
                "deploy the new pricing page to production",
                &c
            )
            .unwrap()
            .0,
            Front::Confirm { .. }
        ));
        // Nonsense falls back to the rules.
        assert!(parse("I think you mean hello", "hi", &c).is_none());
        assert!(parse(r#"{"do":"reply","say":""}"#, "hi", &c).is_none());
        assert!(parse(r#"{"do":"dance"}"#, "hi", &c).is_none());
    }
}
