-- Stable split direction and predictable workspace cycling.
hl.config({
    ["dwindle.preserve_split"] = true,
    ["binds.workspace_back_and_forth"] = false,
    ["binds.allow_workspace_cycles"] = true,
    ["binds.pass_mouse_when_bound"] = false,
})

-- The existing three-finger workspace gesture remains compositor-owned.
hl.gesture({ fingers = 3, direction = "horizontal", action = "workspace" })
