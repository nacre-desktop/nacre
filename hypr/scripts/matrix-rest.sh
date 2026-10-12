#!/usr/bin/env bash

set -euo pipefail

compositor() {
    python3 "$(dirname "$(realpath "$0")")/compositor-compat.py" "$@"
}

app="$HOME/.config/hypr/scripts/matrix-rest.py"
log_dir="${XDG_CACHE_HOME:-$HOME/.cache}/matrix-rest"
log_file="$log_dir/matrix-rest.log"
class_name="nacre-matrix"
title="Nacre Matrix Sleep"

mkdir -p "$log_dir"

existing_window="$(
    compositor query windows 2>/dev/null | jq -r --arg class "$class_name" --arg title "$title" '
        .[] | select(.class == $class or .title == $title) | [.address, .pid] | @tsv
    ' | head -n 1
)"

if [[ -n "$existing_window" && "$existing_window" != "null" ]]; then
    existing_pid="${existing_window#*$'\t'}"

    if [[ "$existing_pid" =~ ^[0-9]+$ ]]; then
        pkill -TERM -P "$existing_pid" >/dev/null 2>&1 || true
        kill -TERM "$existing_pid" >/dev/null 2>&1 || true
        sleep 0.15
        pkill -KILL -P "$existing_pid" >/dev/null 2>&1 || true
        kill -KILL "$existing_pid" >/dev/null 2>&1 || true
    fi

    exit 0
fi

pkill -TERM -u "$UID" -f "$app" >/dev/null 2>&1 || true

setsid -f kitty \
    --class "$class_name" \
    --name "$class_name" \
    --title "$title" \
    --start-as fullscreen \
    --config NONE \
    --override remember_window_size=no \
    --override initial_window_width=1920 \
    --override initial_window_height=1200 \
    --override confirm_os_window_close=0 \
    --override hide_window_decorations=yes \
    --override "map escape close_os_window" \
    --override "map q close_os_window" \
    --override cursor_shape=block \
    --override cursor_blink_interval=0 \
    --override mouse_hide_wait=0.1 \
    env MATRIX_CLASS="$class_name" MATRIX_TITLE="$title" MATRIX_APP="$app" sh -lc '
        for _ in $(seq 1 50); do
            if compositor query windows 2>/dev/null | jq -e --arg class "$MATRIX_CLASS" --arg title "$MATRIX_TITLE" \
                ".[] | select((.class == \$class or .title == \$title) and .fullscreen != 0)" >/dev/null; then
                break
            fi

            sleep 0.05
        done

        exec python3 "$MATRIX_APP"
    ' \
    >"$log_file" 2>&1

(
    for _ in {1..30}; do
        address="$(
            compositor query windows 2>/dev/null | jq -r --arg class "$class_name" --arg title "$title" '
                .[] | select(.class == $class or .title == $title) | .address
            ' | head -n 1
        )"

        if [[ -n "$address" && "$address" != "null" ]]; then
            compositor focus-window "$address" >/dev/null 2>&1 || true
            compositor fullscreen fullscreen "$address" >/dev/null 2>&1 || true
            break
        fi

        sleep 0.05
    done
) >/dev/null 2>&1 &
