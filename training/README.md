# Training Squad's own model

Squad's intelligence layer (`native/runo-app/src/v2/brain/`) picks the kind of
job and the bot in three steps, each used only when the one before isn't sure:

1. **The tiny classifier**, built into the app: about 15,000 word and
   letter-pattern weights (370 KB) from `train_tiny.py`. Microseconds, no
   download, always on.
2. **The keyword rules.**
3. **Smart Peek**, only if the person turns it on: Squad's own small model
   (Qwen3.5-0.8B fine-tuned here, 505 MB), downloaded once and run by a
   `llama-server` that Squad starts on demand and stops when idle. Its answer
   is used only if it checks out.

Scores (`SquadCode v2 brain eval` and `eval hard`):

| | Plain (53) | Messy (120) |
| --- | --- | --- |
| Rules | 51 | 70 |
| Tiny classifier, then rules (seeds only) | 53 | 105 |
| Tiny classifier, then rules (seeds + teacher) | 53 | 117 |
| Peek 0.8B (seeds only) | 53 | 117 |
| Peek 0.8B (seeds + teacher, Smart Peek) | 53 | 120 |

## Peek's other jobs

The same model also does three small jobs when Smart Peek is on, each with its
own instructions (`tasks.py`, matched word for word by the app's
`brain/tasks.rs`) and its own test set:

- **title**: a 2 to 6 word title for a job's card, from what the person typed.
- **recap**: a sentence or two on what a bot did when its job ends, and
  whether the person needs to act.
- **clarify**: whether a job is too vague to start, and the one question to
  ask back.

`make_tasks.py tests` writes the held-out test sets (`eval/route_wild.jsonl`,
300 messy routing sentences; `eval/title.jsonl`, `eval/clarify.jsonl`,
`eval/recap.jsonl`) and `make_tasks.py train` the teacher's training examples
into `data/`. `score.py` scores a model on all of them, served
(`--url http://127.0.0.1:8088/v1`) or straight after training (`--hf`).

## The pieces

| File | What it does |
| --- | --- |
| `eval/route.jsonl`, `eval/route_hard.jsonl` | The fixed test sets: 53 plain sentences and 120 messy ones. Every model is scored on both before it ships, and neither is ever trained on. |
| `seeds/*.txt`, `build_data.py` | 380 hand-written job sentences, and the script that turns them into about 1,900 labelled examples with no API key. |
| `make_data.py` | Writes training examples from a teacher model: varied job sentences, each labelled with its real kind, an honest confidence and a reason. |
| `data/teacher.jsonl` | The teacher's 3,600 examples (cost $4.83), so training needs no key. |
| `train_tiny.py` | Trains the tiny classifier in seconds, with no packages, and writes its weights into the app. |
| `train_lora.py` | Fine-tunes the base model with LoRA on the examples. |
| `publish.sh`, `hf/README.md` | Publishes a new Peek to Hugging Face, with its model card. |
| `requirements.txt` | Python packages for the two scripts. |

## Free, in the cloud

Open `colab.ipynb` in Google Colab, pick the free T4 GPU, and Run all. It
trains, scores the model on the fixed test set and hands you the GGUF file.

## On a Mac, in one go

`./training/mac.sh` does every step below, with no API key, with MLX: data,
training, merging, the GGUF file, serving it on port 8088, turning Smart Peek on
and scoring it. Re-running skips what's done; everything lands in `training/work/`.

## Steps

1. **Your own jobs (optional, stays on your computer).**
   `SquadCode v2 brain export mine.jsonl` writes your finished jobs as
   examples, in the same prompt format the app asks the model in.
2. **More examples from a teacher.**
   `python make_data.py --out synth.jsonl --per-kind 600` (needs `ANTHROPIC_API_KEY`).
   It refuses to write any sentence that is also in `eval/route.jsonl`.
3. **Train.**
   `python train_lora.py --data synth.jsonl mine.jsonl --out peek-lora`
4. **Shrink to one file.** With llama.cpp:
   `python convert_hf_to_gguf.py peek-lora/merged --outfile peek.gguf` then
   `llama-quantize peek.gguf peek-q4_k_m.gguf Q4_K_M` (about 0.5 GB for a 0.8B model).
5. **Serve and score.**
   `llama-server -m peek-q4_k_m.gguf --port 8088 --alias peek --chat-template-kwargs '{"enable_thinking":false}' -c 4096 -np 1`
   (without the first flag Qwen3.5 thinks first and runs out of room; `-c 4096 -np 1`
   keeps it at about 250 MB instead of 3.3 GB), set Smart Peek's model to
   `peek` in settings (`brain_model`), then `SquadCode v2 brain eval`.
   Ship only if it beats the rules (51 of 53 today) and the last model.

## Rules for the data

- The prompt the model is trained on is the one it's asked with
  (`brain::model::route_system`). Change one, retrain.
- Facts never come from the model: it picks a kind of job and a bot, and the
  app checks both. Training data has no facts about a person's project in it.
- A person's own jobs go into training only if they choose to share the file.

## Publishing a new Peek

Peek is published on Hugging Face at `cosmospan/peek`. Each release is
a new file (`peek-q4_k_m-v<N>.gguf`) and an updated `latest.json` naming it,
with its size and SHA-256. Squad reads `latest.json`, downloads the file to a
temporary name, checks the size and checksum, and only then swaps it in, so a
bad upload never replaces a working model.

    hf auth login                                   # once, with a write token
    ./training/publish.sh work/peek-q4_k_m.gguf "What's better"

Ship only a model that beats the last one on both test sets. People with Smart
Peek on get it from `squad brain update` or "Check for a newer model" in
Settings; people who turn it on later get it straight away. To roll back, point
`latest.json` at an earlier file with a higher version number.
