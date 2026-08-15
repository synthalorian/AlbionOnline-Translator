#!/usr/bin/env bash
# dev-with-caps.sh — dev wrapper for Albion Online Translator
#
# cargo rebuilds replace the debug binary, and replacing a file wipes its
# file capabilities (setcap). Packet capture needs cap_net_raw + cap_net_admin,
# so every rebuild silently breaks the sniffer unless caps are re-applied.
#
# setcap alone is not enough: the kernel ignores file caps when the binary is
# owned/writable by the exec'ing user, so we also chown the binary to root
# after every rebuild. A root-owned 755 binary keeps working for the user but
# actually receives its capabilities.
#
# This wrapper runs `npm run tauri dev` while watching the target binary and
# re-applying chown+setcap the moment it changes, so capture survives rebuilds.
#
# Usage: bash scripts/dev-with-caps.sh   (or npm run tauri:dev)
set -euo pipefail

BIN="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/target/debug/albion-translator"
CAPS="cap_net_raw,cap_net_admin=eip"

apply_caps() {
  if [[ -f "$BIN" ]]; then
    if ! getcap "$BIN" 2>/dev/null | grep -q "cap_net_admin,cap_net_raw=eip" \
       || [[ "$(stat -c %U "$BIN")" != "root" ]]; then
      sudo chown root:root "$BIN"
      sudo chmod 755 "$BIN"
      sudo setcap "$CAPS" "$BIN"
      echo "[dev-with-caps] chown root + applied $CAPS to $(basename "$BIN")"
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

# Run the dev server, but pre-warm vite's transform cache before the app
# window opens. On cold start vite can serve a .svelte style module as raw
# source (starts with `<script>`), which postcss chokes on as "Unknown word
# onMount" — a red overlay in the webview on every first boot.
#
# The style sub-module (?svelte&type=style) can only be served once the PARENT
# SFC has been transformed, because vite-plugin-svelte caches the extracted
# <style> block keyed by the parent module. Curling the style URL alone 500s
# forever — which is why the old warm loop never succeeded. Warm the parents
# first, then the style modules.
#
# The `--runner` flag replaces cargo with run-with-caps.sh, which builds,
# applies setcap caps synchronously, THEN spawns the binary — closing the
# rebuild-wipes-caps race that a polling watcher can never win.
RUNNER="$(cd "$(dirname "$0")" && pwd)/run-with-caps.sh"
npm run tauri dev -- --runner "$RUNNER" &
NPM_PID=$!

warm_url() {
  curl -fsS -o /dev/null "http://localhost:1420/$1" 2>/dev/null
}

for _ in $(seq 1 120); do
  if warm_url "src/routes/translate-iframe/+page.svelte" \
     && warm_url "src/routes/+page.svelte" \
     && warm_url "src/routes/translate-iframe/+page.svelte?svelte&type=style&lang.css" \
     && warm_url "src/routes/+page.svelte?svelte&type=style&lang.css"; then
    echo "[dev-with-caps] vite transform cache warmed"
    break
  fi
  sleep 0.5
done
wait $NPM_PID