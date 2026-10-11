#!/usr/bin/env bash
# Shared by launch and package updates; never starts a renderer.
qt_match_check() {
    local version_info
    version_info=$(/usr/bin/quickshell --version -v 2>&1) || return
    if [[ "$version_info" =~ Qt:\ ([0-9.]+)\ \(built\ against\ ([0-9.]+)\) ]] && [[ "${BASH_REMATCH[1]}" == "${BASH_REMATCH[2]}" ]]; then
        return 0
    fi
    printf '%s\n' 'Quickshell/Qt mismatch: install a matching distribution package or rebuild before restarting the desktop.' >&2
    return 1
}
qt_recovery_help() {
    printf '%s\n' 'Recovery: see docs/features/runtime.md.' \
        'Use a distribution Quickshell build matching installed Qt when available.' \
        'Otherwise copy tools/quickshell.PKGBUILD and tools/quickshell-qt612.patch into an isolated build directory.' \
        'Build there as your normal user: makepkg' \
        'Install the resulting package: sudo pacman -U ./quickshell-*.pkg.tar.zst' \
        'Then restart: systemctl --user restart nacre-shell.service'
}
