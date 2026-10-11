#!/usr/bin/env bash

set -euo pipefail

lua_quote() {
    local value="$1"

    value="${value//\\/\\\\}"
    value="${value//\"/\\\"}"
    value="${value//$'\n'/\\n}"
    printf '"%s"' "$value"
}

eval_lua() {
    hyprctl eval "$1"
}

command_name="${1:-}"
shift || true

case "$command_name" in
    focus-workspace)
        workspace="${1:?workspace required}"
        current_monitor="${2:-false}"
        eval_lua "hl.dispatch(hl.dsp.focus({ workspace = $(lua_quote "$workspace"), on_current_monitor = $current_monitor }))"
        ;;
    focus-window)
        window="${1:?window selector required}"
        eval_lua "hl.dispatch(hl.dsp.focus({ window = $(lua_quote "$window") }))"
        ;;
    move-window)
        workspace="${1:?workspace required}"
        window="${2:?window selector required}"
        follow="${3:-false}"
        eval_lua "hl.dispatch(hl.dsp.window.move({ workspace = $(lua_quote "$workspace"), window = $(lua_quote "$window"), follow = $follow }))"
        ;;
    move-workspace)
        workspace="${1:?workspace required}"
        monitor="${2:?monitor required}"
        eval_lua "hl.dispatch(hl.dsp.workspace.move({ workspace = $(lua_quote "$workspace"), monitor = $(lua_quote "$monitor") }))"
        ;;
    close-window)
        window="${1:?window selector required}"
        eval_lua "hl.dispatch(hl.dsp.window.close({ window = $(lua_quote "$window") }))"
        ;;
    kill-window)
        window="${1:?window selector required}"
        eval_lua "hl.dispatch(hl.dsp.window.kill({ window = $(lua_quote "$window") }))"
        ;;
    fullscreen)
        mode="${1:-fullscreen}"
        window="${2:-activewindow}"
        eval_lua "hl.dispatch(hl.dsp.window.fullscreen({ mode = $(lua_quote "$mode"), window = $(lua_quote "$window") }))"
        ;;
    resize-window)
        width="${1:?width required}"
        height="${2:?height required}"
        window="${3:-activewindow}"
        eval_lua "hl.dispatch(hl.dsp.window.resize({ x = $width, y = $height, relative = false, window = $(lua_quote "$window") }))"
        ;;
    dpms)
        action="${1:?enable, disable, or toggle required}"
        monitor="${2:-}"
        if [[ -n "$monitor" ]]; then
            eval_lua "hl.dispatch(hl.dsp.dpms({ action = $(lua_quote "$action"), monitor = $(lua_quote "$monitor") }))"
        else
            eval_lua "hl.dispatch(hl.dsp.dpms({ action = $(lua_quote "$action") }))"
        fi
        ;;
    monitor)
        output="${1:?output required}"
        mode="${2:?mode required}"
        position="${3:-auto}"
        scale="${4:-1}"
        mirror="${5:-}"
        if [[ "$mode" == "disable" ]]; then
            eval_lua "hl.monitor({ output = $(lua_quote "$output"), disabled = true })"
        elif [[ -n "$mirror" ]]; then
            eval_lua "hl.monitor({ output = $(lua_quote "$output"), disabled = false, mode = $(lua_quote "$mode"), position = $(lua_quote "$position"), scale = $scale, mirror = $(lua_quote "$mirror") })"
        else
            eval_lua "hl.monitor({ output = $(lua_quote "$output"), disabled = false, mode = $(lua_quote "$mode"), position = $(lua_quote "$position"), scale = $scale })"
        fi
        ;;
    animations)
        enabled="${1:?true or false required}"
        eval_lua "hl.config({ animations = { enabled = $enabled } })"
        ;;
    performance)
        enabled="${1:?true or false required}"
        if [[ "$enabled" == "true" ]]; then
            eval_lua 'hl.config({ animations = { enabled = false }, decoration = { shadow = { enabled = false }, blur = { enabled = false }, active_opacity = 1, inactive_opacity = 1, fullscreen_opacity = 1, rounding = 0 }, general = { gaps_in = 0, gaps_out = 0, border_size = 1 } })'
        else
            hyprctl reload
        fi
        ;;
    toggle-all-float)
        eval_lua 'local ws = hl.get_active_workspace(); local windows = hl.get_workspace_windows(ws); local make_float = false; for _, window in ipairs(windows) do if not window.floating then make_float = true; break end end; for _, window in ipairs(windows) do hl.dispatch(hl.dsp.window.float({ action = make_float and "set" or "unset", window = window })) end'
        ;;
    exit)
        if command -v hyprshutdown >/dev/null 2>&1; then
            exec hyprshutdown
        fi
        eval_lua 'hl.dispatch(hl.dsp.exit())'
        ;;
    *)
        echo "Usage: $0 {focus-workspace|focus-window|move-window|move-workspace|close-window|kill-window|fullscreen|resize-window|dpms|monitor|animations|performance|toggle-all-float|exit} ..." >&2
        exit 2
        ;;
esac
