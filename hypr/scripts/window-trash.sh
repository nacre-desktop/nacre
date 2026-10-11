#!/usr/bin/env bash

set -euo pipefail

STATE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/hypr-trash"
TRASH_WORKSPACE="special:trash"
TTL_SECONDS=30

mkdir -p "$STATE_DIR"

notify() {
    command -v notify-send >/dev/null 2>&1 && notify-send -t 2500 "$@" || true
}

lua_quote() {
    local value="$1"

    value="${value//\\/\\\\}"
    value="${value//\"/\\\"}"
    printf '"%s"' "$value"
}

window_selector() {
    printf 'address:%s' "$1"
}

dispatch_close() {
    local selector

    selector="$(window_selector "$1")"
    hyprctl eval "hl.dispatch(hl.dsp.window.close({ window = $(lua_quote "$selector") }))"
}

dispatch_kill() {
    local selector

    selector="$(window_selector "$1")"
    hyprctl eval "hl.dispatch(hl.dsp.window.kill({ window = $(lua_quote "$selector") }))"
}

dispatch_move() {
    local address="$1"
    local workspace="$2"
    local selector

    selector="$(window_selector "$address")"
    hyprctl eval "hl.dispatch(hl.dsp.window.move({ workspace = $(lua_quote "$workspace"), window = $(lua_quote "$selector"), follow = false }))"
}

dispatch_focus() {
    local selector

    selector="$(window_selector "$1")"
    hyprctl eval "hl.dispatch(hl.dsp.focus({ window = $(lua_quote "$selector") }))"
}

state_file() {
    printf '%s/%s' "$STATE_DIR" "$1"
}

client_workspace() {
    local address="$1"

    hyprctl clients -j | jq -r --arg address "$address" '
        .[] | select(.address == $address) | .workspace.name
    '
}

client_exists() {
    local address="$1"

    hyprctl clients -j | jq -e --arg address "$address" '
        any(.[]; .address == $address)
    ' >/dev/null
}

client_is_trashed() {
    [[ "$(client_workspace "$1")" == "$TRASH_WORKSPACE" ]]
}

write_state() {
    local address="$1"
    local workspace_id="$2"
    local workspace_name="$3"
    local trashed_at="$4"
    local file

    file="$(state_file "$address")"
    {
        printf 'address=%q\n' "$address"
        printf 'workspace_id=%q\n' "$workspace_id"
        printf 'workspace_name=%q\n' "$workspace_name"
        printf 'trashed_at=%q\n' "$trashed_at"
    } > "${file}.tmp"
    mv "${file}.tmp" "$file"
}

load_state() {
    local address="$1"
    local file

    file="$(state_file "$address")"
    [[ -f "$file" ]] || return 1

    # shellcheck disable=SC1090
    source "$file"
}

target_workspace() {
    if [[ "${workspace_id:-}" =~ ^-?[0-9]+$ && "$workspace_id" -gt 0 ]]; then
        printf '%s' "$workspace_id"
    elif [[ -n "${workspace_name:-}" && "$workspace_name" != "$TRASH_WORKSPACE" ]]; then
        printf 'name:%s' "$workspace_name"
    else
        printf 'name:current'
    fi
}

close_trashed_window() {
    local address="$1"

    client_exists "$address" || return 0
    client_is_trashed "$address" || return 0

    dispatch_close "$address" >/dev/null 2>&1 || true
    sleep 2

    if client_exists "$address" && client_is_trashed "$address"; then
        dispatch_kill "$address" >/dev/null 2>&1 || true
    fi
}

expire_window() {
    local address="$1"
    local expected_trashed_at="${2:-}"
    local now

    load_state "$address" || return 0

    if [[ -n "$expected_trashed_at" && "${trashed_at:-}" != "$expected_trashed_at" ]]; then
        return 0
    fi

    if ! client_exists "$address"; then
        rm -f "$(state_file "$address")"
        return 0
    fi

    if ! client_is_trashed "$address"; then
        rm -f "$(state_file "$address")"
        return 0
    fi

    now="$(date +%s)"
    if (( now - trashed_at >= TTL_SECONDS )); then
        close_trashed_window "$address"
        rm -f "$(state_file "$address")"
    fi
}

purge_expired() {
    local file address

    shopt -s nullglob
    for file in "$STATE_DIR"/0x*; do
        address="${file##*/}"
        expire_window "$address"
    done
    shopt -u nullglob
}

schedule_expiry() {
    local address="$1"
    local trashed_at="$2"
    local unit="hypr-trash-${address#0x}-${trashed_at}"

    if command -v systemd-run >/dev/null 2>&1; then
        systemd-run --user --quiet --collect --unit "$unit" \
            --on-active="${TTL_SECONDS}s" "$0" expire "$address" "$trashed_at" >/dev/null 2>&1 && return 0
    fi

    (
        sleep "$TTL_SECONDS"
        "$0" expire "$address" "$trashed_at" >/dev/null 2>&1
    ) &
}

trash_active() {
    local window_json address workspace_id workspace_name trashed_at

    purge_expired

    window_json="$(hyprctl activewindow -j)"
    address="$(jq -r '.address // empty' <<< "$window_json")"

    if [[ -z "$address" || "$address" == "null" ]]; then
        notify "No active window to trash"
        return 0
    fi

    workspace_name="$(jq -r '.workspace.name // empty' <<< "$window_json")"
    if [[ "$workspace_name" == "$TRASH_WORKSPACE" ]]; then
        notify "Window is already in trash"
        return 0
    fi

    workspace_id="$(jq -r '.workspace.id // empty' <<< "$window_json")"
    trashed_at="$(date +%s)"

    write_state "$address" "$workspace_id" "$workspace_name" "$trashed_at"
    dispatch_move "$address" "$TRASH_WORKSPACE" >/dev/null
    schedule_expiry "$address" "$trashed_at"
    notify "Window moved to trash" "Press SUPER+SHIFT+Q to restore within 30 seconds"
}

trashed_windows_by_age() {
    local file address

    shopt -s nullglob
    for file in "$STATE_DIR"/0x*; do
        address="${file##*/}"
        load_state "$address" || continue
        client_is_trashed "$address" || continue
        printf '%s\t%s\n' "${trashed_at:-0}" "$address"
    done | sort -n
    shopt -u nullglob
}

restore_window() {
    local address="$1"
    local target

    load_state "$address" || return 1
    client_is_trashed "$address" || {
        rm -f "$(state_file "$address")"
        return 1
    }

    target="$(target_workspace)"
    dispatch_move "$address" "$target" >/dev/null
    rm -f "$(state_file "$address")"
    dispatch_focus "$address" >/dev/null 2>&1 || true
}

restore_last() {
    local latest

    purge_expired
    latest="$(trashed_windows_by_age | tail -n 1 | cut -f2)"

    if [[ -z "$latest" ]]; then
        notify "Trash is empty"
        return 0
    fi

    restore_window "$latest"
    notify "Window restored from trash"
}

restore_all() {
    local address count=0 latest=""

    purge_expired

    while IFS=$'\t' read -r _ address; do
        [[ -n "$address" ]] || continue
        restore_window "$address" && {
            ((count += 1))
            latest="$address"
        }
    done < <(trashed_windows_by_age)

    if (( count == 0 )); then
        notify "Trash is empty"
        return 0
    fi

    [[ -n "$latest" ]] && dispatch_focus "$latest" >/dev/null 2>&1 || true
    notify "Restored $count window(s) from trash"
}

status() {
    purge_expired
    trashed_windows_by_age
}

case "${1:-}" in
    trash)
        trash_active
        ;;
    restore-last)
        restore_last
        ;;
    restore-all)
        restore_all
        ;;
    expire)
        expire_window "${2:-}" "${3:-}"
        ;;
    purge)
        purge_expired
        ;;
    status)
        status
        ;;
    *)
        echo "Usage: $0 {trash|restore-last|restore-all|expire|purge|status}" >&2
        exit 2
        ;;
esac
