#!/usr/bin/env bash
# Run a sequence of arms with bench.py, then summarize.
#   run.sh STAGE OUT PROFILE SEQUENCE [bench.py options...]
# SEQUENCE is a comma list of arms, e.g. nacre,noctalia,noctalia,nacre.
# Desktop TEST host: pass --render-node /dev/dri/renderD129 --lock ~/nacre-lead/desktop-heavy.lock
# --mem-rail-gib 3 --niri /usr/bin/<stock compositor>; each arm then runs in a 4 GiB no-swap scope.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
stage=${1:?stage dir}; out=${2:?out dir}; profile=${3:?short|full}; sequence=${4:?arm list}
shift 4
mkdir -p "$out"
scope=()
for arg in "$@"; do
  if [[ "$arg" == --lock ]]; then
    scope=(systemd-run --user --scope --quiet -p MemoryMax=4G -p MemorySwapMax=0 --)
  fi
done
index=0
IFS=, read -r -a arms <<< "$sequence"
for arm in "${arms[@]}"; do
  index=$((index + 1))
  run=$(printf 'run-%02d-%s' "$index" "$arm")
  # A lost lock boundary or a failed memory gate is the host's fault: wait and retry (3 tries).
  for attempt in 1 2 3; do
    { date -Is; free -h; cat /proc/loadavg; } >> "$out/host.log"
    echo "== $run attempt $attempt" | tee -a "$out/host.log"
    if "${scope[@]}" nice -n 10 python3 -I "$here/bench.py" --arm "$arm" --stage "$stage" \
        --out "$out/$run" --profile "$profile" "$@" > "$out/$run.log" 2>&1; then
      break
    fi
    echo "FAILED $run; see $out/$run.log" | tee -a "$out/host.log"
    tail -5 "$out/$run.log"
    grep -q -E 'contended|desktop gate' "$out/$run.log" "$out/$run/lock.log" 2>/dev/null || break
    mv -f "$out/$run/lock.log" "$out/$run/lock-attempt-$attempt.log" 2>/dev/null || true
    sleep 300
  done
  { date -Is; free -h; } >> "$out/host.log"
  if ((index < ${#arms[@]})) && ((${#scope[@]})); then sleep 90; fi
done
python3 -I "$here/summarize.py" "$out" --label "${BENCH_LABEL:-$(hostname) $(date -u +%F)}" | tee "$out/summary.txt"
