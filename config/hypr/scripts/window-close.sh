#!/usr/bin/env bash

set -euo pipefail

compositor() {
    "$(dirname "$(realpath "$0")")/hyprctl-lua.sh" "$@"
}

nacre_message() {
    python3 "$HOME/.local/share/nacre/shell/tools/nacre_i18n.py" "$@" 2>/dev/null || python3 -c 'import json,re,sys; values=json.loads(sys.argv[2] or "{}"); print(re.sub(r"\{([A-Za-z_][A-Za-z0-9_]*)\}", lambda match: str(values.get(match[1], match[0])), sys.argv[1]), end="")' "$2" "${3-}"
}

notify() {
    command -v notify-send >/dev/null 2>&1 && notify-send -a "Nacre Windows" -i nacre-notifications -t 2000 "$@" || true
}

active_address() {
    compositor query active_window | jq -r '.address // empty'
}

window_exists() {
    local address="$1"

    compositor query windows | jq -e --arg address "$address" '
        any(.[]; .address == $address)
    ' >/dev/null
}

close_window() {
    compositor close-window "$1" >/dev/null 2>&1 || true
}

force_window() {
    compositor kill-window "$1" >/dev/null 2>&1 || true
}

main() {
    local mode="${1:-close}"
    local address

    address="$(active_address)"
    if [[ -z "$address" || "$address" == "null" ]]; then
        notify "$(nacre_message "windows.notification.no-active-window-to-close" "No active window to close")"
        return 0
    fi

    if [[ "$mode" == "force" ]]; then
        force_window "$address"
        return 0
    fi

    close_window "$address"
    sleep 1

    if window_exists "$address"; then
        notify "$(nacre_message "windows.notification.window-did-not-close" "Window did not close")" "$(nacre_message "windows.notification.press-shortcut-to-force-quit-it" "Press {shortcut} to force quit it" '{"shortcut": "SUPER+CTRL+SHIFT+Q"}')"
    fi
}

main "$@"
