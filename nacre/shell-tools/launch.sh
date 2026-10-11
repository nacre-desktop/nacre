#!/usr/bin/env bash
set -euo pipefail
shell_runtime="$HOME/.local/share/nacre/palette-runtime"
# Desktop binaries and Qt plugins are owned and updated together by pacman.
unset LD_LIBRARY_PATH QT_PLUGIN_PATH QML_IMPORT_PATH QML2_IMPORT_PATH
clean_path=""
IFS=: read -ra path_entries <<< "${PATH:-/usr/bin:/bin}"
for entry in "${path_entries[@]}"; do
    case "$entry" in
        */shell-runtime/usr/*|*/palette-runtime/usr/*|*/rice-runtime/usr/*|*/thunar-runtime/usr/*) continue ;;
    esac
    clean_path+="${clean_path:+:}$entry"
done
export PATH="$HOME/.local/share/nacre/shell/bin:$shell_runtime/venv/bin:$HOME/.local/bin:$clean_path"
export QT_LOGGING_RULES='*.debug=false'
if [[ ! -x /usr/bin/quickshell ]]; then
    echo 'Install quickshell with pacman before starting Nacre.' >&2
    exit 1
fi
qt_helper="$(dirname -- "${BASH_SOURCE[0]}")/qt-check.sh"
if [[ ! -f "$qt_helper" ]]; then qt_helper="$(dirname -- "${BASH_SOURCE[0]}")/../tools/qt-check.sh"; fi
source "$qt_helper"
if ! qt_match_check; then
    hyprctl notify 3 0 0 "Desktop shell not started: Quickshell/Qt mismatch. See docs/features/runtime.md" || true
    exit 1
fi
# Return unused jemalloc pages asynchronously; live decoder/image caches stay put.
# Later user options keep precedence over these renderer defaults.
export MALLOC_CONF="background_thread:true,dirty_decay_ms:2000,muzzy_decay_ms:2000${MALLOC_CONF:+,$MALLOC_CONF}"
exec /usr/bin/quickshell "$@"
