#!/usr/bin/env bash
# Publish a Peek model so every Squad Code that has Smart Peek on can get it.
#
#   ./training/publish.sh work/peek-q4_k_m.gguf "Honest confidence; 120/120 on the messy set"
#
# Each run uploads the model as a new file, peek-q4_k_m-v<N>.gguf, then points
# latest.json at it. Squad reads latest.json, so a person who turns Smart Peek
# on gets the newest version, and `squad brain update` (or "Check for a newer
# model" in Settings) moves an installed one up. Older files stay in the repo,
# so going back is pointing latest.json at an earlier one.
#
# Needs: the Hugging Face CLI (`pip install -U huggingface_hub`), signed in
# with a write token (`hf auth login`), and the repo created once on
# huggingface.co. Score the model first: ship only if it beats the last one.
set -euo pipefail

REPO="${REPO:-cosmospan/squad-peek}"
HF="${HF:-hf}"
model="${1:?usage: publish.sh <model.gguf> [notes]}"
notes="${2:-}"
here="$(cd "$(dirname "$0")" && pwd)"

[ "$(head -c 4 "$model")" = "GGUF" ] || { echo "$model isn't a GGUF file"; exit 1; }
command -v "$HF" >/dev/null || { echo "No $HF here: pip install -U huggingface_hub"; exit 1; }

current=$(curl -fsSL "https://huggingface.co/$REPO/resolve/main/latest.json" 2>/dev/null \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])' 2>/dev/null || echo 0)
version=$((current + 1))
file="peek-q4_k_m-v$version.gguf"
bytes=$(wc -c < "$model" | tr -d ' ')
if command -v shasum >/dev/null; then
  sha=$(shasum -a 256 "$model" | cut -d' ' -f1)
else
  sha=$(sha256sum "$model" | cut -d' ' -f1)
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
python3 - "$tmp/latest.json" "$version" "$file" "$bytes" "$sha" "$notes" <<'PY'
import json, sys
out, version, file, size, sha, notes = sys.argv[1:]
json.dump({"version": int(version), "file": file, "bytes": int(size), "sha256": sha, "notes": notes},
          open(out, "w"), indent=2)
PY

echo "Publishing $model to $REPO as $file (v$version, $bytes bytes, sha256 $sha)"
# The model first, then latest.json, so latest.json never names a missing file.
"$HF" upload "$REPO" "$model" "$file" --commit-message "Peek v$version"
"$HF" upload "$REPO" "$tmp/latest.json" latest.json --commit-message "Latest is Peek v$version"
if [ -f "$here/hf/README.md" ]; then
  "$HF" upload "$REPO" "$here/hf/README.md" README.md --commit-message "Model card for v$version"
fi
echo "Done. Check it: SquadCode v2 brain update"
