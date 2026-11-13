#!/bin/bash
# M11p chrome-cache measurement (the user runs it; ~10-15 min, interleaved).
#   m11p.sh baseline [--dry-run]   master only
#   m11p.sh final    [--dry-run]   master, the branch, and the branch with FUXX_CHROME_CACHE=off
# Per variant: cpu_base x5 and cpu_working x3, interleaved round by round; plus cpu_noglow x1 on master.
# Results: scripts/bench/results/m11p-<mode>-<date>.md (gitignored).
set -euo pipefail

MODE="${1:-}"; DRY=0
[[ "${2:-}" == "--dry-run" ]] && DRY=1
[[ "$MODE" == baseline || "$MODE" == final ]] || { echo "usage: $0 baseline|final [--dry-run]"; exit 2; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN="$ROOT/scripts/bench/run.py"
WORK="$ROOT/target-m11p"                # gitignored (/target-*/)
BENCH="$WORK/bench"
SRC="$WORK/master-src"
export CARGO_TARGET_DIR="$WORK/cargo"
OUTDIR="$ROOT/scripts/bench/results"
OUT="$OUTDIR/m11p-$MODE-$(date +%Y-%m-%d).md"

say() { printf '%s\n' "$*"; }
run() { if ((DRY)); then printf '+ %s\n' "$*"; else printf '+ %s\n' "$*"; "$@"; fi; }
# Same, with an environment assignment (printed too).
runenv() { local e="$1"; shift; if ((DRY)); then printf '+ %s %s\n' "$e" "$*"; else printf '+ %s %s\n' "$e" "$*"; env "$e" "$@"; fi; }

cat <<'TXT'
M11p chrome-cache measurement. Conditions (the numbers are void otherwise):
  - run from Terminal.app (not fuxx, not Ghostty)
  - every fuxx quit; Chrome and Teams quit
  - Do Not Disturb on
  - hands off the Mac for the whole run (~10-15 min): no typing, no window switching
  - cpu_working keeps a terminal Working with a loop printing a line every 0.5 s; the status bar should read
    Working. This is unconfirmed: on the first run, look at the status bar once and confirm it.
TXT

if ((DRY)); then
  say "(dry run: nothing is built or run; the fuxx-process check is skipped)"
else
  if pgrep -x fuxx >/dev/null; then
    say "REFUSING to start: a fuxx process is running (pgrep -x fuxx). Quit every fuxx; the frontmost checks would fail."
    exit 1
  fi
fi

VARIANTS=(master)
[[ "$MODE" == final ]] && VARIANTS=(master branch branchoff)
app() { echo "fuxx-$1"; }

say "== build =="
run rm -rf "$SRC"
run mkdir -p "$BENCH" "$OUTDIR" "$SRC"
run bash -c "git -C '$ROOT' archive master | tar -x -C '$SRC'"
run cargo build --release --manifest-path "$SRC/Cargo.toml"
cp_master="$CARGO_TARGET_DIR/release/fuxx"
if [[ "$MODE" == final ]]; then
  # Different source, same target dir: the master build is copied aside first.
  run cp "$cp_master" "$WORK/fuxx-master"
  run cargo build --release --manifest-path "$ROOT/Cargo.toml"
  run cp "$cp_master" "$WORK/fuxx-branch"
else
  run cp "$cp_master" "$WORK/fuxx-master"
fi

say "== bundle =="
run python3 "$RUN" --bench "$BENCH" bundle fuxx-master "$WORK/fuxx-master"
if [[ "$MODE" == final ]]; then
  run python3 "$RUN" --bench "$BENCH" bundle fuxx-branch "$WORK/fuxx-branch"
  run python3 "$RUN" --bench "$BENCH" bundle fuxx-branchoff "$WORK/fuxx-branch"
fi

phase() {  # phase APP PHASE TAG
  if [[ "$1" == fuxx-branchoff ]]; then
    runenv FUXX_CHROME_CACHE=off python3 "$RUN" --bench "$BENCH" "$1" "$2" --tag "$3"
  else
    run python3 "$RUN" --bench "$BENCH" "$1" "$2" --tag "$3"
  fi
}

say "== measure (interleaved) =="
for i in 1 2 3 4 5; do for v in "${VARIANTS[@]}"; do phase "$(app "$v")" cpu_base "$i"; done; done
for i in 1 2 3; do for v in "${VARIANTS[@]}"; do phase "$(app "$v")" cpu_working "$i"; done; done
phase "$(app master)" cpu_noglow 1

say "== summary =="
if ((DRY)); then
  say "+ python3 $RUN --bench $BENCH summary | tee $OUT   (preceded by a header line)"
else
  { echo "# M11p $MODE $(date '+%Y-%m-%d %H:%M')  (cpu % medians; runs counted only when frontmost 15/15)"
    echo '```'; python3 "$RUN" --bench "$BENCH" summary; echo '```'; } | tee "$OUT"
  say "wrote $OUT"
  say "Reminder: confirm once that cpu_working really showed Working in the status bar; a variant with <3 counted runs needs a re-run."
fi
