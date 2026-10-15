#!/usr/bin/env sh
# One native process owns generation and paired publication under the same lock.
if [ "${1:-}" = update ]; then exec "$HOME/.local/bin/nacre-shell" updates; fi
helper="${NACRE_HELPER_BINARY:-$HOME/.local/share/nacre/shell/tools/nacre-helper}"
exec "$helper" nacre_shell --publish "$@"
