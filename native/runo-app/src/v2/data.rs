//! The crew: who each bot is, what it is for, and how it looks.

use std::sync::RwLock;

use runo_holo::{Bot, Bubble};

use super::ui;

/// A bot the person made in the maker on Crew: its name, what it's for, the
/// built-in bot whose kind of work it does, and its look.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Made {
    pub id: u16,
    pub name: String,
    pub role: String,
    pub like: Member,
    pub look: [u8; 4],
}

/// Made bots with their words kept for the life of the app, so `Member` stays a
/// small `Copy` value whose names are `&'static str` like the built-in crew's.
struct Kept {
    id: u16,
    name: &'static str,
    role: &'static str,
    persona: &'static str,
    like: Member,
    bubble: Bubble,
}

static MADE: RwLock<Vec<Kept>> = RwLock::new(Vec::new());

/// Built-in bots the person removed from their crew (or didn't add).
static REMOVED: RwLock<Vec<Member>> = RwLock::new(Vec::new());

/// Sets which built-in bots are not on the crew.
pub fn set_removed(removed: &[Member]) {
    *REMOVED.write().unwrap_or_else(|e| e.into_inner()) = removed.to_vec();
}

/// The crew a new person starts with; the rest can be added any time.
pub const STARTER_CREW: [Member; 4] = [
    Member::Builder,
    Member::Fixer,
    Member::Explainer,
    Member::Shipper,
];

/// Makes `made` the bots the app knows about (on load, and after a change).
pub fn set_made(made: &[Made]) {
    let mut kept = MADE.write().unwrap_or_else(|e| e.into_inner());
    // Words are leaked once per distinct text; a few made bots, rarely renamed.
    let keep = |old: &[Kept], text: &str, f: fn(&Kept) -> &'static str| -> &'static str {
        old.iter()
            .map(f)
            .find(|t| *t == text)
            .unwrap_or_else(|| Box::leak(text.to_string().into_boxed_str()))
    };
    let next: Vec<Kept> = made
        .iter()
        .map(|m| {
            let persona = format!(
                "You are {}, a bot the person made for Runo's crew. Your job: {}. {}",
                m.name,
                m.role,
                m.like.persona()
            );
            Kept {
                id: m.id,
                name: keep(&kept, &m.name, |k| k.name),
                role: keep(&kept, &m.role, |k| k.role),
                persona: keep(&kept, &persona, |k| k.persona),
                like: m.like,
                bubble: Bubble::from_code(m.look),
            }
        })
        .collect();
    *kept = next;
}

fn made<T>(id: u16, f: impl FnOnce(&Kept) -> T) -> Option<T> {
    let kept = MADE.read().unwrap_or_else(|e| e.into_inner());
    kept.iter().find(|k| k.id == id).map(f)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Member {
    Builder,
    Checker,
    Guard,
    Fixer,
    Explainer,
    Keeper,
    /// Owns git: opens pull requests, watches their checks, merges and cleans up.
    Shipper,
    /// A bot the person made, by its id.
    Made(u16),
}

impl Member {
    pub const CREW: [Member; 7] = [
        Member::Builder,
        Member::Checker,
        Member::Guard,
        Member::Fixer,
        Member::Explainer,
        Member::Keeper,
        Member::Shipper,
    ];

    /// The built-in bots whose kind of work a made bot can do.
    pub const KINDS: [Member; 6] = [
        Member::Builder,
        Member::Checker,
        Member::Guard,
        Member::Fixer,
        Member::Explainer,
        Member::Keeper,
    ];

    /// The person's crew: the built-in bots they kept, then the ones they made.
    pub fn all() -> Vec<Member> {
        let removed = REMOVED.read().unwrap_or_else(|e| e.into_inner());
        Member::everyone()
            .into_iter()
            .filter(|m| !removed.contains(m))
            .collect()
    }

    /// Every bot, kept in the crew or not.
    pub fn everyone() -> Vec<Member> {
        let kept = MADE.read().unwrap_or_else(|e| e.into_inner());
        Member::CREW
            .into_iter()
            .chain(kept.iter().map(|k| Member::Made(k.id)))
            .collect()
    }

    /// Whether this bot is in the person's crew.
    pub fn on_crew(self) -> bool {
        !REMOVED
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&self)
            && (!self.is_made() || made(self.made_id(), |_| ()).is_some())
    }

    fn made_id(self) -> u16 {
        match self {
            Member::Made(id) => id,
            _ => 0,
        }
    }

    /// The crew member who takes jobs of this kind: the built-in one if it's on the
    /// crew, else a made bot that does that kind of work.
    pub fn doer_for(mode: Mode) -> Option<Member> {
        let all = Member::all();
        let own = Member::for_mode(mode);
        if all.contains(&own) {
            return Some(own);
        }
        // Shipper takes no jobs of its own: it ships what the others made.
        all.into_iter()
            .find(|m| *m != Member::Shipper && m.mode() == mode)
    }

    /// The built-in bot whose kind of work this one does (itself, for the six).
    pub fn like(self) -> Member {
        match self {
            Member::Made(id) => made(id, |k| k.like).unwrap_or(Member::Explainer),
            m => m,
        }
    }

    pub fn is_made(self) -> bool {
        matches!(self, Member::Made(_))
    }

    pub fn name(self) -> &'static str {
        match self {
            Member::Made(id) => made(id, |k| k.name).unwrap_or("Bot"),
            Member::Builder => "Builder",
            Member::Checker => "Checker",
            Member::Guard => "Guard",
            Member::Fixer => "Fixer",
            Member::Explainer => "Explainer",
            Member::Keeper => "Keeper",
            Member::Shipper => "Shipper",
        }
    }

    pub fn role(self) -> &'static str {
        match self {
            Member::Made(id) => made(id, |k| k.role).unwrap_or("Your bot"),
            Member::Builder => "Writes the code",
            Member::Checker => "Tries it for real",
            Member::Guard => "Keeps your rules",
            Member::Fixer => "Hunts the bug",
            Member::Explainer => "Plain words",
            Member::Keeper => "Tidies up",
            Member::Shipper => "Opens and merges PRs",
        }
    }

    /// One line on what to hand this bot, for empty chats and the crew cards.
    pub fn pitch(self) -> &'static str {
        match self {
            Member::Made(_) => self.like().pitch(),
            Member::Builder => {
                "Give it a feature in a sentence. It plans, writes the code and the tests."
            }
            Member::Checker => {
                "Ask it to run the tests or try a flow, and it tells you what broke."
            }
            Member::Guard => {
                "It reviews changes for risks to login, payments and data before they land."
            }
            Member::Fixer => "Paste an error or describe a bug. It finds the cause and fixes it.",
            Member::Explainer => {
                "Ask anything about the code. It reads it and answers in plain words."
            }
            Member::Keeper => "Dependencies, flaky tests, dead code: the chores nobody gets to.",
            Member::Shipper => {
                "When you approve a job, it opens the pull request, watches the checks and merges."
            }
        }
    }

    /// Things to try in an empty chat.
    pub fn starters(self) -> [&'static str; 3] {
        match self {
            Member::Made(_) => self.like().starters(),
            Member::Builder => [
                "How would you add a settings page here?",
                "What's the smallest change to add dark mode?",
                "Where would a new API endpoint go?",
            ],
            Member::Checker => [
                "How do I run the tests in this project?",
                "Which parts have no tests at all?",
                "What should I check before releasing?",
            ],
            Member::Guard => [
                "Is anything risky in my uncommitted changes?",
                "Where does this project handle passwords?",
                "Are any secrets checked into the code?",
            ],
            Member::Fixer => [
                "What's the most likely crash in this code?",
                "Are there any obvious bugs in the main flow?",
                "Why might this be slow?",
            ],
            Member::Explainer => [
                "What does this project do, in plain words?",
                "How is the code organised?",
                "Walk me through what happens on startup.",
            ],
            Member::Keeper => [
                "Which dependencies are out of date?",
                "Is there dead code I can delete?",
                "What would you tidy first?",
            ],
            Member::Shipper => [
                "Which branches are waiting to be merged?",
                "What changed on main today?",
                "Are there old branches I can delete?",
            ],
        }
    }

    pub fn bot(self) -> Bot {
        match self {
            Member::Made(_) => self.like().bot(),
            Member::Shipper => Bot::Keeper,
            Member::Builder => Bot::Builder,
            Member::Checker => Bot::Checker,
            Member::Guard => Bot::Guard,
            Member::Fixer => Bot::Fixer,
            Member::Explainer => Bot::Explainer,
            Member::Keeper => Bot::Keeper,
        }
    }

    pub fn bubble(self) -> Bubble {
        match self {
            Member::Made(id) => made(id, |k| k.bubble).unwrap_or_else(|| self.like().bubble()),
            Member::Shipper => Bubble {
                tint: runo_holo::Tint::TEAL,
                ..Member::Keeper.bot().bubble()
            },
            _ => self.bot().bubble(),
        }
    }

    /// The bot's accent, for its name, status and progress.
    pub fn accent(self) -> u32 {
        match self {
            Member::Made(_) => self.bubble().tint.strong().0,
            Member::Builder => ui::ORANGE,
            Member::Checker => ui::GREEN,
            Member::Guard => ui::AMBER,
            Member::Fixer => ui::RED,
            Member::Explainer => ui::BLUE,
            Member::Keeper => ui::VIOLET,
            Member::Shipper => 0x14B8A6,
        }
    }

    /// The bot a new job in this mode goes to.
    pub fn for_mode(mode: Mode) -> Member {
        match mode {
            Mode::Ask => Member::Explainer,
            Mode::Build => Member::Builder,
            Mode::Fix => Member::Fixer,
            Mode::Review => Member::Guard,
            Mode::Test => Member::Checker,
            Mode::Tidy => Member::Keeper,
        }
    }

    /// What the bot is, told to its engine so the answer sounds like it.
    pub fn persona(self) -> &'static str {
        match self {
            Member::Made(id) => made(id, |k| k.persona).unwrap_or(""),
            Member::Builder => "You are Builder, a bot in Runo's crew that writes code. Plan briefly, make the change with tests, and keep to the project's style.",
            Member::Checker => "You are Checker, a bot in Runo's crew that tries things for real. Run the project's tests and checks, and report plainly what passed and what broke.",
            Member::Guard => "You are Guard, a bot in Runo's crew that keeps the team's rules. Point out risks to login, payments, data and secrets in plain words.",
            Member::Fixer => "You are Fixer, a bot in Runo's crew that hunts bugs. Find the cause with evidence, fix it, and add a test that would have caught it.",
            Member::Explainer => "You are Explainer, a bot in Runo's crew. You answer questions about this codebase in plain words for people who may not code. Lead with the answer, then the evidence (files, numbers).",
            Member::Keeper => "You are Keeper, a bot in Runo's crew that tidies up: dependencies, flaky tests, dead code. Keep changes small and safe.",
            Member::Shipper => "You are Shipper, the bot in Runo's crew that owns git: branches, pull requests, merges and releases. Answer about the repository's branches and history in plain words, and never push or change anything yourself when asked a question.",
        }
    }

    /// A stable key for the frame cache.
    pub fn key(self) -> u64 {
        match self {
            Member::Builder => 1,
            Member::Checker => 2,
            Member::Guard => 3,
            Member::Fixer => 4,
            Member::Explainer => 5,
            Member::Keeper => 6,
            Member::Shipper => 7,
            Member::Made(id) => 100 + id as u64,
        }
    }

    /// The kind of job this bot takes on Today.
    pub fn mode(self) -> Mode {
        match self.like() {
            Member::Builder => Mode::Build,
            Member::Checker => Mode::Test,
            Member::Guard => Mode::Review,
            Member::Fixer => Mode::Fix,
            Member::Keeper => Mode::Tidy,
            _ => Mode::Ask,
        }
    }
}

/// What kind of job the person is handing over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Mode {
    Ask,
    Build,
    Fix,
    Review,
    Test,
    Tidy,
}

impl Mode {
    pub const ALL: [Mode; 6] = [
        Mode::Ask,
        Mode::Build,
        Mode::Fix,
        Mode::Review,
        Mode::Test,
        Mode::Tidy,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Ask => "Ask",
            Mode::Build => "Build",
            Mode::Fix => "Fix",
            Mode::Review => "Review",
            Mode::Test => "Test",
            Mode::Tidy => "Tidy",
        }
    }

    /// Whether the job may change files. Ask and Review only read.
    pub fn writes(self) -> bool {
        !matches!(self, Mode::Ask | Mode::Review)
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            Mode::Ask => "Ask anything about the code…",
            Mode::Build => "Describe what to build, in a sentence or two…",
            Mode::Fix => "Paste an error, or describe what's broken…",
            Mode::Review => "What should Guard look at? Leave empty for your uncommitted changes…",
            Mode::Test => "What should Checker run or try? Leave empty for the whole test suite…",
            Mode::Tidy => "What should Keeper tidy? Leave empty and it picks…",
        }
    }

    /// The job when the box is left empty, for the modes that have one.
    pub fn default_job(self) -> Option<&'static str> {
        match self {
            Mode::Review => Some("Review my uncommitted changes"),
            Mode::Test => Some("Run the test suite and report what fails"),
            Mode::Tidy => Some("Find the most useful small cleanup and do it"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn made_bots_join_the_crew() {
        set_made(&[Made {
            id: 1,
            name: "Scout".into(),
            role: "Checks the docs are current".into(),
            like: Member::Checker,
            look: [4, 2, 1, 0],
        }]);
        let all = Member::all();
        assert_eq!(all.len(), Member::CREW.len() + 1);
        let scout = Member::Made(1);
        assert_eq!(scout.name(), "Scout");
        assert_eq!(scout.role(), "Checks the docs are current");
        assert_eq!(scout.mode(), Mode::Test);
        assert!(scout.persona().contains("Scout"));
        assert_eq!(scout.accent(), runo_holo::Tint::BLUE.strong().0);
        // Renaming keeps the same bot.
        set_made(&[Made {
            id: 1,
            name: "Scout 2".into(),
            role: "Checks the docs are current".into(),
            like: Member::Checker,
            look: [4, 2, 1, 0],
        }]);
        assert_eq!(Member::Made(1).name(), "Scout 2");
        // Removing Checker: Scout takes its Test jobs.
        set_removed(&[Member::Checker]);
        assert_eq!(Member::doer_for(Mode::Test), Some(Member::Made(1)));
        assert!(!Member::Checker.on_crew());
        set_made(&[]);
        assert_eq!(Member::doer_for(Mode::Test), None);
        set_removed(&[]);
        assert_eq!(Member::doer_for(Mode::Test), Some(Member::Checker));
        assert_eq!(Member::all().len(), Member::CREW.len());
        assert_eq!(Member::Made(1).name(), "Bot");
    }
}
