"""Train Peek's tiny router: a word and letter-pattern classifier, no neural net.

It reads the same chat examples as train_lora.py, learns which words and
four-letter pieces point at which kind of job, and writes the weights the app
builds in (native/runo-app/src/v2/brain/tiny.tsv, about 140 KB). It trains in
seconds on any computer, with nothing to install, and answers in microseconds
in the app, with no download and no server.

    python build_data.py --out work/data/synth.jsonl
    python train_tiny.py --data work/data/synth.jsonl

The features must match brain::tiny::features in the app, character for
character; the app's tests check the two agree on the fixed test sets.
"""

import argparse
import json
import math
import pathlib
import random
from collections import defaultdict

HERE = pathlib.Path(__file__).parent
KINDS = ["ask", "build", "fix", "review", "test", "tidy"]
OUT = HERE.parent / "native" / "runo-app" / "src" / "v2" / "brain" / "tiny.tsv"


def word_char(ch: str) -> bool:
    return ("a" <= ch <= "z") or ("0" <= ch <= "9") or ch in "_'"


def tokens(text: str) -> list[str]:
    """Runs of a-z, 0-9, _ and '; any other character that isn't a space
    is a token by itself."""
    out, cur = [], ""
    for ch in text:
        if word_char(ch):
            cur += ch
            continue
        if cur:
            out.append(cur)
            cur = ""
        if not ch.isspace():
            out.append(ch)
    if cur:
        out.append(cur)
    return out


def features(text: str) -> list[str]:
    t = text.lower()
    w = tokens(t)
    f = ["w:" + x for x in w]
    f += ["b:" + a + "_" + b for a, b in zip(w, w[1:])]
    if w:
        f.append("first:" + w[0])
    s = " " + t + " "
    f += ["c:" + s[i : i + 4] for i in range(len(s) - 3)]
    if t.strip().endswith("?"):
        f.append("q?")
    return f


def load(path: str) -> list[tuple[str, str]]:
    rows = []
    for line in open(path):
        if not line.strip():
            continue
        r = json.loads(line)
        if "messages" in r:
            rows.append((r["messages"][1]["content"], json.loads(r["messages"][2]["content"])["mode"]))
        else:
            rows.append((r["text"], r["mode"]))
    return rows


def train(rows, epochs=15, lr=0.2, l2=1e-5):
    """Multinomial logistic regression by plain SGD."""
    w = defaultdict(lambda: [0.0] * len(KINDS))
    data = [(features(t), KINDS.index(m)) for t, m in rows]
    random.Random(7).shuffle(data)
    rng = random.Random(7)
    for _ in range(epochs):
        rng.shuffle(data)
        for f, y in data:
            z = [sum(w[x][k] for x in f) for k in range(len(KINDS))]
            top = max(z)
            e = [math.exp(v - top) for v in z]
            total = sum(e)
            scale = 1 / math.sqrt(len(f))
            for x in f:
                wx = w[x]
                for k in range(len(KINDS)):
                    g = e[k] / total - (1 if k == y else 0)
                    wx[k] -= lr * (g * scale + l2 * wx[k])
    return w


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", nargs="+", required=True)
    ap.add_argument("--out", default=str(OUT))
    ap.add_argument("--keep", type=float, default=0.05, help="drop features whose weights all stay under this")
    args = ap.parse_args()

    rows = [r for p in args.data for r in load(p)]
    w = train(rows)
    # Whole hundredths are plenty, and keep the file small.
    kept = {}
    for x, ws in w.items():
        if max(abs(v) for v in ws) > args.keep:
            q = [round(v * 100) for v in ws]
            if any(q) and "\t" not in x and "\n" not in x:
                kept[x] = q
    lines = ["#kinds\t" + " ".join(KINDS)]
    lines += [x + "\t" + " ".join(map(str, q)) for x, q in sorted(kept.items())]
    pathlib.Path(args.out).write_text("\n".join(lines) + "\n")
    print(f"{len(rows)} examples, {len(kept)} features -> {args.out}")

    def pick(text):
        f = features(text)
        z = [sum(kept[x][k] for x in f if x in kept) for k in range(len(KINDS))]
        return KINDS[max(range(len(KINDS)), key=lambda k: z[k])]

    for name in ("route.jsonl", "route_hard.jsonl"):
        cases = load(str(HERE / "eval" / name))
        right = sum(pick(t) == m for t, m in cases)
        print(f"  {name}: {right}/{len(cases)}")


if __name__ == "__main__":
    main()
