//! The small model on the person's own computer, for when Smart Fin is on.
//!
//! Squad Code doesn't link a model runtime in. It talks to one the person
//! runs, over the OpenAI-style chat API that llama.cpp's `llama-server`,
//! Ollama and LM Studio all serve on localhost. Squad's own model (a small
//! open model fine-tuned on Squad's jobs, see `training/`) is served the same
//! way, so swapping models is a setting, not a rebuild.
//!
//! Every answer is asked for as JSON with a fixed shape and checked before
//! use: an answer that doesn't parse, names a bot that isn't on the crew, or
//! takes longer than [`TIMEOUT_SECS`] is thrown away and the rules decide.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use crate::settings::AppSettings;

use super::super::data::{Member, Mode};
use super::route::{self, Route};

/// Where `llama-server` listens by default (`llama-server --port 8088`).
pub const DEFAULT_URL: &str = "http://127.0.0.1:8088/v1";
/// A local model that can't answer in this long isn't worth waiting for.
pub const TIMEOUT_SECS: u32 = 4;

/// The instructions the model is asked with. Training examples use exactly
/// this text (see `teach`), so what the model learned is what it's asked.
pub fn route_system(crew: &[Member]) -> String {
    let mut s = String::from(
        "You are Fin, the guide in Squad Code. A person typed a job for the crew of coding bots. \
         Pick the kind of job and the bot that should take it. \
         Kinds: ask (a question, nothing changes), build (something new), fix (a bug or error), \
         review (look for risks, nothing changes), test (run or write tests), tidy (refactor, deps, cleanup).\n\
         The crew:\n",
    );
    for m in crew {
        s.push_str(&format!(
            "- {}: {} ({})\n",
            m.name(),
            m.role(),
            m.mode().label().to_lowercase()
        ));
    }
    s.push_str(
        "Answer with JSON only: {\"mode\": kind, \"bot\": name, \"sure\": 0 to 1, \"why\": one short sentence}.",
    );
    s
}

/// The JSON schema routing answers must follow, for servers that enforce it.
fn route_schema(crew: &[Member]) -> Value {
    let modes: Vec<String> = Mode::ALL.iter().map(|m| m.label().to_lowercase()).collect();
    let bots: Vec<&str> = crew.iter().map(|m| m.name()).collect();
    json!({
        "type": "object",
        "properties": {
            "mode": {"type": "string", "enum": modes},
            "bot": {"type": "string", "enum": bots},
            "sure": {"type": "number"},
            "why": {"type": "string"}
        },
        "required": ["mode", "bot", "sure", "why"]
    })
}

/// A model to ask: where it's served and its name there.
#[derive(Clone, Debug)]
pub struct Local {
    pub url: String,
    pub model: String,
}

impl Local {
    /// The person's local model, when Smart Fin is on.
    pub fn from_settings(st: &AppSettings) -> Option<Local> {
        if st.brain_model.trim().is_empty() {
            return None;
        }
        let url = if st.brain_url.trim().is_empty() {
            DEFAULT_URL.to_string()
        } else {
            st.brain_url.trim().trim_end_matches('/').to_string()
        };
        Some(Local {
            url,
            model: st.brain_model.trim().to_string(),
        })
    }

    /// One chat turn, asked for as JSON matching `schema`; the reply's text.
    pub fn chat(&self, system: &str, user: &str, schema: Option<Value>) -> Option<String> {
        let mut body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ],
            "temperature": 0.1,
            "max_tokens": 160,
            "stream": false
        });
        if let Some(schema) = schema {
            body["response_format"] = json!({
                "type": "json_schema",
                "json_schema": {"name": "answer", "schema": schema, "strict": true}
            });
        }
        let url = format!("{}/chat/completions", self.url);
        let mut child = Command::new("curl")
            .args(["-sS", "-m", &TIMEOUT_SECS.to_string()])
            .args([
                "-H",
                "Content-Type: application/json",
                "--data-binary",
                "@-",
            ])
            .arg(&url)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(body.to_string().as_bytes());
        }
        let out = child.wait_with_output().ok()?;
        if !out.status.success() {
            return None;
        }
        let v: Value = serde_json::from_slice(&out.stdout).ok()?;
        v["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_string)
    }

    /// The model's pick for `text`, if it answers in time with one that makes sense.
    pub fn route(&self, text: &str, crew: &[Member]) -> Option<Route> {
        let reply = self.chat(&route_system(crew), text, Some(route_schema(crew)))?;
        parse_route(&reply, crew, &self.model)
    }

    /// Whether anything answers at the address.
    pub fn alive(&self) -> bool {
        Command::new("curl")
            .args(["-sS", "-m", "2", "-w", "%{http_code}"])
            .args(["-o", if cfg!(windows) { "NUL" } else { "/dev/null" }])
            .arg(format!("{}/models", self.url))
            .output()
            .ok()
            .is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "200")
    }
}

/// A model's routing answer, checked: the kind must exist and the bot must be
/// on the crew. A bot that does another kind of work than the one named is
/// swapped for the one that does.
pub fn parse_route(reply: &str, crew: &[Member], by: &str) -> Option<Route> {
    // Small models sometimes wrap JSON in a code fence or a sentence.
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    let v: Value = serde_json::from_str(reply.get(start..=end)?).ok()?;
    let mode_s = v["mode"].as_str()?.to_lowercase();
    let mode = Mode::ALL
        .into_iter()
        .find(|m| m.label().to_lowercase() == mode_s)?;
    let named = v["bot"].as_str().unwrap_or("");
    let bot = crew
        .iter()
        .copied()
        .find(|m| m.name().eq_ignore_ascii_case(named))
        .filter(|m| m.mode() == mode)
        .unwrap_or_else(|| route::doer(mode, crew));
    let sure = v["sure"].as_f64().unwrap_or(0.5).clamp(0.0, 1.0) as f32;
    let why = v["why"].as_str().unwrap_or("").trim().to_string();
    Some(Route {
        mode,
        bot,
        sure,
        why,
        by: by.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_good_answer_is_used() {
        let r = parse_route(
            "```json\n{\"mode\":\"fix\",\"bot\":\"Fixer\",\"sure\":0.9,\"why\":\"It's a crash.\"}\n```",
            &Member::CREW,
            "fin-0.8b",
        )
        .unwrap();
        assert_eq!((r.mode, r.bot), (Mode::Fix, Member::Fixer));
        assert_eq!(r.by, "fin-0.8b");
    }

    #[test]
    fn a_made_up_kind_is_thrown_away() {
        assert!(parse_route(
            "{\"mode\":\"deploy\",\"bot\":\"Shipper\"}",
            &Member::CREW,
            "m"
        )
        .is_none());
        assert!(parse_route("I think Fixer", &Member::CREW, "m").is_none());
    }

    #[test]
    fn a_bot_for_other_work_is_swapped_for_the_right_one() {
        let r = parse_route(
            "{\"mode\":\"test\",\"bot\":\"Builder\"}",
            &Member::CREW,
            "m",
        )
        .unwrap();
        assert_eq!(r.bot, Member::Checker);
    }

    /// The training scripts write the same prompt (training/make_data.py); a
    /// model trained on one prompt and asked with another does worse.
    #[test]
    fn the_prompt_matches_the_one_training_uses() {
        let trained = include_str!("../../../../../training/eval/route_system.txt");
        assert_eq!(route_system(&Member::CREW), trained);
    }

    #[test]
    fn the_prompt_lists_the_crew() {
        let s = route_system(&Member::CREW);
        assert!(s.contains("- Fixer: Hunts the bug (fix)"));
    }
}
