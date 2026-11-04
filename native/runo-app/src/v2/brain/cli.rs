//! `squad do` and `squad brain …`: the intelligence layer from a terminal.

use super::super::data::{Member, Mode};
use super::super::store::{self, Store};
use super::{fin, ideas, model, route, teach};

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

/// `squad brain why|ideas|export|eval`.
pub fn main(args: &[String]) -> Result<String, String> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let rest = args.get(1..).unwrap_or(&[]).join(" ");
    let store = Store::load();
    let st = crate::settings::AppSettings::load();
    let crew = Member::all();
    match sub {
        "why" => {
            if rest.trim().is_empty() {
                return Err("usage: squad brain why <words...>".into());
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
                "{} \u{b7} {} job \u{b7} {:.0}% sure (by {})\n{}\nRuns on {site}, on {}. {}",
                r.bot.name(),
                r.mode.label(),
                r.sure * 100.0,
                r.by,
                r.why,
                super::super::engine::label_of(p.engine.0, &p.engine.1),
                p.why
            );
            Ok(out.trim_end().to_string())
        }
        "ideas" => Ok(match ideas::next(&store, &crew, &[], store::now()) {
            Some(i) => format!(
                "{}\n{}: {} (works like {}). Make it on Crew \u{2192} New bot.",
                fin::line(&fin::Moment::Idea {
                    name: i.name.to_string(),
                    why: i.why.clone(),
                }),
                i.name,
                i.role,
                i.like.name()
            ),
            None => {
                "No ideas yet. Fin suggests a bot once the same kind of job comes up a few times."
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
        "eval" => {
            let cases = match args.get(1) {
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
            let Some(m) = model::Local::from_settings(&st) else {
                out.push_str("\nSmart Fin is off, so only the rules were scored.");
                return Ok(out);
            };
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
        _ => Err("usage: squad brain why|ideas|export|eval".into()),
    }
}
