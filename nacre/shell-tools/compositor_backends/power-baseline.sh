#!/usr/bin/env bash
# Read-only diagnostic metadata; compositor-native queries stay in this boundary.
set -euo pipefail
if [[ -n ${NIRI_SOCKET:-} ]] && command -v niri >/dev/null; then
    niri --version
    niri msg --json outputs
elif [[ -n ${HYPRLAND_INSTANCE_SIGNATURE:-} ]] && command -v hyprctl >/dev/null; then
    hyprctl version
    hyprctl -j monitors
else
    echo 'compositor_output=unavailable (supply live session socket/signature)'
fi
