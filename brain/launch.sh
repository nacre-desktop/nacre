#!/usr/bin/env bash
set -euo pipefail
action="${1:-brain}"
case "$action" in
 brain|capture|notes) exec python3 "$HOME/.local/share/nacre/brain/control.py" action "$action" "${2:-}" ;;
 *) printf 'Usage: nacre-brain-ui [brain|capture|notes]\n' >&2; exit 2 ;;
esac
