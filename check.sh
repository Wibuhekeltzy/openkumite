#!/bin/sh
# The checks: `sh check.sh [commit|push|all|fmt|lint|test|deny|fuzz] [fuzz seconds]`.
#   commit (the pre-commit hook): rustfmt; clippy (native, wasm, the fuzz crate) with warnings as errors; the tests;
#   push (the pre-push hook):     those, and cargo deny (advisories, licences, sources, bans: deny.toml);
#   all:                          those, and every fuzz target for a while (60 s each by default).
# The tests read the converted assets: OPENKUMITE_ASSETS (default the repository's assets/).
set -e
here="$(cd "$(dirname "$0")" && pwd)"
assets="${OPENKUMITE_ASSETS:-$here/assets}"
case "$assets" in /*) ;; *) assets="$(pwd)/$assets" ;; esac
export OPENKUMITE_ASSETS="$assets"
cd "$here"
step() { printf '\n== %s\n' "$*"; }
fmt() {
    step fmt
    cargo fmt --all --check
    cargo fmt --manifest-path fuzz/Cargo.toml --check
}
lint() {
    step clippy
    cargo clippy --workspace --all-targets --release -q -- -D warnings
    step "clippy (wasm)"
    cargo clippy --release --target wasm32-unknown-unknown -p openkumite -q -- -D warnings
    step "clippy (fuzz targets)"
    # (its own target directory: a fuzz run may hold fuzz/target meanwhile, with nightly)
    CARGO_TARGET_DIR="$here/target/fuzz-check" cargo clippy --manifest-path fuzz/Cargo.toml --release -q -- -D warnings
}
tests() {
    step test
    if ! out=$(cargo test --workspace --release 2>&1); then
        printf '%s\n' "$out" | grep -vE '^test .* ok$|^running |^$' | tail -80
        return 1
    fi
    printf '%s\n' "$out" | awk '/^test result/ { p += $4; i += $8 } END { printf "%d passed, %d ignored\n", p, i }'
}
deny() {
    step deny
    cargo deny --log-level error check
}
fuzz() {
    step fuzz
    sh fuzz/run.sh "${1:-60}"
}
case "${1:-push}" in
    commit) fmt; lint; tests ;;
    push) fmt; lint; tests; deny ;;
    all) fmt; lint; tests; deny; fuzz "$2" ;;
    fmt) fmt ;;
    lint) lint ;;
    test) tests ;;
    deny) deny ;;
    fuzz) fuzz "$2" ;;
    *) echo "usage: sh check.sh [commit|push|all|fmt|lint|test|deny|fuzz] [fuzz seconds]"; exit 2 ;;
esac
printf '\nchecks passed\n'
