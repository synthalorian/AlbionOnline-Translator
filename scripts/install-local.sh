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

# Launcher entry + icon (user-level XDG paths, no sudo) so the app shows up
# in Walker / the Omarchy menu / any XDG launcher. StartupWMClass matches the
# Tauri window class, so the running window gets the same icon.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ICONS="$HOME/.local/share/icons/hicolor"
for size in 32 64 128 256 512; do
  case $size in
    256) src="128x128@2x.png" ;;
    512) src="icon.png" ;;
    *)   src="${size}x${size}.png" ;;
  esac
  install -Dm644 "$ROOT/src-tauri/icons/$src" "$ICONS/${size}x${size}/apps/albion-translator.png"
done
gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true

APPS="$HOME/.local/share/applications"
mkdir -p "$APPS"
cat > "$APPS/albion-translator.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Albion Online Translator
GenericName=Chat Translator
Comment=Real-time Albion Online chat translation overlay
Exec=$DEST
Icon=albion-translator
Terminal=false
Categories=Game;Utility;
Keywords=albion;translate;chat;overlay;
StartupWMClass=albion-translator
EOF
update-desktop-database -q "$APPS" 2>/dev/null || true
echo "Launcher entry: $APPS/albion-translator.desktop"
echo "NOTE: if the app is running, restart it — caps only apply at exec time."
