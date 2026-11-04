#!/usr/bin/env bash
# Train Squad's own model on an Apple Silicon Mac, end to end.
#
#   ANTHROPIC_API_KEY=... ./training/mac.sh
#
# 1. writes about 3,600 labelled job sentences with a teacher model (about $5)
# 2. adds your own finished jobs, if Squad Code is installed (they stay here)
# 3. fine-tunes Qwen3.5-0.8B with LoRA using MLX (free, about 1 to 2 hours)
# 4. shrinks it to one ~0.5 GB GGUF file and serves it with llama-server
# 5. turns Smart Fin on and scores it against the rules
#
# Needs: Homebrew, Python 3.10+, git. Re-running skips steps already done.
set -euo pipefail

cd "$(dirname "$0")"
BASE="${BASE:-Qwen/Qwen3.5-0.8B}"
WORK="${WORK:-$PWD/work}"
SQUAD="${SQUAD:-/Applications/Squad Code.app/Contents/MacOS/SquadCode}"
mkdir -p "$WORK/data"

step() { printf '\n== %s\n' "$*"; }

step "Python packages"
[ -d "$WORK/venv" ] || python3 -m venv "$WORK/venv"
# shellcheck disable=SC1091
source "$WORK/venv/bin/activate"
pip install -q --upgrade pip
pip install -q mlx-lm anthropic

step "Training examples from the teacher model"
if [ ! -s "$WORK/data/synth.jsonl" ]; then
  : "${ANTHROPIC_API_KEY:?Set ANTHROPIC_API_KEY to write the training examples}"
  python make_data.py --out "$WORK/data/synth.jsonl" --per-kind 600
fi

step "Your own finished jobs"
if [ -x "$SQUAD" ]; then
  "$SQUAD" v2 brain export "$WORK/data/mine.jsonl" || true
else
  echo "Squad Code not found at $SQUAD; training on the generated examples only."
fi

step "Split into train and validation"
python - "$WORK/data" <<'PY'
import json, pathlib, random, sys
d = pathlib.Path(sys.argv[1])
rows = []
for name in ("synth.jsonl", "mine.jsonl"):
    p = d / name
    if p.exists():
        rows += [l for l in p.read_text().splitlines() if l.strip()]
random.Random(7).shuffle(rows)
cut = max(50, len(rows) // 20)
(d / "valid.jsonl").write_text("\n".join(rows[:cut]) + "\n")
(d / "train.jsonl").write_text("\n".join(rows[cut:]) + "\n")
print(f"{len(rows) - cut} to train on, {cut} to validate")
PY

step "Fine-tune with LoRA (MLX)"
if [ ! -d "$WORK/adapters" ]; then
  TRAIN_N=$(wc -l < "$WORK/data/train.jsonl")
  ITERS=$(( TRAIN_N * 2 / 8 ))   # about 2 epochs at batch size 8
  mlx_lm.lora --model "$BASE" --train --data "$WORK/data" \
    --batch-size 8 --iters "$ITERS" --learning-rate 1e-4 \
    --mask-prompt --adapter-path "$WORK/adapters"
fi

step "Merge the adapter into the model"
[ -d "$WORK/fused" ] || mlx_lm.fuse --model "$BASE" --adapter-path "$WORK/adapters" \
  --save-path "$WORK/fused" --dequantize

step "Shrink to one GGUF file"
command -v llama-server >/dev/null || brew install llama.cpp
if [ ! -s "$WORK/fin-q4_k_m.gguf" ]; then
  [ -d "$WORK/llama.cpp" ] || git clone -q --depth 1 https://github.com/ggml-org/llama.cpp "$WORK/llama.cpp"
  pip install -q -r "$WORK/llama.cpp/requirements/requirements-convert_hf_to_gguf.txt"
  python "$WORK/llama.cpp/convert_hf_to_gguf.py" "$WORK/fused" --outfile "$WORK/fin-f16.gguf" --outtype f16
  llama-quantize "$WORK/fin-f16.gguf" "$WORK/fin-q4_k_m.gguf" Q4_K_M
fi
ls -lh "$WORK/fin-q4_k_m.gguf"

step "Serve it on port 8088"
if ! curl -s -m 2 http://127.0.0.1:8088/v1/models >/dev/null; then
  nohup llama-server -m "$WORK/fin-q4_k_m.gguf" --port 8088 --alias fin \
    > "$WORK/llama-server.log" 2>&1 &
  for _ in $(seq 1 30); do
    curl -s -m 2 http://127.0.0.1:8088/v1/models >/dev/null && break
    sleep 1
  done
fi

step "Turn on Smart Fin and score it"
python - <<'PY'
import json, pathlib
p = pathlib.Path.home() / ".squadcode" / "settings.json"
s = json.loads(p.read_text()) if p.exists() else {}
s["brain_model"] = "fin"
s.setdefault("brain_url", "")
p.parent.mkdir(parents=True, exist_ok=True)
p.write_text(json.dumps(s, indent=2))
print(f"Smart Fin on in {p}")
PY
if [ -x "$SQUAD" ]; then
  "$SQUAD" v2 brain eval
else
  echo "Install Squad Code from this branch, then run: SquadCode v2 brain eval"
fi
echo
echo "Done. To turn Smart Fin off, set \"brain_model\": \"\" in ~/.squadcode/settings.json."
