-- Native desktop tools. Existing app, workspace and power shortcuts stay in place.
hl.bind("SUPER + SPACE", hl.dsp.exec_cmd("~/.local/bin/nacre-shell palette"))
hl.bind("SUPER + K", hl.dsp.exec_cmd("~/.local/bin/nacre-menu learn.shortcuts"))
hl.bind("SUPER + CTRL + B", hl.dsp.exec_cmd("~/.local/bin/nacre-shell left"), { description = "Nacre:ai-sidebar" })

-- Passive hover surfaces do not steal keyboard focus. Escape still reaches apps.
hl.bind("Escape", hl.dsp.global("nacre_shell:dismissHoverEdges"), { non_consuming = true })

-- Detachment belongs to the shell, including UI-only installations.
hl.bind("SUPER + CTRL + D", hl.dsp.exec_cmd("~/.local/bin/nacre-shell detach"), { description = "Nacre:detach-panel" })

-- Float at first map, so popping out never briefly rearranges tiled applications.
hl.window_rule({
    match = { class = "^org[.]quickshell$", title = "^Nacre Pearl (Launcher|Dashboard|AI|Controls|Media) [0-9]+$" },
    float = true,
    no_anim = true,
})

-- Only a visible, owned edge panel receives Super+drag. All other presses retain
-- native compositor movement, including its release callback. No shell IPC runs
-- on a pointer event: the shell publishes finite geometry when panels change.
hl.unbind("SUPER + mouse:272")
local drag_route = nil
local native_peer = nil
local native_timer = nil
local native_last_x, native_last_y = nil, nil
local native_ticks = 0
local pearl_kinds = { Launcher = true, Dashboard = true, AI = true, Controls = true, Media = true }
local function native_event(phase, cursor)
    if not native_peer then return end
    hl.dispatch(hl.dsp.event("nacre_pearl_drag," .. phase .. "," .. native_peer.title .. "," .. cursor.x .. "," .. cursor.y))
end
local function watch_native_drag()
    if not native_peer or native_peer.pid ~= nacre_panel_pid or native_ticks >= 3600 then
        native_timer:set_enabled(false)
        native_peer = nil
        return
    end
    native_ticks = native_ticks + 1
    local cursor = hl.get_cursor_pos()
    if cursor.x ~= native_last_x or cursor.y ~= native_last_y then
        native_last_x, native_last_y = cursor.x, cursor.y
        native_event("move", cursor)
    end
end
local function start_native_drag()
    local result = hl.dispatch(hl.dsp.window.drag())
    local window = hl.get_active_window()
    local kind = window and window.title:match("^Nacre Pearl (%a+) %d+$")
    if window and window.pid == nacre_panel_pid and pearl_kinds[kind] then
        native_peer = { title = window.title, pid = window.pid }
        native_ticks = 0
        native_last_x, native_last_y = nil, nil
        native_event("begin", hl.get_cursor_pos())
        if not native_timer then
            native_timer = hl.timer(watch_native_drag, { timeout = 33, type = "repeat" })
        else
            native_timer:set_enabled(true)
        end
    end
    return result
end
hl.bind("SUPER + mouse:272", function()
    if drag_route ~= nil then
        local shell = drag_route
        drag_route = nil
        if shell then return { ok = true, pass_event = true } end
        local result = hl.dispatch(hl.dsp.window.drag())
        if native_peer then native_event("end", hl.get_cursor_pos()) end
        native_peer = nil
        if native_timer then native_timer:set_enabled(false) end
        return result
    end
    local cursor = hl.get_cursor_pos()
    local shell = false
    for _, layer in ipairs(hl.get_layers()) do
        if layer.namespace == "nacre-drawers" and layer.mapped and layer.pid == nacre_panel_pid then
            for _, rect in ipairs(nacre_panel_regions or {}) do
                if cursor.x >= rect.x and cursor.y >= rect.y and cursor.x < rect.x + rect.w and cursor.y < rect.y + rect.h then
                    shell = true
                    break
                end
            end
        end
        if shell then break end
    end
    drag_route = shell
    if shell then return { ok = true, pass_event = true, request_release = true } end
    return start_native_drag()
end, { description = "Nacre:move-panel-or-window" })
