#!/usr/bin/env bash
# run-with-caps.sh — cargo replacement for `tauri dev --runner`.
#
# tauri dev invokes the runner INSTEAD of cargo, with cargo-style args:
#   <runner> run --no-default-features --color always -- <app args...>
#
# WHY it exists: cargo replaces the debug binary on every rebuild, wiping
# its setcap caps. A freshly spawned app then has no CAP_NET_RAW /
# CAP_NET_ADMIN and the sniffer fails with EPERM. Polling watchers lose
# that race (they tick after tauri has already spawned the binary).
# This runner closes it with an ordering guarantee: it builds with real
# cargo, applies caps synchronously, THEN execs the binary itself.
set -euo pipefail

ARGS=("$@")
APP_ARGS=()
SEP_INDEX=-1

for i in "${!ARGS[@]}"; do
  if [[ "${ARGS[$i]}" == "--" ]]; then
    SEP_INDEX=$i
    break
  fi
done

if [[ $SEP_INDEX -ge 0 ]]; then
  APP_ARGS=("${ARGS[@]:SEP_INDEX+1}")
  BUILD_ARGS=("${ARGS[@]:0:SEP_INDEX}")
else
  BUILD_ARGS=("${ARGS[@]}")
fi

for i in "${!BUILD_ARGS[@]}"; do
  if [[ "${BUILD_ARGS[$i]}" == "run" ]]; then
    BUILD_ARGS[$i]="build"
  fi
done

cargo "${BUILD_ARGS[@]}"

BIN=""
for candidate in "src-tauri/target/debug/albion-translator" "target/debug/albion-translator"; do
  if [[ -f "$candidate" ]]; then
    BIN="$candidate"
    break
  fi
done

if [[ -z "$BIN" ]]; then
  echo "[run-with-caps] ERROR: built binary not found" >&2
  exit 1
fi

sudo chown root:root "$BIN" 2>/dev/null || true
sudo chmod 755 "$BIN" 2>/dev/null || true
sudo setcap cap_net_raw,cap_net_admin=eip "$BIN" 2>/dev/null || true
echo "[run-with-caps] caps applied: $BIN" >&2

exec "$BIN" "${APP_ARGS[@]}"
