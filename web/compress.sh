#!/bin/sh
# Compressed copies for the web server to send (web/serve.sh; nginx: gzip_static / brotli_static): the game
# (pkg/: brotli and gzip) and the assets that compress (models, stages, game data: gzip; brotli too with --br).
# Pictures, music and movies are compressed already; sound effects stay as they are (audio elements ask for ranges).
# Only what changed since its copy is redone.
set -e
web=$(cd "$(dirname "$0")" && pwd)
. "$web/link_assets.sh"
cd "$web"
pack() {   # FILE: FILE.gz (and FILE.br with $BR) when missing or older
  if [ ! -e "$1.gz" ] || [ "$1" -nt "$1.gz" ]; then gzip -9 -k -f -n "$1" && touch "$1.gz"; fi
  if [ -n "$BR" ] && command -v brotli >/dev/null && { [ ! -e "$1.br" ] || [ "$1" -nt "$1.br" ]; }; then brotli -f -q "$BR" -o "$1.br" "$1"; fi
}
BR=11
for f in pkg/*.wasm pkg/*.js preload.json; do [ -e "$f" ] && pack "$f"; done
BR=""
[ "$1" = "--br" ] && BR=9
find -L assets \( -name '*.glb' -o -name '*.json' \) -type f | while read -r f; do pack "$f"; done
echo "compressed: $(find -L assets pkg -name '*.gz' 2>/dev/null | wc -l | tr -d ' ') files"
