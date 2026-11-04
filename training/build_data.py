"""Training examples from the hand-written seeds, with no API key.

seeds/<kind>.txt holds hand-written job sentences, one per line, for each kind
of job. Each seed is written out as is and as a few variants a real person
might type: lowercase, a typo, "can you…" in front, a note on the end. Any
sentence that is also in eval/route.jsonl is dropped, so the test set stays
unseen.

    python build_data.py --out work/data/synth.jsonl
"""

import argparse
import json
import pathlib
import random

from make_data import BOT_FOR, KINDS, route_system

HERE = pathlib.Path(__file__).parent

# Openings that ask for work, and ones that only ask. Ask and Review jobs
# change nothing, so they never get "fix"-sounding openings.
ASKS_FOR_WORK = ["can you ", "please ", "pls ", "hey, ", "I need you to ", "could you ", "quick one: "]
ASKS_ONLY = ["quick question: ", "hey, ", "", "can you tell me ", "I'm curious, "]
REVIEW_OPEN = ["can you ", "please ", "before I merge, ", "hey, ", "could you "]
ENDINGS = [" thanks", " asap", " when you get a chance", " in the web app", " on mobile", " in prod", "!", " ty"]


def typo(s: str, rng: random.Random) -> str:
    """Swaps two neighbouring letters in one longer word."""
    words = s.split(" ")
    long = [i for i, w in enumerate(words) if len(w) > 4 and w.isalpha()]
    if not long:
        return s
    i = rng.choice(long)
    w = words[i]
    j = rng.randrange(1, len(w) - 2)
    words[i] = w[:j] + w[j + 1] + w[j] + w[j + 2 :]
    return " ".join(words)


def variants(seed: str, kind: str, rng: random.Random) -> list[str]:
    opens = {"ask": ASKS_ONLY, "review": REVIEW_OPEN}.get(kind, ASKS_FOR_WORK)
    base = seed.rstrip(".")
    lower = base[0].lower() + base[1:] if base else base
    out = [
        seed,
        base.lower(),
        typo(base, rng),
        rng.choice(opens) + lower,
        base + rng.choice(ENDINGS),
        rng.choice(opens) + typo(lower, rng).lower() + rng.choice(ENDINGS),
    ]
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    held_out = {
        json.loads(l)["text"].strip().lower()
        for l in (HERE / "eval" / "route.jsonl").read_text().splitlines()
        if l.strip()
    }
    rng = random.Random(7)
    system = route_system()
    seen: set[str] = set()
    counts = {}
    out_path = pathlib.Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w") as out:
        for kind in KINDS:
            seeds = [l.strip() for l in (HERE / "seeds" / f"{kind}.txt").read_text().splitlines() if l.strip()]
            n = 0
            for seed in seeds:
                for text in variants(seed, kind, rng):
                    key = text.strip().lower()
                    if not key or key in held_out or key in seen:
                        continue
                    seen.add(key)
                    answer = {"mode": kind, "bot": BOT_FOR[kind], "sure": 0.95, "why": f"It reads as a {kind} job."}
                    out.write(json.dumps({"messages": [
                        {"role": "system", "content": system},
                        {"role": "user", "content": text},
                        {"role": "assistant", "content": json.dumps(answer)},
                    ]}) + "\n")
                    n += 1
            counts[kind] = n
    print(", ".join(f"{k}: {v}" for k, v in counts.items()), f"-> {sum(counts.values())} examples in {args.out}")


if __name__ == "__main__":
    main()
