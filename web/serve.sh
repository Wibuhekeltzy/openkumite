#!/bin/sh
# Serve the browser version at http://127.0.0.1:8770/ (crates/serve: the converted assets through web/assets, a link
# to OPENKUMITE_ASSETS or ../assets; compressed copies when web/compress.sh made them). Pass a port and --bind 0.0.0.0 for a
# LAN.
set -e
web=$(cd "$(dirname "$0")" && pwd)
. "$web/link_assets.sh"
port=8770
case "$1" in [0-9]*) port=$1; shift ;; esac
cargo build -q --release --manifest-path "$web/../Cargo.toml" -p serve
exec "$web/../target/release/serve" "$port" "$web" "$@"
