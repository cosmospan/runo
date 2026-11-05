//! A made-up store for looking at every screen without running anything:
//! `RUNO_V2_DEMO=1`. Jobs in every state, a race, a pull request and a crew
//! bigger than the sidebar shows. It is never saved.

use super::data::{Made, Member, Mode};
use super::goal::{Goal, State, Step};
use super::store::{now, Changed, Folder, Item, Job, Said, Status, Store};
use crate::proof::CheckRun;

fn tool(label: &str, kind: &str, state: u8) -> Item {
    Item::Tool {
        id: label.into(),
        label: label.into(),
        kind: kind.into(),
        state,
    }
}

fn change(path: &str, status: &str, added: usize, removed: usize) -> Changed {
    Changed {
        path: path.into(),
        status: status.into(),
        added,
        removed,
    }
}

fn check(command: &str, ok: bool, secs: u64) -> CheckRun {
    CheckRun {
        command: command.into(),
        ok,
        secs,
        tail: if ok {
            String::new()
        } else {
            "test checkout::apple_pay_total ... FAILED\nexpected 1999, got 2000".into()
        },
        ..Default::default()
    }
}

pub fn seed() -> Store {
    let t = now();
    let folder = Folder {
        name: "shop".into(),
        path: std::env::temp_dir().to_string_lossy().into_owned(),
        ship: String::new(),
    };
    let base = |id: u64, bot: Member, mode: Mode, title: &str, ago: u64, secs: u64| Job {
        id,
        bot,
        mode,
        title: title.into(),
        folder: folder.clone(),
        engine: "Claude Code \u{b7} Sonnet 4.5".into(),
        agent: "claude".into(),
        started: t - ago,
        ended: Some(t - ago + secs),
        branch: format!("squad/demo-{id}"),
        base: "main".into(),
        seen: true,
        summary: format!("{secs}s \u{b7} 9 turns \u{b7} $0.{:02}", 10 + id * 3),
        ..Default::default()
    };
    let words = "Added Apple Pay as a payment option on the checkout page. It shows only on devices that support it, and the total matches the card form. Try it in Safari on a Mac with a card in Wallet.";

    let mut jobs = Vec::new();

    // Shipped a while ago.
    let mut j = base(
        1,
        Member::Fixer,
        Mode::Fix,
        "Fix the coupon field on mobile",
        86_000,
        140,
    );
    j.status = Status::Shipped;
    j.shipped = "Merged into main as 3f2a1c9.".into();
    j.changes = vec![change("src/coupon.tsx", "M", 12, 4)];
    j.feed = vec![Item::Text(
        "The coupon field was hidden by the keyboard. It now scrolls into view.".into(),
    )];
    jobs.push(j);

    // Stopped with a problem.
    let mut j = base(
        2,
        Member::Keeper,
        Mode::Tidy,
        "Update the payment SDK",
        7_200,
        35,
    );
    j.status = Status::Failed;
    j.seen = false;
    j.feed = vec![
        tool("Read package.json", "read", 1),
        tool("Run npm install", "bash", 2),
        Item::Error("You've hit your usage limit for Claude Code. It resets at 4pm.".into()),
    ];
    jobs.push(j);

    // A pull request Shipper is watching.
    let mut j = base(
        3,
        Member::Builder,
        Mode::Build,
        "Add order history page",
        5_400,
        610,
    );
    j.status = Status::Shipping;
    j.pr = "https://github.com/hyperhale/shop/pull/482".into();
    j.pr_number = 482;
    j.pr_state = "OPEN".into();
    j.pr_line = "Checks running: 3 of 5 done".into();
    j.changes = vec![
        change("src/orders/page.tsx", "A", 148, 0),
        change("src/orders/api.ts", "A", 61, 0),
        change("src/nav.tsx", "M", 3, 1),
    ];
    j.checks = vec![check("npm test", true, 41), check("npm run lint", true, 9)];
    j.feed = vec![
        Item::Text("I added an order history page under Account.".into()),
        Item::By(Member::Shipper, "Approved. Opening a pull request.".into()),
        Item::By(Member::Shipper, "Squashed 4 commits into one".into()),
        Item::By(
            Member::Shipper,
            "Pushed squad/add-order-history-page".into(),
        ),
    ];
    jobs.push(j);

    // Waiting for review.
    let mut j = base(
        4,
        Member::Fixer,
        Mode::Fix,
        "Stop double charges on retry",
        1_800,
        260,
    );
    j.status = Status::Review;
    j.seen = false;
    j.changes = vec![
        change("src/checkout/pay.ts", "M", 22, 9),
        change("src/checkout/pay.test.ts", "M", 34, 0),
    ];
    j.checks = vec![check("npm test", true, 44), check("npm run lint", true, 8)];
    j.plan = vec![
        ("Find where retries charge again".into(), 2),
        ("Send an idempotency key".into(), 2),
        ("Add a test".into(), 2),
    ];
    j.feed = vec![
        Item::Note("Working on its own branch, squad/stop-double-charges-retry, so your checkout stays as it is.".into()),
        tool("Read src/checkout/pay.ts", "read", 1),
        tool("Edit src/checkout/pay.ts", "edit", 1),
        tool("Run npm test", "bash", 1),
        Item::Text("A retry after a timeout sent the charge again. Each charge now carries a key, so the payment service ignores the repeat. I added a test that retries twice and checks there's one charge.".into()),
    ];
    jobs.push(j);

    // A race on three engines.
    let engines = [
        (
            "claude",
            "Claude Code \u{b7} Sonnet 4.5",
            240,
            Status::Review,
            true,
            "0.42",
        ),
        (
            "codex",
            "Codex \u{b7} gpt-5",
            180,
            Status::Review,
            false,
            "0.19",
        ),
        ("gemini", "Gemini CLI", 0, Status::Running, true, "0"),
    ];
    for (k, (agent, engine, secs, status, ok, cost)) in engines.into_iter().enumerate() {
        let id = 5 + k as u64;
        let mut j = base(
            id,
            Member::Builder,
            Mode::Build,
            "Add Apple Pay to checkout",
            900,
            secs,
        );
        j.race = 5;
        j.pick = agent.into();
        j.agent = agent.into();
        j.engine = engine.into();
        j.status = status;
        j.seen = status != Status::Review;
        if status == Status::Running {
            j.ended = None;
            j.summary.clear();
            j.plan = vec![
                ("Read the checkout form".into(), 2),
                ("Add the Apple Pay button".into(), 1),
                ("Wire up the payment sheet".into(), 0),
                ("Test it".into(), 0),
            ];
            j.feed = vec![
                tool("Read src/checkout/form.tsx", "read", 1),
                tool("Edit src/checkout/form.tsx", "edit", 0),
            ];
        } else {
            j.summary = format!("{secs}s \u{b7} 12 turns \u{b7} ${cost}");
            j.changes = if k == 0 {
                vec![
                    change("src/checkout/form.tsx", "M", 64, 8),
                    change("src/checkout/applePay.ts", "A", 92, 0),
                    change("src/checkout/applePay.test.ts", "A", 40, 0),
                ]
            } else {
                vec![
                    change("src/checkout/form.tsx", "M", 51, 3),
                    change("src/checkout/applePay.ts", "A", 70, 0),
                ]
            };
            j.checks = vec![check("npm test", ok, 46), check("npm run lint", true, 9)];
            j.feed = vec![Item::Text(words.into())];
        }
        jobs.push(j);
    }

    // Its base moved and two files clash.
    let mut j = base(
        9,
        Member::Builder,
        Mode::Build,
        "Add gift cards",
        3_000,
        420,
    );
    j.status = Status::Review;
    j.seen = false;
    j.changes = vec![
        change("src/checkout/total.ts", "M", 18, 6),
        change("src/cart/summary.tsx", "M", 25, 2),
    ];
    j.checks = vec![check("npm test", true, 40)];
    j.conflict = vec![
        "src/checkout/total.ts".into(),
        "src/cart/summary.tsx".into(),
    ];
    j.feed = vec![
        Item::Text("Gift cards can be added at checkout and come off the total.".into()),
        Item::Note("main moved on and src/checkout/total.ts, src/cart/summary.tsx now clash, so nothing was merged.".into()),
    ];
    jobs.push(j);

    // Stopped by you halfway.
    let mut j = base(
        10,
        Member::Keeper,
        Mode::Tidy,
        "Remove the old analytics code",
        4_000,
        95,
    );
    j.status = Status::Stopped;
    j.plan = vec![
        ("Find the old tracker".into(), 2),
        ("Remove its calls".into(), 1),
        ("Run the tests".into(), 0),
    ];
    j.feed = vec![
        tool("Search for track(", "search", 1),
        tool("Edit src/app.tsx", "edit", 1),
    ];
    jobs.push(j);

    // Working now: the last job, so RUNO_V2_PLACE=live opens it.
    let mut j = base(
        8,
        Member::Guard,
        Mode::Review,
        "Look for leaked keys before launch",
        120,
        0,
    );
    j.status = Status::Running;
    j.ended = None;
    j.summary.clear();
    j.branch.clear();
    j.plan = vec![
        ("Search the code for keys".into(), 2),
        ("Check the git history".into(), 1),
        ("Write up what to rotate".into(), 0),
    ];
    j.feed = vec![
        tool("Search for sk_live_", "search", 1),
        tool("Read .env.example", "read", 1),
        Item::Text("No live keys in the code so far. Checking the history next.".into()),
        tool("Run git log -S sk_live_", "bash", 0),
    ];
    jobs.push(j);

    let made = vec![
        Made {
            id: 1,
            name: "Pixel".into(),
            role: "Polishes the UI".into(),
            like: Member::Builder,
            look: [3, 0, 0, 0],
            outfit: "beret.none.pencil.heart".into(),
        },
        Made {
            id: 2,
            name: "Scribe".into(),
            role: "Writes the docs".into(),
            like: Member::Explainer,
            look: [4, 0, 0, 0],
            outfit: "headset.clipboard.pencil.tag".into(),
        },
        Made {
            id: 3,
            name: "Atlas".into(),
            role: "Keeps the database tidy".into(),
            like: Member::Keeper,
            look: [6, 0, 0, 0],
            outfit: "bandana.none.broom.gear".into(),
        },
    ];
    super::data::set_made(&made);
    let mut chats = std::collections::HashMap::new();
    chats.insert(
        Member::Explainer,
        vec![
            Said::mine("How does checkout talk to the payment service?".into()),
            Said::note("Explainer is reading the code."),
        ],
    );
    for j in jobs.iter_mut().filter(|j| j.id == 1 || j.id == 3) {
        j.goal = 21;
    }
    Store {
        folders: vec![folder.clone()],
        current: 0,
        jobs,
        chats,
        next_id: 11,
        made,
        removed: Vec::new(),
        set_up: true,
        steps: Vec::new(),
        guide_hidden: true,
        offered: Vec::new(),
        pinned: vec![Member::Made(2)],
        goals: vec![
            Goal {
                id: 20,
                text: "Let customers save their cart and come back to it".into(),
                folder: folder.clone(),
                state: State::Ready,
                started: t - 60,
                steps: vec![
                    Step {
                        mode: Mode::Build,
                        title: "Store carts for signed-in customers".into(),
                        ..Default::default()
                    },
                    Step {
                        mode: Mode::Build,
                        title: "Restore the saved cart on sign-in".into(),
                        ..Default::default()
                    },
                    Step {
                        mode: Mode::Test,
                        title: "Cover saving and restoring carts with tests".into(),
                        ..Default::default()
                    },
                    Step {
                        mode: Mode::Review,
                        title: "Check carts can't be read by other customers".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            Goal {
                id: 21,
                text: "Order history for customers".into(),
                folder: folder.clone(),
                state: State::Running,
                started: t - 90_000,
                steps: vec![
                    Step {
                        mode: Mode::Fix,
                        title: "Fix the coupon field on mobile".into(),
                        job: 1,
                        ..Default::default()
                    },
                    Step {
                        mode: Mode::Build,
                        title: "Add order history page".into(),
                        job: 3,
                        ..Default::default()
                    },
                    Step {
                        mode: Mode::Test,
                        title: "Test the order history page".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        ],
        demo: true,
    }
}
