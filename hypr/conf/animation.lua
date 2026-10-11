-- Nacre motion defaults. Private accessibility/desktop overrides load later.
hl.config({ ["animations.enabled"] = true })

-- Observed timing curves preserve the current desktop's motion.
local curves = {
    nacre_window_enter = { { 0.05, 0.7 }, { 0.1, 1 } },
    nacre_window_leave = { { 0.3, 0 }, { 0.8, 0.15 } },
    nacre_panel_enter = { { 0.1, 1 }, { 0, 1 } },
    nacre_panel_leave = { { 0.38, 0.04 }, { 1, 0.07 } },
}
for name, points in pairs(curves) do
    hl.curve(name, { type = "bezier", points = points })
end

-- A family shares one transition; omitted child branches keep native inheritance.
local transitions = {
    { scopes = { "windows", "windowsIn" }, duration = 3, curve = "nacre_window_enter", style = "popin 60%" },
    { scopes = { "windowsOut" }, duration = 3, curve = "nacre_window_leave", style = "popin 60%" },
    { scopes = { "fade", "specialWorkspace" }, duration = 3, curve = "nacre_window_enter" },
    { scopes = { "border" }, duration = 10, curve = "default" },
    { scopes = { "layersIn" }, duration = 3, curve = "nacre_panel_enter", style = "slide" },
    { scopes = { "layersOut" }, duration = 1.6, curve = "nacre_panel_leave" },
    { scopes = { "fadeLayersIn" }, duration = 2, curve = "nacre_panel_enter" },
    { scopes = { "fadeLayersOut" }, duration = 4.5, curve = "nacre_panel_leave" },
    { scopes = { "workspaces" }, duration = 7, curve = "nacre_panel_enter", style = "slide" },
}
for _, transition in ipairs(transitions) do
    for _, scope in ipairs(transition.scopes) do
        local style = scope == "specialWorkspace" and "slidevert" or transition.style
        hl.animation({
            leaf = scope,
            enabled = true,
            speed = transition.duration,
            bezier = transition.curve,
            style = style,
        })
    end
end
