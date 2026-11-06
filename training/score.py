"""Score Peek on every test set, the way the app asks it.

    # a model served by llama-server (on a Mac)
    python score.py --url http://127.0.0.1:8088/v1
    # a merged model straight after training (on Colab)
    python score.py --hf work/peek-lora/merged

Routing is scored on the three test sets; the other jobs on their own sets:
- title: the share that follow the rules (2 to 6 words, a capital first, no
  period or quotes) and share a key word with the teacher's title.
- clarify: the share where clear/unclear matches the label.
- recap: the share that follow the rules (one or two sentences, under 45
  words, no markdown) and get needs_you right.
"""

import argparse
import json
import pathlib
import re
import sys

from make_data import route_system
from tasks import recap_input, system

HERE = pathlib.Path(__file__).parent
EVAL = HERE / "eval"
STOP = set("a an the to of for in on and or is it this that my our with be at by from as".split())


def answer(reply: str) -> dict:
    try:
        return json.loads(reply[reply.find("{") : reply.rfind("}") + 1])
    except Exception:
        return {}


def served(url: str):
    import urllib.request

    def ask(sys_prompt: str, user: str, max_tokens: int) -> str:
        body = {
            "model": "peek",
            "messages": [{"role": "system", "content": sys_prompt}, {"role": "user", "content": user}],
            "temperature": 0.1,
            "max_tokens": max_tokens,
            "chat_template_kwargs": {"enable_thinking": False},
        }
        req = urllib.request.Request(url.rstrip("/") + "/chat/completions", json.dumps(body).encode(),
                                     {"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=60) as r:
            return json.load(r)["choices"][0]["message"]["content"]

    return ask


def local(path: str):
    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer

    tok = AutoTokenizer.from_pretrained(path)
    gpu = torch.cuda.is_available()
    model = AutoModelForCausalLM.from_pretrained(path, dtype=torch.float16 if gpu else torch.float32)
    model = (model.cuda() if gpu else model).eval()

    def ask(sys_prompt: str, user: str, max_tokens: int) -> str:
        msgs = [{"role": "system", "content": sys_prompt}, {"role": "user", "content": user}]
        ids = tok.apply_chat_template(msgs, add_generation_prompt=True, return_tensors="pt", return_dict=True,
                                      enable_thinking=False)["input_ids"].to(model.device)
        out = model.generate(ids, max_new_tokens=max_tokens, do_sample=False)
        return tok.decode(out[0][ids.shape[1] :], skip_special_tokens=True)

    return ask


def rows(name: str) -> list[dict]:
    p = EVAL / name
    return [json.loads(l) for l in p.read_text().splitlines() if l.strip()] if p.exists() else []


def key_words(s: str) -> set[str]:
    return {w for w in re.findall(r"[a-z0-9]+", s.lower()) if w not in STOP and len(w) > 2}


def title_ok(t: str) -> bool:
    w = t.split()
    return 2 <= len(w) <= 6 and t[:1].isupper() and not t.endswith(".") and not any(q in t for q in "\"`")


def recap_ok(r: str) -> bool:
    sentences = [s for s in re.split(r"(?<=[.!?])\s+", r.strip()) if s]
    return 0 < len(r.split()) <= 45 and len(sentences) <= 2 and not re.search(r"[*#`]|^\s*-", r)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--url")
    ap.add_argument("--hf")
    ap.add_argument("--show", type=int, default=5, help="misses to print per test")
    args = ap.parse_args()
    if not (args.url or args.hf):
        sys.exit("give --url or --hf")
    ask = served(args.url) if args.url else local(args.hf)
    scores = {}

    rs = route_system()
    for name in ("route.jsonl", "route_hard.jsonl", "route_wild.jsonl"):
        cases = rows(name)
        if not cases:
            continue
        right, missed = 0, []
        for c in cases:
            got = answer(ask(rs, c["text"], 80)).get("mode", "?")
            right += got == c["mode"]
            if got != c["mode"]:
                missed.append((c["text"][:80], c["mode"], got))
        scores[name] = f"{right}/{len(cases)}"
        print(f"{name}: {right}/{len(cases)}")
        for m in missed[: args.show]:
            print("   missed:", m)

    cases = rows("title.jsonl")
    if cases:
        ok = same = 0
        for c in cases:
            t = str(answer(ask(system("title"), c["text"], 40)).get("title", "")).strip()
            good = title_ok(t)
            ok += good
            hit = good and bool(key_words(t) & key_words(c["title"]))
            same += hit
            if not hit and args.show:
                print(f"   title: {c['text'][:60]!r} -> {t!r} (teacher: {c['title']!r})")
        scores["title"] = f"{same}/{len(cases)} (rules {ok})"
        print(f"title: {same}/{len(cases)} good, {ok} follow the rules")

    cases = rows("clarify.jsonl")
    if cases:
        right = 0
        for c in cases:
            a = answer(ask(system("clarify"), c["text"], 60))
            hit = a.get("clear") is c["clear"]
            right += hit
            if not hit and args.show:
                print(f"   clarify: {c['text'][:70]!r} want clear={c['clear']} got {a}")
        scores["clarify"] = f"{right}/{len(cases)}"
        print(f"clarify: {right}/{len(cases)}")

    cases = rows("recap.jsonl")
    if cases:
        ok = needs = 0
        for c in cases:
            a = answer(ask(system("recap"), recap_input(c), 120))
            r = str(a.get("recap", ""))
            good = recap_ok(r)
            ok += good
            needs += a.get("needs_you") is c["needs_you"]
            if not good and args.show:
                print(f"   recap: {r!r}")
        scores["recap"] = f"{ok}/{len(cases)} well written, needs-you {needs}/{len(cases)}"
        print(f"recap: {ok}/{len(cases)} follow the rules, needs_you right {needs}/{len(cases)}")

    print("\n" + json.dumps(scores, indent=1))


if __name__ == "__main__":
    main()
