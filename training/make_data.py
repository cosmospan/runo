"""Write labelled job sentences for training Squad's router, from a teacher model.

For each kind of job, the teacher writes varied sentences a person might type
to a crew of coding bots: short and long, typos, pasted errors, questions. It
labels each one honestly: the kind of job it really is (which may not be the
kind it was asked for), how sure a careful reader would be, and a short reason
a person would find useful. Genuinely ambiguous sentences get a low "sure", so
the model learns when to say it isn't sure.

Each becomes one chat example in the same format `SquadCode v2 brain export`
writes, so both can be trained on together. Sentences that appear in either
test set (eval/route.jsonl, eval/route_hard.jsonl) are dropped, so the tests
stay unseen.

    ANTHROPIC_API_KEY=... python make_data.py --out teacher.jsonl --per-kind 600

Try `--per-kind 10` first: it prints what the run cost.
"""

import argparse
import json
import pathlib
import random

HERE = pathlib.Path(__file__).parent
TEACHER = "claude-opus-5-5"
# Dollars per million tokens, for the cost line at the end.
PRICE_IN, PRICE_OUT = 4.0, 20.0

KINDS = {
    "ask": "a question about the code; nothing should change",
    "build": "something new to build: a feature, page, endpoint or command",
    "fix": "a bug or error to fix, sometimes with a pasted error or stack trace",
    "review": "a request to look over changes or code for risks; nothing should change",
    "test": "running, writing or extending tests, or trying a flow for real",
    "tidy": "refactoring, renaming, dependency upgrades, lint, dead code, flaky tests",
}

# The built-in crew, as `route_system` lists it in the app.
CREW = [
    ("Builder", "Writes the code", "build"),
    ("Checker", "Tries it for real", "test"),
    ("Guard", "Keeps your rules", "review"),
    ("Fixer", "Hunts the bug", "fix"),
    ("Explainer", "Plain words", "ask"),
    ("Keeper", "Tidies up", "tidy"),
    ("Shipper", "Opens and merges PRs", "ask"),
]
BOT_FOR = {"build": "Builder", "test": "Checker", "review": "Guard", "fix": "Fixer", "ask": "Explainer", "tidy": "Keeper"}


def route_system() -> str:
    """Must match brain::model::route_system in the app, word for word.
    Both are checked against eval/route_system.txt."""
    s = (
        "You are Peek, the guide in Squad Code. A person typed a job for the crew of coding bots. "
        "Pick the kind of job and the bot that should take it. "
        "Kinds: ask (a question, nothing changes), build (something new), fix (a bug or error), "
        "review (look for risks, nothing changes), test (run or write tests), tidy (refactor, deps, cleanup).\n"
        "The crew:\n"
    )
    for name, role, kind in CREW:
        s += f"- {name}: {role} ({kind})\n"
    s += 'Answer with JSON only: {"mode": kind, "bot": name, "sure": 0 to 1, "why": one short sentence}.'
    return s


# Rotated through the batches so the teacher doesn't repeat itself.
SETTINGS = [
    "a React and TypeScript web app", "a Python data pipeline", "a Rust CLI", "a Go backend service",
    "an iOS app in Swift", "an Android app in Kotlin", "a Unity or Godot game", "a Django or Rails monolith",
    "a Next.js marketing site", "a SQL-heavy analytics repo", "a browser extension", "an ML training repo",
    "infrastructure as code (Terraform, Docker, CI)", "an open-source library", "a Node.js API with Postgres",
]

# The kinds that are easy to mix up, so each batch gets a few of them.
TRICKY = {
    "review": "looking over code or changes for risks WITHOUT changing anything: 'check', 'spot check', "
    "'is this safe', 'sanity check', 'don't change it, just tell me', critiques of a draft",
    "test": "actually trying the thing for real, or running and writing tests: 'click through it', "
    "'try signing up', 'run the suite and report', 'does it still work on a small screen'",
    "ask": "questions about the code where nothing should change, including 'who calls X' and "
    "'what would it take to', even when they mention bugs or features",
}

SCHEMA = {
    "type": "object",
    "properties": {
        "jobs": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "text": {"type": "string"},
                    "mode": {"type": "string", "enum": list(KINDS)},
                    "sure": {"type": "number"},
                    "why": {"type": "string"},
                },
                "required": ["text", "mode", "sure", "why"],
                "additionalProperties": False,
            },
        }
    },
    "required": ["jobs"],
    "additionalProperties": False,
}


def ask_teacher(client, kind: str, n: int, rng: random.Random, usage: dict) -> list[dict]:
    kinds = "\n".join(f"- {k}: {v}" for k, v in KINDS.items())
    tricky = TRICKY.get(kind, "")
    prompt = (
        f"Squad Code is an app where a person types a job for a crew of coding bots. Each job is one of these kinds:\n{kinds}\n\n"
        f"Write {n} different things a person might type when the job is mostly {kind}: {KINDS[kind]}. "
        f"Set them in {rng.choice(SETTINGS)} or {rng.choice(SETTINGS)}. Vary length (3 to 60 words), tone and phrasing; "
        "include some with typos, some all lowercase, some with a pasted error or file path, some that never use the "
        "obvious keyword. Never mention the bots by name.\n"
        + (f"Make about a third of them the easily confused kind: {tricky}.\n" if tricky else "")
        + "About one in ten should be genuinely ambiguous between two kinds.\n\n"
        "Then label each sentence honestly, as a careful reader would:\n"
        "- mode: the kind it really is, even if that isn't the kind you were asked for.\n"
        "- sure: 0 to 1. Above 0.9 only when there's no reasonable other reading; 0.4 to 0.6 when two kinds fit.\n"
        "- why: one short plain sentence (under 15 words) naming what in the text decides it, "
        "e.g. 'It names a crash after logout, so it is a bug to hunt.'"
    )
    msg = client.messages.create(
        model=TEACHER,
        max_tokens=16000,
        output_config={"effort": "low", "format": {"type": "json_schema", "schema": SCHEMA}},
        messages=[{"role": "user", "content": prompt}],
    )
    usage["in"] += msg.usage.input_tokens
    usage["out"] += msg.usage.output_tokens
    if msg.stop_reason != "end_turn":
        print(f"  {kind}: the teacher stopped ({msg.stop_reason}); skipping this batch")
        return []
    text = "".join(b.text for b in msg.content if b.type == "text")
    jobs = json.loads(text)["jobs"]
    return [
        j
        for j in jobs
        if isinstance(j.get("text"), str) and j["text"].strip() and j.get("mode") in KINDS
    ]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--per-kind", type=int, default=600)
    ap.add_argument("--batch", type=int, default=50)
    args = ap.parse_args()

    held_out = {
        json.loads(l)["text"].strip().lower()
        for f in ("route.jsonl", "route_hard.jsonl")
        for l in (HERE / "eval" / f).read_text().splitlines()
        if l.strip()
    }
    import anthropic

    client = anthropic.Anthropic()
    rng = random.Random(7)
    system = route_system()
    seen: set[str] = set()
    usage = {"in": 0, "out": 0}
    written = 0
    counts = {k: 0 for k in KINDS}
    with open(args.out, "w") as out:
        for kind in KINDS:
            got, tries = 0, 0
            while got < args.per_kind and tries < args.per_kind // max(1, args.batch) * 3 + 3:
                tries += 1
                for j in ask_teacher(client, kind, min(args.batch, args.per_kind - got), rng, usage):
                    text = j["text"].strip()
                    key = text.lower()
                    if key in held_out or key in seen:
                        continue
                    seen.add(key)
                    mode = j["mode"]
                    sure = round(min(1.0, max(0.0, float(j["sure"]))), 2)
                    answer = {"mode": mode, "bot": BOT_FOR[mode], "sure": sure, "why": j["why"].strip()}
                    out.write(json.dumps({"messages": [
                        {"role": "system", "content": system},
                        {"role": "user", "content": text},
                        {"role": "assistant", "content": json.dumps(answer)},
                    ]}) + "\n")
                    got += 1
                    written += 1
                    counts[mode] += 1
            print(f"asked for {kind}: {got}")
    cost = usage["in"] / 1e6 * PRICE_IN + usage["out"] / 1e6 * PRICE_OUT
    print(f"Wrote {written} examples to {args.out}. Labels: " + ", ".join(f"{k} {v}" for k, v in counts.items()))
    print(f"Tokens: {usage['in']:,} in, {usage['out']:,} out. About ${cost:.2f}.")


if __name__ == "__main__":
    main()
