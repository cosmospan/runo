//! The crew and the sample day the v2 screens show.
//!
//! This is the design's content, so the screens can be judged against it.
//! When `runod` lands, these come from its event log instead.

use runo_holo::{custom, Bot, Eye, Look, Mood, Palette, Shape};

use super::ui;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Member {
    Builder,
    Checker,
    Guard,
    Fixer,
    Explainer,
    Keeper,
    Mochi,
    Pixel,
    Juno,
}

/// Who made a bot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Maker {
    Runo,
    You,
    Teammate(&'static str),
}

impl Member {
    pub const CREW: [Member; 9] = [
        Member::Builder,
        Member::Checker,
        Member::Guard,
        Member::Fixer,
        Member::Explainer,
        Member::Keeper,
        Member::Mochi,
        Member::Pixel,
        Member::Juno,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Member::Builder => "Builder",
            Member::Checker => "Checker",
            Member::Guard => "Guard",
            Member::Fixer => "Fixer",
            Member::Explainer => "Explainer",
            Member::Keeper => "Keeper",
            Member::Mochi => "Mochi",
            Member::Pixel => "Pixel",
            Member::Juno => "Juno",
        }
    }

    pub fn role(self) -> &'static str {
        match self {
            Member::Builder => "Writes the code",
            Member::Checker => "Tries it for real",
            Member::Guard => "Keeps your rules",
            Member::Fixer => "Hunts the bug",
            Member::Explainer => "Plain words",
            Member::Keeper => "Tidies at night",
            Member::Mochi => "Writes weekly notes",
            Member::Pixel => "Checks the designs",
            Member::Juno => "Answers support",
        }
    }

    pub fn maker(self) -> Maker {
        match self {
            Member::Mochi | Member::Juno => Maker::You,
            Member::Pixel => Maker::Teammate("Priya"),
            _ => Maker::Runo,
        }
    }

    /// The bot's own look, before any mood.
    pub fn look(self) -> Look {
        match self {
            Member::Builder => Bot::Builder.signature(),
            Member::Checker => Bot::Checker.signature(),
            Member::Guard => Bot::Guard.signature(),
            Member::Fixer => Bot::Fixer.signature(),
            Member::Explainer => Bot::Explainer.signature(),
            Member::Keeper => Bot::Keeper.signature(),
            Member::Mochi => custom(Palette::RED, Shape::Wide, Eye::Smile),
            Member::Pixel => {
                let mut l = custom(Palette::BLUE, Shape::Tall, Eye::Dots);
                l.props.sparks = true;
                l
            }
            Member::Juno => custom(Palette::GREEN, Shape::Soft, Eye::Sleepy),
        }
    }

    /// What the bot is doing in the sample day.
    pub fn mood(self) -> Mood {
        match self {
            Member::Builder => Mood::Working,
            Member::Guard => Mood::NeedsYou,
            Member::Keeper => Mood::Asleep,
            Member::Mochi | Member::Juno => Mood::Idle,
            _ => Mood::Signature,
        }
    }

    /// A stable key for the frame cache.
    pub fn key(self) -> u64 {
        self as u64 + 1
    }

    /// The bot's accent, for its name and status.
    pub fn accent(self) -> u32 {
        match self {
            Member::Builder => ui::ORANGE,
            Member::Checker | Member::Juno => ui::GREEN,
            Member::Guard => ui::AMBER,
            Member::Fixer | Member::Mochi => ui::RED,
            Member::Explainer | Member::Pixel => ui::BLUE,
            Member::Keeper => ui::VIOLET,
        }
    }

    /// The status line under its name on the Today row.
    pub fn short_status(self) -> (&'static str, Option<u32>) {
        match self {
            Member::Builder => ("refunds 3/5", Some(ui::ORANGE)),
            Member::Checker => ("on iPhone", None),
            Member::Guard => ("needs you", Some(ui::AMBER)),
            Member::Fixer => ("watching", None),
            Member::Explainer => ("free", None),
            Member::Keeper => ("back 09:00", None),
            Member::Mochi => ("yours · free", None),
            Member::Pixel => ("comparing", None),
            Member::Juno => ("yours · free", None),
        }
    }

    /// The crew card: status word and colour, what it's on, its make and its count.
    pub fn card(self) -> (&'static str, u32, &'static str, &'static str, &'static str) {
        match self {
            Member::Builder => (
                "Working",
                ui::ORANGE,
                "Partial refunds · step 3/5",
                "CUBE · DOTS · HOP",
                "142 JOBS",
            ),
            Member::Checker => (
                "On iPhone",
                ui::GREEN,
                "Safari run on pay button",
                "TOWER · LENS · SCAN",
                "88 RUNS",
            ),
            Member::Guard => (
                "Needs you",
                ui::AMBER,
                "Apple Pay · line 40",
                "SLAB · VISOR · SHIELD",
                "12 RULES",
            ),
            Member::Fixer => (
                "Watching",
                ui::GREEN,
                "0 crashes since 11:20",
                "TILTED · SQUINT",
                "31 FIXES",
            ),
            Member::Explainer => (
                "Free",
                ui::FAINT,
                "Last: why checkout slowed",
                "CUBE · SMILES",
                "64 ASKS",
            ),
            Member::Keeper => (
                "Asleep",
                ui::FAINT,
                "Back at 09:00",
                "SOFT · SLEEPY",
                "5 UPDATES",
            ),
            Member::Mochi => (
                "Free",
                ui::FAINT,
                "Sends notes every Friday",
                "WIDE · SMILES",
                "BY YOU",
            ),
            Member::Pixel => (
                "Working",
                ui::BLUE,
                "Comparing 14 screens",
                "TALL · DOTS",
                "BY PRIYA",
            ),
            Member::Juno => (
                "Free",
                ui::FAINT,
                "Ready in #support",
                "SOFT · SLEEPY",
                "BY YOU",
            ),
        }
    }

    pub fn is_working(self) -> bool {
        matches!(self, Member::Builder | Member::Pixel)
    }

    pub fn needs_you(self) -> bool {
        self == Member::Guard
    }

    /// The colour of its dot on the rail.
    pub fn rail_dot(self) -> u32 {
        match self {
            Member::Builder => ui::ORANGE,
            Member::Guard => ui::AMBER,
            Member::Checker | Member::Fixer => ui::GREEN,
            _ => 0xC4C6D4,
        }
    }

    /// Which job mode a new job in this mode goes to.
    pub fn for_mode(mode: Mode) -> Member {
        match mode {
            Mode::Ask => Member::Explainer,
            Mode::Build => Member::Builder,
            Mode::Fix => Member::Fixer,
            Mode::Review => Member::Guard,
            Mode::Ship => Member::Checker,
        }
    }
}

/// What kind of job the person is dropping in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Ask,
    Build,
    Fix,
    Review,
    Ship,
}

impl Mode {
    pub const ALL: [Mode; 5] = [Mode::Ask, Mode::Build, Mode::Fix, Mode::Review, Mode::Ship];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Ask => "Ask",
            Mode::Build => "Build",
            Mode::Fix => "Fix",
            Mode::Review => "Review",
            Mode::Ship => "Ship",
        }
    }
}

/// A job in "Working now".
#[derive(Clone, Debug)]
pub struct Job {
    pub title: String,
    pub bot: Member,
    pub color: u32,
    /// Progress when the screen opened, and how fast it moves per second.
    pub start: f32,
    pub rate: f32,
    pub note: Option<&'static str>,
    /// When it was added, on the screen's clock.
    pub since: f64,
}

impl Job {
    pub fn progress(&self, now: f64) -> f32 {
        (self.start + self.rate * (now - self.since) as f32).min(0.96)
    }
}

pub fn sample_jobs() -> Vec<Job> {
    vec![
        Job {
            title: "Partial refunds".into(),
            bot: Member::Builder,
            color: ui::ORANGE,
            start: 0.62,
            rate: 0.002,
            note: Some("step 3/5"),
            since: 0.0,
        },
        Job {
            title: "Safari run on pay button".into(),
            bot: Member::Checker,
            color: ui::GREEN,
            start: 0.80,
            rate: 0.003,
            note: Some("0:41"),
            since: 0.0,
        },
        Job {
            title: "Why checkout slowed down".into(),
            bot: Member::Explainer,
            color: ui::BLUE,
            start: 0.30,
            rate: 0.004,
            note: Some("reading"),
            since: 0.0,
        },
        Job {
            title: "Rules check on PR #482".into(),
            bot: Member::Guard,
            color: ui::AMBER,
            start: 0.46,
            rate: 0.002,
            note: Some("4 of 9"),
            since: 0.0,
        },
    ]
}

pub const LANDED: [(Member, &str, Option<&str>); 4] = [
    (Member::Fixer, "Search crash fixed", Some("41 users")),
    (Member::Keeper, "5 package updates", None),
    (Member::Fixer, "Flaky test fixed", None),
    (
        Member::Mochi,
        "Weekly notes sent to Priya",
        Some("by Mochi"),
    ),
];

pub const SUGGESTIONS: [(Member, &str, Mode); 4] = [
    (Member::Fixer, "Fix the newest crash", Mode::Fix),
    (Member::Guard, "Review PR #482", Mode::Review),
    (Member::Explainer, "Explain checkout to Priya", Mode::Ask),
    (
        Member::Keeper,
        "Every morning, update packages",
        Mode::Build,
    ),
];

/// A line in Builder's live feed.
pub enum Feed {
    Bot(Member, Vec<Span>),
    Terminal(&'static str, Vec<(&'static str, Option<&'static str>)>),
    You(&'static str),
    Next(Member, &'static str),
}

pub enum Span {
    Text(&'static str),
    Code(&'static str),
}

pub fn live_feed() -> Vec<Feed> {
    use Span::*;
    vec![
        Feed::Bot(
            Member::Builder,
            vec![Text("I read the refund policy and the 3 places that call it. The old rounding happens in"), Code("round_cents()"), Text(", so I'll keep that as it is.")],
        ),
        Feed::Bot(Member::Builder, vec![Text("Wrote"), Code("partial_refund()"), Text("and 9 tests for it."), Code("edit policy.py"), Code("+48 −3")]),
        Feed::Terminal(
            "Running tests now",
            vec![("test_partial_full ........", Some("ok")), ("test_partial_tax .........", Some("ok")), ("test_partial_rounding ....", None)],
        ),
        Feed::You("Make sure tax is refunded too, not just the item."),
        Feed::Bot(Member::Builder, vec![Text("Good call. Adding tax to the partial refund and a test for it.")]),
        Feed::Next(Member::Checker, "Checker is next: I'll refund half an order in the real admin page and record it."),
    ]
}

pub const PLAN: [(&str, Option<Member>, u8); 5] = [
    ("Read how refunds work today", None, 2),
    ("Write partial refunds, keep old rounding", None, 2),
    ("Run the tests", Some(Member::Builder), 1),
    (
        "Checker tries it in the admin page",
        Some(Member::Checker),
        0,
    ),
    (
        "Guard checks your rules, then asks you",
        Some(Member::Guard),
        0,
    ),
];

/// A chat in the Chats list.
pub struct ChatItem {
    pub who: Member,
    pub title: &'static str,
    pub last: &'static str,
    pub when: &'static str,
    pub group: bool,
}

pub const CHATS: [ChatItem; 7] = [
    ChatItem {
        who: Member::Explainer,
        title: "Explainer",
        last: "Product images got 10x bigger on Tuesday",
        when: "now",
        group: false,
    },
    ChatItem {
        who: Member::Guard,
        title: "Guard",
        last: "Line 40 touches login. I have a safer fix",
        when: "4m",
        group: false,
    },
    ChatItem {
        who: Member::Builder,
        title: "Builder",
        last: "Adding tax to the partial refund…",
        when: "18m",
        group: false,
    },
    ChatItem {
        who: Member::Fixer,
        title: "Fixer",
        last: "Fixed. 0 crashes since 11:20.",
        when: "2h",
        group: false,
    },
    ChatItem {
        who: Member::Keeper,
        title: "Keeper",
        last: "Merged 5 updates. Nothing broke.",
        when: "9h",
        group: false,
    },
    ChatItem {
        who: Member::Mochi,
        title: "Mochi",
        last: "Weekly notes are ready for Priya",
        when: "1d",
        group: false,
    },
    ChatItem {
        who: Member::Builder,
        title: "#checkout crew",
        last: "Group chat · 3 bots, 2 people",
        when: "",
        group: true,
    },
];
