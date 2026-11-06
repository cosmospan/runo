---
base_model: Qwen/Qwen3.5-0.8B
license: apache-2.0
tags: [gguf, squad-code, router]
---

# Peek: Squad Code's job router

Peek reads the job a person types into Squad Code and picks the kind of job
(ask, build, fix, review, test, tidy) and the bot that should take it, with how
sure it is and a short reason. It's Qwen3.5-0.8B fine-tuned with LoRA, then
quantized to one 4-bit GGUF file (about 0.5 GB).

- Trained on 1,859 examples from hand-written seeds plus 3,600 examples
  written and labelled by a teacher model.
- Fixed test sets: 53/53 on `route.jsonl`, 120/120 on `route_hard.jsonl`.
  The hand-written rules score 51/53 and 70/120.
- Runs on a laptop: about 250 MB of memory and about 0.4 s an answer on an M1.

Squad Code downloads the file named in `latest.json` when you turn the model
on, and offers newer versions as they're published here. To run it yourself:

```
llama-server -m peek-q4_k_m-v<N>.gguf --port 8088 --alias peek \
  --chat-template-kwargs '{"enable_thinking":false}' -c 4096 -np 1
```

Turn thinking off: Peek was trained to answer straight away.

Training code and data: https://github.com/cosmospan/squadcode (`training/`).
