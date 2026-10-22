#!/usr/bin/env bash
set -euo pipefail
action="${1:-brain}"
case "$action" in
 brain|capture|notes) exec "${NACRE_HELPER_BINARY:-$HOME/.local/share/nacre/shell/tools/nacre-helper}" brain-control action "$action" "${2:-}" ;;
 *) printf 'Usage: nacre-brain-ui [brain|capture|notes]\n' >&2; exit 2 ;;
esac
