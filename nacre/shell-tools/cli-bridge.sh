#!/usr/bin/env bash
set -euo pipefail
cli="$HOME/.local/share/nacre/palette-runtime/venv/bin/nacre_shell"
bridge="$HOME/.local/share/nacre/shell/tools/classic-state.py"
if [[ "${1:-}" == update ]]; then exec "$HOME/.local/bin/nacre-shell" updates; fi
case "${1:-}" in -h|--help|-v|--version) exec "$cli" "$@" ;; esac
# Queries prepare/read caches without locking or publishing the active desktop.
if [[ "${1:-}" == scheme && "${2:-}" != set ]]; then exec "$cli" "$@"; fi
if [[ "${1:-}" == wallpaper ]]; then
    for arg in "$@"; do
        if [[ "$arg" == -p* || "$arg" == --print || "$arg" == --print=* ]]; then exec "$cli" "$@"; fi
    done
fi
mkdir -p "$HOME/.local/state/nacre"
exec 9>"$HOME/.local/state/nacre/palette-commit.lock"
flock 9
export NACRE_PALETTE_LOCKED=1
"$cli" "$@"
if [[ "${1:-}" == scheme ]]; then python3 "$bridge"; fi
if [[ "${1:-}" == wallpaper ]]; then
    poster="$(cat "$HOME/.local/state/nacre/wallpaper/path.txt")"
    python3 "$bridge" "$poster"
fi
