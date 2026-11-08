//! What Peek does with a message the person sends. The person's own engine
//! decides (see [`super::think`]); this is only the decision's shape. There
//! are no keyword rules: an engine that can't be reached says so, and work
//! never starts on a guess.

use super::super::data::{Member, Mode};
use super::route::{self, Route};

/// What Peek does with a message.
#[derive(Clone, Debug, PartialEq)]
pub enum Front {
    /// Peek answers itself, with this.
    Chat { reply: String },
    /// A question for an engine to answer, read-only. `code` when it is about
    /// the project, so the engine should read the folder.
    Answer { question: String, code: bool },
    /// Too vague to start: ask this, offering `choices` when there are some.
    AskBack {
        question: String,
        choices: Vec<String>,
    },
    /// Real work, for this bot.
    Hand(Route),
    /// Real work that can't easily be undone (production, deleting, secrets,
    /// sending): show `why` and start it as `Hand` only on the person's yes.
    Confirm { why: String, route: Route },
    /// "stop app", "cancel that", "kill the server": stop what's running
    /// here, with no new job.
    Stop,
    /// "run the app": Squad Code starts it in the terminal and keeps it up,
    /// with no job.
    RunApp,
}

/// What Peek decided, when the person named the bot with @: the named bot
/// answers what Peek would answer, and does the work Peek would hand to
/// another bot. A bot that only answers (one that never writes code) answers
/// work asks too. Stop, run the app and asking back stay Peek's.
pub fn pinned(front: Front, bot: Member, text: &str) -> Front {
    let answer = || Front::Answer {
        question: text.to_string(),
        code: true,
    };
    let to = |mut r: Route| {
        r.bot = bot;
        r.mode = bot.mode();
        r
    };
    match front {
        Front::Chat { .. } => answer(),
        Front::Hand(_) | Front::Confirm { .. } if !bot.mode().writes() => answer(),
        Front::Hand(r) => Front::Hand(to(r)),
        Front::Confirm { why, route } => Front::Confirm {
            why,
            route: to(route),
        },
        other => other,
    }
}

/// Who answers a question when nobody was picked: the crew's explainer.
pub fn answerer(crew: &[Member]) -> Member {
    route::doer(Mode::Ask, crew)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(bot: Member) -> Route {
        Route {
            mode: bot.mode(),
            bot,
            sure: 1.0,
            why: String::new(),
            by: "engine".into(),
            site: String::new(),
        }
    }

    #[test]
    fn small_talk_to_a_named_bot_is_its_to_answer_not_a_job() {
        let f = pinned(
            Front::Chat {
                reply: "Hi!".into(),
            },
            Member::Builder,
            "who are you",
        );
        assert!(matches!(f, Front::Answer { ref question, .. } if question == "who are you"));
    }

    #[test]
    fn work_goes_to_the_named_bot() {
        let f = pinned(
            Front::Hand(route(Member::Fixer)),
            Member::Builder,
            "add a login page",
        );
        match f {
            Front::Hand(r) => assert_eq!(r.bot, Member::Builder),
            other => panic!("{other:?}"),
        }
        assert_eq!(pinned(Front::Stop, Member::Builder, "stop"), Front::Stop);
    }
}
