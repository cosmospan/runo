# Training Squad's own model

Squad's intelligence layer (`native/runo-app/src/v2/brain/`) works with rules
alone. When Smart Fin is on and the rules aren't sure, it asks a small model on the person's
computer first, and uses the answer only if it checks out. This folder trains
that model: a small open base model fine-tuned on Squad's own jobs.

Nothing here runs on its own, and nothing has been trained yet. See the
proposal for the base model, the data, where training runs and what it costs.

## The pieces

| File | What it does |
| --- | --- |
| `eval/route.jsonl` | The fixed test set. Every model is scored on it before it ships, and it is never trained on. |
| `seeds/*.txt`, `build_data.py` | 380 hand-written job sentences, and the script that turns them into about 1,900 labelled examples with no API key. |
| `make_data.py` | Writes training examples: varied job sentences for each kind of job, labelled, from a teacher model. |
| `train_lora.py` | Fine-tunes the base model with LoRA on the examples. |
| `requirements.txt` | Python packages for the two scripts. |

## Free, in the cloud

Open `colab.ipynb` in Google Colab, pick the free T4 GPU, and Run all. It
trains, scores the model on the fixed test set and hands you the GGUF file.

## On a Mac, in one go

`./training/mac.sh` does every step below, with no API key, with MLX: data,
training, merging, the GGUF file, serving it on port 8088, turning Smart Fin on
and scoring it. Re-running skips what's done; everything lands in `training/work/`.

## Steps

1. **Your own jobs (optional, stays on your computer).**
   `SquadCode v2 brain export mine.jsonl` writes your finished jobs as
   examples, in the same prompt format the app asks the model in.
2. **More examples from a teacher.**
   `python make_data.py --out synth.jsonl --per-kind 600` (needs `ANTHROPIC_API_KEY`).
   It refuses to write any sentence that is also in `eval/route.jsonl`.
3. **Train.**
   `python train_lora.py --data synth.jsonl mine.jsonl --out fin-lora`
4. **Shrink to one file.** With llama.cpp:
   `python convert_hf_to_gguf.py fin-lora/merged --outfile fin.gguf` then
   `llama-quantize fin.gguf fin-q4_k_m.gguf Q4_K_M` (about 0.5 GB for a 0.8B model).
5. **Serve and score.**
   `llama-server -m fin-q4_k_m.gguf --port 8088 --alias fin --chat-template-kwargs '{"enable_thinking":false}' -c 4096 -np 1`
   (without the first flag Qwen3.5 thinks first and runs out of room; `-c 4096 -np 1`
   keeps it at about 250 MB instead of 3.3 GB), set Smart Fin's model to
   `fin` in settings (`brain_model`), then `SquadCode v2 brain eval`.
   Ship only if it beats the rules (51 of 53 today) and the last model.

## Rules for the data

- The prompt the model is trained on is the one it's asked with
  (`brain::model::route_system`). Change one, retrain.
- Facts never come from the model: it picks a kind of job and a bot, and the
  app checks both. Training data has no facts about a person's project in it.
- A person's own jobs go into training only if they choose to share the file.
