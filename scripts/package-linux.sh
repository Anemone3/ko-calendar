#!/usr/bin/env bash
# Builds the official Linux x86_64 release archive: dist/ko-calendar-linux-x86_64.tar.gz
#
# The Google OAuth client is compiled in from KOKONUT_GOOGLE_CLIENT_ID (and optionally
# KOKONUT_GOOGLE_CLIENT_SECRET). They are read at build time only and never printed.
# In CI they come from the "release" environment secrets of the GitHub repository.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [[ ! "${KOKONUT_GOOGLE_CLIENT_ID:-}" =~ ^[A-Za-z0-9._-]+\.apps\.googleusercontent\.com$ ]]; then
  echo "KOKONUT_GOOGLE_CLIENT_ID no está definido o no tiene el formato de un cliente de escritorio de Google." >&2
  echo "Las versiones oficiales necesitan el cliente OAuth (secreto KOKONUT_GOOGLE_CLIENT_ID del entorno release de GitHub)." >&2
  exit 1
fi

cargo build --locked --release

name="ko-calendar-linux-x86_64"
stage="dist/$name"
rm -rf "$stage" "dist/$name.tar.gz"
mkdir -p "$stage"
install -m755 target/release/ko-calendar "$stage/ko-calendar"
install -m644 packaging/linux/ko-calendar.desktop "$stage/ko-calendar.desktop"
install -m644 assets/kokonut.png "$stage/ko-calendar.png"
install -m644 LICENSE "$stage/"
tar -C dist -czf "dist/$name.tar.gz" "$name"
rm -rf "$stage"
echo "dist/$name.tar.gz"
