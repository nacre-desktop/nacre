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
    border_size = 0,
})

-- Only a visible, owned edge panel receives Super+drag. All other presses retain
-- native compositor movement, including its release callback. No shell IPC runs
-- on a pointer event: the shell publishes finite geometry when panels change.
hl.unbind("SUPER + mouse:272")
local drag_route = nil
hl.bind("SUPER + mouse:272", function()
    if drag_route ~= nil then
        local shell = drag_route
        drag_route = nil
        if shell then return { ok = true, pass_event = true } end
        return hl.dispatch(hl.dsp.window.drag())
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
    return hl.dispatch(hl.dsp.window.drag())
end, { description = "Nacre:move-panel-or-window" })
