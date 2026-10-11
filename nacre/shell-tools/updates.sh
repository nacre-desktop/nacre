#!/usr/bin/env bash
set -Eeuo pipefail
step="initialization"
failed() {
    local code=$?
    trap - ERR
    printf '\nUpdate failed during %s (exit %s).\n' "$step" "$code" >&2
    if [[ -t 0 ]]; then read -r -p 'Press Enter to close.' || true; fi
    exit "$code"
}
trap failed ERR
source "$(dirname -- "${BASH_SOURCE[0]}")/qt-check.sh"
if command -v gum >/dev/null; then
    palette="$HOME/.config/nacre/colors"
    accent=$(cat "$palette/primary" 2>/dev/null || printf '#808080')
    text=$(cat "$palette/onprimary" 2>/dev/null || printf '#ffffff')
    gum confirm --selected.background="$accent" --selected.foreground="$text" 'Update system packages now?' || exit 0
else
    read -r -p 'Update system packages now? [y/N] ' answer
    [[ "$answer" == [yY] ]] || exit 0
fi
step="Quickshell update preflight"
python3 "$(dirname -- "${BASH_SOURCE[0]}")/updates.py" --preflight
step="system package update"
if command -v paru >/dev/null; then
    printf '\nUpdating packages without opening the AUR file-review screen.\n'
    paru -Syu --skipreview
elif command -v yay >/dev/null; then yay -Syu
else sudo pacman -Syu; fi
step="Quickshell/Qt compatibility check"
if ! qt_match_check; then qt_recovery_help; false; fi
step="Flatpak update"
if command -v flatpak >/dev/null; then flatpak update; fi
step="update status refresh"
python3 "$HOME/.local/share/nacre/shell/tools/updates.py" --refresh
read -r -p 'Update complete. Press Enter to close.'
