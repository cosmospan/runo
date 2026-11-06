"""Teacher-written examples for Peek's other jobs, and the test sets for all of them.

    # once: the held-out test sets (eval/*.jsonl), written carefully
    python make_tasks.py tests
    # then the training examples (data/<task>.jsonl), which never repeat a test
    python make_tasks.py train --title 2500 --clarify 1500 --recap 1500 --route 2400

Needs ANTHROPIC_API_KEY. Try small counts first: it prints what the run cost.

- route: more routing sentences, aimed at what the first batch lacked: fast
  typing full of typos, several sentences, pasted logs, two asks in one.
- title: the card title for job sentences already in data/teacher.jsonl.
- clarify: vague jobs with the question to ask back, mixed with clear ones.
- recap: whole finished jobs (what was asked, what changed, the bot's last
  message) and the one or two sentences Peek says about them.
"""

import argparse
import json
import pathlib
import random

from make_data import BOT_FOR, KINDS, PRICE_IN, PRICE_OUT, SETTINGS, TEACHER, route_system
from tasks import example, recap_input

HERE = pathlib.Path(__file__).parent
EVAL = HERE / "eval"
DATA = HERE / "data"
BOTS = sorted(set(BOT_FOR.values()))

USAGE = {"in": 0, "out": 0}


def obj(props: dict) -> dict:
    return {
        "type": "object",
        "properties": props,
        "required": list(props),
        "additionalProperties": False,
    }


def items(props: dict) -> dict:
    return obj({"items": {"type": "array", "items": obj(props)}})


S = {"type": "string"}
B = {"type": "boolean"}
N = {"type": "number"}


def teach(client, prompt: str, schema: dict, effort: str) -> list[dict]:
    # Streamed: the SDK won't wait on a 32K-token answer without it.
    with client.messages.stream(
        model=TEACHER,
        max_tokens=32000,
        output_config={"effort": effort, "format": {"type": "json_schema", "schema": schema}},
        messages=[{"role": "user", "content": prompt}],
    ) as stream:
        msg = stream.get_final_message()
    USAGE["in"] += msg.usage.input_tokens
    USAGE["out"] += msg.usage.output_tokens
    if msg.stop_reason != "end_turn":
        print(f"  the teacher stopped ({msg.stop_reason}); skipping this batch")
        return []
    return json.loads("".join(b.text for b in msg.content if b.type == "text"))["items"]


KIND_LINES = "\n".join(f"- {k}: {v}" for k, v in KINDS.items())

STYLES = [
    "typed fast on a phone, lots of typos and missing letters ('fix teh logn buton', 'wher is the confg loded')",
    "two or three sentences of context before the actual ask",
    "a pasted error, stack trace or log line, with a few words around it",
    "one word or a fragment ('flaky checkout test', 'dark mode??')",
    "written by someone whose first language isn't English",
    "polite and long-winded",
    "two asks in one message, where one is clearly the main job",
    "an angry or frustrated tone",
    "a question that sounds like a request, or a request that sounds like a question",
    "naming files, functions or tickets ('PROJ-412', 'src/auth/session.ts', 'useCart()')",
]


# ---- route ----------------------------------------------------------------


def route_prompt(n: int, rng: random.Random, careful: bool) -> str:
    styles = rng.sample(STYLES, 4)
    return (
        "Squad Code is an app where a person types a job for a crew of coding bots. Each job is one of these kinds:\n"
        f"{KIND_LINES}\n\n"
        f"Write {n} different messages people really send, set in {rng.choice(SETTINGS)}, {rng.choice(SETTINGS)} "
        f"and {rng.choice(SETTINGS)}. Mix all six kinds about evenly. Lean on these styles: "
        + "; ".join(styles)
        + ". Include ones where the obvious keyword points the wrong way (a 'fix' that is really a question, "
        "a 'test' that is really a review, 'clean up' that is really a bug). Never name the bots.\n\n"
        "Label each as a careful reader would: mode is the kind it really is; sure is 0 to 1 "
        "(above 0.9 only with no reasonable other reading, 0.4 to 0.6 when two kinds fit); "
        "why is one short plain sentence under 15 words naming what decides it."
        + (
            "\n\nThese are test questions, so be exact: think about each label, and leave out any message "
            "where two kinds fit equally well."
            if careful
            else ""
        )
    )


ROUTE_ITEM = {"text": S, "mode": {"type": "string", "enum": list(KINDS)}, "sure": N, "why": S}


def route_rows(client, n, rng, careful, avoid):
    out = []
    while len(out) < n:
        for j in teach(client, route_prompt(min(40, n - len(out)), rng, careful), items(ROUTE_ITEM),
                       "high" if careful else "low"):
            t = j["text"].strip()
            if t and t.lower() not in avoid:
                avoid.add(t.lower())
                out.append(j)
        print(f"  route: {len(out)}/{n}")
    return out[:n]


def route_example(j: dict) -> dict:
    mode = j["mode"]
    sure = round(min(1.0, max(0.0, float(j["sure"]))), 2)
    answer = {"mode": mode, "bot": BOT_FOR[mode], "sure": sure, "why": j["why"].strip()}
    return {"messages": [
        {"role": "system", "content": route_system()},
        {"role": "user", "content": j["text"].strip()},
        {"role": "assistant", "content": json.dumps(answer)},
    ]}


# ---- title ----------------------------------------------------------------

TITLE_RULES = (
    "Write the title for each job's card: 2 to 6 words, sentence case, starting with a verb "
    "(Fix, Add, Explain, Review, Test, Rename, Upgrade...), naming the specific thing, keeping the person's "
    "own names for things (files, screens, tickets). Fix their typos in the title. No period, no quotes. "
    "Good: 'Fix Safari export button', 'Explain the auth middleware', 'Add dark mode to settings', "
    "'Review the payments PR for risks'. Bad: 'Bug fix', 'User request about login', 'Fix the bug.'"
)


def title_rows(client, texts: list[str], careful: bool):
    out = []
    for i in range(0, len(texts), 60):
        batch = texts[i : i + 60]
        listing = "\n".join(f"{k}. {t}" for k, t in enumerate(batch))
        got = teach(
            client,
            "A person typed these jobs for a crew of coding bots.\n" + TITLE_RULES
            + "\nAnswer with one item per job, with its number.\n\n" + listing,
            items({"n": {"type": "integer"}, "title": S}),
            "medium" if careful else "low",
        )
        for g in got:
            if 0 <= g["n"] < len(batch) and g["title"].strip():
                out.append({"text": batch[g["n"]], "title": g["title"].strip().rstrip(".")})
        print(f"  title: {len(out)}/{len(texts)}")
    return out


# ---- clarify --------------------------------------------------------------

CLARIFY_RULES = (
    "The bots can read the whole project, so a job is clear when a careful developer who reads the code could "
    "start on it, even if it's short, casual or full of typos ('fix the flaky checkout test', 'why is ci red', "
    "'add dark mode' are clear). It is unclear only when they'd have to guess what the person wants: nothing "
    "named ('fix it', 'make it better', 'do the thing'), two very different readings ('update the users page' "
    "with no hint what to change), or something only the person knows ('do what we discussed', 'the bug Sam "
    "found'). For unclear ones, write the one short question that would unblock the work, in plain words, "
    "offering choices when that helps ('Which page should the export button be on?'). For clear ones the "
    "question is \"\"."
)

CLARIFY_ITEM = {"text": S, "clear": B, "question": S}


def clarify_rows(client, n, rng, careful, avoid):
    """About 40% unclear; the clear ones are deliberately close calls."""
    out = []
    while len(out) < n:
        k = min(40, n - len(out))
        prompt = (
            "Squad Code is an app where a person types a job for a crew of coding bots, set in "
            f"{rng.choice(SETTINGS)} or {rng.choice(SETTINGS)}.\n{CLARIFY_RULES}\n\n"
            f"Write {k} jobs people really type: about 40% unclear, 60% clear but short or casual enough that "
            "a careless reader might ask anyway. Vary styles: " + "; ".join(rng.sample(STYLES, 3))
            + ". Label each honestly."
            + ("\n\nThese are test questions: leave out any job where reasonable developers would disagree."
               if careful else "")
        )
        for j in teach(client, prompt, items(CLARIFY_ITEM), "high" if careful else "low"):
            t = j["text"].strip()
            if t and t.lower() not in avoid:
                avoid.add(t.lower())
                out.append({"text": t, "clear": bool(j["clear"]),
                            "question": "" if j["clear"] else j["question"].strip()})
        print(f"  clarify: {len(out)}/{n}")
    return out[:n]


# ---- recap ----------------------------------------------------------------

RECAP_ITEM = {
    "text": S,
    "bot": {"type": "string", "enum": BOTS},
    "status": {"type": "string", "enum": ["done", "failed", "stopped", "waiting for your approval"]},
    "changed": {"type": "array", "items": S},
    "last": S,
    "recap": S,
    "needs_you": B,
}


def recap_rows(client, n, rng, careful, avoid):
    out = []
    bots = ", ".join(f"{b} ({k})" for k, b in BOT_FOR.items())
    while len(out) < n:
        k = min(12, n - len(out))
        prompt = (
            "Squad Code is an app where a crew of coding bots does jobs on a person's project: " + bots + ".\n"
            f"Invent {k} jobs that just ended, set in {rng.choice(SETTINGS)} or {rng.choice(SETTINGS)}. For each:\n"
            "- text: what the person typed (any style, typos welcome).\n"
            "- bot: the bot that did it.\n"
            "- status: mostly 'waiting for your approval' (it changed files) or 'done' (questions, reviews, "
            "test runs); some 'failed' or 'stopped'.\n"
            "- changed: the files it changed as 'path (+added -removed)', empty when it changed none.\n"
            "- last: the bot's last message, as a coding agent really writes it: 40 to 250 words, often markdown "
            "with headings, bullet lists, file paths and code spans; sometimes it hit an error, needs a decision, "
            "found nothing wrong, or did only part of the job.\n"
            "- recap: what Peek tells the person, one or two short plain sentences under 40 words, like a "
            "colleague: what changed or what it found, and what they need to do if anything. No greeting, no "
            "markdown, no 'The bot'. Name the real things (files, screens, numbers). Never claim more than the "
            "last message says.\n"
            "- needs_you: true when the person has to decide, answer or fix something before the work can go "
            "on (a failure they must fix, a question the bot asked, a choice between options). Plain approval "
            "of finished changes is false.\n"
            "Make about a third of them need the person."
            + ("\n\nThese are test questions: make each recap exactly right." if careful else "")
        )
        for j in teach(client, prompt, items(RECAP_ITEM), "high" if careful else "low"):
            if j["text"].strip().lower() in avoid or not j["recap"].strip():
                continue
            avoid.add(j["text"].strip().lower())
            out.append(j)
        print(f"  recap: {len(out)}/{n}")
    return out[:n]


def recap_example(j):
    return example("recap", recap_input(j), {"recap": j["recap"].strip(), "needs_you": bool(j["needs_you"])})


# ---- files ----------------------------------------------------------------


def read_jsonl(p: pathlib.Path) -> list[dict]:
    return [json.loads(l) for l in p.read_text().splitlines() if l.strip()] if p.exists() else []


def write_jsonl(p: pathlib.Path, rows: list[dict]) -> None:
    p.write_text("".join(json.dumps(r) + "\n" for r in rows))
    print(f"Wrote {len(rows)} to {p.relative_to(HERE)}")


def held_out() -> set[str]:
    seen = set()
    for name in ("route.jsonl", "route_hard.jsonl", "route_wild.jsonl", "title.jsonl", "clarify.jsonl", "recap.jsonl"):
        for r in read_jsonl(EVAL / name):
            seen.add(r["text"].strip().lower())
    return seen


def teacher_texts() -> list[str]:
    return [r["messages"][1]["content"] for r in read_jsonl(DATA / "teacher.jsonl")]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("what", choices=["tests", "train"])
    ap.add_argument("--route", type=int, default=0)
    ap.add_argument("--title", type=int, default=0)
    ap.add_argument("--clarify", type=int, default=0)
    ap.add_argument("--recap", type=int, default=0)
    args = ap.parse_args()

    import anthropic

    client = anthropic.Anthropic()
    rng = random.Random(11 if args.what == "tests" else 7)

    if args.what == "tests":
        avoid = held_out() | {t.lower() for t in teacher_texts()}
        write_jsonl(EVAL / "route_wild.jsonl", [
            {"text": j["text"].strip(), "mode": j["mode"]}
            for j in route_rows(client, args.route or 300, rng, True, avoid)
        ])
        # Titles for a mix of the old test sentences and new wild ones.
        texts = [r["text"] for r in read_jsonl(EVAL / "route_hard.jsonl")]
        texts += [r["text"] for r in read_jsonl(EVAL / "route_wild.jsonl")]
        rng.shuffle(texts)
        write_jsonl(EVAL / "title.jsonl", title_rows(client, texts[: args.title or 80], True))
        write_jsonl(EVAL / "clarify.jsonl", clarify_rows(client, args.clarify or 100, rng, True, avoid))
        write_jsonl(EVAL / "recap.jsonl", [
            {k: j[k] for k in RECAP_ITEM} for j in recap_rows(client, args.recap or 48, rng, True, avoid)
        ])
    else:
        avoid = held_out()
        if args.route:
            rows = route_rows(client, args.route, rng, False, avoid | {t.lower() for t in teacher_texts()})
            write_jsonl(DATA / "route2.jsonl", [route_example(j) for j in rows])
        if args.title:
            pool = [t for t in teacher_texts() + [r["messages"][1]["content"] for r in read_jsonl(DATA / "route2.jsonl")]
                    if t.strip().lower() not in avoid]
            rng.shuffle(pool)
            rows = title_rows(client, pool[: args.title], False)
            write_jsonl(DATA / "title.jsonl", [example("title", r["text"], {"title": r["title"]}) for r in rows])
        if args.clarify:
            rows = clarify_rows(client, args.clarify, rng, False, set(avoid))
            write_jsonl(DATA / "clarify.jsonl", [
                example("clarify", r["text"], {"clear": r["clear"], "question": r["question"]}) for r in rows
            ])
        if args.recap:
            rows = recap_rows(client, args.recap, rng, False, set(avoid))
            write_jsonl(DATA / "recap.jsonl", [recap_example(j) for j in rows])

    cost = USAGE["in"] / 1e6 * PRICE_IN + USAGE["out"] / 1e6 * PRICE_OUT
    print(f"Tokens: {USAGE['in']:,} in, {USAGE['out']:,} out. About ${cost:.2f}.")


if __name__ == "__main__":
    main()
