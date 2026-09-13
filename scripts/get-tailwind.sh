#!/usr/bin/env bash
# Laedt die Tailwind-Standalone-Binary passend zur Plattform nach .bin/.
# Standalone heisst: kein Node, kein npm, keine node_modules.
set -euo pipefail

VERSION="${TAILWIND_VERSION:-v4.3.3}"
DEST="${1:-.bin/tailwindcss}"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  ASSET="tailwindcss-macos-arm64" ;;
  Darwin-x86_64) ASSET="tailwindcss-macos-x64" ;;
  Linux-aarch64) ASSET="tailwindcss-linux-arm64" ;;
  Linux-x86_64)  ASSET="tailwindcss-linux-x64" ;;
  *) echo "Nicht unterstuetzte Plattform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

if [ -x "$DEST" ]; then
  echo "Tailwind liegt bereits unter $DEST"
  exit 0
fi

mkdir -p "$(dirname "$DEST")"
echo "Lade $ASSET ($VERSION) ..."
curl -sSL --fail -o "$DEST" \
  "https://github.com/tailwindlabs/tailwindcss/releases/download/${VERSION}/${ASSET}"
chmod +x "$DEST"
echo "Fertig: $DEST"
