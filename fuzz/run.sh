#!/bin/sh
# Fuzz every target for a while (seconds, default 60): `sh fuzz/run.sh [seconds] [target...]`.
# Seeds come from the converted assets (OPENKUMITE_ASSETS, default the repository's assets/) and a few room messages.
# Fails when a target does not build, a run fails, or a run leaves a new crash in fuzz/artifacts/<target>/.
set -e
here="$(cd "$(dirname "$0")" && pwd)"
assets="${OPENKUMITE_ASSETS:-$here/../assets}"
# (a relative OPENKUMITE_ASSETS is the caller's, not this directory's)
case "$assets" in /*) ;; *) assets="$(pwd)/$assets" ;; esac
cd "$here"
secs=${1:-60}
[ $# -gt 0 ] && shift
cargo +nightly fuzz build -O
seed() { mkdir -p "corpus/$1"; }
seed lobby_msg
printf '{"Hello":{"version":4,"name":"P","locals":2,"token":7,"host":true,"assets":"c3a9","build":"0123456789abcdef"}}' > corpus/lobby_msg/hello
printf '{"Room":{"members":[{"token":7,"name":"P","locals":1}],"seed":3}}' > corpus/lobby_msg/room
printf '{"Event":{"n":1,"player":0,"key":5}}' > corpus/lobby_msg/event
printf '"Bye"' > corpus/lobby_msg/bye
seed lobby_packet
printf '{"ack":2,"msgs":[[2,"Ping"],[3,{"Key":{"player":1,"key":16}}]]}' > corpus/lobby_packet/p
seed reliable
printf '{"ack":0,"msgs":[[1,"Ping"],[0,"Bye"]]}\n{"ack":1,"msgs":[[2,"Ping"]]}' > corpus/reliable/p
seed ggrs_message
printf '\0\0\0\0\064\022\007\0\0\0' > corpus/ggrs_message/keepalive
seed ggrs_session
printf '\2\0\0\0\1\0\0\0\0\0\0\0\0\377\377\377\377\0\2\0\0\0\377\377\377\377\4\0\0\0\0\0\0\0\6\1\0\7' > corpus/ggrs_session/input
seed ggrs_inputs
printf '\006\001\000\007' > corpus/ggrs_inputs/one
seed relay_header
seed pad_script
printf 'f*10,df+1,b+2*3,n' > corpus/pad_script/s
seed avi
if [ -f "$assets/video/118.avi" ]; then
    { printf '\377'; head -c 200000 "$assets/video/118.avi"; } > corpus/avi/118
fi
seed glb_character
[ -f "$assets/characters/embu_2_01.glb" ] && cp "$assets/characters/embu_2_01.glb" corpus/glb_character/
seed glb_stage
[ -f "$assets/stages/stage_18.glb" ] && cp "$assets/stages/stage_18.glb" corpus/glb_stage/
seed glb_parts
# (a glb as its JSON, a zero byte, its binary data)
for f in "$assets/stages/stage_18.glb" "$here/../crates/kumite_assets/tests/bad/"*.glb; do
    [ -f "$f" ] || continue
    j=$(od -An -tu4 -j12 -N4 "$f" | tr -d ' ')   # the JSON chunk's length (little-endian, as the host)
    { head -c $((20 + j)) "$f" | tail -c +21; printf '\0'; tail -c +$((20 + j + 9)) "$f"; } > "corpus/glb_parts/$(basename "$f")"
done
targets=${*:-$(cargo +nightly fuzz list)}
failed=""
for t in $targets; do
    echo "== $t ($secs s)"
    mkdir -p "artifacts/$t"
    before=$(ls "artifacts/$t" | grep -E '^(crash|oom|leak|timeout)-' || true)
    dict=""
    case "$t" in glb_*) dict="-dict=glb.dict" ;; esac
    if ! cargo +nightly fuzz run -O "$t" "corpus/$t" -- -max_total_time="$secs" -rss_limit_mb=2048 -max_len=1048576 $dict > "artifacts/$t.log" 2>&1; then
        failed="$failed $t"
    fi
    grep -E "^Done|ERROR|panicked at|SUMMARY|Test unit written" "artifacts/$t.log" || true
    after=$(ls "artifacts/$t" | grep -E '^(crash|oom|leak|timeout)-' || true)
    if [ "$before" != "$after" ]; then
        failed="$failed $t"
    fi
done
if [ -n "$failed" ]; then
    echo "!! fuzz failed:$failed (logs and crashes: fuzz/artifacts/)"
    exit 1
fi
echo "fuzz: no crashes"
