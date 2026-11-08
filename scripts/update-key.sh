#!/bin/sh
# Makes the key that signs the SHA256SUMS of each Squad Code release, once. The private key
# goes to the repository's UPDATE_SIGNING_KEY secret (the Release workflow signs with it)
# and stays in ~/.squadcode-update-key.pem; its public half goes into
# native/squad-app/src/update.pub, built into Squad Code, whose updater refuses a download
# whose checksums aren't signed with it.
set -eu
key="$HOME/.squadcode-update-key.pem"
# A key made when the app was called Runo is moved, not made again.
if [ ! -e "$key" ] && [ -e "$HOME/.runo-update-key.pem" ]; then
  mv "$HOME/.runo-update-key.pem" "$key"
fi
root="$(cd "$(dirname "$0")/.." && pwd)"
if [ -e "$key" ]; then
  echo "$key already exists; using it."
else
  (umask 077 && openssl genpkey -algorithm ed25519 -out "$key")
  echo "Made $key. Keep it safe and offline: it is the only way to publish updates Squad Code trusts."
fi
openssl pkey -in "$key" -pubout -outform DER | tail -c 32 | base64 > "$root/native/squad-app/src/update.pub"
echo "Wrote the public key to native/squad-app/src/update.pub."
if command -v gh >/dev/null 2>&1; then
  gh secret set UPDATE_SIGNING_KEY < "$key"
  echo "Set the UPDATE_SIGNING_KEY secret."
else
  echo "Install the GitHub CLI and run: gh secret set UPDATE_SIGNING_KEY < $key"
fi
echo "Commit update.pub; Squad Code builds from then on only accept signed releases."
