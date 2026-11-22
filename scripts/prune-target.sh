#!/usr/bin/env bash
# Deletes fuxx's stale object files from the build folder. On macOS a dev build keeps its debug
# info in the compiler's object files (split-debuginfo "unpacked"), so cargo leaves them beside
# the binary; every rebuild adds a new set under new names and the old ones stay (74 GB in a day
# and a half, found 2026-11-23). Each fuxx binary lists the object files it uses (its OSO
# entries): a fuxx object file that no binary lists is deleted. Files newer than the newest binary
# are kept (a build may be writing them), and a folder whose binaries can't all be read is left
# alone. Prints what it freed. bash 3.2-safe.
set -u -o pipefail  # a failed `nm` read must not look like an empty list
cd "$(dirname "$0")/.." || exit 1

freed_kb=0
removed=0
for deps in target/*/deps target/*/*/deps; do
  [ -d "$deps" ] || continue
  deps="$PWD/$deps"
  keep=$(mktemp)
  newest=""
  ok=1
  for bin in "$deps"/fuxx-*; do
    case "${bin##*/}" in *.*) continue ;; esac  # .d files, .rcgu.o, .dSYM
    [ -f "$bin" ] && [ -x "$bin" ] || continue
    if ! nm -ap "$bin" 2>/dev/null | awk '/ OSO / {print $NF}' >>"$keep"; then
      ok=0
      break
    fi
    if [ -z "$newest" ] || [ "$bin" -nt "$newest" ]; then newest="$bin"; fi
  done
  if [ "$ok" = 1 ] && [ -n "$newest" ]; then
    sort -u -o "$keep" "$keep"
    stale=$(mktemp)
    find "$deps" -maxdepth 1 -name 'fuxx-*.rcgu.o' ! -newer "$newest" | sort >"$stale.all"
    comm -23 "$stale.all" "$keep" >"$stale"
    if [ -s "$stale" ]; then
      kb=$(tr '\n' '\0' <"$stale" | xargs -0 du -ck | tail -1 | awk '{print $1}')
      freed_kb=$((freed_kb + kb))
      removed=$((removed + $(wc -l <"$stale")))
      tr '\n' '\0' <"$stale" | xargs -0 rm -f
    fi
    rm -f "$stale" "$stale.all"
  fi
  rm -f "$keep"
done
echo "prune-target: removed $removed stale object files, $((freed_kb / 1024)) MB"
