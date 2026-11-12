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
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# A preview/session code overlay can run in a single-user namespace. Pacman's
# DownloadUser and sudo must run with the host's real IDs and privileges.
if python3 "$script_dir/updates.py" --needs-host; then
    step="starting the host update service"
    printf 'Opening the package updater in your normal user session.\n'
    terminal_mode=--pipe
    if [[ -t 0 && -t 1 ]]; then terminal_mode=--pty; fi
    systemd-run --user --wait "$terminal_mode" --collect --quiet -- \
        /usr/bin/bash "$script_dir/updates.sh"
    exit 0
fi
if (( EUID == 0 )); then
    printf 'Run nacre update as your desktop user, without sudo.\n' >&2
    exit 1
fi
package_snapshot() {
    local -a packages=()
    mapfile -t packages < <(
        pacman -Qq | awk '/^(nacre.*|niri|hyprland|orient)$/'
    )
    if ((${#packages[@]})); then pacman -Q "${packages[@]}"; fi
}
if command -v gum >/dev/null; then
    palette="$HOME/.config/nacre/colors"
    accent=$(cat "$palette/primary" 2>/dev/null || printf '#808080')
    text=$(cat "$palette/onprimary" 2>/dev/null || printf '#ffffff')
    gum confirm --selected.background="$accent" --selected.foreground="$text" 'Update system packages now?' || exit 0
else
    read -r -p 'Update system packages now? [y/N] ' answer
    [[ "$answer" == [yY] ]] || exit 0
fi
step="recording installed Nacre package versions"
before_packages=$(package_snapshot)
step="package update preflight"
python3 "$script_dir/updates.py" --preflight
step="system package update"
if command -v paru >/dev/null; then
    printf '\nUpdating packages without opening the AUR file-review screen.\n'
    printf 'Starting paru for the explicitly confirmed system package update.\n' >&2
    paru -Syu --skipreview
elif command -v yay >/dev/null; then
    printf 'Starting yay for the explicitly confirmed system package update.\n' >&2
    yay -Syu
else
    printf 'Starting pacman for the explicitly confirmed system package update.\n' >&2
    sudo pacman -Syu
fi
step="recording updated Nacre package versions"
after_packages=$(package_snapshot)
step="Flatpak update"
if command -v flatpak >/dev/null; then
    printf 'Starting flatpak for the explicitly confirmed Flatpak update.\n' >&2
    flatpak update
fi
step="Nacre setup plan"
if pacman -Qq nacre >/dev/null 2>&1; then
    nacre setup --plan
    if command -v gum >/dev/null; then
        palette="$HOME/.config/nacre/colors"
        accent=$(cat "$palette/primary" 2>/dev/null || printf '#808080')
        text=$(cat "$palette/onprimary" 2>/dev/null || printf '#ffffff')
        if gum confirm --selected.background="$accent" --selected.foreground="$text" 'Apply these setup changes?'; then
            step="Nacre setup apply"
            nacre setup
        else
            printf 'Setup changes were left for later. Run nacre setup to apply them.\n'
        fi
    else
        read -r -p 'Apply these setup changes? [y/N] ' answer
        if [[ "$answer" == [yY] ]]; then
            step="Nacre setup apply"
            nacre setup
        else
            printf 'Setup changes were left for later. Run nacre setup to apply them.\n'
        fi
    fi
else
    printf 'Nacre is not installed from its package; skipping the setup review.\n'
fi
changed_packages=$(join -a1 -a2 -e 'not-installed' -o 0,1.2,2.2 \
    <(sort <<<"$before_packages") <(sort <<<"$after_packages") |
    awk '$2 != $3 {print $1" "$2" -> "$3}')
if [[ -n "$changed_packages" ]]; then
    printf 'Changed packages:\n%s\n' "$changed_packages"
else
    printf 'no Nacre package changes\n'
fi
if grep -Eq \
    '^(niri|hyprland|nacre-niri|nacre-session-(niri|hyprland|nacre-niri)) ' \
    <<<"$changed_packages"; then
    printf 'Log out and back in to use the updated compositor.\n'
elif grep -Eq '^nacre ' <<<"$changed_packages"; then
    printf 'Restart the shell with: nacre restart\n'
fi
step="update status refresh"
python3 "$HOME/.local/share/nacre/shell/tools/updates.py" --refresh
read -r -p 'Update complete. Press Enter to close.'
