//! `squad do` and `squad brain …`: the intelligence layer from a terminal.

use super::super::data::{Member, Mode};
use super::super::store::{self, Store};
use super::{helper, ideas, model, route, tasks, teach, voice};

/// Where `squad do <words>` sends a job: the bot, the kind of job, and a line
/// saying why, for the terminal.
pub fn route_do(text: &str) -> route::Route {
    let _ = Store::load(); // knows the person's crew and made bots
    let st = crate::settings::AppSettings::load();
    route::route(&st, text)
}

fn here() -> store::Folder {
    let dir = std::env::current_dir()
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default();
    store::Folder {
        name: std::path::Path::new(&dir)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: dir,
        ship: String::new(),
    }
}

/// `squad brain why|may|learned|forget|ideas|export|eval|install|update|remove`.
pub fn main(args: &[String]) -> Result<String, String> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let rest = args.get(1..).unwrap_or(&[]).join(" ");
    let store = Store::load();
    let st = crate::settings::AppSettings::load();
    let crew = Member::all();
    match sub {
        // How a bot's step would be handled: `squad brain may npm install -g x`.
        "may" => {
            if rest.trim().is_empty() {
                return Err("usage: squad brain may <command...>".into());
            }
            let here = std::env::current_dir()
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default();
            let j = super::permit::judge(
                "The bot",
                "Bash",
                &serde_json::json!({ "command": rest }),
                &here,
            );
            let tier = match j.tier {
                super::permit::Tier::Auto => "Just does it.",
                super::permit::Tier::Ask if j.can_always => "Asks: Allow once, Always allow, Deny.",
                super::permit::Tier::Ask => "Asks: Allow once, Deny.",
                super::permit::Tier::EveryTime => "Asks every time: Allow once, Deny.",
                super::permit::Tier::Never => "Refused.",
            };
            Ok(format!("{tier}\n{} {}", j.headline, j.why)
                .trim_end()
                .to_string())
        }
        // Forgets what Peek learned from the engine.
        "forget" => {
            let n = super::learn::load().len();
            super::learn::forget().map_err(|e| e.to_string())?;
            Ok(format!(
                "Peek forgot {n} things it learned. It starts fresh."
            ))
        }
        // What Peek has learned, and how much room it takes.
        "learned" => {
            let all = super::learn::load();
            let bytes: usize = all
                .iter()
                .filter_map(|e| serde_json::to_string(e).ok())
                .map(|l| l.len() + 1)
                .sum();
            Ok(format!(
                "Peek has learned {} things from your engine ({} KB, at most {}).",
                all.len(),
                bytes / 1024,
                super::learn::MAX
            ))
        }
        "why" => {
            if rest.trim().is_empty() {
                return Err("usage: squad brain why <words...>".into());
            }
            // What Peek does with it, before any bot is picked.
            match super::front::read(&st, &rest, &[]) {
                super::front::Front::Chat { reply } => {
                    return Ok(format!("Peek replies itself, no job:\n{reply}"))
                }
                super::front::Front::AskBack { question, choices } => {
                    let mut out = format!("Peek asks first, no job yet:\n{question}");
                    if !choices.is_empty() {
                        out.push_str(&format!("\n[{}]", choices.join("] [")));
                    }
                    return Ok(out);
                }
                super::front::Front::Answer { code, .. } => {
                    return Ok(format!(
                        "{} answers it, read-only, with no job{}.",
                        super::front::answerer(&crew).name(),
                        if code { ", reading the project" } else { "" }
                    ))
                }
                super::front::Front::Confirm { why, route } => {
                    return Ok(format!(
                        "{} would take it as a {} job, after your yes.\n{why}",
                        route.bot.name(),
                        route.mode.label().to_lowercase()
                    ))
                }
                super::front::Front::Stop => {
                    return Ok("Stops what's running here, with no new job.".into())
                }
                super::front::Front::Hand(_) => {}
            }
            // From a terminal, wait for Squad's own server to load when the
            // rules aren't sure, rather than answering with the rules.
            if route::built_in(&rest, &crew).sure < route::Route::SURE {
                helper::ensure(&st, std::time::Duration::from_secs(30));
            }
            let r = route::route(&st, &rest);
            let p = super::place(
                &st,
                &super::JobAsk {
                    bot: r.bot,
                    mode: r.mode,
                    text: rest.clone(),
                    folder: here(),
                    race: 0,
                    goal: false,
                },
                &super::Sites::default(),
            );
            let site = match &p.site {
                super::Site::Here => "this computer".to_string(),
                super::Site::Server(name) => name.clone(),
            };
            let out = format!(
                "Peek: {}\n{} \u{b7} {} job \u{b7} {:.0}% sure (by {})\n{}\nRuns on {site}, on {}. {}",
                r.handoff(),
                r.bot.name(),
                r.mode.label(),
                r.sure * 100.0,
                r.by,
                r.why,
                super::super::engine::label_of(p.engine.0, &p.engine.1),
                p.why
            );
            let mut out = out.trim_end().to_string();
            // Smart Peek's other jobs, when it's on.
            if let Some(t) = tasks::title(&st, &rest) {
                out.push_str(&format!("\nCard title: {t}"));
            }
            if let Some(q) = tasks::clarify(&st, &rest) {
                out.push_str(&format!("\nPeek would ask first: {q}"));
            }
            Ok(out)
        }
        "ideas" if rest.trim() == "make" => {
            let Some(i) = ideas::next(&store, &crew, &[], store::now()) else {
                return Err("No idea to make yet.".into());
            };
            let mut rt = super::super::runtime::Runtime::new(store);
            rt.make_idea(&st, i.key)
                .ok_or("That idea no longer fits.")?;
            Ok(format!(
                "{} joined the crew. It knows {}, with the standard rules for its kind of work.",
                i.name,
                i.words.join(", ")
            ))
        }
        "ideas" => Ok(match ideas::next(&store, &crew, &[], store::now()) {
            Some(i) => format!(
                "{}\n{}: {} (works like {}). Make it with: squad brain ideas make",
                voice::line(&voice::Moment::Idea {
                    name: i.name.to_string(),
                    why: i.why.clone(),
                }),
                i.name,
                i.role,
                i.like.name()
            ),
            None => {
                "No ideas yet. Peek suggests a bot once the same kind of job comes up a few times."
                    .into()
            }
        }),
        "export" => {
            let path = args
                .get(1)
                .ok_or("usage: squad brain export <file.jsonl>")?;
            let lines = teach::examples(&store, &crew);
            std::fs::write(path, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
            Ok(format!(
                "Wrote {} examples to {path}. They stay on this computer unless you share the file.",
                lines.len()
            ))
        }
        "eval" if args.get(1).is_some_and(|a| a == "front") => {
            let path = args
                .get(2)
                .cloned()
                .unwrap_or_else(|| "training/eval/front.jsonl".into());
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
            Ok(super::front::score(&st, &text).report())
        }
        "eval" => {
            let cases = match args.get(1) {
                Some(h) if h == "hard" => teach::HARD.to_string(),
                Some(p) => std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?,
                None => teach::CASES.to_string(),
            };
            let rules = teach::score(&cases, |t| route::by_rules(t, &crew).mode);
            let mut out = format!(
                "Rules: {}/{} ({:.0}%)",
                rules.right,
                rules.total,
                rules.percent()
            );
            for (name, s) in [
                (
                    "Tiny",
                    teach::score(&cases, |t| route::by_tiny(t, &crew).mode),
                ),
                (
                    "Built in (tiny, then rules)",
                    teach::score(&cases, |t| route::built_in(t, &crew).mode),
                ),
            ] {
                out.push_str(&format!(
                    "\n{name}: {}/{} ({:.0}%)",
                    s.right,
                    s.total,
                    s.percent()
                ));
            }
            let Some(m) = model::Local::from_settings(&st) else {
                out.push_str("\nSmart Peek is off, so only the rules were scored.");
                return Ok(out);
            };
            helper::ensure(&st, std::time::Duration::from_secs(30));
            if !m.alive() {
                out.push_str(&format!("\n{} isn't answering at {}.", m.model, m.url));
                return Ok(out);
            }
            let s = teach::score(&cases, |t| {
                m.route(t, &crew).map(|r| r.mode).unwrap_or(Mode::Build)
            });
            out.push_str(&format!(
                "\n{}: {}/{} ({:.0}%)",
                m.model,
                s.right,
                s.total,
                s.percent()
            ));
            for (text, want, got) in s.missed.iter().take(10) {
                out.push_str(&format!("\n  missed: {text} (want {want}, got {got})"));
            }
            Ok(out)
        }
        "install" | "update" => {
            // squad brain install [file.gguf | https://…]: Squad's own model,
            // then Smart Peek on. With nothing named, the latest release.
            // squad brain update: the latest release, if it's newer.
            let base = st.brain_model_url.trim().to_string();
            let msg = match args.get(1) {
                Some(from) if sub == "install" => {
                    helper::install_file(from)?;
                    format!("Installed {from}.")
                }
                _ => match helper::install_latest(&base, sub == "update")? {
                    Some(r) => format!(
                        "Installed Peek v{}{}.",
                        r.version,
                        if r.notes.is_empty() {
                            String::new()
                        } else {
                            format!(": {}", r.notes)
                        }
                    ),
                    None => format!(
                        "Peek v{} is already the latest.",
                        helper::installed().map(|r| r.version).unwrap_or(0)
                    ),
                },
            };
            let mut st = st.clone();
            st.brain_model = helper::MODEL.to_string();
            st.brain_url.clear();
            st.save();
            Ok(format!(
                "{msg} Smart Peek is on, with Squad's own model at {}.",
                helper::model_path().display()
            ))
        }
        "remove" => {
            helper::remove();
            let mut st = st.clone();
            if helper::is_ours(&st.brain_model) {
                st.brain_model.clear();
            }
            st.save();
            Ok("Smart Peek is off and its model is deleted.".into())
        }
        _ => Err(
            "usage: squad brain why|may|learned|forget|ideas|export|eval|install|update|remove"
                .into(),
        ),
    }
}
