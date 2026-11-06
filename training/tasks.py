"""Peek's jobs besides routing: the instructions each is asked with, and the
shape of its answer.

Each prompt must match the app's word for word (brain::model in the app);
both are checked against eval/<task>_system.txt, which `python tasks.py`
rewrites.

- title: the short name on a job's card, from what the person typed.
- recap: what a bot did, in a sentence or two, when its job ends.
- clarify: whether a job is too vague to start, and the one question to ask.
"""

import json
import pathlib

HERE = pathlib.Path(__file__).parent

TASKS = ["title", "recap", "clarify"]


def system(task: str) -> str:
    head = "You are Peek, the guide in Squad Code, where a crew of coding bots works on a person's project. "
    if task == "title":
        return head + (
            "The person typed a job for the crew. Write the title for the job's card: "
            "2 to 6 words, sentence case, starting with a verb, naming the specific thing. "
            "Keep the person's names for things. No period, no quotes.\n"
            'Answer with JSON only: {"title": text}.'
        )
    if task == "recap":
        return head + (
            "A bot's job just ended. From the job, what it changed and its last message, "
            "tell the person what happened in one or two short plain sentences, under 40 words: "
            "what changed or what it found, and what they need to do, if anything. "
            "Say it like a colleague would, with no greeting and no markdown. "
            "needs_you is true when the person has to decide, answer or fix something before the work can go on.\n"
            'Answer with JSON only: {"recap": text, "needs_you": true or false}.'
        )
    if task == "clarify":
        return head + (
            "The person typed a job for the crew. The bots can read the whole project, so a job is clear "
            "when a careful developer who reads the code could start on it, even if it is short or has typos. "
            "It is unclear only when they would have to guess what the person wants: no thing named, "
            "two very different readings, or it points at something only the person knows.\n"
            'Answer with JSON only: {"clear": true or false, "question": one short question to ask back, or ""}.'
        )
    raise ValueError(task)


SCHEMA = {
    "title": {
        "type": "object",
        "properties": {"title": {"type": "string"}},
        "required": ["title"],
    },
    "recap": {
        "type": "object",
        "properties": {"recap": {"type": "string"}, "needs_you": {"type": "boolean"}},
        "required": ["recap", "needs_you"],
    },
    "clarify": {
        "type": "object",
        "properties": {"clear": {"type": "boolean"}, "question": {"type": "string"}},
        "required": ["clear", "question"],
    },
}


def recap_input(job: dict) -> str:
    """What the recap task is shown about a job, in the order the app writes it
    (brain::model::recap_input). `job` has text, bot, status, changed (a list
    of "path (+a -r)" strings) and last (the bot's last message)."""
    changed = ", ".join(job.get("changed") or []) or "nothing"
    last = (job.get("last") or "").strip()
    if len(last) > 1500:
        last = last[:1500] + "…"
    return (
        f"Job: {job['text'].strip()}\n"
        f"Bot: {job['bot']}\n"
        f"Ended: {job['status']}\n"
        f"Changed: {changed}\n"
        f"Last message:\n{last}"
    )


def example(task: str, user: str, answer: dict) -> dict:
    return {
        "messages": [
            {"role": "system", "content": system(task)},
            {"role": "user", "content": user},
            {"role": "assistant", "content": json.dumps(answer)},
        ]
    }


if __name__ == "__main__":
    for t in TASKS:
        (HERE / "eval" / f"{t}_system.txt").write_text(system(t))
    print("Wrote eval/<task>_system.txt for", ", ".join(TASKS))
