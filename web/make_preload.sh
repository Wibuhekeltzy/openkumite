#!/bin/sh
# web/preload.json: the files the browser fetches before the game starts (with a bar), so its first screens read
# nothing over the network while they show. The native build plays those screens and records what they read
# (--record-reads); a fight's characters (models, movesets), stages and the movies are left out (fetched when
# needed).
# usage: sh web/make_preload.sh   (OPENKUMITE_ASSETS: the converted assets, default assets/)
set -e
cd "$(dirname "$0")/.."
export OPENKUMITE_ASSETS="${OPENKUMITE_ASSETS:-assets}"
tmp=$(mktemp -d)
for screen in title main-menu options select vs hud; do
  ./target/release/openkumite --screen "$screen" --shot "$tmp/$screen.png" --frames 120 --record-reads "$tmp/$screen.json" >/dev/null 2>&1
done
cargo run -q --release --example make_preload -- "$tmp" web/preload.json
rm -rf "$tmp"
