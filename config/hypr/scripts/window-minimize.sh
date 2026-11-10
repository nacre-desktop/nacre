#!/usr/bin/env bash
set -euo pipefail
helper="${NACRE_HELPER_BINARY:-$HOME/.local/share/nacre/shell/tools/nacre-helper}"
if [[ ! -x "$helper" ]]; then
    helper="$(dirname "$(realpath "$0")")/../../nacre/shell-tools/nacre-helper"
fi
exec "$helper" window-hide "$@"
