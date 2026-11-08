#!/bin/sh
# Makes the key that signs the agent manifests Squad Code downloads, once. The private key
# goes to the repository's REGISTRY_SIGNING_KEY secret (the Agent registry workflow signs
# with it) and stays in ~/.squadcode-registry-key.pem; its public half goes into
# native/squad-app/src/agents.pub, built into Squad Code, which checks every download with it.
set -eu
key="$HOME/.squadcode-registry-key.pem"
# A key made when the app was called Runo is moved, not made again.
if [ ! -e "$key" ] && [ -e "$HOME/.runo-registry-key.pem" ]; then
  mv "$HOME/.runo-registry-key.pem" "$key"
fi
root="$(cd "$(dirname "$0")/.." && pwd)"
if [ -e "$key" ]; then
  echo "$key already exists; using it."
else
  (umask 077 && openssl genpkey -algorithm ed25519 -out "$key")
  echo "Made $key. Keep it safe: it is the only way to publish manifests Squad Code trusts."
fi
openssl pkey -in "$key" -pubout -outform DER | tail -c 32 | base64 > "$root/native/squad-app/src/agents.pub"
echo "Wrote the public key to native/squad-app/src/agents.pub."
if command -v gh >/dev/null 2>&1; then
  gh secret set REGISTRY_SIGNING_KEY < "$key"
  echo "Set the REGISTRY_SIGNING_KEY secret."
else
  echo "Install the GitHub CLI and run: gh secret set REGISTRY_SIGNING_KEY < $key"
fi
echo "Commit agents.pub; the next Squad Code release checks downloads with it."
