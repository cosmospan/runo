"""Write labelled job sentences for training Squad's router, from a teacher model.

For each kind of job, the teacher writes varied sentences a person might type
to a crew of coding bots: short and long, typos, pasted errors, questions.
Each becomes one chat example in the same format `SquadCode v2 brain export`
writes, so both can be trained on together.

Sentences that appear in eval/route.jsonl are dropped, so the test set stays
unseen.

    python make_data.py --out synth.jsonl --per-kind 600
"""

import argparse
import json
import pathlib

HERE = pathlib.Path(__file__).parent

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
        "You are Fin, the guide in Squad Code. A person typed a job for the crew of coding bots. "
        "Pick the kind of job and the bot that should take it. "
        "Kinds: ask (a question, nothing changes), build (something new), fix (a bug or error), "
        "review (look for risks, nothing changes), test (run or write tests), tidy (refactor, deps, cleanup).\n"
        "The crew:\n"
    )
    for name, role, kind in CREW:
        s += f"- {name}: {role} ({kind})\n"
    s += 'Answer with JSON only: {"mode": kind, "bot": name, "sure": 0 to 1, "why": one short sentence}.'
    return s


def ask_teacher(client, kind: str, n: int) -> list[str]:
    prompt = (
        f"Write {n} different things a person might type to a crew of coding bots when the job is "
        f"{KINDS[kind]}. Vary length (3 to 60 words), tone, projects (web, mobile, CLI, data, games) "
        "and languages (TypeScript, Python, Rust, Go, Swift, SQL). Include some with typos and some "
        "lowercase. Never mention the bots by name. Return a JSON array of strings only."
    )
    msg = client.messages.create(
        model="claude-opus-5-5",
        max_tokens=16000,
        output_config={"effort": "low"},
        messages=[{"role": "user", "content": prompt}],
    )
    text = "".join(b.text for b in msg.content if b.type == "text")
    start, end = text.find("["), text.rfind("]")
    return [s for s in json.loads(text[start : end + 1]) if isinstance(s, str) and s.strip()]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--per-kind", type=int, default=600)
    ap.add_argument("--batch", type=int, default=100)
    args = ap.parse_args()

    held_out = {
        json.loads(l)["text"].strip().lower()
        for l in (HERE / "eval" / "route.jsonl").read_text().splitlines()
        if l.strip()
    }
    import anthropic

    client = anthropic.Anthropic()
    system = route_system()
    seen: set[str] = set()
    written = 0
    with open(args.out, "w") as out:
        for kind in KINDS:
            got = 0
            while got < args.per_kind:
                for text in ask_teacher(client, kind, min(args.batch, args.per_kind - got)):
                    key = text.strip().lower()
                    if key in held_out or key in seen:
                        continue
                    seen.add(key)
                    answer = {"mode": kind, "bot": BOT_FOR[kind], "sure": 0.95, "why": f"It reads as a {kind} job."}
                    out.write(json.dumps({"messages": [
                        {"role": "system", "content": system},
                        {"role": "user", "content": text},
                        {"role": "assistant", "content": json.dumps(answer)},
                    ]}) + "\n")
                    got += 1
                    written += 1
            print(f"{kind}: {got}")
    print(f"Wrote {written} examples to {args.out}")


if __name__ == "__main__":
    main()
