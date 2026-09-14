#!/usr/bin/env bash
# Erzeugt die PNG-Icons fuer die PWA aus den SVG-Quellen in assets/icons/.
#
# Die PNGs liegen fertig im Repository — dieses Skript braucht nur, wer das
# Icon aendert. Benutzt rsvg-convert, falls vorhanden (Linux: librsvg2-bin,
# macOS: brew install librsvg), sonst das auf macOS eingebaute qlmanage.
set -euo pipefail

QUELLE="assets/icons"
ZIEL="static/icons"
mkdir -p "$ZIEL"

render() { # render <svg> <groesse> <ziel.png>
  local svg="$1" px="$2" out="$3"
  if command -v rsvg-convert >/dev/null 2>&1; then
    rsvg-convert -w "$px" -h "$px" "$svg" -o "$out"
  elif command -v qlmanage >/dev/null 2>&1; then
    local tmp; tmp="$(mktemp -d)"
    qlmanage -t -s "$px" -o "$tmp" "$svg" >/dev/null 2>&1
    mv "$tmp/$(basename "$svg").png" "$out"
    rm -rf "$tmp"
  else
    echo "Weder rsvg-convert noch qlmanage gefunden." >&2
    exit 1
  fi
  echo "  $out (${px}px)"
}

echo "Erzeuge Icons ..."
render "$QUELLE/icon.svg"          192 "$ZIEL/icon-192.png"
render "$QUELLE/icon.svg"          512 "$ZIEL/icon-512.png"
render "$QUELLE/icon-maskable.svg" 512 "$ZIEL/icon-maskable-512.png"
# iOS rundet selbst ab und mag keine Transparenz — daher die vollflaechige Fassung.
render "$QUELLE/icon-maskable.svg" 180 "$ZIEL/apple-touch-icon.png"
cp "$QUELLE/icon.svg" "$ZIEL/icon.svg"
cp "$QUELLE/icon.svg" static/favicon.svg
echo "Fertig."
