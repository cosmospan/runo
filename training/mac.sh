#!/usr/bin/env bash
# Train Squad's own model on an Apple Silicon Mac, end to end.
#
#   ./training/mac.sh
#
# 1. builds about 1,900 labelled job sentences from the hand-written seeds
#    (with ANTHROPIC_API_KEY set, a teacher model adds 3,600 more for about $5)
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
SQUAD="${SQUAD:-$PWD/../target/release/SquadCode}"
mkdir -p "$WORK/data"

step() { printf '\n== %s\n' "$*"; }

step "Squad Code from this branch (for export and eval)"
if [ ! -x "$SQUAD" ] && command -v cargo >/dev/null; then
  (cd .. && cargo build --release -p runo)
fi

step "Python packages"
[ -d "$WORK/venv" ] || python3 -m venv "$WORK/venv"
# shellcheck disable=SC1091
source "$WORK/venv/bin/activate"
pip install -q --upgrade pip
pip install -q mlx-lm anthropic

step "Training examples"
if [ ! -s "$WORK/data/synth.jsonl" ]; then
  # From the hand-written seeds, no key needed. With ANTHROPIC_API_KEY set,
  # a teacher model writes 3,600 more varied ones too (about $5).
  python build_data.py --out "$WORK/data/synth.jsonl"
  if [ -n "${ANTHROPIC_API_KEY:-}" ]; then
    python make_data.py --out "$WORK/data/teacher.jsonl" --per-kind 600
    cat "$WORK/data/teacher.jsonl" >> "$WORK/data/synth.jsonl"
  fi
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
if [ ! -e "$WORK/adapters/.done" ]; then
  # A run that was stopped carries on from its last saved adapter.
  RESUME=()
  [ -s "$WORK/adapters/adapters.safetensors" ] && RESUME=(--resume-adapter-file "$WORK/adapters/adapters.safetensors")
  TRAIN_N=$(wc -l < "$WORK/data/train.jsonl")
  # Macs with 8 GB of memory train in smaller batches, with checkpointing.
  RAM_GB=$(( $(sysctl -n hw.memsize) / 1073741824 ))
  if [ "$RAM_GB" -le 8 ]; then BATCH="${BATCH:-2}"; else BATCH="${BATCH:-8}"; fi
  ITERS=$(( TRAIN_N * 2 / BATCH ))   # about 2 epochs
  mlx_lm.lora --model "$BASE" --train --data "$WORK/data" \
    --batch-size "$BATCH" --iters "$ITERS" --learning-rate 1e-4 \
    --grad-checkpoint --mask-prompt --adapter-path "$WORK/adapters" \
    --save-every 50 ${RESUME[@]+"${RESUME[@]}"}
  touch "$WORK/adapters/.done"
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
  rm -f "$WORK/fin-f16.gguf"
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
