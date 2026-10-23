#!/bin/sh
# The Alpine build (.github/workflows/release.yml), in an Alpine container on a runner of the same architecture:
# openkumite linked dynamically to musl (a static program could not load the GPU and display libraries), so it is
# Alpine's own. Writes target/<arch>-unknown-linux-musl/dist/openkumite.
# usage: docker run --rm -v "$PWD:/src" -w /src alpine:3.20 sh packaging/linux/alpine.sh
set -e
apk add -q build-base pkgconf alsa-lib-dev eudev-dev curl
curl -sSf https://sh.rustup.rs | sh -s -- -y -q --profile minimal
. "$HOME/.cargo/env"
target=$(uname -m)-unknown-linux-musl
RUSTFLAGS="-C target-feature=-crt-static" cargo build --profile dist -p openkumite --target "$target"
bin=${CARGO_TARGET_DIR:-target}/$target/dist/openkumite
# dynamic (musl's loader), and it runs
file_needed=$(readelf -d "$bin" | grep NEEDED | grep -o '\[.*\]' | tr '\n' ' ')
echo "needs: $file_needed"
readelf -l "$bin" | grep -q 'ld-musl' || { echo "not linked to musl's loader"; exit 1; }
"$bin" --help | head -1
