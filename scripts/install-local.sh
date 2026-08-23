#!/usr/bin/env bash
# install-local.sh — install the release binary to ~/.local/bin with capture caps.
#
# Two rules from dev-with-caps.sh apply here too:
#   1. Copying a binary WIPES its file capabilities (setcap) — so caps must be
#      re-applied after every install/update.
#   2. The kernel IGNORES file caps when the binary is owned/writable by the
#      exec'ing user — so the installed copy must be chown'd root + 755.
#
# Usage: bash scripts/install-local.sh
set -euo pipefail

SRC="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/target/release/albion-translator"
DEST="$HOME/.local/bin/albion-translator"
CAPS="cap_net_raw,cap_net_admin=eip"

if [[ ! -f "$SRC" ]]; then
  echo "Release binary not found — build first: npm run tauri build -- --no-bundle" >&2
  exit 1
fi

mkdir -p "$HOME/.local/bin"
# cp needs sudo: after the first install the binary is root-owned (required
# for file caps), so a plain user cp can no longer overwrite it.
sudo cp "$SRC" "$DEST"
sudo chown root:root "$DEST"
sudo chmod 755 "$DEST"
sudo setcap "$CAPS" "$DEST"

echo "Installed $DEST"
getcap "$DEST"
echo "NOTE: if the app is running, restart it — caps only apply at exec time."
