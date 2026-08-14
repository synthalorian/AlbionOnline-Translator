#!/usr/bin/env bash
# dev-with-caps.sh — dev wrapper for Albion Online Translator
#
# cargo rebuilds replace the debug binary, and replacing a file wipes its
# file capabilities (setcap). Packet capture needs cap_net_raw + cap_net_admin,
# so every rebuild silently breaks the sniffer unless caps are re-applied.
#
# This wrapper runs `npm run tauri dev` while watching the target binary and
# re-applying setcap the moment it changes, so capture survives rebuilds.
#
# Usage: bash scripts/dev-with-caps.sh   (or npm run tauri:dev)
set -euo pipefail

BIN="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/target/debug/albion-translator"
CAPS="cap_net_raw,cap_net_admin=eip"

apply_caps() {
  if [[ -f "$BIN" ]]; then
    if ! getcap "$BIN" 2>/dev/null | grep -q "cap_net_admin,cap_net_raw=eip"; then
      sudo setcap "$CAPS" "$BIN"
      echo "[dev-with-caps] applied $CAPS to $(basename "$BIN")"
    fi
  fi
}

apply_caps

# Watch for rebuilds in the background; re-apply caps on every binary change.
(
  last_mtime=""
  while true; do
    if [[ -f "$BIN" ]]; then
      mtime="$(stat -c %Y "$BIN" 2>/dev/null || echo 0)"
      if [[ "$mtime" != "$last_mtime" ]]; then
        last_mtime="$mtime"
        apply_caps
      fi
    fi
    sleep 0.5
  done
) &
WATCHER_PID=$!
trap 'kill $WATCHER_PID 2>/dev/null || true' EXIT

npm run tauri dev
