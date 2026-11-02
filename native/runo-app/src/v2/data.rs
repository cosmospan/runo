//! The crew: who each bot is, what it is for, and how it looks.

use runo_holo::{Bot, Bubble};

use super::ui;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Member {
    Builder,
    Checker,
    Guard,
    Fixer,
    Explainer,
    Keeper,
}

impl Member {
    pub const CREW: [Member; 6] = [
        Member::Builder,
        Member::Checker,
        Member::Guard,
        Member::Fixer,
        Member::Explainer,
        Member::Keeper,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Member::Builder => "Builder",
            Member::Checker => "Checker",
            Member::Guard => "Guard",
            Member::Fixer => "Fixer",
            Member::Explainer => "Explainer",
            Member::Keeper => "Keeper",
        }
    }

    pub fn role(self) -> &'static str {
        match self {
            Member::Builder => "Writes the code",
            Member::Checker => "Tries it for real",
            Member::Guard => "Keeps your rules",
            Member::Fixer => "Hunts the bug",
            Member::Explainer => "Plain words",
            Member::Keeper => "Tidies up",
        }
    }

    /// One line on what to hand this bot, for empty chats and the crew cards.
    pub fn pitch(self) -> &'static str {
        match self {
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
        }
    }

    /// Things to try in an empty chat.
    pub fn starters(self) -> [&'static str; 3] {
        match self {
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
        }
    }

    pub fn bot(self) -> Bot {
        match self {
            Member::Builder => Bot::Builder,
            Member::Checker => Bot::Checker,
            Member::Guard => Bot::Guard,
            Member::Fixer => Bot::Fixer,
            Member::Explainer => Bot::Explainer,
            Member::Keeper => Bot::Keeper,
        }
    }

    pub fn bubble(self) -> Bubble {
        self.bot().bubble()
    }

    /// The bot's accent, for its name, status and progress.
    pub fn accent(self) -> u32 {
        match self {
            Member::Builder => ui::ORANGE,
            Member::Checker => ui::GREEN,
            Member::Guard => ui::AMBER,
            Member::Fixer => ui::RED,
            Member::Explainer => ui::BLUE,
            Member::Keeper => ui::VIOLET,
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
            Member::Builder => "You are Builder, a bot in Runo's crew that writes code. Plan briefly, make the change with tests, and keep to the project's style.",
            Member::Checker => "You are Checker, a bot in Runo's crew that tries things for real. Run the project's tests and checks, and report plainly what passed and what broke.",
            Member::Guard => "You are Guard, a bot in Runo's crew that keeps the team's rules. Point out risks to login, payments, data and secrets in plain words.",
            Member::Fixer => "You are Fixer, a bot in Runo's crew that hunts bugs. Find the cause with evidence, fix it, and add a test that would have caught it.",
            Member::Explainer => "You are Explainer, a bot in Runo's crew. You answer questions about this codebase in plain words for people who may not code. Lead with the answer, then the evidence (files, numbers).",
            Member::Keeper => "You are Keeper, a bot in Runo's crew that tidies up: dependencies, flaky tests, dead code. Keep changes small and safe.",
        }
    }

    /// A stable key for the frame cache.
    pub fn key(self) -> u64 {
        self as u64 + 1
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

/// Starter jobs on an empty Today, one per kind of work.
pub const STARTERS: [(Mode, &str); 4] = [
    (Mode::Ask, "What does this project do, in plain words?"),
    (Mode::Review, "Review my uncommitted changes"),
    (Mode::Test, "Run the test suite and report what fails"),
    (Mode::Tidy, "Find the most useful small cleanup and do it"),
];
