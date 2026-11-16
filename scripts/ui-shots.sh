#!/usr/bin/env bash
# The off-screen screenshot suite (M11aj) in parallel processes (AppKit: one window-owning thread
# per process, so never threads). Builds once with the `ui-shots` feature, runs the scenes that
# aren't `serial` in N shards side by side (`__ui-shots --shard I/N`, each line prefixed `[I/N]`),
# then the serial scenes alone (`__ui-shots --serial-only`, prefixed `[serial]`), and prints one
# summary: passed, failed, the wall time after the build, and the 15 slowest scenes. A full run
# (no filter, no --changed) merges the shards' timings into target/ui-shots/timings.tsv, which
# the next run's shards are balanced by. Exits 1 if any process failed. bash 3.2-safe.
# --changed <base> runs only the scenes whose sources changed since <base> (git diff <base>...HEAD
# plus uncommitted files), says which and why, and runs nothing (exit 0) if no src change.
# Usage: scripts/ui-shots.sh [filter] [--png] [--all-themes] [--changed <base>] [-j N]
#   -j N: shards (default: CPU cores - 2, at most 8, at least 1).
set -uo pipefail

usage() {
    echo "usage: scripts/ui-shots.sh [filter] [--png] [--all-themes] [--changed <base>] [-j N]" >&2
    exit 2
}

cd "$(dirname "$0")/.." || exit 2

jobs=""
changed=""
full=1
pass=()
while [ $# -gt 0 ]; do
    case "$1" in
        -j)
            [ $# -ge 2 ] || usage
            jobs="$2"
            shift 2
            ;;
        -j*)
            jobs="${1#-j}"
            shift
            ;;
        --changed)
            [ $# -ge 2 ] || usage
            full=0
            changed="$2"
            pass+=("$1" "$2")
            shift 2
            ;;
        --shard | --serial-only)
            echo "ui-shots.sh: $1 is the script's own (it runs the shards and the serial scenes)" >&2
            usage
            ;;
        --*)
            pass+=("$1")
            shift
            ;;
        *)
            full=0
            pass+=("$1")
            shift
            ;;
    esac
done

if [ -z "$jobs" ]; then
    cores=$(sysctl -n hw.ncpu 2>/dev/null || echo 4)
    jobs=$((cores - 2))
    [ "$jobs" -gt 8 ] && jobs=8
    [ "$jobs" -lt 1 ] && jobs=1
fi
case "$jobs" in
    '' | *[!0-9]* | 0) echo "ui-shots.sh: -j needs a whole number of shards, 1 or more" >&2; usage ;;
esac

# The built binary, from cargo's own report (so `CARGO_TARGET_DIR` is honoured); errors and
# warnings still print as usual.
built=$(cargo build --features ui-shots --message-format=json-render-diagnostics) || exit 1
bin=$(printf '%s\n' "$built" | grep '"reason":"compiler-artifact"' | grep '"kind":\["bin"\]' |
    grep '"name":"fuxx"' | sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' | tail -1)
if [ -z "$bin" ] || [ ! -x "$bin" ]; then
    echo "ui-shots.sh: cargo reported no fuxx binary" >&2
    exit 1
fi

# Where the binary writes its pictures and timings (next to Cargo.toml, whatever the target dir).
out=target/ui-shots
mkdir -p "$out"
rm -f "$out"/timings.*.tsv
tmp=$(mktemp -d "${TMPDIR:-/tmp}/ui-shots.XXXXXX") || exit 2

# --changed: ask the runner what the change touches (it prints one line saying so, and exits 3
# when nothing under src/, assets/ or Cargo.* changed); every shard and the serial run then work
# out the same set themselves.
if [ -n "$changed" ]; then
    "$bin" __ui-shots --changed "$changed" --plan
    code=$?
    if [ "$code" = 3 ]; then
        rm -rf "$tmp"
        exit 0
    elif [ "$code" != 0 ]; then
        rm -rf "$tmp"
        exit 1
    fi
fi

# The parts started (their subshells' pids), for Ctrl-C.
pids=""

# `$1` and every process under it, deepest first.
tree() {
    local child
    for child in $(pgrep -P "$1" 2>/dev/null); do
        tree "$child"
    done
    echo "$1"
}

# Ctrl-C (or a TERM): end the parts, their binaries and whatever those started, then clean up.
# Background jobs of a script ignore Ctrl-C, so they are ended here, and only they.
stop() {
    trap - INT TERM
    local all="" pid
    for pid in $pids; do
        all="$all $(tree "$pid")"
    done
    [ -n "$all" ] && kill $all 2>/dev/null
    rm -rf "$tmp"
    echo "ui-shots: stopped" >&2
    exit 130
}
trap stop INT TERM

now() { perl -MTime::HiRes=time -e 'printf "%.1f\n", time'; }

# `$1` run as `__ui-shots $2… <the arguments>`, each line of its output prefixed `[$1] `; its
# exit status in $tmp/exit.<n> (`$3`).
run_part() {
    local label="$1" part="$2" flag="$3"
    shift 3
    ( "$bin" __ui-shots $flag "$@" 2>&1; echo $? >"$tmp/exit.$part" ) | sed -l "s|^|[$label] |"
}

started=$(now)
i=1
while [ "$i" -le "$jobs" ]; do
    run_part "$i/$jobs" "$i" "--shard $i/$jobs" ${pass[@]+"${pass[@]}"} &
    pids="$pids $!"
    i=$((i + 1))
done
wait
# The shards have ended: their pids may be reused, so Ctrl-C from here signals only the serial run.
pids=""
# In the background too, so a Ctrl-C (or a `kill -INT` of this script) stops it at once: `wait`
# returns to the trap, where a command in the foreground would be waited out first.
run_part serial serial --serial-only ${pass[@]+"${pass[@]}"} &
pids="$pids $!"
wait
total=$(perl -e 'printf "%.1f", $ARGV[1] - $ARGV[0]' "$started" "$(now)")

# Each part's exit status and timings (`<scene>\t<seconds>\tok|FAILED`).
broken=0
parts=""
whole=1
i=1
while [ "$i" -le "$jobs" ]; do
    parts="$parts $i"
    i=$((i + 1))
done
parts="$parts serial"
for part in $parts; do
    code=$(cat "$tmp/exit.$part" 2>/dev/null || echo "?")
    if [ "$code" != 0 ]; then
        broken=1
        case "$part" in
            serial) echo "ui-shots: the serial run exited $code" ;;
            *) echo "ui-shots: shard $part/$jobs exited $code" ;;
        esac
    fi
    if [ ! -f "$out/timings.$part.tsv" ]; then
        whole=0
        case "$part" in
            serial) echo "ui-shots: the serial run wrote no results: its scenes are not counted" ;;
            *) echo "ui-shots: shard $part/$jobs wrote no results: its scenes are not counted" ;;
        esac
    fi
done
rm -rf "$tmp"

results=$(cat "$out"/timings.*.tsv 2>/dev/null)
passed=$(printf '%s\n' "$results" | awk -F'\t' '$3 == "ok"' | wc -l | tr -d ' ')
failed=$(printf '%s\n' "$results" | awk -F'\t' '$3 == "FAILED"' | wc -l | tr -d ' ')

if [ "$full" = 1 ] && [ "$whole" = 1 ] && [ -n "$results" ]; then
    printf '%s\n' "$results" | cut -f1,2 >"$out/timings.tsv"
fi

if [ $((passed + failed)) = 0 ]; then
    # A process that broke has said so above; otherwise the filter matched nothing.
    [ "$broken" = 1 ] || echo "ui-shots: no scene matches ${pass[*]+"${pass[*]}"}"
    exit 1
fi

printf '%s\n' "$results" | awk -F'\t' -v dir="$out/failed" \
    '$3 == "FAILED" { printf "ui-shots: %s FAILED (%.1f s) \342\200\224 pictures in %s/%s\n", $1, $2, dir, $1 }'
echo "ui-shots: $passed passed, $failed failed, $total s"
printf '%s\n' "$results" | awk -F'\t' 'NF >= 2' | sort -t "$(printf '\t')" -k2,2 -rn | head -15 |
    awk -F'\t' '{ printf "ui-shots:   %6.1f s  %s\n", $2, $1 }'

[ "$broken" = 0 ] || exit 1
